//! Semantic comparison of two schedules for TotalFloat.
//!
//! Compares two versions of a schedule (for example consecutive updates) in
//! scheduling terms: added, removed and changed activities, logic, durations,
//! constraints and dates. Both sides are read as stored; nothing is calculated.

#![forbid(unsafe_code)]

use std::borrow::Cow;
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;
use totalfloat_model::{
    Activity, ActivityCode, ActivityId, ActivityStatus, CalendarId, Constraint, ConstraintType,
    CriticalDefinition, DateTime, Minutes, Relationship, RelationshipType, Schedule,
};

/// What kind of change something is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Category {
    /// Activity ID, name, type or WBS.
    Identity,
    /// Original or remaining duration.
    Duration,
    /// Status, actual dates or percent complete.
    Progress,
    /// A primary or secondary constraint.
    Constraint,
    /// The activity's calendar.
    Calendar,
    /// Activity code assignments.
    Codes,
}

/// One changed field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FieldChange {
    /// The kind of change.
    pub category: Category,
    /// Field name.
    pub field: &'static str,
    /// Value in the earlier schedule, or empty.
    pub from: String,
    /// Value in the later schedule, or empty.
    pub to: String,
}

/// An activity on one side only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ActivityRef {
    /// `TASK.task_code`.
    pub code: String,
    /// `TASK.task_name`.
    pub name: String,
}

/// An activity on both sides that differs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ActivityChange {
    /// `TASK.task_code` in the later schedule.
    pub code: String,
    /// Every changed field.
    pub changes: Vec<FieldChange>,
}

/// A relationship on one side only, or with a different lag.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LinkChange {
    /// Predecessor's activity ID.
    pub predecessor: String,
    /// Successor's activity ID.
    pub successor: String,
    /// `FS`, `SS`, `FF` or `SF`.
    pub kind: &'static str,
    /// Lag in the earlier schedule, if it had this link.
    pub from_lag: Option<String>,
    /// Lag in the later schedule, if it has this link.
    pub to_lag: Option<String>,
}

/// A milestone whose date moved.
#[derive(Debug, Clone, PartialEq, Serialize, schemars::JsonSchema)]
pub struct DateMove {
    /// `TASK.task_code`.
    pub code: String,
    /// Stored date in the earlier schedule.
    pub from: Option<String>,
    /// Stored date in the later schedule.
    pub to: Option<String>,
    /// Elapsed hours later, negative for earlier, if both have a date.
    pub hours: Option<f64>,
}

/// Why a change deserves a look.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Suspicion {
    /// A critical activity got shorter with no progress to explain it.
    CriticalDurationCut,
    /// Logic was removed from a critical activity.
    CriticalLogicDeleted,
    /// An activity gained a constraint.
    NewConstraint,
    /// Critical work moved to another calendar.
    CriticalCalendarChange,
    /// An actual date recorded earlier now reads differently.
    ActualChanged,
}

/// A change to look at. Not necessarily wrong.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct Finding {
    /// Why it was flagged.
    pub kind: Suspicion,
    /// The activity or relationship.
    pub subject: String,
    /// What happened.
    pub detail: String,
}

/// How far the project finish moved.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Project {
    /// `PROJECT.proj_short_name` in the later schedule.
    pub code: String,
    /// Data date in the earlier schedule.
    pub from_data_date: Option<String>,
    /// Data date in the later schedule.
    pub to_data_date: Option<String>,
    /// Stored finish in the earlier schedule.
    pub from_finish: Option<String>,
    /// Stored finish in the later schedule.
    pub to_finish: Option<String>,
    /// Elapsed hours later, negative for earlier, if both have a finish.
    pub finish_moved_hours: Option<f64>,
}

/// How many of each thing changed, before any listing limit.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Counts {
    /// Activities only in the later schedule.
    pub added: usize,
    /// Activities only in the earlier schedule.
    pub deleted: usize,
    /// Activities on both sides that differ.
    pub changed: usize,
    /// Relationships only in the later schedule.
    pub links_added: usize,
    /// Relationships only in the earlier schedule.
    pub links_deleted: usize,
    /// Relationships whose lag differs.
    pub links_changed: usize,
    /// Milestones whose date moved.
    pub milestones_moved: usize,
    /// Activities that became critical.
    pub became_critical: usize,
    /// Activities no longer critical.
    pub left_critical: usize,
    /// Changes flagged for a look.
    pub findings: usize,
}

