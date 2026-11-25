//! `totalfloat schedule`: calculate and write results back into
//! the fields P6 stores them in, changing nothing else (ARCHITECTURE.md, Lossless round trips).

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::Serialize;
use totalfloat_engine::{
    ActivityResult, Calculated, DateField, DateProvenance, ScheduleResult, schedule_open_together,
    schedule_reusing,
};
use totalfloat_model::{
    Activity, ActivityId, ActivityStatus, ActivityType, DateTime, FloatPathMode, Minutes,
    ProjectId, RelationshipId, Schedule, StoredResults,
};
use totalfloat_xer::{RawXer, mapping::Diagnostics, patch_all};

use crate::error::{CliError, exit, parse_date};
use crate::loaded::Loaded;
use crate::output::{
    create_new_text, permissions_of, print_line, refuse_existing, rounded, write_new,
};

/// What one run changes for itself. The file's own options stay as they are.
#[derive(Debug, Clone, Default)]
pub struct Overrides {
    /// Ignore relationships to and from other projects (TF-030).
    pub ignore_external: bool,
    /// Float paths asked for (docs/rules/TF-002-multiple-float-paths.md).
    pub float_paths: Option<FloatPathAsk>,
}

/// Float paths asked for one run: `--float-paths`, `--float-path-to` and
/// `--float-paths-by`.
#[derive(Debug, Clone)]
pub struct FloatPathAsk {
    /// At most this many paths, 1 or more.
    pub paths: u32,
    /// The Activity ID the paths end at; default is the file's own end.
    pub to: Option<String>,
    /// Number them by total or free float; default is the file's own.
    pub by: Option<FloatPathMode>,
}

