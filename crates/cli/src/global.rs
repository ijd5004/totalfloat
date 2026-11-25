//! Global change: one change to every activity a filter finds. The filter is `query`'s, so it finds the same
//! activities, and the change becomes ordinary edits, one an activity, that
//! apply together or not at all. They come first in a run, since relative
//! changes such as `duration+=2d` are worked out from the file as it stands.

mod expr;

use std::collections::BTreeSet;

use serde_json::{Map, Value};
use totalfloat_calendar::WorkCalendar;
use totalfloat_edit::{ConstraintKind, Edit, Span, duration_lengths};
use totalfloat_model::{
    ActivityStatus, ActivityType, CalendarId, Minutes, UdfDataType, UdfSubject,
};

use crate::error::CliError;
use crate::loaded::Loaded;
use crate::query::{self, Query};

/// The filters `--where` takes: `query`'s, by the names its flags have.
const FILTERS: [&str; 14] = [
    "project",
    "wbs",
    "code",
    "resource",
    "status",
    "type",
    "float-min",
    "float-max",
    "critical",
    "constrained",
    "start-from",
    "start-to",
    "finish-from",
    "finish-to",
];

/// `--where` as `query`'s filter: `KEY=VALUE` pairs separated by `;`, with
/// `query`'s flag names, as `wbs=CIV;code=Phase=CON;status=not-started`.
/// `critical` and `constrained` take no value; `status` and `type` can come
/// more than once; `text=` finds text in the code or name.
///
/// # Errors
///
/// [`CliError`] naming a filter it doesn't know or a value it can't read.
pub fn parse_where(text: &str) -> Result<Query, CliError> {
    let mut object = Map::new();
    for part in text.split(';').map(str::trim).filter(|p| !p.is_empty()) {
        let (key, value) = part.split_once('=').unwrap_or((part, ""));
        let key = key.trim();
        if key != "text" && !FILTERS.contains(&key) {
            return Err(CliError::new(
                "invalid-filter",
                format!("{key} isn't a filter"),
                format!("filters: {}, text", FILTERS.join(", ")),
            ));
        }
        let name = key.replace('-', "_");
        let value = value.trim();
        let value = match key {
            "critical" | "constrained" => match value {
                "" | "true" | "yes" => Value::Bool(true),
                "false" | "no" => continue,
                _ => {
                    return Err(CliError::new(
                        "invalid-filter",
                        format!("{part} is on or off"),
                        format!("write {key} alone, or leave it out"),
                    ));
                }
            },
            "project" => value
                .trim()
                .parse::<u64>()
                .map(Value::from)
                .map_err(|_| bad_value(part))?,
            "float-min" | "float-max" => value
                .trim()
                .parse::<f64>()
                .map(Value::from)
                .map_err(|_| bad_value(part))?,
            "status" | "type" => {
                let held = object
                    .entry(name.clone())
                    .or_insert_with(|| Value::Array(Vec::new()));
                if let Value::Array(values) = held {
                    values.push(Value::from(value));
                }
                continue;
            }
            _ => Value::from(value),
        };
        object.insert(name, value);
    }
    serde_json::from_value(Value::Object(object)).map_err(|error| {
        CliError::new(
            "invalid-filter",
            format!("{text} isn't a filter: {error}"),
            "status takes not-started, in-progress or completed, and type task, resource, loe, \
             start-milestone, finish-milestone or wbs-summary",
        )
    })
}

fn bad_value(part: &str) -> CliError {
    CliError::new(
        "invalid-filter",
        format!("{part} needs a number"),
        "give one, as float-max=40",
    )
}

