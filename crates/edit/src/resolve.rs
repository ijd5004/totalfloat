//! Stable references for stored edits (ARCHITECTURE.md, History and changesets). An edit names things the
//! way a scheduler does, `A100`; a recorded edit also keeps the source IDs
//! (`task_id`, `wbs_id`, `clndr_id`, `rsrc_id`) of what it named, so replaying
//! it on another branch, where `A100` may have been renamed, still finds it.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use totalfloat_model::{ActivityId, CalendarId, ResourceId, Schedule, WbsId};

use crate::{Edit, calendars_named, resource_named};

/// What a resolved name is.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum NameKind {
    /// An Activity ID; the source ID is `TASK.task_id`.
    Activity,
    /// A WBS code; the source ID is `PROJWBS.wbs_id`.
    Wbs,
    /// A calendar name; the source ID is `CALENDAR.clndr_id`.
    Calendar,
    /// A resource short name or name; the source ID is `RSRC.rsrc_id`.
    Resource,
    /// A role short name or name; the source ID is `ROLES.role_id`.
    Role,
}

/// A name an edit used, and what it meant when the edit was recorded.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Resolved {
    /// What the name is.
    pub kind: NameKind,
    /// The name as the edits give it, without any `PROJECT:` prefix.
    pub name: String,
    /// The project it was found in (`proj_id`).
    pub project: u64,
    /// Its source ID in that project's file.
    pub id: u64,
}

/// A resolved name the schedule no longer holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Missing {
    /// The name as recorded.
    pub name: Resolved,
}

/// A name split into the project short name it gives, if any, and the part
/// a project holds: `P2:A100` is `(Some("P2"), "A100")`. Only a prefix that
/// is a project's short name counts, so an Activity ID holding a colon stays
/// whole.
fn split<'a>(schedules: &[Schedule], name: &'a str) -> (Option<&'a str>, &'a str) {
    for (at, _) in name.match_indices(':') {
        let project = &name[..at];
        if schedules.iter().any(|s| s.project.short_name == project) {
            return (Some(project), &name[at + 1..]);
        }
    }
    (None, name)
}