/// `totalfloat.schedule/1`.
#[derive(Debug, Serialize)]
pub struct Report {
    schema: &'static str,
    file: String,
    output: Option<String>,
    /// Relationships to and from other projects were ignored (`--ignore-external`).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    ignore_external: bool,
    /// `PROJECT` rows that couldn't be mapped, which nothing here calculated.
    skipped_projects: usize,
    projects: Vec<ProjectReport>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct ProjectReport {
    id: u64,
    code: String,
    data_date: Option<String>,
    scheduled_finish: Option<String>,
    previous_finish: Option<String>,
    /// The latest early finish saved on unfinished work. It can disagree with
    /// `previous_finish`, which is one project-level field.
    activity_finish: Option<String>,
    activities: usize,
    critical: usize,
    /// Activities with at least one stored result that changed.
    changed_activities: usize,
    /// Unfinished activities whose early or late dates or total float changed.
    changed_dates_or_float: usize,
    changes: Changes,
}

/// What changed. Counts are complete and lists keep [`MAX_LISTED`] items.
#[derive(Debug, Serialize, schemars::JsonSchema)]
struct Changes {
    /// Activities changed on each stored result.
    by_field: BTreeMap<&'static str, usize>,
    /// Where differences begin: changed early dates with no changed
    /// predecessor, or changed late dates or float with no changed successor.
    starts_at: Vec<String>,
    starts_at_total: usize,
    /// Unfinished activities changed on dates or float, with those fields,
    /// `starts_at` first.
    activities: Vec<ActivityChange>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct ActivityChange {
    code: String,
    name: String,
    fields: Vec<FieldChange>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct FieldChange {
    field: &'static str,
    from: Option<String>,
    to: Option<String>,
}

const MAX_LISTED: usize = 25;

/// Where differences begin, as the summary shows them.
const MAX_SHOWN: usize = 10;

const EARLY_FIELDS: [&str; 2] = ["early_start", "early_finish"];
const LATE_FIELDS: [&str; 3] = ["late_start", "late_finish", "total_float"];

fn text(date: Option<DateTime>) -> Option<String> {
    date.map(|date| date.to_string())
}

/// Takes what the engine calculated and keeps the rest: P6 stores fields the
/// engine doesn't set, such as total float on finished work. `replace` takes
/// everything, because moving the data date makes kept values stale. With float
/// paths on, an activity off every path has no path.
fn merge(
    stored: &StoredResults,
    new: &StoredResults,
    replace: bool,
    paths_on: bool,
) -> StoredResults {
    if replace {
        return new.clone();
    }
    let StoredResults {
        early_start,
        early_finish,
        late_start,
        late_finish,
        remaining_early_start,
        remaining_early_finish,
        remaining_late_start,
        remaining_late_finish,
        total_float,
        free_float,
        on_longest_path,
        float_path,
        float_path_order,
    } = new.clone();
    StoredResults {
        early_start: early_start.or(stored.early_start),
        early_finish: early_finish.or(stored.early_finish),
        late_start: late_start.or(stored.late_start),
        late_finish: late_finish.or(stored.late_finish),
        remaining_early_start: remaining_early_start.or(stored.remaining_early_start),
        remaining_early_finish: remaining_early_finish.or(stored.remaining_early_finish),
        remaining_late_start: remaining_late_start.or(stored.remaining_late_start),
        remaining_late_finish: remaining_late_finish.or(stored.remaining_late_finish),
        total_float: total_float.or(stored.total_float),
        free_float: free_float.or(stored.free_float),
        on_longest_path: on_longest_path.or(stored.on_longest_path),
        float_path: if paths_on {
            float_path
        } else {
            float_path.or(stored.float_path)
        },
        float_path_order: if paths_on {
            float_path_order
        } else {
            float_path_order.or(stored.float_path_order)
        },
    }
}

type KeepStored = fn(&mut StoredResults, &StoredResults);

/// The columns results go in. An older export may lack some, and P6 wrote
/// nothing there, so the value isn't kept for that file.
const RESULT_COLUMNS: [(&str, KeepStored); 13] = [
    ("early_start_date", |n, o| n.early_start = o.early_start),
    ("early_end_date", |n, o| n.early_finish = o.early_finish),
    ("late_start_date", |n, o| n.late_start = o.late_start),
    ("late_end_date", |n, o| n.late_finish = o.late_finish),
    ("restart_date", |n, o| {
        n.remaining_early_start = o.remaining_early_start;
    }),
    ("reend_date", |n, o| {
        n.remaining_early_finish = o.remaining_early_finish;
    }),
    ("rem_late_start_date", |n, o| {
        n.remaining_late_start = o.remaining_late_start;
    }),
    ("rem_late_end_date", |n, o| {
        n.remaining_late_finish = o.remaining_late_finish;
    }),
    ("total_float_hr_cnt", |n, o| n.total_float = o.total_float),
    ("free_float_hr_cnt", |n, o| n.free_float = o.free_float),
    ("driving_path_flag", |n, o| {
        n.on_longest_path = o.on_longest_path;
    }),
    ("float_path", |n, o| n.float_path = o.float_path),
    ("float_path_order", |n, o| {
        n.float_path_order = o.float_path_order;
    }),
];

/// Puts a project's results in place. Work not started that moves takes planned
/// dates from its remaining early dates, as files in the corpus hold them.
/// Writes results into `schedule` and says whether anything changed.
fn apply(schedule: &mut Schedule, result: &ScheduleResult, replace: bool, absent: &[&str]) -> bool {
    let paths_on = schedule.project.options.float_paths.enabled;
    let mut changed = false;
    for (id, activity) in &mut schedule.activities {
        let Some(new) = result.activities.get(id) else {
            continue;
        };
        let mut merged = merge(&activity.stored, &new.values, replace, paths_on);
        for (column, keep) in RESULT_COLUMNS {
            if absent.contains(&column) {
                keep(&mut merged, &activity.stored);
            }
        }
        let moved = merged != activity.stored;
        changed |= moved;
        activity.stored = merged;
        if moved && activity.status == ActivityStatus::NotStarted {
            activity.planned_start = activity.stored.remaining_early_start;
            activity.planned_finish = activity.stored.remaining_early_finish;
        }
    }
    if result.project.scheduled_finish.is_some() {
        changed |= schedule.project.stored.scheduled_finish != result.project.scheduled_finish;
        schedule.project.stored.scheduled_finish = result.project.scheduled_finish;
    }
    changed
}

/// A result an activity stores: its name, whether two activities hold
/// different values for it, and its value as text.
type ResultField = (
    &'static str,
    fn(&Activity, &Activity) -> bool,
    fn(&Activity) -> Option<String>,
);

#[allow(
    clippy::cast_precision_loss,
    reason = "display hours; minutes are far below 2^52"
)]
fn span_text(span: Option<Minutes>) -> Option<String> {
    span.map(|span| format!("{}h", rounded(span.get() as f64 / 60.0)))
}

/// Every result an activity stores, so any two can be compared and shown.
/// Equal values always read the same, so only fields that differ are written
/// out to compare.
const RESULT_FIELDS: [ResultField; 15] = [
    (
        "early_start",
        |b, a| b.stored.early_start != a.stored.early_start,
        |a| text(a.stored.early_start),
    ),
    (
        "early_finish",
        |b, a| b.stored.early_finish != a.stored.early_finish,
        |a| text(a.stored.early_finish),
    ),
    (
        "late_start",
        |b, a| b.stored.late_start != a.stored.late_start,
        |a| text(a.stored.late_start),
    ),
    (
        "late_finish",
        |b, a| b.stored.late_finish != a.stored.late_finish,
        |a| text(a.stored.late_finish),
    ),
    (
        "total_float",
        |b, a| b.stored.total_float != a.stored.total_float,
        |a| span_text(a.stored.total_float),
    ),
    (
        "free_float",
        |b, a| b.stored.free_float != a.stored.free_float,
        |a| span_text(a.stored.free_float),
    ),
    (
        "remaining_early_start",
        |b, a| b.stored.remaining_early_start != a.stored.remaining_early_start,
        |a| text(a.stored.remaining_early_start),
    ),
    (
        "remaining_early_finish",
        |b, a| b.stored.remaining_early_finish != a.stored.remaining_early_finish,
        |a| text(a.stored.remaining_early_finish),
    ),
    (
        "remaining_late_start",
        |b, a| b.stored.remaining_late_start != a.stored.remaining_late_start,
        |a| text(a.stored.remaining_late_start),
    ),
    (
        "remaining_late_finish",
        |b, a| b.stored.remaining_late_finish != a.stored.remaining_late_finish,
        |a| text(a.stored.remaining_late_finish),
    ),
    (
        "longest_path",
        |b, a| b.stored.on_longest_path != a.stored.on_longest_path,
        |a| a.stored.on_longest_path.map(|v| v.to_string()),
    ),
    (
        "float_path",
        |b, a| b.stored.float_path != a.stored.float_path,
        |a| a.stored.float_path.map(|v| v.to_string()),
    ),
    (
        "float_path_order",
        |b, a| b.stored.float_path_order != a.stored.float_path_order,
        |a| a.stored.float_path_order.map(|v| v.to_string()),
    ),
    (
        "planned_start",
        |b, a| b.planned_start != a.planned_start,
        |a| text(a.planned_start),
    ),
    (
        "planned_finish",
        |b, a| b.planned_finish != a.planned_finish,
        |a| text(a.planned_finish),
    ),
];

/// The stored results that differ between two activities, a bit for each in
/// [`RESULT_FIELDS`] order. Values that differ always read differently: dates
/// are written exactly, and float in hundredths of an hour, finer than its
/// whole minutes. So the bits say which fields change without writing any
/// out.
fn differing(before: &Activity, after: &Activity) -> u16 {
    RESULT_FIELDS
        .iter()
        .enumerate()
        .filter(|(_, (_, differs, _))| differs(before, after))
        .fold(0, |mask, (at, _)| mask | 1 << at)
}

/// The bits [`differing`] gives `fields`.
fn mask_of(fields: &[&str]) -> u16 {
    RESULT_FIELDS
        .iter()
        .enumerate()
        .filter(|(_, (field, _, _))| fields.contains(field))
        .fold(0, |mask, (at, _)| mask | 1 << at)
}

/// The fields in `mask`, written out.
fn field_changes(before: &Activity, after: &Activity, mask: u16) -> Vec<FieldChange> {
    RESULT_FIELDS
        .iter()
        .enumerate()
        .filter(|(at, _)| mask & 1 << at != 0)
        .map(|(_, (field, _, text))| (*field, text(before), text(after)))
        .filter(|(_, from, to)| from != to)
        .map(|(field, from, to)| FieldChange { field, from, to })
        .collect()
}

/// Changed activities that nothing changed led to: the first in logic to differ
/// on early dates, and the last to differ on late dates.
fn origins(
    schedule: &Schedule,
    early: &BTreeSet<ActivityId>,
    late: &BTreeSet<ActivityId>,
) -> (BTreeSet<ActivityId>, BTreeSet<ActivityId>) {
    let (mut first, mut last) = (early.clone(), late.clone());
    for link in schedule.relationships.values() {
        if early.contains(&link.predecessor) {
            first.remove(&link.successor);
        }
        if late.contains(&link.successor) {
            last.remove(&link.predecessor);
        }
    }
    (first, last)
}

fn changes(before: &Schedule, after: &Schedule) -> (Changes, usize) {
    // Each changed activity, as it was, and the fields that changed.
    let found: Vec<(&Activity, &Activity, u16)> = after
        .activities
        .iter()
        .filter_map(|(id, a)| {
            let was = before.activities.get(id).filter(|was| *was != a)?;
            let fields = differing(was, a);
            (fields != 0).then_some((a, was, fields))
        })
        .collect();
    let on = |fields: &[&str]| -> BTreeSet<ActivityId> {
        let mask = mask_of(fields);
        found
            .iter()
            .filter(|(_, _, changed)| changed & mask != 0)
            .map(|(a, _, _)| a.id)
            .collect()
    };
    // Finished work follows the data date (TF-020), so it isn't news. Spans are
    // measured from other activities, so they start nothing: they only follow.
    let open = |ids: &BTreeSet<ActivityId>, spans: bool| -> BTreeSet<ActivityId> {
        let keep = |id: &&ActivityId| {
            let a = &after.activities[id];
            a.status != ActivityStatus::Completed && (spans || !a.activity_type.is_span())
        };
        ids.iter().filter(keep).copied().collect()
    };
    let (early, late) = (on(&EARLY_FIELDS), on(&LATE_FIELDS));
    let (early_starts, late_starts) = origins(before, &open(&early, false), &open(&late, false));
    let starts: BTreeSet<ActivityId> = early_starts.union(&late_starts).copied().collect();
    let (early, late) = (open(&early, true), open(&late, true));
    let mut by_field = BTreeMap::new();
    for (_, _, changed) in &found {
        for (at, (field, _, _)) in RESULT_FIELDS.iter().enumerate() {
            if changed & 1 << at != 0 {
                *by_field.entry(*field).or_default() += 1;
            }
        }
    }
    let shown = mask_of(&EARLY_FIELDS) | mask_of(&LATE_FIELDS);
    let mut listed: Vec<_> = found
        .into_iter()
        .filter(|(a, _, _)| early.contains(&a.id) || late.contains(&a.id))
        .collect();
    let total = listed.len();
    listed.sort_by_key(|(a, _, _)| (!starts.contains(&a.id), a.stored.early_start, a.id));
    let starts_at: Vec<String> = listed
        .iter()
        .filter(|(a, _, _)| starts.contains(&a.id))
        .take(MAX_LISTED)
        .map(|(a, _, _)| a.code.clone())
        .collect();
    // Only the activities listed are written out.
    let activities = listed
        .into_iter()
        .take(MAX_LISTED)
        .map(|(a, was, changed)| {
            let mut fields = field_changes(was, a, changed & shown);
            // The field shown first is the one that makes this a starting place.
            if late_starts.contains(&a.id) && !early_starts.contains(&a.id) {
                fields.sort_by_key(|c| EARLY_FIELDS.contains(&c.field));
            }
            ActivityChange {
                code: a.code.clone(),
                name: a.name.clone(),
                fields,
            }
        })
        .collect();
    let changes = Changes {
        by_field,
        starts_at,
        starts_at_total: starts.len(),
        activities,
    };
    (changes, total)
}

fn describe(before: &Schedule, after: &Schedule, result: &ScheduleResult) -> ProjectReport {
    let any_result = after
        .activities
        .iter()
        .filter(|(id, a)| before.activities.get(id).is_none_or(|b| a != &b))
        .count();
    let (changes, dates_or_float) = changes(before, after);
    // As the engine takes the project finish: spans count only if nothing else does.
    let finish_of = |spans: bool| {
        before
            .activities
            .values()
            .filter(|a| a.status != ActivityStatus::Completed && a.activity_type.is_span() == spans)
            .filter_map(|a| a.stored.early_finish)
            .max()
    };
    let activity_finish = finish_of(false).or_else(|| finish_of(true));
    let critical = result
        .activities
        .values()
        .filter(|a| a.critical == Some(true))
        .count();
    ProjectReport {
        id: before.project.id.0,
        code: before.project.short_name.clone(),
        data_date: text(after.project.data_date),
        scheduled_finish: text(after.project.stored.scheduled_finish),
        previous_finish: text(before.project.stored.scheduled_finish),
        activity_finish: text(activity_finish),
        activities: before.activities.len(),
        critical,
        changed_activities: any_result,
        changed_dates_or_float: dates_or_float,
        changes,
    }
}

fn refused(project: u64, error: impl std::fmt::Display) -> CliError {
    CliError::new(
        "schedule-refused",
        format!("project {project}: {error}"),
        "nothing was changed; schedule a single other project in the file instead",
    )
}

/// Passes before results across projects count as unsettled.
const MAX_PASSES: usize = 64;

/// A loop through relationships among the open projects that passes through
/// more than one of them, as `PROJECT: ACTIVITY` labels with the first again at
/// the end. A loop within one project is the engine's to report.
fn loop_across(schedules: &[Schedule], open: &[ProjectId]) -> Option<Vec<String>> {
    let mut activities: BTreeMap<ActivityId, (&Schedule, &Activity)> = BTreeMap::new();
    for s in schedules.iter().filter(|s| open.contains(&s.project.id)) {
        // A span takes no part in the passes, so a link to one can't make a loop.
        for a in s.activities.values().filter(|a| !a.activity_type.is_span()) {
            activities.insert(a.id, (s, a));
        }
    }
    let mut successors: BTreeMap<ActivityId, BTreeSet<ActivityId>> = BTreeMap::new();
    for s in schedules.iter().filter(|s| open.contains(&s.project.id)) {
        for r in s.relationships.values() {
            if activities.contains_key(&r.predecessor) && activities.contains_key(&r.successor) {
                successors
                    .entry(r.predecessor)
                    .or_default()
                    .insert(r.successor);
            }
        }
    }
    // Depth first without recursion: 1 on the current path, 2 done.
    let mut state: BTreeMap<ActivityId, u8> = BTreeMap::new();
    let empty = BTreeSet::new();
    for &root in activities.keys() {
        if state.contains_key(&root) {
            continue;
        }
        state.insert(root, 1);
        let mut stack = vec![(root, successors.get(&root).unwrap_or(&empty).iter())];
        while let Some((node, next)) = stack.last_mut() {
            let node = *node;
            let Some(&succ) = next.next() else {
                state.insert(node, 2);
                stack.pop();
                continue;
            };
            match state.get(&succ) {
                Some(1) => {
                    let mut ids: Vec<ActivityId> = stack
                        .iter()
                        .map(|(n, _)| *n)
                        .skip_while(|n| *n != succ)
                        .collect();
                    ids.push(succ);
                    let projects: BTreeSet<_> =
                        ids.iter().map(|id| activities[id].0.project.id).collect();
                    if projects.len() < 2 {
                        // The engine names a loop within one project.
                        continue;
                    }
                    return Some(
                        ids.iter()
                            .map(|id| {
                                let (s, a) = activities[id];
                                format!("{}: {}", s.project.short_name, a.code)
                            })
                            .collect(),
                    );
                }
                Some(_) => {}
                None => {
                    state.insert(succ, 1);
                    stack.push((succ, successors.get(&succ).unwrap_or(&empty).iter()));
                }
            }
        }
    }
    None
}

/// Gives linked activities that have no stored dates a first set, from a run
/// of their project on its own logic, so the projects they link to can read
/// them. The passes that follow replace every one: in a network without loops
/// the results don't depend on where they start. A project that can't be
/// calculated on its own is left for the passes to report.
fn seed(schedules: &mut [Schedule], open: &[ProjectId], absent: &[&str]) {
    let linked: BTreeSet<ActivityId> = schedules
        .iter()
        .flat_map(|s| {
            s.relationships
                .values()
                .filter(|r| s.is_external(r))
                .flat_map(|r| [r.predecessor, r.successor])
        })
        .collect();
    for index in 0..schedules.len() {
        let s = &schedules[index];
        let undated = s.activities.values().any(|a| {
            linked.contains(&a.id)
                && (a.stored.early_finish.is_none() || a.stored.late_finish.is_none())
        });
        if !open.contains(&s.project.id) || !undated {
            continue;
        }
        let mut options = s.project.options.clone();
        options.ignore_external_relationships = true;
        if let Ok(result) = schedule_open_together(s, &options, schedules, None) {
            apply(&mut schedules[index], &result, false, absent);
        }
    }
}

impl ProjectReport {
    /// Critical activities after calculating.
    #[must_use]
    pub const fn critical(&self) -> usize {
        self.critical
    }

