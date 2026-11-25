//! What an edit creates and what it needs, by the names a scheduler uses, so a
//! batch can say which refusals follow from another, and suggest a name for a
//! typo.

use totalfloat_model::Schedule;

use crate::{Edit, calendars_named, holds_wbs, resource_named};

/// Something an edit names.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Name {
    /// An activity, by Activity ID.
    Activity(String),
    /// A WBS node, by code.
    Wbs(String),
    /// A calendar, by name or ID.
    Calendar(String),
    /// A resource, by short name or name.
    Resource(String),
    /// A role, by short name or name.
    Role(String),
    /// A resource curve, by name.
    Curve(String),
    /// An activity code type, by name.
    CodeType(String),
    /// A value of a code type: the type's name and the value.
    CodeValue(String, String),
    /// An OBS node, by name.
    Obs(String),
    /// A user-defined field, by label.
    Udf(String),
    /// A project code type, by name.
    ProjectCodeType(String),
    /// A value of a project code type: the type's name and the value.
    ProjectCodeValue(String, String),
    /// A relationship: predecessor and successor Activity IDs.
    Link(String, String),
}

impl Name {
    /// The same name with letters lowered where lookups ignore case, so names
    /// compare as lookups do.
    fn folded(&self) -> Self {
        match self {
            Self::Activity(code) => Self::Activity(code.clone()),
            Self::Wbs(code) => Self::Wbs(code.to_lowercase()),
            Self::Calendar(name) => Self::Calendar(name.to_lowercase()),
            Self::Resource(name) => Self::Resource(name.to_lowercase()),
            Self::Role(name) => Self::Role(name.to_lowercase()),
            Self::Curve(name) => Self::Curve(name.to_lowercase()),
            Self::CodeType(name) => Self::CodeType(name.to_lowercase()),
            Self::CodeValue(kind, value) => Self::CodeValue(kind.to_lowercase(), value.clone()),
            Self::Obs(name) => Self::Obs(name.to_lowercase()),
            Self::Udf(name) => Self::Udf(name.to_lowercase()),
            Self::ProjectCodeType(name) => Self::ProjectCodeType(name.to_lowercase()),
            Self::ProjectCodeValue(kind, value) => {
                Self::ProjectCodeValue(kind.to_lowercase(), value.to_lowercase())
            }
            Self::Link(from, to) => Self::Link(from.clone(), to.clone()),
        }
    }

    /// True if both name the same thing.
    #[must_use]
    pub fn same(&self, other: &Self) -> bool {
        self.folded() == other.folded()
    }
}

/// An Activity ID as one project holds it, without a `PROJECT:` prefix.
fn plain(code: &str) -> &str {
    code.rsplit_once(':').map_or(code, |(_, code)| code)
}

fn activity(code: &str) -> Name {
    Name::Activity(plain(code).to_owned())
}

impl Edit {
    /// The names this edit brings into being.
    #[must_use]
    pub fn creates(&self) -> Vec<Name> {
        match self {
            Self::AddActivity { code, .. } => vec![activity(code)],
            Self::AddWbs { code, .. }
            | Self::RenameWbs {
                code: Some(code), ..
            } => vec![Name::Wbs(code.clone())],
            Self::AddCodeValue {
                code_type, value, ..
            } => vec![Name::CodeValue(code_type.clone(), value.clone())],
            Self::AddCodeType { name, .. } => vec![Name::CodeType(name.clone())],
            Self::AddProjectCodeType { name, .. } => vec![Name::ProjectCodeType(name.clone())],
            Self::AddObs { obs, .. } => vec![Name::Obs(obs.clone())],
            Self::AddUdfType { field, .. } => vec![Name::Udf(field.clone())],
            Self::AddProjectCodeValue {
                code_type, value, ..
            } => vec![Name::ProjectCodeValue(code_type.clone(), value.clone())],
            Self::AddResource { resource, name, .. } => {
                vec![
                    Name::Resource(resource.clone()),
                    Name::Resource(name.clone()),
                ]
            }
            Self::AddRole { role, name, .. } => {
                vec![Name::Role(role.clone()), Name::Role(name.clone())]
            }
            Self::AddCalendar { name, .. } => vec![Name::Calendar(name.clone())],
            Self::AddLink {
                predecessor,
                successor,
                ..
            } => vec![Name::Link(
                plain(predecessor).to_owned(),
                plain(successor).to_owned(),
            )],
            _ => Vec::new(),
        }
    }