/// A comparison of two versions of one project.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Comparison {
    /// Project-level movement.
    pub project: Project,
    /// Complete counts.
    pub counts: Counts,
    /// Activities only in the later schedule.
    pub added: Vec<ActivityRef>,
    /// Activities only in the earlier schedule.
    pub deleted: Vec<ActivityRef>,
    /// Activities that differ.
    pub changed: Vec<ActivityChange>,
    /// Relationships only in the later schedule.
    pub links_added: Vec<LinkChange>,
    /// Relationships only in the earlier schedule.
    pub links_deleted: Vec<LinkChange>,
    /// Relationships whose lag differs.
    pub links_changed: Vec<LinkChange>,
    /// Milestones whose date moved.
    pub milestones_moved: Vec<DateMove>,
    /// Activities that became critical.
    pub became_critical: Vec<String>,
    /// Activities no longer critical.
    pub left_critical: Vec<String>,
    /// Changes flagged for a look.
    pub findings: Vec<Finding>,
}

impl Comparison {
    /// Keeps at most `limit` items in each list. `counts` still says how many
    /// there were, so a large comparison can't flood a model's context.
    pub fn limit(&mut self, limit: usize) {
        self.added.truncate(limit);
        self.deleted.truncate(limit);
        self.changed.truncate(limit);
        self.links_added.truncate(limit);
        self.links_deleted.truncate(limit);
        self.links_changed.truncate(limit);
        self.milestones_moved.truncate(limit);
        self.became_critical.truncate(limit);
        self.left_critical.truncate(limit);
        self.findings.truncate(limit);
    }
}

fn hours(span: Minutes) -> String {
    let sign = if span.get() < 0 { "-" } else { "" };
    let total = span.get().unsigned_abs();
    match total % 60 {
        0 => format!("{sign}{}h", total / 60),
        minutes => format!("{sign}{}h{minutes:02}m", total / 60),
    }
}

fn hours_between(from: Option<DateTime>, to: Option<DateTime>) -> Option<f64> {
    let minutes = to?.minutes_since(from?).get();
    #[allow(
        clippy::cast_precision_loss,
        reason = "display hours; minutes are far below 2^52"
    )]
    let hours = minutes as f64 / 60.0;
    Some(hours)
}

fn text(date: Option<DateTime>) -> String {
    date.map(|d| d.to_string()).unwrap_or_default()
}

const fn constraint_label(kind: ConstraintType) -> &'static str {
    match kind {
        ConstraintType::StartOn => "start on",
        ConstraintType::StartOnOrAfter => "start on or after",
        ConstraintType::StartOnOrBefore => "start on or before",
        ConstraintType::FinishOn => "finish on",
        ConstraintType::FinishOnOrAfter => "finish on or after",
        ConstraintType::FinishOnOrBefore => "finish on or before",
        ConstraintType::MandatoryStart => "mandatory start",
        ConstraintType::MandatoryFinish => "mandatory finish",
        ConstraintType::AsLateAsPossible => "as late as possible",
    }
}

fn constraint_text(constraint: Option<Constraint>) -> String {
    constraint.map_or_else(String::new, |c| match c.date {
        Some(date) => format!("{} {date}", constraint_label(c.kind)),
        None => constraint_label(c.kind).to_owned(),
    })
}

const fn link_label(kind: RelationshipType) -> &'static str {
    match kind {
        RelationshipType::FinishToStart => "FS",
        RelationshipType::StartToStart => "SS",
        RelationshipType::FinishToFinish => "FF",
        RelationshipType::StartToFinish => "SF",
    }
}

fn calendar_name(schedule: &Schedule, id: CalendarId) -> String {
    schedule
        .calendars
        .get(&id)
        .map_or_else(|| id.0.to_string(), |c| c.name.clone())
}

fn code_names(schedule: &Schedule, activity: &Activity) -> String {
    let mut names: Vec<String> = activity
        .codes
        .values()
        .filter_map(|id| schedule.activity_codes.get(id))
        .map(|code| {
            let kind = schedule.activity_code_types.get(&code.code_type);
            format!(
                "{}={}",
                kind.map_or("?", |kind| kind.name.as_str()),
                code.value
            )
        })
        .collect();
    names.sort();
    names.join(", ")
}

fn critical(schedule: &Schedule, activity: &Activity) -> bool {
    match schedule.project.options.critical {
        CriticalDefinition::TotalFloatAtMost(limit) => {
            activity.stored.total_float.is_some_and(|f| f <= limit)
        }
        CriticalDefinition::LongestPath => activity.stored.on_longest_path == Some(true),
    }
}