impl Edit {
    /// Every name this edit uses for an existing thing, by kind, to read or
    /// rewrite. Names it creates, such as a new activity's code, aren't here.
    #[allow(
        clippy::too_many_lines,
        reason = "one short arm per kind of edit, as in `apply_one`"
    )]
    fn names_mut(&mut self) -> Vec<(NameKind, &mut String)> {
        let mut out: Vec<(NameKind, &mut String)> = Vec::new();
        match self {
            Self::SetDuration { activity, .. }
            | Self::SetConstraint { activity, .. }
            | Self::ClearConstraint { activity, .. }
            | Self::SetName { activity, .. }
            | Self::SetType { activity, .. }
            | Self::SetExpectedFinish { activity, .. }
            | Self::SetProgress { activity, .. }
            | Self::ClearProgress { activity }
            | Self::SetSuspend { activity, .. }
            | Self::ClearSuspend { activity }
            | Self::SetCode { activity, .. }
            | Self::ClearCode { activity, .. }
            | Self::AddStep { activity, .. }
            | Self::SetStep { activity, .. }
            | Self::CompleteStep { activity, .. }
            | Self::DeleteStep { activity, .. }
            | Self::AddExpense { activity, .. }
            | Self::SetExpense { activity, .. }
            | Self::DeleteExpense { activity, .. }
            | Self::DeleteActivity { activity } => out.push((NameKind::Activity, activity)),
            Self::AddNote { activity, wbs, .. }
            | Self::SetNote { activity, wbs, .. }
            | Self::DeleteNote { activity, wbs, .. } => {
                out.extend(activity.as_mut().map(|a| (NameKind::Activity, a)));
                out.extend(wbs.as_mut().map(|w| (NameKind::Wbs, w)));
            }
            Self::SetUdf {
                activity,
                wbs,
                resource,
                ..
            }
            | Self::ClearUdf {
                activity,
                wbs,
                resource,
                ..
            } => {
                out.extend(activity.as_mut().map(|a| (NameKind::Activity, a)));
                out.extend(wbs.as_mut().map(|w| (NameKind::Wbs, w)));
                out.extend(resource.as_mut().map(|r| (NameKind::Resource, r)));
            }
            Self::SetCalendar { activity, calendar } => {
                out.push((NameKind::Activity, activity));
                out.push((NameKind::Calendar, calendar));
            }
            Self::SetWbs { activity, wbs } => {
                out.push((NameKind::Activity, activity));
                out.push((NameKind::Wbs, wbs));
            }
            Self::AssignResource {
                activity, resource, ..
            }
            | Self::UnassignResource { activity, resource }
            | Self::SetAssignmentUnits {
                activity, resource, ..
            }
            | Self::SetCurve {
                activity, resource, ..
            }
            | Self::SetSpread {
                activity, resource, ..
            } => {
                out.push((NameKind::Activity, activity));
                out.push((NameKind::Resource, resource));
            }
            Self::AssignRole { activity, role, .. } | Self::UnassignRole { activity, role } => {
                out.push((NameKind::Activity, activity));
                out.push((NameKind::Role, role));
            }
            Self::SetCostAccount {
                activity,
                resource,
                role,
                ..
            } => {
                out.push((NameKind::Activity, activity));
                out.extend(resource.as_mut().map(|r| (NameKind::Resource, r)));
                out.extend(role.as_mut().map(|r| (NameKind::Role, r)));
            }
            Self::AddRole { parent, .. } => {
                out.extend(parent.as_mut().map(|p| (NameKind::Role, p)));
            }
            Self::AddWbs { parent, .. } => out.push((NameKind::Wbs, parent)),
            Self::RenameWbs { wbs, .. } | Self::SetWbsObs { wbs, .. } => {
                out.push((NameKind::Wbs, wbs));
            }
            Self::MoveWbs { wbs, parent } => {
                out.push((NameKind::Wbs, wbs));
                out.push((NameKind::Wbs, parent));
            }
            Self::AddCalendar { copy_of, .. } => {
                out.extend(copy_of.as_mut().map(|c| (NameKind::Calendar, c)));
            }
            Self::AddCalendarException { calendar, .. }
            | Self::RemoveCalendarException { calendar, .. }
            | Self::SetWorkWeek { calendar, .. }
            | Self::SetHoursPerDay { calendar, .. }
            | Self::SetHoursPerPeriod { calendar, .. } => out.push((NameKind::Calendar, calendar)),
            Self::AddActivity { wbs, calendar, .. } => {
                out.push((NameKind::Wbs, wbs));
                out.extend(calendar.as_mut().map(|c| (NameKind::Calendar, c)));
            }
            Self::AddLink {
                predecessor,
                successor,
                ..
            }
            | Self::RemoveLink {
                predecessor,
                successor,
                ..
            }
            | Self::ChangeLink {
                predecessor,
                successor,
                ..
            } => {
                out.push((NameKind::Activity, predecessor));
                out.push((NameKind::Activity, successor));
            }
            Self::AddResource { calendar, .. } => {
                out.extend(calendar.as_mut().map(|c| (NameKind::Calendar, c)));
            }
            Self::SetResourceLimit { resource, .. } => out.push((NameKind::Resource, resource)),
            Self::SetRoleLimit { role, .. } => out.push((NameKind::Role, role)),
            Self::AddCodeType { .. }
            | Self::AddCodeValue { .. }
            | Self::AddObs { .. }
            | Self::AddUdfType { .. }
            | Self::SetProjectCode { .. }
            | Self::ClearProjectCode { .. }
            | Self::AddProjectCodeType { .. }
            | Self::AddProjectCodeValue { .. } => {}
        }
        out
    }
}

/// The source ID a name has in a project, if it names exactly one thing
/// there by its usual name: an Activity ID, WBS code, calendar name or
/// resource short name. A name given as a number, or a resource by its full
/// name, is already exact or isn't the usual name, so it's left as given.
fn id_in(schedule: &Schedule, kind: NameKind, name: &str) -> Option<u64> {
    let id = match kind {
        NameKind::Activity => schedule.activity_by_code(name).map(|a| a.id.0),
        NameKind::Wbs => crate::find_wbs(schedule, name).ok().map(|w| w.0),
        NameKind::Calendar => match calendars_named(schedule, name).as_slice() {
            [one] => Some(one.0),
            _ => None,
        },
        NameKind::Resource => resource_named(schedule, name).map(|r| r.0),
        NameKind::Role => crate::role_named(schedule, name).map(|r| r.0),
    }?;
    (name_of(schedule, kind, id)?.to_lowercase() == name.to_lowercase()).then_some(id)
}