    /// The project's `proj_id`.
    #[must_use]
    pub const fn id(&self) -> u64 {
        self.id
    }

    /// `Project CODE (id)`.
    #[must_use]
    pub fn heading(&self) -> String {
        format!("Project {} ({})", self.code, self.id)
    }

    /// `finish: DATE (was DATE)`.
    #[must_use]
    pub fn finish_line(&self) -> String {
        format!(
            "finish: {} (was {})",
            self.scheduled_finish.as_deref().unwrap_or("none"),
            self.previous_finish.as_deref().unwrap_or("none")
        )
    }
}

struct Run {
    edited: Vec<Schedule>,
    /// Each project's calculation, by project ID.
    kept: Kept,
    reports: Vec<ProjectReport>,
    skipped: usize,
    /// Relationships that drive their successor, in the projects calculated.
    driving: BTreeSet<RelationshipId>,
    /// Activities the engine found critical.
    critical: BTreeSet<ActivityId>,
}

/// What the engine found across the projects calculated: the links that drive,
/// and the activities that are critical.
fn findings(
    results: &Kept,
    schedules: &[Schedule],
) -> (BTreeSet<RelationshipId>, BTreeSet<ActivityId>) {
    let mut driving = BTreeSet::new();
    let mut critical = BTreeSet::new();
    for schedule in schedules {
        let Some(result) = results.get(&schedule.project.id.0).map(Calculated::result) else {
            continue;
        };
        for (id, activity) in &result.activities {
            driving.extend(driving_into(schedule, activity));
            if activity.critical == Some(true) {
                critical.insert(*id);
            }
        }
    }
    (driving, critical)
}

/// The links that drive an activity's early dates: the engine's, which are its
/// own project's links with no slack, and a link from another project that set
/// one of them. The engine weighs slack only within its project, so a tie
/// between links from another project lists the one that won. A link within
/// the project is left to the engine: one that set a date before a move to
/// working time can still have slack (TF-026).
fn driving_into(schedule: &Schedule, activity: &ActivityResult) -> Vec<RelationshipId> {
    let forward = [
        DateField::EarlyStart,
        DateField::EarlyFinish,
        DateField::RemainingEarlyStart,
        DateField::RemainingEarlyFinish,
    ];
    let mut links = activity.driving_relationships.clone();
    for field in forward {
        let mut cause = activity.provenance.get(&field);
        // A date moved to working time keeps what set it underneath.
        while let Some(DateProvenance::CalendarAdjustment { cause: inner, .. }) = cause {
            cause = Some(inner);
        }
        if let Some(DateProvenance::Relationship { relationship, .. }) = cause
            && schedule
                .relationships
                .get(relationship)
                .is_some_and(|link| schedule.is_external(link))
        {
            links.push(*relationship);
        }
    }
    links
}

/// Turns float paths on in the selected projects for this run, as asked.
/// The end activity is looked up by Activity ID in each; a project that
/// doesn't hold it keeps its own end, and none holding it is refused.
fn ask_for_paths(
    schedules: &mut [Schedule],
    selected: &impl Fn(&Schedule) -> bool,
    ask: &FloatPathAsk,
) -> Result<(), CliError> {
    if ask.paths == 0 {
        return Err(CliError::new(
            "bad-float-paths",
            "float paths need at least 1 path",
            "ask for 1 or more paths, such as 3",
        ));
    }
    let mut ended = false;
    for schedule in schedules.iter_mut().filter(|s| selected(s)) {
        let end = match &ask.to {
            None => None,
            Some(code) => {
                // Only the project that holds the end activity is asked.
                let Some(activity) = schedule.activities.values().find(|a| a.code == *code) else {
                    continue;
                };
                if activity.status == ActivityStatus::Completed
                    || matches!(
                        activity.activity_type,
                        ActivityType::LevelOfEffort | ActivityType::WbsSummary
                    )
                {
                    return Err(CliError::new(
                        "bad-float-path-end",
                        format!(
                            "float paths can't end at {code}: it's finished, a level of effort or a WBS summary"
                        ),
                        "give an unfinished task or milestone, as `query` lists them",
                    ));
                }
                Some(activity.id)
            }
        };
        let options = &mut schedule.project.options.float_paths;
        options.enabled = true;
        options.max_paths = Some(ask.paths);
        if let Some(by) = ask.by {
            options.mode = by;
        }
        if let Some(end) = end {
            options.end_activity = Some(end);
            ended = true;
        }
    }
    match &ask.to {
        Some(code) if !ended => Err(CliError::new(
            "no-such-activity",
            format!("no project calculated here has activity {code}"),
            "give the Activity ID the paths end at, as `query` lists them",
        )),
        _ => Ok(()),
    }
}

/// Each project's calculation, by project ID, kept so the next calculation
/// of the same project can take from it what an edit doesn't reach.
pub type Kept = BTreeMap<u64, Calculated>;

/// A file as read, its schedules, and what mapping it found: a [`Loaded`]'s
/// parts, which a dry run's edited schedules can stand in for.
type Parts<'a> = (&'a RawXer, &'a [Schedule], &'a Diagnostics);