/// Pairs activities by Activity ID, then by internal ID for what's left, so a
/// renumbered activity reads as a change, not a deletion and an addition.
#[must_use]
pub fn pairs<'a>(from: &'a Schedule, to: &'a Schedule) -> Vec<(&'a Activity, &'a Activity)> {
    let by_code: BTreeMap<&str, &Activity> = to
        .activities
        .values()
        .map(|a| (a.code.as_str(), a))
        .collect();
    let mut matched = Vec::new();
    let mut taken = BTreeSet::new();
    let mut unmatched = Vec::new();
    for a in from.activities.values() {
        match by_code.get(a.code.as_str()) {
            Some(b) => {
                taken.insert(b.id);
                matched.push((a, *b));
            }
            None => unmatched.push(a),
        }
    }
    for a in unmatched {
        if let Some(b) = to.activities.get(&a.id).filter(|b| !taken.contains(&b.id)) {
            taken.insert(b.id);
            matched.push((a, b));
        }
    }
    matched
}

/// The name a code type has in `schedule`, if it has the type.
fn code_type_name<'a>(schedule: &'a Schedule, code: &ActivityCode) -> Option<&'a str> {
    schedule
        .activity_code_types
        .get(&code.code_type)
        .map(|kind| kind.name.as_str())
}

/// Whether two activities' codes read the same, judged without writing them
/// out: the same codes, each with the same value and type name on both sides.
/// When this says no, [`code_names`] decides.
fn same_codes(from: &Schedule, to: &Schedule, a: &Activity, b: &Activity) -> bool {
    a.codes == b.codes
        && a.codes.values().all(|id| {
            match (from.activity_codes.get(id), to.activity_codes.get(id)) {
                (Some(x), Some(y)) => {
                    x.value == y.value
                        && x.code_type == y.code_type
                        && code_type_name(from, x) == code_type_name(to, y)
                }
                (None, None) => true,
                _ => false,
            }
        })
}

/// Whether two activities' calendars read the same, judged without writing
/// them out. When this says no, [`calendar_name`] decides.
fn same_calendar(from: &Schedule, to: &Schedule, a: &Activity, b: &Activity) -> bool {
    match (
        from.calendars.get(&a.calendar),
        to.calendars.get(&b.calendar),
    ) {
        (Some(x), Some(y)) => x.name == y.name,
        (None, None) => a.calendar == b.calendar,
        _ => false,
    }
}

fn wbs_code<'a>(schedule: &'a Schedule, activity: &Activity) -> &'a str {
    schedule
        .wbs
        .get(&activity.wbs)
        .map_or("", |w| w.code.as_str())
}

/// Field changes as they're found. A field is written out only where its
/// values differ: values that are equal always read the same.
#[derive(Default)]
struct Notes(Vec<FieldChange>);

