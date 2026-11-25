//! User-defined field values on activities, WBS nodes, resources and
//! assignments, checked against the field's data type, and new fields.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use totalfloat_model::{Schedule, UdfDataType, UdfSubject, UdfType, UdfTypeId, UdfValue};

use crate::{EditError, NextIds, clean, find_mut, find_wbs, parse_date, refuse, resource_named};

/// What a user-defined field applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum UdfFor {
    /// Activities (`TASK`).
    Activity,
    /// WBS nodes (`PROJWBS`).
    Wbs,
    /// Resources (`RSRC`).
    Resource,
    /// Resource assignments (`TASKRSRC`).
    Assignment,
}

impl From<UdfFor> for UdfSubject {
    fn from(kind: UdfFor) -> Self {
        match kind {
            UdfFor::Activity => Self::Activity,
            UdfFor::Wbs => Self::Wbs,
            UdfFor::Resource => Self::Resource,
            UdfFor::Assignment => Self::Assignment,
        }
    }
}

/// A user-defined field's data type, as a new field takes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum UdfKind {
    /// Free text.
    Text,
    /// A number with two decimals.
    Number,
    /// A whole number.
    Integer,
    /// A cost.
    Cost,
    /// A start date.
    StartDate,
    /// A finish date.
    FinishDate,
    /// An indicator: `UDF_G0` to `UDF_G4`.
    Indicator,
}

impl From<UdfKind> for UdfDataType {
    fn from(kind: UdfKind) -> Self {
        match kind {
            UdfKind::Text => Self::Text,
            UdfKind::Number => Self::Number,
            UdfKind::Integer => Self::Integer,
            UdfKind::Cost => Self::Cost,
            UdfKind::StartDate => Self::StartDate,
            UdfKind::FinishDate => Self::FinishDate,
            UdfKind::Indicator => Self::Indicator,
        }
    }
}

/// What a value is on, by the names an edit gives: an activity, a WBS node,
/// a resource, or an activity and a resource for an assignment.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Target<'a> {
    pub(crate) activity: Option<&'a str>,
    pub(crate) wbs: Option<&'a str>,
    pub(crate) resource: Option<&'a str>,
}

impl Target<'_> {
    fn subject(&self) -> Result<UdfSubject, EditError> {
        match (self.activity, self.wbs, self.resource) {
            (Some(_), None, None) => Ok(UdfSubject::Activity),
            (None, Some(_), None) => Ok(UdfSubject::Wbs),
            (None, None, Some(_)) => Ok(UdfSubject::Resource),
            (Some(_), None, Some(_)) => Ok(UdfSubject::Assignment),
            _ => refuse(
                "udf-target",
                "a value is on an activity, a WBS node, a resource, or an activity's resource",
                "give `activity`, `wbs`, `resource`, or `activity` and `resource`",
            ),
        }
    }

    fn label(&self) -> String {
        match (self.activity, self.wbs, self.resource) {
            (Some(a), _, Some(r)) => format!("{a}'s {r}"),
            (Some(a), _, None) => a.to_owned(),
            (None, Some(w), _) => format!("WBS {w}"),
            (None, None, Some(r)) => r.to_owned(),
            (None, None, None) => String::new(),
        }
    }
}

const fn subject_label(subject: &UdfSubject) -> &'static str {
    match subject {
        UdfSubject::Activity => "activities",
        UdfSubject::Wbs => "WBS nodes",
        UdfSubject::Resource => "resources",
        UdfSubject::Assignment => "assignments",
        UdfSubject::Project => "projects",
        UdfSubject::Other(_) => "other records",
    }
}