/// Selected projects are scheduled together: each reads the others' latest
/// results, until a pass changes nothing. Fails before anything is written if a
/// selected project can't be calculated. Relationships to and from other
/// projects are ignored when `ignore_external` is set (TF-030): each project is calculated on its own logic
/// only. Each calculation takes what it can from `previous`.
fn calculate_with(
    (raw, before, diagnostics): Parts<'_>,
    (project, data_date): (Option<u64>, Option<DateTime>),
    overrides: &Overrides,
    previous: Option<&Kept>,
) -> Result<Run, CliError> {
    let ignore_external = overrides.ignore_external;
    let mut schedules = before.to_vec();
    let selected = |s: &Schedule| project.is_none_or(|id| s.project.id.0 == id);
    if !schedules.iter().any(selected) {
        return Err(match project {
            Some(id) => CliError::new(
                "no-such-project",
                format!("the file has no project {id}"),
                "`inspect` lists the projects in the file",
            ),
            None => CliError::new(
                "no-projects",
                "the file has no project that can be calculated",
                "`inspect` reports what the file holds and what was left out",
            ),
        });
    }
    let table = |name: &str| raw.tables().iter().find(|t| t.name == name);
    for schedule in schedules.iter_mut().filter(|s| selected(s)) {
        crate::loaded::refuse_incomplete((raw, diagnostics), schedule)?;
        if data_date.is_some() {
            schedule.project.data_date = data_date;
        }
    }
    if let Some(ask) = &overrides.float_paths {
        ask_for_paths(&mut schedules, &selected, ask)?;
    }
    let absent: Vec<&str> = RESULT_COLUMNS
        .iter()
        .map(|(column, _)| *column)
        .filter(|column| table("TASK").is_none_or(|t| t.field_index(column).is_none()))
        .collect();
    if overrides.float_paths.is_some()
        && absent
            .iter()
            .any(|c| matches!(*c, "float_path" | "float_path_order"))
    {
        return Err(CliError::new(
            "no-float-path-columns",
            "the file's TASK table has no float_path and float_path_order columns, so float paths can't be written to it",
            "use an export that has those columns",
        ));
    }
    // Only another project's results can move a project's dates again.
    let results = settle(
        &mut schedules,
        &selected,
        (data_date.is_some(), ignore_external),
        &absent,
        previous,
    )?;
    let reports = before
        .iter()
        .zip(&schedules)
        .filter(|(b, _)| selected(b))
        .filter_map(|(b, a)| Some(describe(b, a, results.get(&b.project.id.0)?.result())))
        .collect();
    let (driving, critical) = findings(&results, &schedules);
    let edited = schedules.into_iter().filter(selected).collect();
    let skipped = diagnostics
        .omitted
        .iter()
        .filter(|row| raw.tables()[row.table].name == "PROJECT")
        .count();
    Ok(Run {
        edited,
        kept: results,
        reports,
        skipped,
        driving,
        critical,
    })
}