    /// The names this edit needs to be there already.
    #[must_use]
    pub fn needs(&self) -> Vec<Name> {
        let mut needs: Vec<Name> = crate::activities_named(self)
            .into_iter()
            .map(activity)
            .collect();
        match self {
            Self::AddActivity { wbs, calendar, .. } => {
                needs.push(Name::Wbs(wbs.clone()));
                needs.extend(calendar.iter().map(|c| Name::Calendar(c.clone())));
            }
            Self::AddWbs { parent: wbs, .. }
            | Self::RenameWbs { wbs, .. }
            | Self::SetWbs { wbs, .. }
            | Self::AddNote { wbs: Some(wbs), .. }
            | Self::SetNote { wbs: Some(wbs), .. }
            | Self::DeleteNote { wbs: Some(wbs), .. } => needs.push(Name::Wbs(wbs.clone())),
            Self::MoveWbs { wbs, parent } => {
                needs.extend([Name::Wbs(wbs.clone()), Name::Wbs(parent.clone())]);
            }
            Self::SetCalendar { calendar, .. }
            | Self::AddCalendarException { calendar, .. }
            | Self::RemoveCalendarException { calendar, .. }
            | Self::SetWorkWeek { calendar, .. }
            | Self::SetHoursPerDay { calendar, .. }
            | Self::SetHoursPerPeriod { calendar, .. } => {
                needs.push(Name::Calendar(calendar.clone()));
            }
            Self::AddCalendar {
                copy_of: Some(copy),
                ..
            }
            | Self::AddResource {
                calendar: Some(copy),
                ..
            } => needs.push(Name::Calendar(copy.clone())),
            Self::AddCodeValue { code_type, .. } => needs.push(Name::CodeType(code_type.clone())),
            Self::AddProjectCodeValue { code_type, .. } | Self::ClearProjectCode { code_type } => {
                needs.push(Name::ProjectCodeType(code_type.clone()));
            }
            Self::AddObs {
                parent: Some(obs), ..
            } => needs.push(Name::Obs(obs.clone())),
            Self::SetUdf {
                field,
                wbs,
                resource,
                ..
            }
            | Self::ClearUdf {
                field,
                wbs,
                resource,
                ..
            } => {
                needs.push(Name::Udf(field.clone()));
                needs.extend(wbs.iter().map(|w| Name::Wbs(w.clone())));
                needs.extend(resource.iter().map(|r| Name::Resource(r.clone())));
            }
            Self::SetWbsObs { wbs, obs } => {
                needs.extend([Name::Wbs(wbs.clone()), Name::Obs(obs.clone())]);
            }
            Self::SetProjectCode { code_type, value } => {
                needs.push(Name::ProjectCodeValue(code_type.clone(), value.clone()));
            }
            Self::AssignResource { resource, .. }
            | Self::UnassignResource { resource, .. }
            | Self::SetAssignmentUnits { resource, .. }
            | Self::SetResourceLimit { resource, .. }
            | Self::SetSpread { resource, .. } => needs.push(Name::Resource(resource.clone())),
            Self::SetCurve {
                resource, curve, ..
            } => {
                needs.push(Name::Resource(resource.clone()));
                needs.extend(curve.iter().map(|c| Name::Curve(c.clone())));
            }
            Self::AssignRole { role, .. }
            | Self::UnassignRole { role, .. }
            | Self::SetRoleLimit { role, .. }
            | Self::AddRole {
                parent: Some(role), ..
            } => needs.push(Name::Role(role.clone())),
            Self::SetCode {
                code_type, value, ..
            } => needs.push(Name::CodeValue(code_type.clone(), value.clone())),
            Self::RemoveLink {
                predecessor,
                successor,
                ..
            }
            | Self::ChangeLink {
                predecessor,
                successor,
                ..
            } => needs.push(Name::Link(
                plain(predecessor).to_owned(),
                plain(successor).to_owned(),
            )),
            _ => {}
        }
        needs
    }

    /// True if this edit needs something `earlier` would have created.
    #[must_use]
    pub fn depends_on(&self, earlier: &Self) -> bool {
        let made = earlier.creates();
        self.needs()
            .iter()
            .any(|need| made.iter().any(|m| m.same(need)))
    }
}

/// True if the schedule holds this name. Links and code values aren't looked
/// up here, so they count as held.
#[must_use]
pub fn holds(schedule: &Schedule, name: &Name) -> bool {
    match name {
        Name::Activity(code) => schedule.activity_by_code(code).is_some(),
        Name::Wbs(code) => holds_wbs(schedule, code),
        Name::Calendar(name) => !calendars_named(schedule, name).is_empty(),
        Name::Resource(name) => resource_named(schedule, name).is_some(),
        Name::Role(name) => crate::role_named(schedule, name).is_some(),
        Name::Curve(name) => crate::curve_named(schedule, name).is_some(),
        Name::CodeType(name) => schedule
            .activity_code_types
            .values()
            .any(|t| t.name.eq_ignore_ascii_case(name)),
        Name::ProjectCodeType(name) => schedule
            .project_code_types
            .values()
            .any(|t| t.name.eq_ignore_ascii_case(name)),
        Name::Udf(name) => {
            let lower = name.to_lowercase();
            schedule
                .udf_types
                .values()
                .any(|t| t.label.to_lowercase() == lower || t.id.0.to_string() == *name)
        }
        Name::Obs(name) => {
            let lower = name.to_lowercase();
            schedule
                .obs
                .values()
                .any(|n| n.name.to_lowercase() == lower)
                || crate::obs_named(schedule, name).is_ok()
        }
        Name::CodeValue(..) | Name::ProjectCodeValue(..) | Name::Link(..) => true,
    }
}