impl Notes {
    fn note(
        &mut self,
        (category, field): (Category, &'static str),
        same: bool,
        texts: impl FnOnce() -> (String, String),
    ) {
        if same {
            return;
        }
        let (from, to) = texts();
        if from != to {
            self.0.push(FieldChange {
                category,
                field,
                from,
                to,
            });
        }
    }
}

/// The fields that read differently.
fn field_changes(from: &Schedule, to: &Schedule, a: &Activity, b: &Activity) -> Vec<FieldChange> {
    use Category::{Calendar, Codes, Duration, Identity};
    let mut notes = Notes::default();
    notes.note((Identity, "code"), a.code == b.code, || {
        (a.code.clone(), b.code.clone())
    });
    notes.note((Identity, "name"), a.name == b.name, || {
        (a.name.clone(), b.name.clone())
    });
    notes.note(
        (Identity, "type"),
        a.activity_type == b.activity_type,
        || {
            (
                format!("{:?}", a.activity_type),
                format!("{:?}", b.activity_type),
            )
        },
    );
    let (wbs_a, wbs_b) = (wbs_code(from, a), wbs_code(to, b));
    notes.note((Identity, "wbs"), wbs_a == wbs_b, || {
        (wbs_a.to_owned(), wbs_b.to_owned())
    });
    notes.note(
        (Duration, "original_duration"),
        a.original_duration == b.original_duration,
        || (hours(a.original_duration), hours(b.original_duration)),
    );
    notes.note(
        (Duration, "remaining_duration"),
        a.remaining_duration == b.remaining_duration,
        || (hours(a.remaining_duration), hours(b.remaining_duration)),
    );
    progress_changes(&mut notes, a, b);
    notes.note(
        (Calendar, "calendar"),
        same_calendar(from, to, a, b),
        || {
            (
                calendar_name(from, a.calendar),
                calendar_name(to, b.calendar),
            )
        },
    );
    notes.note((Codes, "codes"), same_codes(from, to, a, b), || {
        (code_names(from, a), code_names(to, b))
    });
    notes.0
}

/// Progress and constraint fields that read differently.
fn progress_changes(notes: &mut Notes, a: &Activity, b: &Activity) {
    use Category::{Constraint as ConstraintCategory, Progress};
    notes.note((Progress, "status"), a.status == b.status, || {
        (format!("{:?}", a.status), format!("{:?}", b.status))
    });
    notes.note(
        (Progress, "actual_start"),
        a.actual_start == b.actual_start,
        || (text(a.actual_start), text(b.actual_start)),
    );
    notes.note(
        (Progress, "actual_finish"),
        a.actual_finish == b.actual_finish,
        || (text(a.actual_finish), text(b.actual_finish)),
    );
    let percent = |v: Option<f64>| v.map_or_else(String::new, |v| v.to_string());
    // Compared bit for bit: 0.0 and -0.0 are equal but read differently.
    let bits = |v: Option<f64>| v.map(f64::to_bits);
    notes.note(
        (Progress, "physical_percent_complete"),
        bits(a.physical_percent_complete) == bits(b.physical_percent_complete),
        || {
            (
                percent(a.physical_percent_complete),
                percent(b.physical_percent_complete),
            )
        },
    );
    for (field, was, is) in [
        (
            "primary_constraint",
            a.primary_constraint,
            b.primary_constraint,
        ),
        (
            "secondary_constraint",
            a.secondary_constraint,
            b.secondary_constraint,
        ),
    ] {
        notes.note((ConstraintCategory, field), was == is, || {
            (constraint_text(was), constraint_text(is))
        });
    }
}

/// A relationship by its activities' IDs and type, so renumbered internal IDs
/// don't read as changed logic. Borrowed where it can be, so keying 50,000
/// links writes no strings.
type LinkKey<'a> = (Cow<'a, str>, Cow<'a, str>, RelationshipType);

/// Every link of `schedule` by its key, in key order, keeping the last of
/// any that share a key, as a map from key to link would.
fn links<'a>(
    schedule: &'a Schedule,
    renames: &BTreeMap<ActivityId, &'a str>,
) -> Vec<(LinkKey<'a>, &'a Relationship)> {
    let code = |id: ActivityId| -> Cow<'a, str> {
        renames
            .get(&id)
            .copied()
            .or_else(|| schedule.activities.get(&id).map(|a| a.code.as_str()))
            .map_or_else(|| Cow::Owned(id.0.to_string()), Cow::Borrowed)
    };
    let mut keyed: Vec<(LinkKey<'a>, &'a Relationship)> = schedule
        .relationships
        .values()
        .map(|link| {
            (
                (code(link.predecessor), code(link.successor), link.kind),
                link,
            )
        })
        .collect();
    // Stable, so links that share a key stay in ID order, and the last is kept.
    keyed.sort_by(|x, y| x.0.cmp(&y.0));
    let mut kept: Vec<(LinkKey<'a>, &'a Relationship)> = Vec::with_capacity(keyed.len());
    for (key, link) in keyed {
        match kept.last_mut() {
            Some(last) if last.0 == key => last.1 = link,
            _ => kept.push((key, link)),
        }
    }
    kept
}

fn link_change(key: &LinkKey<'_>, from: Option<Minutes>, to: Option<Minutes>) -> LinkChange {
    LinkChange {
        predecessor: key.0.clone().into_owned(),
        successor: key.1.clone().into_owned(),
        kind: link_label(key.2),
        from_lag: from.map(hours),
        to_lag: to.map(hours),
    }
}

/// When a milestone falls. A finished one stores the data date as its early
/// finish (TF-020), so its actual date is the real one.
/// When a milestone happened or is due: its actual finish, else its actual
/// start, else its early finish.
#[must_use]
pub fn milestone_date(a: &Activity) -> Option<DateTime> {
    a.actual_finish.or(a.actual_start).or(a.stored.early_finish)
}

fn reference(activity: &Activity) -> ActivityRef {
    ActivityRef {
        code: activity.code.clone(),
        name: activity.name.clone(),
    }
}