/// Calculates the selected projects, the ones open together, until a pass
/// changes nothing, and gives each one's last result. Results replace every
/// stored value when `replace` is set, as when the data date moves. Each
/// calculation takes what it can from the project's last one, or from
/// `previous`; the engine checks what changed, so the results are a
/// full calculation's whatever it is given.
fn settle(
    schedules: &mut [Schedule],
    selected: &impl Fn(&Schedule) -> bool,
    (replace, ignore_external): (bool, bool),
    absent: &[&str],
    previous: Option<&Kept>,
) -> Result<Kept, CliError> {
    let links_out: usize = schedules
        .iter()
        .map(|s| {
            s.relationships
                .values()
                .filter(|r| s.is_external(r))
                .count()
        })
        .sum();
    // Ignored, they move nothing.
    let linked = links_out > 0 && !ignore_external;
    // The projects calculated are the ones open together (TF-031).
    let open: Vec<ProjectId> = schedules
        .iter()
        .filter(|s| selected(s))
        .map(|s| s.project.id)
        .collect();
    // Float from the opened projects' finish reads the others' finishes, so
    // those projects move each other's late dates even with no link between them.
    let float_together = open.len() > 1
        && schedules
            .iter()
            .any(|s| selected(s) && !s.project.options.float_from_project_finish);
    if linked {
        if let Some(cycle) = loop_across(schedules, &open) {
            return Err(CliError::new(
                "schedule-refused",
                format!(
                    "relationships between projects make a loop: {}",
                    cycle.join(" -> ")
                ),
                "remove or reverse one of the relationships in the loop",
            ));
        }
        seed(schedules, &open, absent);
    }
    let mut results: Kept = BTreeMap::new();
    // A change crosses at least one link between projects each pass, so early
    // dates settle within a pass per link, plus one to show it. The opened
    // projects' finish then takes a pass, and late dates as many again.
    let passes = if linked || float_together {
        (2 * links_out + 3).min(MAX_PASSES)
    } else {
        1
    };
    // Each open project's own finish, from its latest result. It doesn't
    // depend on the others' finishes, so it settles with the early dates.
    let mut finishes: BTreeMap<ProjectId, DateTime> = BTreeMap::new();
    let mut moved = false;
    for _ in 0..passes {
        moved = false;
        for index in 0..schedules.len() {
            if !selected(&schedules[index]) {
                continue;
            }
            let project = schedules[index].project.id;
            let opened = finishes
                .iter()
                .filter(|(other, _)| **other != project)
                .map(|(_, finish)| *finish)
                .max();
            // Asked for this run only: the project's own options, and so the
            // file, are left as they are.
            let mut options = schedules[index].project.options.clone();
            options.ignore_external_relationships |= ignore_external;
            let prior = results
                .get(&project.0)
                .or_else(|| previous.and_then(|kept| kept.get(&project.0)));
            let calculated =
                schedule_reusing(&schedules[index], &options, (schedules, opened), prior)
                    .map_err(|e| refused(project.0, e))?;
            let result = calculated.result();
            if let Some(finish) = result.project.network_finish {
                moved |= finishes.insert(project, finish) != Some(finish);
            }
            moved |= apply(&mut schedules[index], result, replace, absent);
            results.insert(project.0, calculated);
        }
        if !moved {
            break;
        }
    }
    if (linked || float_together) && moved {
        return Err(CliError::new(
            "schedule-refused",
            format!("results across projects hadn't settled after {passes} passes"),
            "schedule one project at a time with --project",
        ));
    }
    Ok(results)
}