/// Edits needed to turn one string into the other: insertions, deletions,
/// substitutions and swaps of neighbours.
fn distance(a: &str, b: &str) -> usize {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let mut before: Vec<usize> = (0..=b.len()).collect();
    let mut last: Vec<usize> = before.clone();
    for i in 1..=a.len() {
        let mut row = vec![i; b.len() + 1];
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            row[j] = (last[j] + 1).min(row[j - 1] + 1).min(last[j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                row[j] = row[j].min(before[j - 2] + 1);
            }
        }
        before = std::mem::replace(&mut last, row);
    }
    last[b.len()]
}

/// The names of this kind the schedule holds.
fn candidates(schedule: &Schedule, name: &Name) -> Vec<String> {
    match name {
        Name::Activity(_) => schedule
            .activities
            .values()
            .map(|a| a.code.clone())
            .collect(),
        Name::Wbs(_) => schedule.wbs.values().map(|w| w.code.clone()).collect(),
        Name::Calendar(_) => schedule
            .calendars
            .values()
            .map(|c| c.name.clone())
            .collect(),
        Name::Resource(_) => schedule
            .resources
            .values()
            .map(|r| r.short_name.clone())
            .collect(),
        Name::Role(_) => schedule
            .roles
            .values()
            .map(|r| r.short_name.clone())
            .collect(),
        Name::Curve(_) => schedule
            .resource_curves
            .values()
            .map(|c| c.name.clone())
            .collect(),
        Name::CodeType(_) => schedule
            .activity_code_types
            .values()
            .map(|t| t.name.clone())
            .collect(),
        Name::ProjectCodeType(_) => schedule
            .project_code_types
            .values()
            .map(|t| t.name.clone())
            .collect(),
        Name::Obs(_) => schedule.obs.values().map(|n| n.name.clone()).collect(),
        Name::Udf(_) => schedule
            .udf_types
            .values()
            .map(|t| t.label.clone())
            .collect(),
        Name::CodeValue(..) | Name::ProjectCodeValue(..) | Name::Link(..) => Vec::new(),
    }
}

/// The closest names to a missing one, one or two keystrokes away, closest
/// first and then in order. Case is ignored except for Activity IDs.
#[must_use]
pub fn suggest(schedules: &[Schedule], name: &Name) -> Vec<String> {
    let wanted = match name {
        Name::Activity(code) => code.clone(),
        Name::Wbs(text)
        | Name::Calendar(text)
        | Name::Resource(text)
        | Name::Role(text)
        | Name::Curve(text)
        | Name::CodeType(text)
        | Name::ProjectCodeType(text)
        | Name::Obs(text)
        | Name::Udf(text) => text.to_lowercase(),
        Name::CodeValue(..) | Name::ProjectCodeValue(..) | Name::Link(..) => return Vec::new(),
    };
    let fold = |text: &str| match name {
        Name::Activity(_) => text.to_owned(),
        _ => text.to_lowercase(),
    };
    let mut near: Vec<(usize, String)> = schedules
        .iter()
        .flat_map(|s| candidates(s, name))
        .filter_map(|held| {
            let gap = distance(&fold(&held), &wanted);
            (1..=2).contains(&gap).then_some((gap, held))
        })
        .collect();
    near.sort();
    near.dedup();
    near.into_iter().take(3).map(|(_, held)| held).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distance_counts_keystrokes() {
        assert_eq!(distance("A1010", "A1010"), 0);
        assert_eq!(distance("A1O10", "A1010"), 1);
        assert_eq!(distance("A1001", "A1010"), 1);
        assert_eq!(distance("A10", "A1010"), 2);
        assert_eq!(distance("", "ab"), 2);
    }

    #[test]
    fn a_link_to_a_new_activity_depends_on_adding_it() {
        let add = Edit::AddActivity {
            code: "B200".into(),
            name: "New".into(),
            wbs: "W".into(),
            activity_type: None,
            calendar: None,
            duration: None,
        };
        let link = Edit::AddLink {
            predecessor: "A100".into(),
            successor: "P2:B200".into(),
            link_type: None,
            lag: None,
        };
        assert!(link.depends_on(&add));
        assert!(!add.depends_on(&link));
        let wbs = Edit::AddWbs {
            code: "Civil".into(),
            name: "Civil".into(),
            parent: "ROOT".into(),
        };
        let moved = Edit::SetWbs {
            activity: "A1".into(),
            wbs: "CIVIL".into(),
        };
        assert!(moved.depends_on(&wbs));
    }
}