/// Work under way uses up remaining duration as time passes, so a cut no bigger
/// than the time between data dates needs no other explanation.
fn worked_off(from: &Schedule, to: &Schedule, a: &Activity, b: &Activity) -> bool {
    let elapsed = match (from.project.data_date, to.project.data_date) {
        (Some(earlier), Some(later)) => later.minutes_since(earlier),
        _ => Minutes::ZERO,
    };
    b.status == ActivityStatus::InProgress
        && b.original_duration >= a.original_duration
        && a.remaining_duration.get() - b.remaining_duration.get() <= elapsed.get()
}

fn duration_cut(from: &Schedule, to: &Schedule, a: &Activity, b: &Activity) -> Option<Finding> {
    let no_progress = a.status == b.status
        && a.actual_start == b.actual_start
        && a.actual_finish == b.actual_finish
        && a.physical_percent_complete == b.physical_percent_complete
        && !worked_off(from, to, a, b);
    let cut =
        b.remaining_duration < a.remaining_duration || b.original_duration < a.original_duration;
    (critical(from, a) && no_progress && cut && b.status != ActivityStatus::Completed).then(|| {
        Finding {
            kind: Suspicion::CriticalDurationCut,
            subject: b.code.clone(),
            detail: if b.remaining_duration < a.remaining_duration {
                format!(
                    "remaining duration went from {} to {} with no progress",
                    hours(a.remaining_duration),
                    hours(b.remaining_duration)
                )
            } else {
                format!(
                    "original duration went from {} to {} with no progress",
                    hours(a.original_duration),
                    hours(b.original_duration)
                )
            },
        }
    })
}

fn calendar_change(from: &Schedule, to: &Schedule, a: &Activity, b: &Activity) -> Option<Finding> {
    if !critical(from, a) || b.status == ActivityStatus::Completed || a.calendar == b.calendar {
        return None;
    }
    let (was, is) = (
        from.calendars.get(&a.calendar),
        to.calendars.get(&b.calendar),
    );
    // IDs can differ between exports, so a new ID alone proves nothing: the same
    // name and the same working time is the same calendar.
    let same = was.zip(is).is_some_and(|(was, is)| {
        was.name == is.name && was.work_week == is.work_week && was.exceptions == is.exceptions
    });
    (!same).then(|| Finding {
        kind: Suspicion::CriticalCalendarChange,
        subject: b.code.clone(),
        detail: format!(
            "calendar went from {} to {}",
            calendar_name(from, a.calendar),
            calendar_name(to, b.calendar)
        ),
    })
}

/// Actuals only grow between updates: one that reads differently, or has gone,
/// was rewritten.
fn actuals_changed(a: &Activity, b: &Activity) -> impl Iterator<Item = Finding> {
    [
        ("start", a.actual_start, b.actual_start),
        ("finish", a.actual_finish, b.actual_finish),
    ]
    .into_iter()
    .filter_map(move |(name, was, is)| {
        let was = was?;
        (is != Some(was)).then(|| Finding {
            kind: Suspicion::ActualChanged,
            subject: b.code.clone(),
            detail: match is {
                Some(is) => format!("actual {name} went from {was} to {is}"),
                None => format!("actual {name} of {was} was removed"),
            },
        })
    })
}

#[derive(Default)]
struct LinkChanges {
    added: Vec<LinkChange>,
    deleted: Vec<LinkChange>,
    changed: Vec<LinkChange>,
}

fn compare_links(
    from: &Schedule,
    to: &Schedule,
    matched: &[(&Activity, &Activity)],
    findings: &mut Vec<Finding>,
) -> LinkChanges {
    // Both sides key a link by the later Activity ID.
    let renames: BTreeMap<ActivityId, &str> = matched
        .iter()
        .map(|(a, b)| (a.id, b.code.as_str()))
        .collect();
    let old_links = links(from, &renames);
    let new_links = links(to, &BTreeMap::new());
    let mut changes = LinkChanges::default();
    // Both lists are in key order, so one pass over each pairs them.
    let (mut old, mut new) = (old_links.iter().peekable(), new_links.iter().peekable());
    loop {
        let order = match (old.peek(), new.peek()) {
            (None, None) => break,
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (Some((was, _)), Some((is, _))) => was.cmp(is),
        };
        match order {
            Ordering::Less => {
                let Some((key, link)) = old.next() else { break };
                changes.deleted.push(link_change(key, Some(link.lag), None));
                let on_path = [link.predecessor, link.successor]
                    .iter()
                    .filter_map(|id| from.activities.get(id))
                    .any(|a| critical(from, a));
                if on_path {
                    findings.push(Finding {
                        kind: Suspicion::CriticalLogicDeleted,
                        subject: format!("{} to {}", key.0, key.1),
                        detail: format!("{} link removed from critical work", link_label(key.2)),
                    });
                }
            }
            Ordering::Greater => {
                let Some((key, link)) = new.next() else { break };
                changes.added.push(link_change(key, None, Some(link.lag)));
            }
            Ordering::Equal => {
                let (Some((key, link)), Some((_, now))) = (old.next(), new.next()) else {
                    break;
                };
                if now.lag != link.lag {
                    changes
                        .changed
                        .push(link_change(key, Some(link.lag), Some(now.lag)));
                }
            }
        }
    }
    changes
}