fn differences(lines: &mut Vec<String>, changes: &Changes) {
    if changes.by_field.is_empty() {
        return;
    }
    let mut fields: Vec<_> = changes.by_field.iter().collect();
    fields.sort_by_key(|(_, count)| std::cmp::Reverse(**count));
    let fields: Vec<String> = fields
        .into_iter()
        .map(|(field, count)| format!("{field} {count}"))
        .collect();
    lines.push(format!("  changed fields: {}", fields.join(", ")));
    if changes.starts_at.is_empty() {
        return;
    }
    lines.push("  differences start at:".to_owned());
    let starts = changes
        .activities
        .iter()
        .filter(|a| changes.starts_at.contains(&a.code));
    for activity in starts.take(MAX_SHOWN) {
        if let Some(change) = activity.fields.first() {
            lines.push(format!(
                "    {} {}: {} to {}",
                activity.code,
                change.field,
                change.from.as_deref().unwrap_or("none"),
                change.to.as_deref().unwrap_or("none")
            ));
        }
    }
    if changes.starts_at_total > MAX_SHOWN {
        lines.push(format!(
            "    and {} more",
            changes.starts_at_total - MAX_SHOWN
        ));
    }
}

fn human(report: &Report) -> String {
    let mut lines = Vec::new();
    if report.ignore_external {
        lines.push(
            "Relationships to and from other projects ignored, as asked; the file keeps its own setting".to_owned(),
        );
    }
    for p in &report.projects {
        lines.push(p.heading());
        lines.push(format!(
            "  data date: {}",
            p.data_date.as_deref().unwrap_or("none")
        ));
        lines.push(format!("  {}", p.finish_line()));
        if let (Some(project), Some(activities)) = (&p.previous_finish, &p.activity_finish)
            && project != activities
        {
            lines.push(format!(
                "  note: the file's project finish is {project}, but its unfinished work finishes {activities}"
            ));
        }
        lines.push(format!(
            "  activities: {}, critical: {}, changed: {}, on dates or float: {}",
            p.activities, p.critical, p.changed_activities, p.changed_dates_or_float
        ));
        differences(&mut lines, &p.changes);
    }
    if report.skipped_projects > 0 {
        lines.push(format!(
            "Skipped {} projects that couldn't be mapped",
            report.skipped_projects
        ));
    }
    lines.push(match &report.output {
        Some(path) => format!("Wrote {path}"),
        None => "Nothing written; pass -o to write a new file".to_owned(),
    });
    lines.join("\n")
}

/// Calculates a file that's already been read, returning it with results in
/// place when `write` is set. Without `write`, nothing goes near the patch writer.
pub fn report(
    file: &str,
    loaded: &Loaded,
    (project, data_date): (Option<u64>, Option<&str>),
    (write, overrides): (bool, &Overrides),
) -> Result<(Report, Option<RawXer>), CliError> {
    let (report, scheduled, _) = logged(
        file,
        loaded,
        (project, data_date),
        (write, false),
        overrides,
    )?;
    Ok((report, scheduled))
}

/// [`report`], and the run's log (`schedule --log`) when `log`.
///
/// # Errors
///
/// As [`report`].
pub fn logged(
    file: &str,
    loaded: &Loaded,
    (project, data_date): (Option<u64>, Option<&str>),
    (write, log): (bool, bool),
    overrides: &Overrides,
) -> Result<(Report, Option<RawXer>, Option<String>), CliError> {
    let ignore_external = overrides.ignore_external;
    let data_date = data_date.map(parse_date).transpose()?;
    let parts = (
        &loaded.raw,
        loaded.schedules.as_slice(),
        &loaded.diagnostics,
    );
    let mut run = calculate_with(parts, (project, data_date), overrides, None)?;
    let text = log.then(|| {
        crate::schedlog::text(
            file,
            (&loaded.schedules, &run.edited),
            &run.critical,
            ignore_external,
        )
    });
    // The log shows the paths this run asked for; the file keeps its own.
    if overrides.float_paths.is_some() {
        for edited in &mut run.edited {
            if let Some(was) = loaded
                .schedules
                .iter()
                .find(|b| b.project.id == edited.project.id)
            {
                edited.project.options.float_paths = was.project.options.float_paths.clone();
            }
        }
    }
    let scheduled = write
        .then(|| {
            patch_all(&loaded.raw, &loaded.schedules, &run.edited)
                .map_err(|(id, e)| refused(id.0, e))
        })
        .transpose()?;
    let report = Report {
        schema: "totalfloat.schedule/1",
        file: file.to_owned(),
        output: None,
        ignore_external,
        skipped_projects: run.skipped,
        projects: run.reports,
    };
    Ok((report, scheduled, text))
}

