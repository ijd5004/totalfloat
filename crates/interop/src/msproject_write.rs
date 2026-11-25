//! Writes MS Project XML (`MSPDI`) from a schedule, for [`crate::msproject`] to
//! read back and Project to open. WBS nodes become summary tasks.
//!
//! P6 holds things Project doesn't, so what's lost goes in the notes, as for an
//! import (ARCHITECTURE.md, File interop).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use totalfloat_model as m;

use crate::{Note, Notes};

/// The file's text, and what didn't translate.
#[derive(Debug)]
pub struct Exported {
    /// The XML document.
    pub text: String,
    /// What didn't translate exactly.
    pub notes: Vec<Note>,
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for letter in text.chars() {
        match letter {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            // Neither a control character nor these two can appear in XML 1.0.
            '\u{FFFE}' | '\u{FFFF}' => out.push(' '),
            letter if letter.is_control() => out.push(' '),
            letter => out.push(letter),
        }
    }
    out
}

fn stamp(date: m::DateTime) -> String {
    date.to_string().replace(' ', "T") + ":00"
}

fn span(minutes: m::Minutes) -> String {
    let total = minutes.get().max(0);
    format!("PT{}H{}M0S", total / 60, total % 60)
}

/// One XML element per line, indented, so the file can be read and diffed.
struct Out {
    text: String,
    depth: usize,
}

impl Out {
    fn open(&mut self, name: &str) {
        let _ = writeln!(self.text, "{:width$}<{name}>", "", width = self.depth * 2);
        self.depth += 1;
    }

    fn close(&mut self, name: &str) {
        self.depth -= 1;
        let _ = writeln!(self.text, "{:width$}</{name}>", "", width = self.depth * 2);
    }

    fn field(&mut self, name: &str, value: impl std::fmt::Display) {
        let value = escape(&value.to_string());
        let _ = writeln!(
            self.text,
            "{:width$}<{name}>{value}</{name}>",
            "",
            width = self.depth * 2
        );
    }

    fn date(&mut self, name: &str, value: Option<m::DateTime>) {
        if let Some(value) = value {
            self.field(name, stamp(value));
        }
    }
}

fn shifts(out: &mut Out, intervals: &[m::WorkInterval]) {
    if intervals.is_empty() {
        return;
    }
    out.open("WorkingTimes");
    for interval in intervals {
        out.open("WorkingTime");
        out.field("FromTime", format!("{}:00", interval.start));
        let finish = if interval.finish == m::TimeOfDay::END_OF_DAY {
            "00:00:00".to_owned()
        } else {
            format!("{}:00", interval.finish)
        };
        out.field("ToTime", finish);
        out.close("WorkingTime");
    }
    out.close("WorkingTimes");
}

fn calendar(out: &mut Out, uid: usize, calendar: &m::Calendar) {
    out.open("Calendar");
    out.field("UID", uid);
    out.field("Name", &calendar.name);
    out.field("IsBaseCalendar", 1);
    out.field("BaseCalendarUID", -1);
    out.open("WeekDays");
    for day in m::Weekday::ALL {
        let intervals = calendar.work_week.day(day);
        out.open("WeekDay");
        out.field("DayType", day.index() + 1);
        out.field("DayWorking", u8::from(!intervals.is_empty()));
        shifts(out, intervals);
        out.close("WeekDay");
    }
    out.close("WeekDays");
    if !calendar.exceptions.is_empty() {
        out.open("Exceptions");
        for (date, intervals) in &calendar.exceptions {
            out.open("Exception");
            out.open("TimePeriod");
            out.field("FromDate", format!("{date}T00:00:00"));
            out.field("ToDate", format!("{date}T23:59:00"));
            out.close("TimePeriod");
            out.field("Occurrences", 1);
            out.field("Type", 1);
            out.field("DayWorking", u8::from(!intervals.is_empty()));
            shifts(out, intervals);
            out.close("Exception");
        }
        out.close("Exceptions");
    }
    out.close("Calendar");
}

const fn constraint_code(kind: m::ConstraintType) -> u8 {
    match kind {
        m::ConstraintType::AsLateAsPossible => 1,
        m::ConstraintType::StartOn | m::ConstraintType::MandatoryStart => 2,
        m::ConstraintType::FinishOn | m::ConstraintType::MandatoryFinish => 3,
        m::ConstraintType::StartOnOrAfter => 4,
        m::ConstraintType::StartOnOrBefore => 5,
        m::ConstraintType::FinishOnOrAfter => 6,
        m::ConstraintType::FinishOnOrBefore => 7,
    }
}

const fn link_code(kind: m::RelationshipType) -> u8 {
    match kind {
        m::RelationshipType::FinishToFinish => 0,
        m::RelationshipType::FinishToStart => 1,
        m::RelationshipType::StartToFinish => 2,
        m::RelationshipType::StartToStart => 3,
    }
}

/// What the rows of the task list point at.
struct Numbers {
    activities: BTreeMap<m::ActivityId, usize>,
    calendars: BTreeMap<m::CalendarId, usize>,
}