/// The field a label or `udf_type_id` means for this kind of record.
///
/// # Errors
///
/// [`EditError`] naming the fields there are for it.
pub fn udf_named(
    schedule: &Schedule,
    wanted: &str,
    subject: &UdfSubject,
) -> Result<UdfTypeId, EditError> {
    let lower = wanted.to_lowercase();
    let fields: Vec<&UdfType> = schedule
        .udf_types
        .values()
        .filter(|t| t.subject == *subject)
        .collect();
    // A label first; an ID only when no label matches.
    let labelled: Vec<&&UdfType> = fields
        .iter()
        .filter(|t| t.label.to_lowercase() == lower)
        .collect();
    let found = if labelled.is_empty() {
        fields
            .iter()
            .filter(|t| t.id.0.to_string() == wanted)
            .collect()
    } else {
        labelled
    };
    match found.as_slice() {
        [one] => Ok(one.id),
        [] => {
            let names: Vec<&str> = fields.iter().map(|t| t.label.as_str()).collect();
            let hint = if names.is_empty() {
                format!(
                    "the file has no fields for {}. add_udf_type adds one",
                    subject_label(subject)
                )
            } else {
                format!(
                    "fields for {}: {}. add_udf_type adds one",
                    subject_label(subject),
                    names.join(", ")
                )
            };
            refuse(
                "no-such-udf",
                format!("no field for {} is named {wanted}", subject_label(subject)),
                hint,
            )
        }
        _ => {
            let ids: Vec<String> = found.iter().map(|t| t.id.0.to_string()).collect();
            refuse(
                "ambiguous-udf",
                format!("{} fields are named {wanted}", found.len()),
                format!("use one's `udf_type_id`: {}", ids.join(", ")),
            )
        }
    }
}

/// The value as the field's data type holds it.
fn typed_value(field: &UdfType, text: &str) -> Result<UdfValue, EditError> {
    let label = &field.label;
    let number = |what: &str, hint: &str| {
        let parsed: Result<f64, _> = text.trim().parse();
        match parsed {
            Ok(n) if n.is_finite() => Ok(n),
            _ => refuse(
                "wrong-udf-type",
                format!("{label} holds {what}, and {text:?} isn't one"),
                hint.to_owned(),
            ),
        }
    };
    match &field.data_type {
        UdfDataType::Text => {
            if text.contains(['\t', '\n', '\r']) {
                return refuse(
                    "wrong-udf-type",
                    format!("{label} holds one line of text"),
                    "write it on one line",
                );
            }
            Ok(UdfValue::Text(text.to_owned()))
        }
        UdfDataType::Number => {
            let n = number("a number", "give a number such as 12.5")?;
            Ok(UdfValue::Number((n * 100.0).round() / 100.0))
        }
        UdfDataType::Cost => {
            let n = number("a cost", "give an amount such as 1500 or 99.95")?;
            Ok(UdfValue::Number((n * 100.0).round() / 100.0))
        }
        UdfDataType::Integer => match text.trim().parse::<i32>() {
            Ok(n) => Ok(UdfValue::Number(f64::from(n))),
            Err(_) => refuse(
                "wrong-udf-type",
                format!("{label} holds a whole number, and {text:?} isn't one"),
                "give a whole number such as 3, within about 2 billion either way",
            ),
        },
        UdfDataType::StartDate | UdfDataType::FinishDate => match parse_date(text.trim()) {
            Ok(date) => Ok(UdfValue::Date(date)),
            Err(_) => refuse(
                "wrong-udf-type",
                format!("{label} holds a date, and {text:?} isn't one"),
                "use the form YYYY-MM-DD HH:MM",
            ),
        },
        UdfDataType::Indicator => {
            let trimmed = text.trim();
            let level = trimmed
                .strip_prefix("UDF_G")
                .or_else(|| trimmed.strip_prefix("udf_g"))
                .unwrap_or(trimmed);
            match level {
                "0" | "1" | "2" | "3" | "4" => Ok(UdfValue::Text(format!("UDF_G{level}"))),
                _ => refuse(
                    "wrong-udf-type",
                    format!("{label} holds an indicator, and {text:?} isn't one"),
                    "give 0 to 4, or UDF_G0 to UDF_G4",
                ),
            }
        }
        UdfDataType::Code | UdfDataType::Other(_) => refuse(
            "unsupported-udf",
            format!("{label}'s values come from a code list, which edits can't set yet"),
            "set it in P6",
        ),
    }
}