/// A calculated file. Results the engine doesn't give, such as float on finished
/// work, stay as stored, as they do in `schedule`.
pub struct Fresh {
    /// One report per project calculated.
    pub projects: Vec<ProjectReport>,
    /// Those projects with their results.
    pub schedules: Vec<Schedule>,
    /// The file with those results.
    pub raw: RawXer,
    /// Relationships that drive their successor's dates.
    pub driving: BTreeSet<RelationshipId>,
    /// Activities the engine found critical, by the project's definition.
    pub critical: BTreeSet<ActivityId>,
    /// Each project's calculation, for the next one to take from.
    pub kept: Kept,
}

/// Calculates a file for `edit`.
pub fn fresh(
    loaded: &Loaded,
    project: Option<u64>,
    data_date: Option<DateTime>,
) -> Result<Fresh, CliError> {
    fresh_reusing(loaded, (project, data_date), None)
}

/// [`fresh`], taking what it can from `previous`, calculations of the same
/// projects.
///
/// # Errors
///
/// As [`fresh`].
pub fn fresh_reusing(
    loaded: &Loaded,
    (project, data_date): (Option<u64>, Option<DateTime>),
    previous: Option<&Kept>,
) -> Result<Fresh, CliError> {
    let file = (
        &loaded.raw,
        loaded.schedules.as_slice(),
        &loaded.diagnostics,
    );
    let run = calculate_with(file, (project, data_date), &Overrides::default(), previous)?;
    let raw = patch_all(&loaded.raw, &loaded.schedules, &run.edited)
        .map_err(|(id, e)| refused(id.0, e))?;
    Ok(Fresh {
        projects: run.reports,
        schedules: run.edited,
        raw,
        driving: run.driving,
        critical: run.critical,
        kept: run.kept,
    })
}

/// [`fresh`] without writing the results into the file, for a report that
/// won't be written: its projects' reports and calculated schedules. Where
/// writing them could fail, they're written anyway, so a dry run fails as
/// the edit itself would.
///
/// # Errors
///
/// As [`fresh`].
pub fn fresh_unwritten(
    loaded: &Loaded,
    (project, data_date): (Option<u64>, Option<DateTime>),
    previous: Option<&Kept>,
) -> Result<Unwritten, CliError> {
    unwritten_critical(loaded, (project, data_date), previous).map(|(unwritten, _)| unwritten)
}

/// [`fresh_unwritten`], with the activities the engine found critical.
fn unwritten_critical(
    loaded: &Loaded,
    (project, data_date): (Option<u64>, Option<DateTime>),
    previous: Option<&Kept>,
) -> Result<(Unwritten, BTreeSet<ActivityId>), CliError> {
    if !totalfloat_xer::patch::holds_every_project_and_activity_column(&loaded.raw) {
        let fresh = fresh_reusing(loaded, (project, data_date), previous)?;
        return Ok((
            (fresh.projects, fresh.schedules, fresh.kept),
            fresh.critical,
        ));
    }
    let file = (
        &loaded.raw,
        loaded.schedules.as_slice(),
        &loaded.diagnostics,
    );
    let run = calculate_with(file, (project, data_date), &Overrides::default(), previous)?;
    Ok(((run.reports, run.edited, run.kept), run.critical))
}

/// Each calculated project's report, its schedule calculated, and its
/// calculation.
pub type Unwritten = (Vec<ProjectReport>, Vec<Schedule>, Kept);

/// A file's projects calculated as `serve` shows the file, and the activities
/// the engine found critical, for figures that must agree with the grid
///. The results are written into the file only where writing them
/// could fail, so a file `serve` would show with its stored results fails
/// here too.
///
/// # Errors
///
/// As [`fresh`].
pub fn calculated(loaded: &Loaded) -> Result<(Vec<Schedule>, BTreeSet<ActivityId>), CliError> {
    let ((_, schedules, _), critical) = unwritten_critical(loaded, (None, None), None)?;
    Ok((schedules, critical))
}

/// [`fresh_unwritten`] for schedules that stand for the file mapped: a dry
/// run's edited schedules, which the patched file would map back to. `None`
/// where the file can't take every result, so writing them must be tried.
///
/// # Errors
///
/// As [`fresh`].
pub fn unwritten_from(
    file: Parts<'_>,
    (project, data_date): (Option<u64>, Option<DateTime>),
    previous: Option<&Kept>,
) -> Option<Result<Unwritten, CliError>> {
    if !totalfloat_xer::patch::holds_every_project_and_activity_column(file.0) {
        return None;
    }
    Some(
        calculate_with(file, (project, data_date), &Overrides::default(), previous)
            .map(|run| (run.reports, run.edited, run.kept)),
    )
}

/// Runs the command. The source file is never changed: results go to a new
/// file, and only with `-o`.
pub fn run(
    input: &Path,
    (output, log): (Option<&Path>, Option<&Path>),
    (project, data_date): (Option<u64>, Option<&str>),
    json: bool,
    overrides: &Overrides,
) -> Result<u8, CliError> {
    for path in output.into_iter().chain(log) {
        refuse_existing(path)?;
    }
    if output.is_some() && output == log {
        return Err(CliError::new(
            "output-exists",
            "the log and the output are the same file",
            "give the log a path of its own",
        ));
    }
    data_date.map(parse_date).transpose()?;
    let raw = crate::location::read(input)?;
    let label = input.display().to_string();
    let loaded = Loaded::new(raw);
    let (mut report, scheduled, text) = logged(
        &label,
        &loaded,
        (project, data_date),
        (output.is_some(), log.is_some()),
        overrides,
    )?;
    if let (Some(output), Some(scheduled)) = (output, scheduled) {
        write_new(&scheduled, permissions_of(input)?, output)?;
        report.output = Some(output.display().to_string());
    }
    if let (Some(log), Some(text)) = (log, text) {
        create_new_text(&text, log)?;
    }
    if json {
        print_line(&serde_json::to_string_pretty(&report).unwrap_or_default());
    } else {
        print_line(&human(&report));
    }
    Ok(exit::OK)
}