/// The file's shape, indexed once: a schedule can hold a hundred thousand rows.
#[derive(Default)]
struct Index<'a> {
    work: BTreeMap<m::WbsId, Vec<&'a m::Activity>>,
    children: BTreeMap<m::WbsId, Vec<&'a m::Wbs>>,
    links_into: BTreeMap<m::ActivityId, Vec<&'a m::Relationship>>,
}

impl<'a> Index<'a> {
    fn new(schedule: &'a m::Schedule) -> Self {
        let mut index = Self::default();
        for activity in schedule.activities.values() {
            index.work.entry(activity.wbs).or_default().push(activity);
        }
        for node in schedule.wbs.values() {
            if let Some(parent) = node.parent {
                index.children.entry(parent).or_default().push(node);
            }
        }
        for held in index.children.values_mut() {
            held.sort_by_key(|w| (w.sequence, w.id));
        }
        for link in schedule.relationships.values() {
            index
                .links_into
                .entry(link.successor)
                .or_default()
                .push(link);
        }
        index
    }
}

/// Rows in outline order: each node, then its work, then the nodes under it.
enum Row<'a> {
    Node(&'a m::Wbs),
    Work(&'a m::Activity),
}

fn outline<'a>(
    index: &Index<'a>,
    node: m::WbsId,
    prefix: &str,
    level: usize,
    seen: &mut BTreeSet<m::WbsId>,
    rows: &mut Vec<(Row<'a>, usize, String)>,
    notes: &mut Notes,
) {
    if !seen.insert(node) {
        return;
    }
    let mut place = 0;
    let number = |place: &mut usize| {
        *place += 1;
        if prefix.is_empty() {
            place.to_string()
        } else {
            format!("{prefix}.{place}")
        }
    };
    for activity in index.work.get(&node).into_iter().flatten() {
        rows.push((Row::Work(activity), level, number(&mut place)));
    }
    for child in index.children.get(&node).into_iter().flatten() {
        let at = number(&mut place);
        let row = rows.len();
        rows.push((Row::Node(child), level, at.clone()));
        outline(index, child.id, &at, level + 1, seen, rows, notes);
        // A summary task's dates roll up from the tasks under it, so one with
        // none has nothing to show.
        if rows.len() == row + 1 {
            rows.pop();
            place -= 1;
            notes.add(
                "empty-wbs",
                "a WBS node with no activities under it was left out",
                &child.name,
            );
        }
    }
}

fn note_what_project_lacks(activity: &m::Activity, notes: &mut Notes) {
    let name = activity.name.as_str();
    match activity.activity_type {
        m::ActivityType::LevelOfEffort => notes.add(
            "level-of-effort",
            "a level of effort became a plain task, so it won't stretch with its links",
            name,
        ),
        m::ActivityType::WbsSummary => notes.add(
            "wbs-summary",
            "a WBS summary activity became a plain task, so it won't span its node",
            name,
        ),
        m::ActivityType::ResourceDependent => notes.add(
            "resource-dependent",
            "resource-dependent work became a plain task on its own calendar",
            name,
        ),
        m::ActivityType::StartMilestone => notes.add(
            "start-milestone",
            "MS Project XML has one kind of milestone, so a start milestone can move to the finish before",
            name,
        ),
        _ => {}
    }
    if activity.suspend.is_some() || activity.resume.is_some() {
        notes.add(
            "suspended",
            "suspend and resume dates were left out, so the work reads as one unbroken span",
            name,
        );
    }
    if activity.secondary_constraint.is_some() {
        notes.add(
            "secondary-constraint",
            "MS Project XML holds one constraint on a task, so the secondary was left out",
            name,
        );
    }
    if activity.expected_finish.is_some() {
        notes.add("expected-finish", "an expected finish was left out", name);
    }
    if activity.primary_constraint.is_some_and(|c| {
        matches!(
            c.kind,
            m::ConstraintType::StartOn | m::ConstraintType::FinishOn
        )
    }) {
        notes.add(
            "start-on",
            "a Start On or Finish On constraint became Project's Must Start or Must Finish On",
            name,
        );
    }
}

/// The constraint to write. Project's need a date, except as late as possible.
fn usable_constraint(activity: &m::Activity, notes: &mut Notes) -> Option<m::Constraint> {
    let held = activity.primary_constraint?;
    if held.date.is_some() || held.kind == m::ConstraintType::AsLateAsPossible {
        return Some(held);
    }
    notes.add(
        "constraint-no-date",
        "a constraint with no date was left out",
        &activity.name,
    );
    None
}

fn task(
    out: &mut Out,
    links: &[&m::Relationship],
    activity: &m::Activity,
    (uid, level, number): (usize, usize, &str),
    numbers: &Numbers,
    notes: &mut Notes,
) {
    note_what_project_lacks(activity, notes);
    let constraint = usable_constraint(activity, notes);
    // MSPDI is a sequence, so this order is the schema's, not a choice.
    out.open("Task");
    out.field("UID", uid);
    out.field("ID", uid);
    out.field("Name", &activity.name);
    out.field("OutlineNumber", number);
    out.field("OutlineLevel", level);
    // P6 stores the data date as the early dates of work that's underway.
    let stored = &activity.stored;
    let start = activity.actual_start.or(stored.early_start);
    let finish = activity.actual_finish.or(stored.early_finish);
    out.date("Start", start.or(activity.planned_start));
    out.date("Finish", finish.or(activity.planned_finish));
    out.field("Duration", span(activity.original_duration));
    out.field("Milestone", u8::from(activity.activity_type.is_milestone()));
    out.date("ActualStart", activity.actual_start);
    out.date("ActualFinish", activity.actual_finish);
    out.field("RemainingDuration", span(activity.remaining_duration));
    if let Some(held) = constraint {
        out.field("ConstraintType", constraint_code(held.kind));
    }
    if let Some(calendar) = numbers.calendars.get(&activity.calendar) {
        out.field("CalendarUID", calendar);
    } else {
        notes.add(
            "missing-calendar",
            "an activity's calendar isn't in the file, so Project will use its default",
            &activity.name,
        );
    }
    if let Some(held) = constraint {
        out.date("ConstraintDate", held.date);
    }
    for link in links.iter().copied() {
        // Links reaching outside are noted once, in `note_outside_links`.
        let Some(predecessor) = numbers.activities.get(&link.predecessor) else {
            continue;
        };
        out.open("PredecessorLink");
        out.field("PredecessorUID", predecessor);
        out.field("Type", link_code(link.kind));
        out.field("LinkLag", link.lag.get().saturating_mul(10));
        out.field("LagFormat", 7);
        out.close("PredecessorLink");
    }
    out.close("Task");
}

fn resources(out: &mut Out, schedule: &m::Schedule, numbers: &Numbers, notes: &mut Notes) {
    let uids: BTreeMap<m::ResourceId, usize> = schedule
        .resources
        .keys()
        .enumerate()
        .map(|(index, id)| (*id, index + 1))
        .collect();
    out.open("Resources");
    for (id, resource) in &schedule.resources {
        out.open("Resource");
        out.field("UID", uids[id]);
        out.field("ID", uids[id]);
        out.field("Name", &resource.name);
        let kind = match resource.kind {
            m::ResourceType::Material => 0,
            m::ResourceType::Labor | m::ResourceType::Nonlabor => 1,
        };
        if resource.kind == m::ResourceType::Nonlabor {
            notes.add(
                "nonlabor",
                "a nonlabor resource became a work resource",
                &resource.name,
            );
        }
        out.field("Type", kind);
        out.field("Initials", &resource.short_name);
        let runs = limit(resource, notes);
        if let [(_, only)] = runs.as_slice() {
            out.field("MaxUnits", only);
        }
        let calendar = resource.calendar.and_then(|c| numbers.calendars.get(&c));
        if let Some(calendar) = calendar {
            out.field("CalendarUID", calendar);
        }
        if runs.len() > 1 {
            availability(out, &runs);
        }
        out.close("Resource");
    }
    out.close("Resources");
    out.open("Assignments");
    let assigned = schedule.assignments.values();
    for (uid, assignment) in (1_usize..).zip(assigned) {
        let Some(resource_id) = assignment.resource else {
            notes.add(
                "role-assignments",
                "an assignment by role with no resource, which MS Project has no place for, was left out",
                schedule
                    .activities
                    .get(&assignment.activity)
                    .map_or("", |a| a.name.as_str()),
            );
            continue;
        };
        let (Some(task), Some(resource)) = (
            numbers.activities.get(&assignment.activity),
            uids.get(&resource_id),
        ) else {
            continue;
        };
        let material = schedule
            .resources
            .get(&resource_id)
            .is_some_and(|r| r.kind == m::ResourceType::Material);
        let a = &assignment.amounts;
        if material || a.budgeted_units.is_none() {
            notes.add(
                "assignment-units",
                "an assignment came across without its units",
                schedule
                    .activities
                    .get(&assignment.activity)
                    .map_or("", |a| a.name.as_str()),
            );
        }
        out.open("Assignment");
        out.field("UID", uid);
        out.field("TaskUID", task);
        out.field("ResourceUID", resource);
        amounts(out, &assignment.amounts, material);
        out.close("Assignment");
    }
    out.close("Assignments");
    for x in schedule.expenses.values() {
        notes.add(
            "expenses",
            "an expense, which this export doesn't carry yet, was left out",
            schedule
                .activities
                .get(&x.activity)
                .map_or("", |a| a.name.as_str()),
        );
    }
}

/// A resource's limit over time (TF-001): the limit in force before every
/// dated rate, then each change, with the moment it takes effect. Empty when
/// there's no limit, or one MS Project can't hold, which the notes list.
fn limit(resource: &m::Resource, notes: &mut Notes) -> Vec<(Option<m::DateTime>, f64)> {
    let mut changes = resource
        .rates
        .iter()
        .filter_map(|r| r.effective)
        .collect::<Vec<_>>();
    changes.sort_unstable();
    changes.dedup();
    let at = |when| resource.rate_at(when).and_then(|r| r.max_units_per_hour);
    let mut runs: Vec<(Option<m::DateTime>, Option<f64>)> = vec![(None, at(None))];
    for when in changes {
        let limit = at(Some(when));
        if runs.last().is_some_and(|(_, last)| *last != limit) {
            runs.push((Some(when), limit));
        }
    }
    if runs.iter().all(|(_, limit)| limit.is_none()) {
        return Vec::new();
    }
    let (kind, why) = if resource.kind == m::ResourceType::Material {
        (
            "material-limit",
            "a material resource's limit, which MS Project has no place for, was left out",
        )
    } else if runs.iter().any(|(_, limit)| limit.is_none()) {
        (
            "partial-limit",
            "a resource's limit, which is unset for part of the time, was left out",
        )
    } else {
        return runs
            .into_iter()
            .filter_map(|(when, limit)| Some((when, limit?)))
            .collect();
    };
    notes.add(kind, why, &resource.name);
    Vec::new()
}

/// A limit that changes over time, as availability periods: each from its
/// moment to the one before the next, the first and last open-ended, with
/// the dates MS Project writes for "NA" (TF-003).
fn availability(out: &mut Out, runs: &[(Option<m::DateTime>, f64)]) {
    let (y, mo, d) = crate::msproject::NA_FROM;
    let open_from = m::DateTime::from_ymd_hm(y, mo, d, 0, 0);
    let (y, mo, d) = crate::msproject::NA_TO;
    let open_to = m::DateTime::from_ymd_hm(y, mo, d, 23, 59);
    out.open("AvailabilityPeriods");
    for (at, (from, limit)) in runs.iter().enumerate() {
        let to = runs
            .get(at + 1)
            .and_then(|(next, _)| next.and_then(|n| n.checked_add(m::Minutes::new(-1))));
        out.open("AvailabilityPeriod");
        out.date("AvailableFrom", if at == 0 { open_from } else { *from });
        out.date("AvailableTo", to.or(open_to));
        out.field("AvailableUnits", limit);
        out.close("AvailabilityPeriod");
    }
    out.close("AvailabilityPeriods");
}

/// An assignment's work, units and costs, in the schema's order: work as a
/// duration for labor and nonlabor, costs in hundredths.
fn amounts(out: &mut Out, a: &m::Amounts, material: bool) {
    let work = |hours: Option<f64>| {
        // To the second, since work need not be whole minutes.
        #[allow(
            clippy::cast_possible_truncation,
            reason = "hours of work as whole seconds, far inside i64"
        )]
        hours.filter(|h| h.is_finite() && !material).map(|h| {
            let total = ((h * 3_600.0).round() as i64).max(0);
            format!("PT{}H{}M{}S", total / 3_600, total / 60 % 60, total % 60)
        })
    };
    let money = |cost: Option<f64>| {
        cost.filter(|c| c.is_finite())
            .map(|c| format!("{}", (c * 100.0).round()))
    };
    for (name, value) in [
        ("ActualCost", money(a.actual_cost)),
        ("ActualWork", work(a.actual_units)),
        ("Cost", money(a.budgeted_cost)),
        ("RemainingCost", money(a.remaining_cost)),
        ("RemainingWork", work(a.remaining_units)),
        (
            "Units",
            a.budgeted_units_per_hour
                .filter(|u| u.is_finite())
                .map(|u| format!("{u}")),
        ),
        ("Work", work(a.budgeted_units)),
    ] {
        if let Some(value) = value {
            out.field(name, value);
        }
    }
}