/// A filter that narrows nothing, or asks for more than activities, isn't a
/// global change's: it would change every activity, or not the ones meant.
fn check_filter(query: &Query) -> Result<(), CliError> {
    let extra = [
        ("preds", query.preds.is_some()),
        ("succs", query.succs.is_some()),
        ("driving_path_to", query.driving_path_to.is_some()),
        ("baseline", query.baseline.is_some()),
        ("baseline_project", query.baseline_project.is_some()),
        ("columns", !query.columns.is_empty()),
    ];
    if let Some((name, _)) = extra.iter().find(|(_, given)| *given) {
        return Err(CliError::new(
            "invalid-filter",
            format!("{name} isn't a filter a global change takes"),
            format!("filters: {}, text", FILTERS.join(", ")),
        ));
    }
    let narrows = query.project.is_some()
        || query.wbs.is_some()
        || query.code.is_some()
        || query.resource.is_some()
        || !query.status.is_empty()
        || !query.kind.is_empty()
        || query.float_min.is_some()
        || query.float_max.is_some()
        || query.critical
        || query.constrained
        || query.start_from.is_some()
        || query.start_to.is_some()
        || query.finish_from.is_some()
        || query.finish_to.is_some()
        || query.text.is_some();
    if !narrows {
        return Err(CliError::new(
            "invalid-filter",
            "the filter finds every activity",
            "give a filter, such as wbs=CIV or code=Phase=CON",
        ));
    }
    Ok(())
}

/// One change to make to each activity found.
#[derive(Debug, Clone, PartialEq)]
pub enum Change {
    /// `duration=5d`: the original duration.
    Duration(Span),
    /// `duration+=2d` or `duration-=2d`: more or less original duration, in
    /// days of each activity's calendar.
    AddDuration(Span, bool),
    /// `calendar=NAME`.
    Calendar(String),
    /// `constraint=KIND@DATE`: the primary constraint.
    Constraint(String),
    /// `code=TYPE=VALUE`.
    Code(String, String),
    /// `udf=FIELD=VALUE`.
    Udf(String, String),
    /// `prefix=TEXT`: text before the name.
    Prefix(String),
    /// `suffix=TEXT`: text after the name.
    Suffix(String),
    /// `FIELD=EXPRESSION`, such as `remaining_duration=original_duration*0.5`,
    /// `constraint_date=early_start+5d` or `udf:Crew hours=original_duration/1h`
    ///.
    Formula(Target, expr::Expr),
}

/// A field an expression can set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// The original duration.
    Original,
    /// The remaining duration.
    Remaining,
    /// The primary constraint's date, keeping its kind.
    ConstraintDate,
    /// An activity user-defined field that holds a number, a cost, an
    /// integer or a date, by label or `udf_type_id`.
    Udf(String),
}

impl Target {
    fn name(&self) -> String {
        match self {
            Self::Original => "duration".to_owned(),
            Self::Remaining => "remaining_duration".to_owned(),
            Self::ConstraintDate => "constraint_date".to_owned(),
            Self::Udf(field) => format!("udf:{field}"),
        }
    }

    /// The kind the field takes; a user field's depends on the file.
    const fn kind(&self) -> Option<expr::Kind> {
        match self {
            Self::Original | Self::Remaining => Some(expr::Kind::Duration),
            Self::ConstraintDate => Some(expr::Kind::Date),
            Self::Udf(_) => None,
        }
    }
}