#[cfg(test)]
mod tests {
    use totalfloat_model::{
        ActivityStatus, ActivityType, CalendarId, Project, ProjectId, Relationship, RelationshipId,
        RelationshipType, WbsId,
    };

    use super::*;

    /// Float reads in hundredths of an hour, so every whole minute reads
    /// differently, and a changed result can be counted without writing it
    /// out.
    #[test]
    fn float_text_tells_every_minute_apart() {
        let mut seen = BTreeSet::new();
        for minutes in -300_000..300_000 {
            let text = span_text(Some(Minutes::new(minutes))).unwrap();
            assert!(seen.insert(text), "{minutes}");
        }
    }

    fn at(text: &str) -> DateTime {
        text.parse().unwrap()
    }

    /// A1 to A2 to A3.
    fn chain() -> Schedule {
        let mut s = Schedule::new(Project::new(ProjectId(1), "AUTHORED"));
        for n in 1..=3 {
            let a = Activity::new(
                ActivityId(n),
                ProjectId(1),
                WbsId(1),
                CalendarId(1),
                format!("A{n}"),
                "Authored",
            );
            s.activities.insert(a.id, a);
        }
        for (id, from, to) in [(1, 1, 2), (2, 2, 3)] {
            s.relationships.insert(
                RelationshipId(id),
                Relationship {
                    id: RelationshipId(id),
                    predecessor: ActivityId(from),
                    successor: ActivityId(to),
                    predecessor_project: ProjectId(1),
                    successor_project: ProjectId(1),
                    kind: RelationshipType::FinishToStart,
                    lag: Minutes::ZERO,
                    source: None,
                },
            );
        }
        s
    }

    fn set(s: &mut Schedule, n: u64, change: impl Fn(&mut Activity)) {
        change(s.activities.get_mut(&ActivityId(n)).unwrap());
    }

    #[test]
    fn early_differences_start_at_the_first_in_logic() {
        let before = chain();
        let mut after = before.clone();
        for n in [2, 3] {
            set(&mut after, n, |a| {
                a.stored.early_start = Some(at("2026-01-05 08:00"));
            });
        }
        let (changes, total) = changes(&before, &after);
        assert_eq!(total, 2);
        assert_eq!(changes.starts_at, ["A2"]);
        assert_eq!(changes.starts_at_total, 1);
        assert_eq!(changes.by_field["early_start"], 2);
        assert_eq!(changes.activities[0].code, "A2");
        assert_eq!(changes.activities[0].fields[0].field, "early_start");
        assert_eq!(changes.activities[0].fields[0].from, None);
    }

    #[test]
    fn late_differences_start_at_the_last_in_logic() {
        let before = chain();
        let mut after = before.clone();
        for n in [1, 2] {
            set(&mut after, n, |a| {
                a.stored.late_finish = Some(at("2026-01-05 17:00"));
            });
        }
        assert_eq!(changes(&before, &after).0.starts_at, ["A2"]);
    }

    #[test]
    fn finished_work_following_the_data_date_starts_nothing() {
        let mut before = chain();
        set(&mut before, 1, |a| a.status = ActivityStatus::Completed);
        let mut after = before.clone();
        for n in [1, 2] {
            set(&mut after, n, |a| {
                a.stored.early_start = Some(at("2026-01-05 08:00"));
            });
        }
        let (changes, total) = changes(&before, &after);
        assert_eq!(total, 1);
        assert_eq!(changes.starts_at, ["A2"]);
        assert_eq!(changes.by_field["early_start"], 2);
    }

    #[test]
    fn spans_follow_other_activities_so_they_start_nothing() {
        let mut before = chain();
        set(&mut before, 1, |a| {
            a.activity_type = ActivityType::WbsSummary;
        });
        let mut after = before.clone();
        for n in [1, 2] {
            set(&mut after, n, |a| {
                a.stored.early_start = Some(at("2026-01-05 08:00"));
            });
        }
        let (changes, total) = changes(&before, &after);
        assert_eq!(total, 2);
        assert_eq!(changes.starts_at, ["A2"]);
    }

    #[test]
    fn a_late_starting_place_shows_the_late_field_first() {
        let before = chain();
        let mut after = before.clone();
        set(&mut after, 1, |a| {
            a.stored.early_start = Some(at("2026-01-05 08:00"));
        });
        set(&mut after, 2, |a| {
            a.stored.early_start = Some(at("2026-01-06 08:00"));
            a.stored.late_finish = Some(at("2026-01-07 17:00"));
        });
        let (changes, _) = changes(&before, &after);
        assert_eq!(changes.starts_at, ["A1", "A2"]);
        assert_eq!(changes.activities[1].fields[0].field, "late_finish");
    }

    #[test]
    fn changes_outside_dates_and_float_are_counted_but_not_listed() {
        let before = chain();
        let mut after = before.clone();
        set(&mut after, 1, |a| a.stored.float_path = Some(1));
        let (changes, total) = changes(&before, &after);
        assert_eq!(total, 0);
        assert_eq!(changes.by_field["float_path"], 1);
        assert!(changes.activities.is_empty());
    }

    #[test]
    fn lists_stop_at_the_limit_and_counts_do_not() {
        let mut before = Schedule::new(Project::new(ProjectId(1), "AUTHORED"));
        for n in 1..=60 {
            let a = Activity::new(
                ActivityId(n),
                ProjectId(1),
                WbsId(1),
                CalendarId(1),
                format!("A{n}"),
                "Authored",
            );
            before.activities.insert(a.id, a);
        }
        let mut after = before.clone();
        for n in 1..=60 {
            set(&mut after, n, |a| {
                a.stored.early_start = Some(at("2026-01-05 08:00"));
            });
        }
        let (changes, total) = changes(&before, &after);
        assert_eq!(total, 60);
        assert_eq!(changes.starts_at_total, 60);
        assert_eq!(changes.starts_at.len(), MAX_LISTED);
        assert_eq!(changes.activities.len(), MAX_LISTED);
    }
}