fn note_project(project: &m::Project, notes: &mut Notes) {
    if let Some(date) = project.must_finish_by {
        notes.add(
            "must-finish-by",
            "MS Project XML has no must-finish date for a project scheduled from its start, so it was left out",
            &date.to_string(),
        );
    }
    notes.add(
        "scheduling-options",
        "the XER file's scheduling options were left out, such as the lag calendar and retained logic",
        &project.short_name,
    );
}

/// A link is in the outside's hands if either end is, so each is walked here.
fn note_outside_links(schedule: &m::Schedule, numbers: &Numbers, notes: &mut Notes) {
    for link in schedule.relationships.values() {
        let held = [link.predecessor, link.successor];
        let inside = held.map(|id| numbers.activities.contains_key(&id));
        if inside != [true, true] {
            let name = held
                .iter()
                .find_map(|id| schedule.activities.get(id))
                .map_or("", |a| a.name.as_str());
            notes.add(
                "other-project",
                "a link to another project was left out",
                name,
            );
        }
    }
}

/// Writes the task list, and says which task each activity became.
fn tasks(
    out: &mut Out,
    schedule: &m::Schedule,
    calendars: BTreeMap<m::CalendarId, usize>,
    notes: &mut Notes,
) -> Numbers {
    let shape = Index::new(schedule);
    let mut rows = Vec::new();
    let mut seen = BTreeSet::new();
    let roots: Vec<m::WbsId> = schedule
        .wbs
        .values()
        .filter(|w| w.is_project_node)
        .map(|w| w.id)
        .collect();
    for root in roots {
        outline(&shape, root, "", 1, &mut seen, &mut rows, notes);
    }
    let numbers = Numbers {
        activities: rows
            .iter()
            .enumerate()
            .filter_map(|(index, (row, ..))| match row {
                Row::Work(a) => Some((a.id, index + 1)),
                Row::Node(_) => None,
            })
            .collect(),
        calendars,
    };
    for (id, activity) in &schedule.activities {
        if !numbers.activities.contains_key(id) {
            notes.add(
                "outside-wbs",
                "an activity outside the project's WBS was left out",
                &activity.name,
            );
        }
    }
    if !rows.is_empty() {
        notes.add(
            "activity-ids",
            "Activity IDs and WBS codes were left out; tasks are numbered by their place in the outline",
            &schedule.project.short_name,
        );
    }
    out.open("Tasks");
    for (index, (row, level, number)) in rows.iter().enumerate() {
        match row {
            Row::Node(node) => {
                out.open("Task");
                out.field("UID", index + 1);
                out.field("ID", index + 1);
                out.field("Name", &node.name);
                out.field("OutlineNumber", number);
                out.field("OutlineLevel", level);
                out.field("Summary", 1);
                out.close("Task");
            }
            Row::Work(activity) => task(
                out,
                shape
                    .links_into
                    .get(&activity.id)
                    .map_or(&[], Vec::as_slice),
                activity,
                (index + 1, *level, number),
                &numbers,
                notes,
            ),
        }
    }
    out.close("Tasks");
    numbers
}