/// The name a project now gives a source ID, if it still holds it.
fn name_of(schedule: &Schedule, kind: NameKind, id: u64) -> Option<String> {
    match kind {
        NameKind::Activity => schedule
            .activities
            .get(&ActivityId(id))
            .map(|a| a.code.clone()),
        NameKind::Wbs => schedule.wbs.get(&WbsId(id)).map(|w| w.code.clone()),
        NameKind::Calendar => schedule
            .calendars
            .get(&CalendarId(id))
            .map(|c| c.name.clone()),
        NameKind::Resource => schedule
            .resources
            .get(&ResourceId(id))
            .map(|r| r.short_name.clone()),
        NameKind::Role => schedule
            .roles
            .get(&totalfloat_model::RoleId(id))
            .map(|r| r.short_name.clone()),
    }
}

/// The names an edit makes or changes the meaning of, after which a later
/// edit in the same batch using that name means something else.
fn touches(edit: &Edit) -> Vec<(NameKind, String)> {
    match edit {
        Edit::AddActivity { code, .. } | Edit::DeleteActivity { activity: code } => {
            vec![(NameKind::Activity, code.clone())]
        }
        Edit::AddWbs { code, .. } => vec![(NameKind::Wbs, code.clone())],
        Edit::RenameWbs { wbs, code, .. } => {
            let mut out = vec![(NameKind::Wbs, wbs.clone())];
            out.extend(code.iter().map(|c| (NameKind::Wbs, c.clone())));
            out
        }
        Edit::AddCalendar { name, .. } => vec![(NameKind::Calendar, name.clone())],
        _ => Vec::new(),
    }
}

/// Lower-cased where lookups ignore case, so names compare as lookups do.
fn key(kind: NameKind, name: &str) -> (NameKind, String) {
    match kind {
        NameKind::Activity => (kind, name.to_owned()),
        _ => (kind, name.to_lowercase()),
    }
}

/// Each name field of each edit that still means what the file says, in
/// order, as (edit index, kind, prefix, name). A field stops counting once an
/// earlier edit in the batch makes or renames that name.
fn fields(
    schedules: &[Schedule],
    edits: &[Edit],
) -> Vec<(usize, NameKind, Option<String>, String)> {
    let mut touched = std::collections::BTreeSet::new();
    let mut out = Vec::new();
    for (at, edit) in edits.iter().enumerate() {
        let mut copy = edit.clone();
        for (kind, name) in copy.names_mut() {
            let (prefix, code) = split(schedules, name);
            if !touched.contains(&key(kind, code)) {
                out.push((at, kind, prefix.map(str::to_owned), code.to_owned()));
            }
        }
        for (kind, name) in touches(edit) {
            touched.insert(key(kind, &name));
        }
    }
    out
}

/// The source IDs of every name the edits use that the schedules held before
/// them: in the project a `PROJECT:` prefix names, or in every project holding
/// it. Names made or renamed earlier in the batch have none.
#[must_use]
pub fn resolve(schedules: &[Schedule], edits: &[Edit]) -> Vec<Resolved> {
    let mut found = std::collections::BTreeSet::new();
    // Activity IDs are looked up once per project, not once per edit.
    let codes: Vec<BTreeMap<&str, u64>> = schedules
        .iter()
        .map(|s| {
            s.activities
                .values()
                .map(|a| (a.code.as_str(), a.id.0))
                .collect()
        })
        .collect();
    for (_, kind, prefix, name) in fields(schedules, edits) {
        let reach = schedules
            .iter()
            .zip(&codes)
            .filter(|(s, _)| prefix.as_deref().is_none_or(|p| s.project.short_name == p));
        for (schedule, codes) in reach {
            let id = match kind {
                NameKind::Activity => codes.get(name.as_str()).copied(),
                _ => id_in(schedule, kind, &name),
            };
            if let Some(id) = id {
                found.insert(Resolved {
                    kind,
                    name: name.clone(),
                    project: schedule.project.id.0,
                    id,
                });
            }
        }
    }
    found.into_iter().collect()
}

/// What a recorded name is called in `schedules` now, project by project:
/// `Some(name)` if every project it was found in still holds its ID under one
/// name, `None` to leave it as written. A name whose IDs are all gone, and
/// which the schedules don't hold by name either, is missing.
fn now_called(
    recorded: &[&Resolved],
    schedules: &[Schedule],
) -> Result<Option<String>, Vec<Missing>> {
    let mut called = std::collections::BTreeSet::new();
    let mut gone = Vec::new();
    for resolved in recorded {
        let project = schedules
            .iter()
            .find(|s| s.project.id.0 == resolved.project);
        match project.and_then(|s| name_of(s, resolved.kind, resolved.id)) {
            Some(now) => {
                called.insert(now);
            }
            None => gone.push(Missing {
                name: (*resolved).clone(),
            }),
        }
    }
    if called.is_empty() {
        let (kind, name) = (recorded[0].kind, &recorded[0].name);
        // Renumbered, perhaps by an export from another database: the name
        // still finds it.
        if schedules.iter().any(|s| id_in(s, kind, name).is_some()) {
            return Ok(None);
        }
        return Err(gone);
    }
    let mut called = called.into_iter();
    match (called.next(), called.next()) {
        (Some(only), None) if only != recorded[0].name => Ok(Some(only)),
        _ => Ok(None),
    }
}