/// The field values of the record the target names.
fn values_of<'a>(
    schedule: &'a mut Schedule,
    target: Target<'_>,
) -> Result<&'a mut BTreeMap<UdfTypeId, UdfValue>, EditError> {
    match (target.activity, target.wbs, target.resource) {
        (Some(activity), None, Some(resource)) => {
            let activity_id = find_mut(schedule, activity)?.id;
            let Some(resource_id) = resource_named(schedule, resource) else {
                return refuse(
                    "no-such-resource",
                    format!("no resource here is named {resource}"),
                    "`inspect` lists resources",
                );
            };
            let mut held = schedule
                .assignments
                .values_mut()
                .filter(|a| a.activity == activity_id && a.resource == Some(resource_id));
            match (held.next(), held.next()) {
                (Some(one), None) => Ok(&mut one.udfs),
                (None, _) => refuse(
                    "no-such-assignment",
                    format!("{activity} has no assignment of {resource}"),
                    "`query --columns code,resources` shows assignments",
                ),
                (Some(_), Some(_)) => refuse(
                    "ambiguous-assignment",
                    format!("{activity} has {resource} more than once"),
                    "set it in P6",
                ),
            }
        }
        (Some(activity), None, None) => Ok(&mut find_mut(schedule, activity)?.udfs),
        (None, Some(wbs), None) => {
            let id = find_wbs(schedule, wbs)?;
            match schedule.wbs.get_mut(&id) {
                Some(node) => Ok(&mut node.udfs),
                None => refuse(
                    "no-such-wbs",
                    format!("no WBS node has the code {wbs}"),
                    "`query --columns code,wbs` shows WBS codes",
                ),
            }
        }
        (None, None, Some(resource)) => {
            let Some(id) = resource_named(schedule, resource) else {
                return refuse(
                    "no-such-resource",
                    format!("no resource here is named {resource}"),
                    "`inspect` lists resources",
                );
            };
            match schedule.resources.get_mut(&id) {
                Some(r) => Ok(&mut r.udfs),
                None => refuse(
                    "no-such-resource",
                    format!("no resource here is named {resource}"),
                    "`inspect` lists resources",
                ),
            }
        }
        _ => Err(target.subject().err().unwrap_or_else(|| EditError {
            code: "udf-target",
            message: "a value is on one record".to_owned(),
            hint: "give `activity`, `wbs`, `resource`, or `activity` and `resource`".to_owned(),
        })),
    }
}

pub(crate) fn set_udf(
    schedule: &mut Schedule,
    target: Target<'_>,
    wanted: &str,
    value: &str,
) -> Result<String, EditError> {
    let subject = target.subject()?;
    let id = udf_named(schedule, wanted, &subject)?;
    let field = schedule.udf_types[&id].clone();
    let typed = typed_value(&field, value)?;
    values_of(schedule, target)?.insert(id, typed);
    Ok(format!("{} {} set to {value}", target.label(), field.label))
}

pub(crate) fn clear_udf(
    schedule: &mut Schedule,
    target: Target<'_>,
    wanted: &str,
) -> Result<String, EditError> {
    let subject = target.subject()?;
    let id = udf_named(schedule, wanted, &subject)?;
    let label = schedule.udf_types[&id].label.clone();
    if values_of(schedule, target)?.remove(&id).is_none() {
        return refuse(
            "no-udf-value",
            format!("{} has no {label} value", target.label()),
            "`query --columns code,udf:NAME` shows values",
        );
    }
    Ok(format!("{} no longer has a {label} value", target.label()))
}

pub(crate) fn add_udf_type(
    schedule: &mut Schedule,
    ids: &mut NextIds,
    label: &str,
    subject: UdfFor,
    kind: UdfKind,
) -> Result<String, EditError> {
    if !clean(label) {
        return refuse(
            "invalid-udf",
            format!("{label:?} can't name a field"),
            "write it on one line, with no spaces at the ends",
        );
    }
    let subject = UdfSubject::from(subject);
    let lower = label.to_lowercase();
    if schedule
        .udf_types
        .values()
        .any(|t| t.subject == subject && t.label.to_lowercase() == lower)
    {
        return refuse(
            "duplicate-udf",
            format!(
                "there's already a field for {} named {label}",
                subject_label(&subject)
            ),
            "use set_udf to give it values",
        );
    }
    let free = schedule
        .udf_types
        .keys()
        .next_back()
        .map_or(0, |last| last.0 + 1);
    let id = UdfTypeId(ids.udf_type.max(free));
    ids.udf_type = id.0 + 1;
    let what = subject_label(&subject);
    schedule.udf_types.insert(
        id,
        UdfType {
            id,
            subject,
            label: label.to_owned(),
            data_type: kind.into(),
            source: None,
        },
    );
    Ok(format!("Added the field {label}, for {what}"))
}