/// `--set` as a change.
///
/// # Errors
///
/// [`CliError`] naming the forms it takes.
pub fn parse_set(text: &str) -> Result<Change, CliError> {
    let bad = || {
        CliError::new(
            "invalid-change",
            format!("{text} isn't a change"),
            "use duration=5d, duration+=2d, duration-=1d, calendar=NAME, \
             constraint=KIND@DATE, code=TYPE=VALUE, udf=FIELD=VALUE, prefix=TEXT or suffix=TEXT, \
             or duration, remaining_duration, constraint_date or udf:FIELD = an expression",
        )
    };
    let span = |value: &str| value.trim().parse::<Span>().map_err(|_| bad());
    if let Some(value) = text.strip_prefix("duration+=") {
        return Ok(Change::AddDuration(span(value)?, true));
    }
    if let Some(value) = text.strip_prefix("duration-=") {
        return Ok(Change::AddDuration(span(value)?, false));
    }
    let (key, value) = text.split_once('=').ok_or_else(bad)?;
    let pair = |value: &str| {
        value
            .split_once('=')
            .filter(|(name, value)| !name.is_empty() && !value.is_empty())
            .map(|(name, value)| (name.to_owned(), value.to_owned()))
            .ok_or_else(bad)
    };
    let target = match key.trim() {
        "duration" | "original_duration" => Some(Target::Original),
        "remaining_duration" => Some(Target::Remaining),
        "constraint_date" => Some(Target::ConstraintDate),
        key => key
            .strip_prefix("udf:")
            .filter(|field| !field.trim().is_empty())
            .map(|field| Target::Udf(field.trim().to_owned())),
    };
    if let Some(target) = target {
        if target == Target::Original
            && let Ok(span) = value.trim().parse::<Span>()
        {
            return Ok(Change::Duration(span));
        }
        return formula(text, target, value);
    }
    match key.trim() {
        "calendar" if !value.is_empty() => Ok(Change::Calendar(value.to_owned())),
        "constraint" if !value.is_empty() => Ok(Change::Constraint(value.to_owned())),
        "code" => pair(value).map(|(t, v)| Change::Code(t, v)),
        "udf" => pair(value).map(|(f, v)| Change::Udf(f, v)),
        "prefix" if !value.is_empty() => Ok(Change::Prefix(value.to_owned())),
        "suffix" if !value.is_empty() => Ok(Change::Suffix(value.to_owned())),
        _ => Err(bad()),
    }
}

/// `FIELD=EXPRESSION` as a change, its kind checked against the field's.
fn formula(text: &str, target: Target, value: &str) -> Result<Change, CliError> {
    let refuse = |why: String| {
        CliError::new(
            "invalid-expression",
            format!("--set {text}: {why}"),
            format!(
                "an expression takes numbers, spans such as 5d, + - * / and ( ), and these fields: {}",
                expr::field_names()
            ),
        )
    };
    let parsed = expr::parse(value).map_err(refuse)?;
    let kind = expr::kind_of(&parsed).map_err(refuse)?;
    let wrong = match target.kind() {
        Some(wanted) => (kind != wanted).then(|| wanted.words()),
        // A user field takes a number or a date, as the file says which.
        None => (kind == expr::Kind::Duration).then_some("a number or a date"),
    };
    if let Some(wanted) = wrong {
        return Err(refuse(format!(
            "it gives {}, and {} takes {wanted}",
            kind.words(),
            target.name(),
        )));
    }
    Ok(Change::Formula(target, parsed))
}

/// What a global change comes to: its edits, where each came from, and the
/// one project they're in.
pub struct Expanded {
    /// The edits, in the order `query` lists the activities.
    pub edits: Vec<Edit>,
    /// Each edit's `--set` and activity.
    pub sources: Vec<String>,
    /// The project the activities are in (`proj_id`).
    pub project: u64,
}