/// The edits with each resolved name changed to what its source ID is called
/// in `schedules` now, so an edit recorded before a rename still finds what
/// it meant. A name whose projects disagree about it now is left as written,
/// and so is a name made or renamed earlier in the batch.
///
/// # Errors
///
/// Every resolved name that's gone, by its ID and by its name.
pub fn rename(
    edits: &[Edit],
    resolved: &[Resolved],
    schedules: &[Schedule],
) -> Result<Vec<Edit>, Vec<Missing>> {
    let mut by_name: BTreeMap<(NameKind, String), Vec<&Resolved>> = BTreeMap::new();
    for named in resolved {
        by_name
            .entry(key(named.kind, &named.name))
            .or_default()
            .push(named);
    }
    let mut map = BTreeMap::new();
    let mut missing = Vec::new();
    for (name, named) in &by_name {
        match now_called(named, schedules) {
            Ok(Some(now)) => {
                map.insert(name.clone(), now);
            }
            Ok(None) => {}
            Err(gone) => missing.extend(gone),
        }
    }
    if !missing.is_empty() {
        return Err(missing);
    }
    let wanted: std::collections::BTreeSet<(usize, NameKind, String)> = fields(schedules, edits)
        .into_iter()
        .map(|(at, kind, _, name)| (at, kind, name))
        .collect();
    // Each field is looked up once, so renames that swap two names don't
    // undo each other.
    let mut out = edits.to_vec();
    for (at, edit) in out.iter_mut().enumerate() {
        for (kind, name) in edit.names_mut() {
            let (prefix, code) = split(schedules, name);
            let (prefix, code) = (prefix.map(str::to_owned), code.to_owned());
            if !wanted.contains(&(at, kind, code.clone())) {
                continue;
            }
            if let Some(now) = map.get(&key(kind, &code)) {
                *name = prefix.map_or_else(|| now.clone(), |p| format!("{p}:{now}"));
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use totalfloat_model::{Activity, ActivityId, CalendarId, Project, ProjectId, Schedule, WbsId};

    use super::*;

    fn schedule(codes: &[(u64, &str)]) -> Schedule {
        let mut schedule = Schedule::new(Project::new(ProjectId(7), "P"));
        for (id, code) in codes {
            let activity = Activity::new(
                ActivityId(*id),
                ProjectId(7),
                WbsId(1),
                CalendarId(1),
                *code,
                "work",
            );
            schedule.activities.insert(activity.id, activity);
        }
        schedule
    }

    fn duration(activity: &str) -> Edit {
        Edit::SetDuration {
            activity: activity.to_owned(),
            original: Some("2d".parse().unwrap()),
            remaining: None,
        }
    }

    #[test]
    fn a_renamed_activity_is_found_by_its_id() {
        let before = [schedule(&[(1, "A100"), (2, "A200")])];
        let edits = [
            duration("A100"),
            Edit::AddLink {
                predecessor: "P:A100".into(),
                successor: "A200".into(),
                link_type: None,
                lag: None,
            },
        ];
        let resolved = resolve(&before, &edits);
        assert_eq!(resolved.len(), 2);
        let after = [schedule(&[(1, "A100-R"), (2, "A200")])];
        let renamed = rename(&edits, &resolved, &after).unwrap();
        assert_eq!(renamed[0], duration("A100-R"));
        let Edit::AddLink { predecessor, .. } = &renamed[1] else {
            panic!();
        };
        assert_eq!(predecessor, "P:A100-R");
    }

    #[test]
    fn swapped_names_follow_their_ids() {
        let before = [schedule(&[(1, "A"), (2, "B")])];
        let edits = [duration("A"), duration("B")];
        let resolved = resolve(&before, &edits);
        let after = [schedule(&[(1, "B"), (2, "A")])];
        let renamed = rename(&edits, &resolved, &after).unwrap();
        assert_eq!(renamed, [duration("B"), duration("A")]);
    }

    fn named(project: u64, short: &str, codes: &[(u64, &str)]) -> Schedule {
        let mut s = schedule(codes);
        s.project = Project::new(ProjectId(project), short);
        for a in s.activities.values_mut() {
            a.project = ProjectId(project);
        }
        s
    }

    #[test]
    fn a_rename_in_one_project_leaves_another_project_s_name_alone() {
        let before = [
            named(1, "P1", &[(1, "A100")]),
            named(2, "P2", &[(9, "A100")]),
        ];
        let edits = [duration("P1:A100")];
        let resolved = resolve(&before, &edits);
        assert_eq!(resolved.len(), 1, "only the project the prefix names");
        let after = [
            named(1, "P1", &[(1, "A100")]),
            named(2, "P2", &[(9, "A100-X")]),
        ];
        assert_eq!(rename(&edits, &resolved, &after).unwrap(), edits);
        // An unprefixed name both projects held, renamed in only one, is left
        // as written: the projects disagree about it.
        let bare = [duration("A100")];
        let resolved = resolve(&before, &bare);
        assert_eq!(resolved.len(), 2);
        assert_eq!(rename(&bare, &resolved, &after).unwrap(), bare);
    }

    #[test]
    fn a_renumbered_activity_is_still_found_by_name() {
        let before = [schedule(&[(1, "A1")])];
        let edits = [duration("A1")];
        let resolved = resolve(&before, &edits);
        let after = [schedule(&[(55, "A1")])];
        assert_eq!(rename(&edits, &resolved, &after).unwrap(), edits);
    }

    #[test]
    fn a_colon_in_an_activity_id_isn_t_a_project() {
        let before = [schedule(&[(1, "A:10"), (2, "10")])];
        let edits = [duration("A:10")];
        let resolved = resolve(&before, &edits);
        assert_eq!(resolved[0].id, 1, "{resolved:?}");
    }

    #[test]
    fn a_name_renamed_earlier_in_the_batch_isn_t_resolved() {
        let before = [schedule(&[(1, "A1")])];
        let edits = [
            Edit::DeleteActivity {
                activity: "A1".into(),
            },
            duration("A1"),
        ];
        let resolved = resolve(&before, &edits);
        assert_eq!(resolved.len(), 1);
        let after = [schedule(&[(1, "Z1")])];
        let renamed = rename(&edits, &resolved, &after).unwrap();
        assert_eq!(
            renamed[0],
            Edit::DeleteActivity {
                activity: "Z1".into()
            }
        );
        assert_eq!(renamed[1], duration("A1"), "it meant the one made after");
    }

    #[test]
    fn every_name_an_edit_needs_can_be_resolved() {
        // `needs` and `names_mut` list names separately; they must agree on
        // activities, WBS nodes, calendars, resources and roles for every edit.
        let text = include_str!("../tests/golden/command-1.json");
        let commands: crate::Commands = serde_json::from_str(text).unwrap();
        for edit in commands.edits {
            let mut wanted: Vec<(NameKind, String)> = edit
                .needs()
                .into_iter()
                .filter_map(|n| match n {
                    crate::Name::Activity(c) => Some((NameKind::Activity, c)),
                    crate::Name::Wbs(c) => Some((NameKind::Wbs, c)),
                    crate::Name::Calendar(c) => Some((NameKind::Calendar, c)),
                    crate::Name::Resource(c) => Some((NameKind::Resource, c)),
                    crate::Name::Role(c) => Some((NameKind::Role, c)),
                    _ => None,
                })
                .collect();
            let mut copy = edit.clone();
            let mut listed: Vec<(NameKind, String)> = copy
                .names_mut()
                .into_iter()
                .map(|(k, n)| (k, n.rsplit_once(':').map_or(n.as_str(), |p| p.1).to_owned()))
                .collect();
            wanted.sort();
            wanted.dedup();
            listed.sort();
            listed.dedup();
            assert_eq!(listed, wanted, "{edit:?}");
        }
    }

    #[test]
    fn a_deleted_activity_is_missing_and_new_names_need_nothing() {
        let before = [schedule(&[(1, "A100")])];
        let edits = [duration("A100"), duration("NEW")];
        let resolved = resolve(&before, &edits);
        assert_eq!(resolved.len(), 1, "a name made in the batch has no ID");
        let missing = rename(&edits, &resolved, &[schedule(&[])]).unwrap_err();
        assert_eq!(missing[0].name.name, "A100");
        // Without resolved names, edits replay as written.
        assert_eq!(rename(&edits, &[], &[schedule(&[])]).unwrap(), edits);
    }
}