/// Writes one schedule as MS Project XML.
#[must_use]
pub fn write(schedule: &m::Schedule) -> Exported {
    let mut notes = Notes::default();
    let mut out = Out {
        text: "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n".to_owned(),
        depth: 0,
    };
    let project = &schedule.project;
    let name = project.name.as_deref().unwrap_or(&project.short_name);
    out.text
        .push_str("<Project xmlns=\"http://schemas.microsoft.com/project\">\n");
    out.depth = 1;
    // The schema needs it first: the Project 2010 format, which this follows.
    out.field("SaveVersion", 14);
    out.field("Title", name);
    out.field("ScheduleFromStart", 1);
    out.date("StartDate", project.planned_start.or(project.data_date));
    out.date("FinishDate", project.stored.scheduled_finish);
    // The schema needs a currency, and the model doesn't hold one yet.
    out.field("CurrencyCode", "USD");
    if schedule.assignments.values().any(|a| {
        let a = &a.amounts;
        [a.budgeted_cost, a.actual_cost, a.remaining_cost]
            .iter()
            .any(|c| c.is_some_and(|c| c != 0.0))
    }) {
        notes.add(
            "currency",
            "costs are written as US dollars, since the file needs a currency and TotalFloat doesn't record one",
            &project.short_name,
        );
    }
    let calendars: BTreeMap<m::CalendarId, usize> = schedule
        .calendars
        .keys()
        .enumerate()
        .map(|(index, id)| (*id, index + 1))
        .collect();
    if let Some(uid) = project.default_calendar.and_then(|id| calendars.get(&id)) {
        out.field("CalendarUID", uid);
    }
    if let Some(day) = project
        .default_calendar
        .and_then(|id| schedule.calendars.get(&id))
        .and_then(|c| c.hours_per_period.day)
    {
        out.field("MinutesPerDay", day.get());
    }
    out.date("StatusDate", project.data_date);
    out.open("Calendars");
    for (id, held) in &schedule.calendars {
        calendar(&mut out, calendars[id], held);
    }
    out.close("Calendars");
    let numbers = tasks(&mut out, schedule, calendars, &mut notes);
    note_project(project, &mut notes);
    note_outside_links(schedule, &numbers, &mut notes);
    resources(&mut out, schedule, &numbers, &mut notes);
    out.text.push_str("</Project>\n");
    Exported {
        text: out.text,
        notes: notes.finish(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::msproject::read;

    const PLANT: &str = include_str!("../../../fixtures/msproject/plant.xml");

    fn shape(schedule: &m::Schedule) -> Vec<(String, i64, m::ActivityType)> {
        let mut shape: Vec<_> = schedule
            .activities
            .values()
            .map(|a| (a.name.clone(), a.original_duration.get(), a.activity_type))
            .collect();
        shape.sort();
        shape
    }

    fn links(schedule: &m::Schedule) -> Vec<(String, String, m::RelationshipType, i64)> {
        let name = |id: m::ActivityId| schedule.activities[&id].name.clone();
        let mut links: Vec<_> = schedule
            .relationships
            .values()
            .map(|r| (name(r.predecessor), name(r.successor), r.kind, r.lag.get()))
            .collect();
        links.sort();
        links
    }

    #[test]
    fn a_round_trip_keeps_the_schedule() {
        let first = read(PLANT).unwrap().schedule;
        let written = write(&first);
        let second = read(&written.text).unwrap().schedule;
        assert_eq!(shape(&first), shape(&second));
        assert_eq!(links(&first), links(&second));
        assert_eq!(first.wbs.len(), second.wbs.len());
        assert_eq!(first.project.data_date, second.project.data_date);
        let constraints = |s: &m::Schedule| {
            let mut held: Vec<_> = s
                .activities
                .values()
                .map(|a| (a.name.clone(), a.primary_constraint))
                .collect();
            held.sort_by(|a, b| a.0.cmp(&b.0));
            held
        };
        assert_eq!(constraints(&first), constraints(&second));
        let progress = |s: &m::Schedule| {
            let mut held: Vec<_> = s
                .activities
                .values()
                .map(|a| (a.name.clone(), a.status, a.remaining_duration.get()))
                .collect();
            held.sort();
            held
        };
        assert_eq!(progress(&first), progress(&second));
        let days = |s: &m::Schedule| {
            s.calendars
                .values()
                .map(|c| (c.name.clone(), c.work_week.clone(), c.exceptions.clone()))
                .collect::<Vec<_>>()
        };
        assert_eq!(days(&first), days(&second));
    }

    #[test]
    fn text_is_escaped() {
        let mut schedule = read(PLANT).unwrap().schedule;
        schedule.activities.values_mut().next().unwrap().name = "A <&> \"B\"".to_owned();
        let written = write(&schedule);
        assert!(written.text.contains("A &lt;&amp;&gt; &quot;B&quot;"));
        let back = read(&written.text).unwrap().schedule;
        assert!(back.activities.values().any(|a| a.name == "A <&> \"B\""));
    }

    #[test]
    fn what_project_lacks_is_reported() {
        let mut schedule = read(PLANT).unwrap().schedule;
        let a = schedule.activities.values_mut().next().unwrap();
        a.activity_type = m::ActivityType::LevelOfEffort;
        a.expected_finish = "2026-05-01 17:00".parse().ok();
        let kinds: Vec<&str> = write(&schedule).notes.iter().map(|n| n.kind).collect();
        assert!(kinds.contains(&"level-of-effort"), "{kinds:?}");
        assert!(kinds.contains(&"expected-finish"), "{kinds:?}");
        assert!(kinds.contains(&"activity-ids"), "{kinds:?}");
        assert!(!kinds.contains(&"expenses"), "{kinds:?}");
        let activity = *schedule.activities.keys().next().unwrap();
        let project = schedule.project.id;
        schedule.expenses.insert(
            m::ExpenseId(1),
            m::Expense {
                id: m::ExpenseId(1),
                activity,
                project,
                name: "Permit".to_owned(),
                cost_account: None,
                category: None,
                accrual: Some(m::Accrual::Uniform),
                budgeted_cost: Some(1500.0),
                actual_cost: Some(0.0),
                remaining_cost: Some(1500.0),
                price_per_unit: Some(1500.0),
                budgeted_units: Some(1.0),
                unit_of_measure: String::new(),
                auto_compute_actuals: false,
                po_number: String::new(),
                vendor: String::new(),
                description: String::new(),
                source: None,
            },
        );
        let kinds: Vec<&str> = write(&schedule).notes.iter().map(|n| n.kind).collect();
        assert!(kinds.contains(&"expenses"), "{kinds:?}");
    }

    /// `MSPDI` is a sequence, and the reader goes by name, so a round trip can't see this.
    #[test]
    fn elements_follow_the_schema_order() {
        const ORDERS: [(&str, &[&str]); 5] = [
            (
                "Task",
                &[
                    "UID",
                    "ID",
                    "Name",
                    "OutlineNumber",
                    "OutlineLevel",
                    "Start",
                    "Finish",
                    "Duration",
                    "Milestone",
                    "Summary",
                    "ActualStart",
                    "ActualFinish",
                    "RemainingDuration",
                    "ConstraintType",
                    "CalendarUID",
                    "ConstraintDate",
                    "PredecessorLink",
                ],
            ),
            (
                "Resource",
                &[
                    "UID",
                    "ID",
                    "Name",
                    "Type",
                    "Initials",
                    "MaxUnits",
                    "CalendarUID",
                    "AvailabilityPeriods",
                ],
            ),
            (
                "Exception",
                &[
                    "TimePeriod",
                    "Occurrences",
                    "Type",
                    "DayWorking",
                    "WorkingTimes",
                ],
            ),
            (
                "Assignment",
                &[
                    "UID",
                    "TaskUID",
                    "ResourceUID",
                    "ActualCost",
                    "ActualWork",
                    "Cost",
                    "RemainingCost",
                    "RemainingWork",
                    "Units",
                    "Work",
                ],
            ),
            (
                "Project",
                &[
                    "SaveVersion",
                    "Title",
                    "ScheduleFromStart",
                    "StartDate",
                    "FinishDate",
                    "CurrencyCode",
                    "CalendarUID",
                    "MinutesPerDay",
                    "StatusDate",
                    "Calendars",
                    "Tasks",
                    "Resources",
                    "Assignments",
                ],
            ),
        ];
        let mut schedule = with_limits(read(PLANT).unwrap().schedule);
        for activity in schedule.activities.values_mut() {
            activity.primary_constraint = Some(m::Constraint {
                kind: m::ConstraintType::StartOnOrAfter,
                date: "2026-03-02 08:00".parse().ok(),
            });
        }
        let text = write(&schedule).text;
        for (element, order) in ORDERS {
            let found = children_of(&text, element);
            assert!(!found.is_empty(), "no {element} was written");
            for children in found {
                let places = children.iter().map(|name| {
                    let place = order.iter().position(|o| o == name);
                    place.unwrap_or_else(|| panic!("{element}: {name} isn't listed"))
                });
                let places: Vec<usize> = places.collect();
                assert!(places.is_sorted(), "{element}: {children:?}");
            }
        }
    }

    /// The names under each `element`, going by the file's indentation.
    fn children_of(text: &str, element: &str) -> Vec<Vec<String>> {
        let tag = |line: &str| {
            let name = line.trim_start().strip_prefix('<')?;
            name.split(['>', ' ']).next().map(str::to_owned)
        };
        let indent = |line: &str| line.len() - line.trim_start().len();
        let lines: Vec<&str> = text.lines().collect();
        let opened = lines
            .iter()
            .enumerate()
            .filter(|(_, l)| tag(l).as_deref() == Some(element));
        opened
            .map(|(at, line)| {
                let depth = indent(line);
                let body = lines[at + 1..].iter().take_while(|l| indent(l) > depth);
                body.filter(|l| indent(l) == depth + 2)
                    .filter_map(|l| tag(l))
                    .filter(|name| !name.starts_with('/'))
                    .collect()
            })
            .collect()
    }

    #[test]
    fn finished_work_shows_its_actual_dates() {
        let mut schedule = read(PLANT).unwrap().schedule;
        let a = schedule.activities.values_mut().next().unwrap();
        a.actual_start = "2026-01-05 08:00".parse().ok();
        a.actual_finish = "2026-01-09 17:00".parse().ok();
        a.stored.early_start = "2026-03-02 08:00".parse().ok();
        a.stored.early_finish = "2026-03-02 08:00".parse().ok();
        let text = write(&schedule).text;
        assert!(
            text.contains("<Start>2026-01-05T08:00:00</Start>"),
            "{text}"
        );
        assert!(
            text.contains("<Finish>2026-01-09T17:00:00</Finish>"),
            "{text}"
        );
    }

    #[test]
    fn links_out_of_the_project_either_way_are_reported() {
        let held = read(PLANT).unwrap().schedule;
        for outside in [true, false] {
            let mut schedule = held.clone();
            let link = schedule.relationships.values_mut().next().unwrap();
            if outside {
                link.successor = m::ActivityId(u64::MAX);
            } else {
                link.predecessor = m::ActivityId(u64::MAX);
            }
            let notes = write(&schedule).notes;
            let note = notes.iter().find(|n| n.kind == "other-project");
            assert_eq!(note.map(|n| n.count), Some(1), "{notes:?}");
        }
    }

    #[test]
    fn a_translation_that_loses_something_says_so() {
        let mut schedule = read(PLANT).unwrap().schedule;
        schedule.project.must_finish_by = "2026-05-01 17:00".parse().ok();
        let a = schedule.activities.values_mut().next().unwrap();
        a.activity_type = m::ActivityType::StartMilestone;
        a.suspend = "2026-03-20 17:00".parse().ok();
        for activity in schedule.activities.values_mut() {
            activity.primary_constraint = Some(m::Constraint {
                kind: m::ConstraintType::StartOnOrAfter,
                date: None,
            });
        }
        schedule.resources.values_mut().next().unwrap().kind = m::ResourceType::Nonlabor;
        let written = write(&schedule);
        let kinds: Vec<&str> = written.notes.iter().map(|n| n.kind).collect();
        for kind in [
            "must-finish-by",
            "start-milestone",
            "suspended",
            "constraint-no-date",
            "nonlabor",
            "scheduling-options",
        ] {
            assert!(kinds.contains(&kind), "{kind} in {kinds:?}");
        }
        assert!(
            !written.text.contains("<ConstraintType>"),
            "{}",
            written.text
        );
    }

    #[test]
    fn a_wbs_node_with_nothing_under_it_is_left_out() {
        let mut schedule = read(PLANT).unwrap().schedule;
        let root = schedule.wbs.values().find(|w| w.is_project_node).unwrap();
        let mut empty = root.clone();
        empty.id = m::WbsId(u64::MAX);
        empty.parent = Some(root.id);
        empty.is_project_node = false;
        empty.name = "Nothing here".to_owned();
        schedule.wbs.insert(empty.id, empty);
        let written = write(&schedule);
        assert!(!written.text.contains("Nothing here"), "{}", written.text);
        let kinds: Vec<&str> = written.notes.iter().map(|n| n.kind).collect();
        assert!(kinds.contains(&"empty-wbs"), "{kinds:?}");
    }

    #[test]
    fn a_missing_calendar_is_reported() {
        let mut schedule = read(PLANT).unwrap().schedule;
        schedule.activities.values_mut().next().unwrap().calendar = m::CalendarId(u64::MAX);
        let kinds: Vec<&str> = write(&schedule).notes.iter().map(|n| n.kind).collect();
        assert!(kinds.contains(&"missing-calendar"), "{kinds:?}");
    }

    /// One resource with a limit over time, one with the same limit throughout.
    fn with_limits(mut schedule: m::Schedule) -> m::Schedule {
        let crew = schedule.resources.values_mut().next().unwrap();
        crew.rates = limits(&[
            (Some("2026-03-02 00:00"), 1.0),
            (Some("2026-04-01 00:00"), 2.0),
        ]);
        let mut steady = crew.clone();
        steady.id = m::ResourceId(steady.id.0 + 1);
        "S".clone_into(&mut steady.short_name);
        steady.rates = limits(&[(None, 2.0)]);
        schedule.resources.insert(steady.id, steady);
        schedule
    }

    /// Rates holding these limits, from these moments.
    fn limits(limits: &[(Option<&str>, f64)]) -> Vec<m::ResourceRate> {
        (1..)
            .zip(limits)
            .map(|(id, (from, limit))| m::ResourceRate {
                id: m::ResourceRateId(id),
                effective: from.map(|f| f.parse().unwrap()),
                prices: [None; 5],
                max_units_per_hour: Some(*limit),
                source: None,
            })
            .collect()
    }

    fn crew_limits(schedule: &m::Schedule) -> Vec<(Option<String>, Option<f64>)> {
        let crew = schedule.resources.values().next().unwrap();
        crew.rates
            .iter()
            .map(|r| (r.effective.map(|e| e.to_string()), r.max_units_per_hour))
            .collect()
    }

    /// A limit that holds throughout is `MaxUnits`; one that changes is
    /// availability periods, open before the first change and after the
    /// last. Both read back as they were (TF-003).
    #[test]
    fn a_resource_s_limit_round_trips() {
        let mut schedule = read(PLANT).unwrap().schedule;
        let crew = schedule.resources.values_mut().next().unwrap();
        crew.rates = limits(&[(Some("2026-03-02 00:00"), 2.0)]);
        let text = write(&schedule).text;
        assert!(text.contains("<MaxUnits>2</MaxUnits>"), "{text}");
        assert!(!text.contains("AvailabilityPeriods"), "{text}");
        let back = read(&text).unwrap().schedule;
        assert_eq!(
            crew_limits(&back),
            [(Some("2026-03-02 00:00".to_owned()), Some(2.0))]
        );

        let crew = schedule.resources.values_mut().next().unwrap();
        crew.rates = limits(&[
            (Some("2026-03-02 00:00"), 2.0),
            (Some("2026-04-01 00:00"), 3.0),
            (Some("2026-05-01 00:00"), 3.0),
        ]);
        let text = write(&schedule).text;
        assert!(!text.contains("<MaxUnits>"), "{text}");
        let periods = children_of(&text, "AvailabilityPeriod");
        assert_eq!(
            periods,
            [["AvailableFrom", "AvailableTo", "AvailableUnits"]; 2]
        );
        for date in [
            "<AvailableFrom>1984-01-01T00:00:00</AvailableFrom>",
            "<AvailableTo>2026-03-31T23:59:00</AvailableTo>",
            "<AvailableTo>2049-12-31T23:59:00</AvailableTo>",
        ] {
            assert!(text.contains(date), "{text}");
        }
        assert!(
            text.contains("<AvailableFrom>2026-04-01T00:00:00</AvailableFrom>"),
            "{text}"
        );
        let back = read(&text).unwrap().schedule;
        assert_eq!(
            crew_limits(&back),
            [
                (Some("2026-03-02 00:00".to_owned()), Some(2.0)),
                (Some("2026-04-01 00:00".to_owned()), Some(3.0)),
            ]
        );
    }

    /// A limit MS Project can't hold is left out and listed.
    #[test]
    fn a_limit_ms_project_can_t_hold_is_reported() {
        let mut schedule = read(PLANT).unwrap().schedule;
        let crew = schedule.resources.values_mut().next().unwrap();
        crew.rates = limits(&[
            (Some("2026-03-02 00:00"), 2.0),
            (Some("2026-04-01 00:00"), 1.0),
        ]);
        crew.rates[1].max_units_per_hour = None;
        let written = write(&schedule);
        assert!(!written.text.contains("MaxUnits"));
        assert!(!written.text.contains("AvailabilityPeriods"));
        let kinds: Vec<&str> = written.notes.iter().map(|n| n.kind).collect();
        assert!(kinds.contains(&"partial-limit"), "{kinds:?}");

        let crew = schedule.resources.values_mut().next().unwrap();
        crew.rates = limits(&[(None, 2.0)]);
        crew.kind = m::ResourceType::Material;
        let written = write(&schedule);
        assert!(!written.text.contains("MaxUnits"));
        let kinds: Vec<&str> = written.notes.iter().map(|n| n.kind).collect();
        assert!(kinds.contains(&"material-limit"), "{kinds:?}");
    }

    #[test]
    fn midnight_shifts_round_trip() {
        let mut schedule = read(PLANT).unwrap().schedule;
        let day = m::WorkInterval::new(m::TimeOfDay::MIDNIGHT, m::TimeOfDay::END_OF_DAY).unwrap();
        let calendar = schedule.calendars.values_mut().next().unwrap();
        *calendar.work_week.day_mut(m::Weekday::Sunday) = vec![day];
        let back = read(&write(&schedule).text).unwrap().schedule;
        let sunday = back.calendars.values().next().unwrap();
        assert_eq!(sunday.work_week.day(m::Weekday::Sunday), [day]);
    }
}