/// The edits the changes, each with its text as given, make to every activity
/// the filter finds. Durations skip milestones, level of effort and WBS
/// summary activities, which have none of their own. The filter and the
/// changes read the file as it stands, so the edits go before the run's
/// others.
///
/// # Errors
///
/// [`CliError`] when the filter narrows nothing or asks for more than
/// activities, finds nothing to change, finds activities in several
/// projects, or a duration can't be worked out.
pub fn expand(
    file: &str,
    loaded: &Loaded,
    mut query: Query,
    changes: &[(String, Change)],
) -> Result<Expanded, CliError> {
    check_filter(&query)?;
    query.columns = vec!["project".to_owned(), "code".to_owned()];
    query.limit = usize::MAX / 2;
    query.offset = 0;
    let report = query::report(file, loaded, &query)?;
    let found: Vec<(u64, String)> = report
        .rows
        .iter()
        .filter_map(|row| {
            Some((
                row.get("project")?.as_u64()?,
                row.get("code")?.as_str()?.to_owned(),
            ))
        })
        .collect();
    if found.is_empty() {
        return Err(CliError::new(
            "nothing-found",
            "the filter finds no activities",
            "try it with `query` first",
        ));
    }
    let projects: BTreeSet<u64> = found.iter().map(|(p, _)| *p).collect();
    if projects.len() > 1 {
        return Err(CliError::new(
            "needs-project",
            format!("the filter finds activities in {} projects", projects.len()),
            "add project=ID to the filter, or --project",
        ));
    }
    let project = found[0].0;
    let Some(schedule) = loaded.schedules.iter().find(|s| s.project.id.0 == project) else {
        return Err(CliError::new(
            "no-such-project",
            format!("the file has no project {project}"),
            "`inspect` lists the projects in the file",
        ));
    };
    let by_code: std::collections::BTreeMap<&str, &totalfloat_model::Activity> = schedule
        .activities
        .values()
        .map(|a| (a.code.as_str(), a))
        .collect();
    // A user field's kind is the file's, so it's checked before anything changes.
    for (text, change) in changes {
        if let Change::Formula(Target::Udf(field), formula) = change {
            udf_kind(schedule, text, field, formula)?;
        }
    }
    let mut calendars = Calendars::new();
    let mut edits = Vec::new();
    let mut sources = Vec::new();
    for (_, code) in &found {
        let Some(activity) = by_code.get(code.as_str()) else {
            continue;
        };
        // Each change builds on the ones before it, so `duration=5d` then
        // `duration+=1d` gives 6d, and a prefix and a suffix both stay.
        let mut staged = (*activity).clone();
        for (text, change) in changes {
            let Some(edit) = edit_for(schedule, &staged, (text, change), &mut calendars)? else {
                continue;
            };
            stage(&mut staged, &edit, schedule);
            edits.push(edit);
            sources.push(format!("--set {text} ({code})"));
        }
    }
    if edits.is_empty() {
        return Err(CliError::new(
            "nothing-to-change",
            format!(
                "the filter finds {} activities, and none has a duration of its own",
                found.len()
            ),
            "milestones, level of effort and WBS summaries take their durations from elsewhere",
        ));
    }
    Ok(Expanded {
        edits,
        sources,
        project,
    })
}

/// What an edit leaves the activity as, for the changes after it: its
/// calendar, durations, primary constraint and name.
fn stage(
    staged: &mut totalfloat_model::Activity,
    edit: &Edit,
    schedule: &totalfloat_model::Schedule,
) {
    match edit {
        Edit::SetDuration {
            original,
            remaining,
            ..
        } => {
            let lengths = duration_lengths(schedule, staged);
            let minutes = |span: &Option<Span>| span.and_then(|s| s.minutes_in(lengths.as_ref()));
            let (original, remaining) = (minutes(original), minutes(remaining));
            if let Some(value) = original {
                staged.original_duration = value;
                // As the edit does: remaining follows on work not started.
                if staged.status == ActivityStatus::NotStarted && remaining.is_none() {
                    staged.remaining_duration = value;
                }
            }
            if let Some(value) = remaining {
                staged.remaining_duration = value;
            }
        }
        Edit::SetConstraint {
            constraint,
            date,
            secondary: false,
            ..
        } => {
            staged.primary_constraint = Some(totalfloat_model::Constraint {
                kind: (*constraint).into(),
                date: date.as_deref().and_then(|d| d.parse().ok()),
            });
        }
        Edit::ClearConstraint {
            secondary: false, ..
        } => staged.primary_constraint = None,
        Edit::SetName { name, .. } => name.clone_into(&mut staged.name),
        Edit::SetCalendar { calendar, .. } => {
            if let [one] = totalfloat_edit::calendars_named(schedule, calendar).as_slice() {
                staged.calendar = *one;
            }
        }
        _ => {}
    }
}