/// Compares an earlier and a later version of one project.
#[must_use]
pub fn compare(from: &Schedule, to: &Schedule) -> Comparison {
    let matched = pairs(from, to);
    let matched_from: BTreeSet<ActivityId> = matched.iter().map(|(a, _)| a.id).collect();
    let matched_to: BTreeSet<ActivityId> = matched.iter().map(|(_, b)| b.id).collect();
    let added: Vec<ActivityRef> = to
        .activities
        .values()
        .filter(|b| !matched_to.contains(&b.id))
        .map(reference)
        .collect();
    let deleted: Vec<ActivityRef> = from
        .activities
        .values()
        .filter(|a| !matched_from.contains(&a.id))
        .map(reference)
        .collect();

    let mut changed = Vec::new();
    let mut findings = Vec::new();
    let mut moved = Vec::new();
    let (mut became_critical, mut left_critical) = (Vec::new(), Vec::new());
    for (a, b) in &matched {
        let fields = field_changes(from, to, a, b);
        findings.extend(duration_cut(from, to, a, b));
        findings.extend(calendar_change(from, to, a, b));
        findings.extend(actuals_changed(a, b));
        for (before, after) in [
            (a.primary_constraint, b.primary_constraint),
            (a.secondary_constraint, b.secondary_constraint),
        ] {
            if before.is_none() && after.is_some() {
                findings.push(Finding {
                    kind: Suspicion::NewConstraint,
                    subject: b.code.clone(),
                    detail: constraint_text(after),
                });
            }
        }
        if !fields.is_empty() {
            changed.push(ActivityChange {
                code: b.code.clone(),
                changes: fields,
            });
        }
        let (was, is) = (milestone_date(a), milestone_date(b));
        if b.activity_type.is_milestone() && was != is {
            moved.push(DateMove {
                code: b.code.clone(),
                from: was.map(|d| d.to_string()),
                to: is.map(|d| d.to_string()),
                hours: hours_between(was, is),
            });
        }
        match (critical(from, a), critical(to, b)) {
            (false, true) => became_critical.push(b.code.clone()),
            (true, false) => left_critical.push(b.code.clone()),
            _ => {}
        }
    }

    let links = compare_links(from, to, &matched, &mut findings);
    findings.sort_by(|x, y| (x.kind, &x.subject).cmp(&(y.kind, &y.subject)));

    let project = Project {
        code: to.project.short_name.clone(),
        from_data_date: from.project.data_date.map(|d| d.to_string()),
        to_data_date: to.project.data_date.map(|d| d.to_string()),
        from_finish: from.project.stored.scheduled_finish.map(|d| d.to_string()),
        to_finish: to.project.stored.scheduled_finish.map(|d| d.to_string()),
        finish_moved_hours: hours_between(
            from.project.stored.scheduled_finish,
            to.project.stored.scheduled_finish,
        ),
    };
    let counts = Counts {
        added: added.len(),
        deleted: deleted.len(),
        changed: changed.len(),
        links_added: links.added.len(),
        links_deleted: links.deleted.len(),
        links_changed: links.changed.len(),
        milestones_moved: moved.len(),
        became_critical: became_critical.len(),
        left_critical: left_critical.len(),
        findings: findings.len(),
    };
    Comparison {
        project,
        counts,
        added,
        deleted,
        changed,
        links_added: links.added,
        links_deleted: links.deleted,
        links_changed: links.changed,
        milestones_moved: moved,
        became_critical,
        left_critical,
        findings,
    }
}

#[cfg(test)]
mod tests;