/// Whether the activity has a duration of its own to change.
const fn has_duration(kind: ActivityType) -> bool {
    matches!(
        kind,
        ActivityType::TaskDependent | ActivityType::ResourceDependent
    )
}

/// Each calendar compiled once, for the date arithmetic of every activity on it.
type Calendars = std::collections::BTreeMap<CalendarId, Option<WorkCalendar>>;

/// What a user field takes, checked against what the expression gives.
fn udf_kind(
    schedule: &totalfloat_model::Schedule,
    text: &str,
    field: &str,
    formula: &expr::Expr,
) -> Result<UdfDataType, CliError> {
    let id = totalfloat_edit::udf_named(schedule, field, &UdfSubject::Activity).map_err(|why| {
        CliError::new(why.code, format!("--set {text}: {}", why.message), why.hint)
    })?;
    let data_type = schedule
        .udf_types
        .get(&id)
        .map_or(UdfDataType::Text, |t| t.data_type.clone());
    let takes = match data_type {
        UdfDataType::Number | UdfDataType::Cost | UdfDataType::Integer => Some(expr::Kind::Number),
        UdfDataType::StartDate | UdfDataType::FinishDate => Some(expr::Kind::Date),
        _ => None,
    };
    let gives = expr::kind_of(formula).unwrap_or(expr::Kind::Number);
    match takes {
        Some(kind) if kind == gives => Ok(data_type),
        Some(kind) => Err(CliError::new(
            "invalid-expression",
            format!(
                "--set {text}: it gives {}, and udf:{field} takes {}",
                gives.words(),
                kind.words()
            ),
            "a number, cost or integer field takes a number; a date field takes a date",
        )),
        None => Err(CliError::new(
            "invalid-expression",
            format!("--set {text}: udf:{field} holds neither a number nor a date"),
            "set text and indicator fields with udf=FIELD=VALUE",
        )),
    }
}

fn edit_for(
    schedule: &totalfloat_model::Schedule,
    activity: &totalfloat_model::Activity,
    (text, change): (&str, &Change),
    calendars: &mut Calendars,
) -> Result<Option<Edit>, CliError> {
    let code = activity.code.clone();
    Ok(Some(match change {
        Change::Duration(_)
        | Change::AddDuration(..)
        | Change::Formula(Target::Original | Target::Remaining, _)
            if !has_duration(activity.activity_type) =>
        {
            return Ok(None);
        }
        // Finished work has no remaining duration, and a constraint on it does nothing.
        Change::Formula(Target::Remaining | Target::ConstraintDate, _)
            if activity.status == ActivityStatus::Completed =>
        {
            return Ok(None);
        }
        Change::Formula(target, formula) => {
            return worked_out(schedule, activity, (text, target, formula), calendars).map(Some);
        }
        Change::Duration(span) => Edit::SetDuration {
            activity: code,
            original: Some(*span),
            remaining: None,
        },
        Change::AddDuration(span, more) => {
            let lengths = duration_lengths(schedule, activity);
            let by = span.minutes_in(lengths.as_ref()).ok_or_else(|| {
                // The edits' own refusal, so the two say it alike.
                let why = totalfloat_edit::no_length(&code, *span);
                CliError::new(why.code, why.message, why.hint)
            })?;
            let by = if *more { by.get() } else { -by.get() };
            let total = Minutes::new(activity.original_duration.get() + by);
            if total.is_negative() {
                return Err(CliError::new(
                    "negative-duration",
                    format!("{code} would have a negative duration"),
                    "take off less, or narrow the filter",
                ));
            }
            let span = Span::hours_of(total).ok_or_else(|| {
                CliError::new(
                    "invalid-duration",
                    format!("{code}'s new duration can't be written exactly"),
                    "give the change in whole hours",
                )
            })?;
            Edit::SetDuration {
                activity: code,
                original: Some(span),
                remaining: None,
            }
        }
        Change::Calendar(name) => Edit::SetCalendar {
            activity: code,
            calendar: name.clone(),
        },
        Change::Constraint(value) => crate::edit::constraint(&format!("{code}={value}"), false)?,
        Change::Code(kind, value) => Edit::SetCode {
            activity: code,
            code_type: kind.clone(),
            value: value.clone(),
        },
        Change::Udf(field, value) => Edit::SetUdf {
            field: field.clone(),
            value: value.clone(),
            activity: Some(code),
            wbs: None,
            resource: None,
        },
        Change::Prefix(text) => Edit::SetName {
            name: format!("{text}{}", activity.name),
            activity: code,
        },
        Change::Suffix(text) => Edit::SetName {
            name: format!("{}{text}", activity.name),
            activity: code,
        },
    }))
}

/// A value as a user field of this type holds it: an integer whole, a
/// number or cost to 2 places, a date as `YYYY-MM-DD HH:MM`. `None` for a
/// value out of range or of another kind.
fn udf_text(data_type: &UdfDataType, value: expr::Value) -> Option<String> {
    match value {
        expr::Value::Number(n) if !n.is_finite() || n.abs() >= 1e12 => None,
        expr::Value::Number(n) if *data_type == UdfDataType::Integer => {
            Some(format!("{:.0}", n.round()))
        }
        expr::Value::Number(n) => Some(format!("{}", (n * 100.0).round() / 100.0)),
        expr::Value::Date(date) => Some(date.to_string()),
        expr::Value::Duration(_) => None,
    }
}

/// The edit an expression comes to for one activity.
fn worked_out(
    schedule: &totalfloat_model::Schedule,
    activity: &totalfloat_model::Activity,
    (text, target, formula): (&str, &Target, &expr::Expr),
    calendars: &mut Calendars,
) -> Result<Edit, CliError> {
    let code = activity.code.clone();
    let failed = |why: String| {
        CliError::new(
            "expression-failed",
            format!("--set {text}: {why}"),
            "narrow the filter to activities that have what the expression reads",
        )
    };
    let calendar = calendars
        .entry(activity.calendar)
        .or_insert_with(|| {
            schedule
                .calendars
                .get(&activity.calendar)
                .and_then(|c| WorkCalendar::new(c).ok())
        })
        .as_ref();
    let value = expr::Context::new(schedule, activity, calendar)
        .value(formula)
        .map_err(failed)?;
    let finite = |n: f64| {
        if n.is_finite() && n.abs() < 1e12 {
            Ok(n)
        } else {
            Err(failed(format!("{code}'s value would be out of range")))
        }
    };
    match (target, value) {
        (Target::Original | Target::Remaining, expr::Value::Duration(minutes)) => {
            #[allow(
                clippy::cast_possible_truncation,
                reason = "rounded to whole minutes, checked to be in range"
            )]
            let minutes = Minutes::new(finite(minutes)?.round() as i64);
            if minutes.is_negative() {
                return Err(CliError::new(
                    "negative-duration",
                    format!("--set {text}: {code} would have a negative duration"),
                    "change the expression, or narrow the filter",
                ));
            }
            let span = Span::hours_of(minutes)
                .ok_or_else(|| failed(format!("{code}'s new duration is too long to write")))?;
            let (original, remaining) = if *target == Target::Original {
                (Some(span), None)
            } else {
                (None, Some(span))
            };
            Ok(Edit::SetDuration {
                activity: code,
                original,
                remaining,
            })
        }
        (Target::ConstraintDate, expr::Value::Date(date)) => {
            let kind = activity
                .primary_constraint
                .filter(|c| c.date.is_some())
                .map(|c| ConstraintKind::from(c.kind))
                .ok_or_else(|| {
                    CliError::new(
                        "no-constraint-date",
                        format!(
                            "--set {text}: {code} has no primary constraint with a date to move"
                        ),
                        "add constrained to the filter, or set constraint=KIND@DATE",
                    )
                })?;
            Ok(Edit::SetConstraint {
                activity: code,
                constraint: kind,
                date: Some(date.to_string()),
                secondary: false,
            })
        }
        (Target::Udf(field), value) => {
            let data_type = udf_kind(schedule, text, field, formula)?;
            let written = udf_text(&data_type, value)
                .ok_or_else(|| failed(format!("{code}'s value would be out of range")))?;
            Ok(Edit::SetUdf {
                field: field.clone(),
                value: written,
                activity: Some(code),
                wbs: None,
                resource: None,
            })
        }
        _ => Err(failed(
            "the expression gave the wrong kind of value".to_owned(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_filter_reads_as_query_flags() {
        let q = parse_where("wbs=CIV; code=Phase=CON; status=not-started; status=in-progress; critical; float-max=40").unwrap();
        assert_eq!(q.wbs.as_deref(), Some("CIV"));
        assert_eq!(q.code.as_deref(), Some("Phase=CON"));
        assert_eq!(q.status.len(), 2);
        assert!(q.critical);
        assert_eq!(q.float_max, Some(40.0));
        let q = parse_where("wbs= CIV ;critical=false").unwrap();
        assert_eq!(q.wbs.as_deref(), Some("CIV"));
        assert!(!q.critical);
        for (text, code) in [
            ("wbz=CIV", "invalid-filter"),
            ("float-max=lots", "invalid-filter"),
            ("status=late", "invalid-filter"),
            ("critical=maybe", "invalid-filter"),
        ] {
            assert_eq!(parse_where(text).unwrap_err().code, code, "{text}");
        }
    }

    #[test]
    fn changes_read_in_their_forms() {
        assert_eq!(
            parse_set("duration=5d").unwrap(),
            Change::Duration("5d".parse().unwrap())
        );
        assert_eq!(
            parse_set("duration+=2d").unwrap(),
            Change::AddDuration("2d".parse().unwrap(), true)
        );
        assert_eq!(
            parse_set("duration-=4h").unwrap(),
            Change::AddDuration("4h".parse().unwrap(), false)
        );
        assert_eq!(
            parse_set("code=Phase=CON").unwrap(),
            Change::Code("Phase".to_owned(), "CON".to_owned())
        );
        assert_eq!(
            parse_set("prefix=OLD ").unwrap(),
            Change::Prefix("OLD ".to_owned())
        );
        assert!(matches!(
            parse_set("remaining_duration=original_duration*0.5").unwrap(),
            Change::Formula(Target::Remaining, _)
        ));
        assert!(matches!(
            parse_set("constraint_date=early_start+5d").unwrap(),
            Change::Formula(Target::ConstraintDate, _)
        ));
        assert!(matches!(
            parse_set("duration=remaining_duration+1d").unwrap(),
            Change::Formula(Target::Original, _)
        ));
        // A plain span is still the plain change.
        assert_eq!(
            parse_set("original_duration=5d").unwrap(),
            Change::Duration("5d".parse().unwrap())
        );
        for (text, said) in [
            (
                "remaining_duration=early_start",
                "--set remaining_duration=early_start: it gives a date, and remaining_duration takes a duration",
            ),
            (
                "constraint_date=early_start*2",
                "--set constraint_date=early_start*2: a date can't be multiplied by a number",
            ),
            (
                "duration=original_duration*0.5+3",
                "--set duration=original_duration*0.5+3: a duration can't be added to a number",
            ),
            (
                "constraint_date=colour+1d",
                "--set constraint_date=colour+1d: colour isn't a field an expression can read",
            ),
        ] {
            let error = parse_set(text).unwrap_err();
            assert_eq!(error.code, "invalid-expression", "{text}");
            assert_eq!(error.message, said, "{text}");
        }
        // `duration=` takes an expression now, so text that's neither is an expression's error.
        assert_eq!(
            parse_set("duration=soon").unwrap_err().code,
            "invalid-expression"
        );
        for text in ["code=Phase", "colour=red", "calendar="] {
            assert_eq!(
                parse_set(text).unwrap_err().code,
                "invalid-change",
                "{text}"
            );
        }
    }
}
