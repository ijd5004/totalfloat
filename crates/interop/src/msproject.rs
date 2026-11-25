//! MS Project XML (`MSPDI`), the format Project saves as "XML".
//!
//! Tasks, links, constraints, calendars and resources come across, and summary
//! tasks become WBS nodes. Anything that can't translate exactly is a [`Note`]:
//! The file interop decision (ARCHITECTURE.md) makes that report part of the contract. Only what a scheduler sees
//! in Project is read, from the published schema's field names.

use std::collections::{BTreeMap, BTreeSet};

use totalfloat_model as m;

use crate::xml::{self, Element};
use crate::{ImportError, Note, Notes};

/// What a file gave, and what it couldn't.
#[derive(Debug)]
pub struct Imported {
    /// The project, in TotalFloat's model.
    pub schedule: m::Schedule,
    /// What didn't translate exactly.
    pub notes: Vec<Note>,
}

const ROOT_WBS: u64 = 1;
/// Longest project code, as for a WBS code.
const MAX_CODE: usize = 40;
/// Longest activity name, the longest in the corpus.
const MAX_NAME: usize = 120;
/// Longest WBS name, the longest in the corpus.
const MAX_WBS_NAME: usize = 100;
/// Days an exception can cover, so a bad range can't fill memory.
const MAX_EXCEPTION_DAYS: i32 = 3660;
/// Duration and lag formats that count every hour of every day.
const ELAPSED: [i64; 10] = [4, 6, 8, 10, 12, 36, 38, 40, 42, 44];
/// Lag formats that are a percent of the predecessor.
const PERCENT: [i64; 4] = [19, 20, 51, 52];
/// A calendar for elapsed work, which runs every hour of every day.
const ELAPSED_CALENDAR: u64 = 2_000_000_000;

fn bad(what: &'static str, value: &str) -> ImportError {
    ImportError::BadValue {
        what,
        value: value.to_owned(),
    }
}

/// `2026-03-02T08:00:00` as a date and time.
fn date_time(what: &'static str, text: &str) -> Result<m::DateTime, ImportError> {
    let minutes = text.get(..16).unwrap_or(text).replace('T', " ");
    minutes.parse().map_err(|_| bad(what, text))
}

fn optional_date(e: &Element, name: &'static str) -> Result<Option<m::DateTime>, ImportError> {
    e.text_of(name)
        .map(|text| date_time(name, text))
        .transpose()
}

/// `PT8H30M0S`, with an optional day count, as minutes.
fn duration(what: &'static str, text: &str) -> Result<m::Minutes, ImportError> {
    let seconds = seconds_of(what, text)?;
    #[allow(
        clippy::cast_possible_truncation,
        reason = "rounded to minutes, far inside an i64 for any real duration"
    )]
    Ok(m::Minutes::new((seconds / 60.0).round() as i64))
}

/// `PT8H30M0S`, with an optional day count, as seconds.
fn seconds_of(what: &'static str, text: &str) -> Result<f64, ImportError> {
    let rest = text.strip_prefix('P').ok_or_else(|| bad(what, text))?;
    let (mut seconds, mut number) = (0.0_f64, String::new());
    for letter in rest.chars() {
        match letter {
            'T' => {}
            '0'..='9' | '.' => number.push(letter),
            'D' | 'H' | 'M' | 'S' => {
                let value: f64 = number.parse().map_err(|_| bad(what, text))?;
                seconds += value
                    * match letter {
                        'D' => 86_400.0,
                        'H' => 3_600.0,
                        'M' => 60.0,
                        _ => 1.0,
                    };
                number.clear();
            }
            _ => return Err(bad(what, text)),
        }
    }
    if !number.is_empty() {
        return Err(bad(what, text));
    }
    Ok(seconds)
}

fn time(text: &str) -> Option<m::TimeOfDay> {
    let mut parts = text.split(':');
    let hour: u16 = parts.next()?.parse().ok()?;
    let minute: u16 = parts.next()?.parse().ok()?;
    m::TimeOfDay::from_minutes(hour.checked_mul(60)?.checked_add(minute)?)
}

fn shifts(day: &Element, notes: &mut Notes) -> Vec<m::WorkInterval> {
    let Some(times) = day.child("WorkingTimes") else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for shift in times.all("WorkingTime") {
        let read = (|| {
            let start = time(shift.text_of("FromTime")?)?;
            let finish = time(shift.text_of("ToTime")?)?;
            // Midnight at the end of a shift is the end of the day.
            let finish = if finish.minutes() == 0 {
                m::TimeOfDay::END_OF_DAY
            } else {
                finish
            };
            m::WorkInterval::new(start, finish)
        })();
        match read {
            Some(interval) => found.push(interval),
            None if shift.children.is_empty() => {}
            None => notes.add(
                "unread-shift",
                "a working time couldn't be read and was left out",
                shift.text_of("FromTime").unwrap_or_default(),
            ),
        }
    }
    found.sort();
    found
}

fn weekday(day_type: i64) -> Option<m::Weekday> {
    m::Weekday::ALL
        .iter()
        .copied()
        .find(|day| i64::try_from(day.index()).ok() == Some(day_type - 1))
}

/// Dates a period covers, each with the shifts a working one has.
fn exception_days(
    period: &Element,
    worked: &[m::WorkInterval],
    into: &mut BTreeMap<m::Date, Vec<m::WorkInterval>>,
) -> Result<(), ImportError> {
    let (Some(from), Some(to)) = (period.text_of("FromDate"), period.text_of("ToDate")) else {
        return Ok(());
    };
    let (mut date, last) = (
        date_time("FromDate", from)?.date(),
        date_time("ToDate", to)?.date(),
    );
    if last.days_since(date) > MAX_EXCEPTION_DAYS {
        return Err(bad("an exception's date range", from));
    }
    while date <= last {
        into.insert(date, worked.to_vec());
        match date.checked_add_days(1) {
            Some(next) => date = next,
            None => break,
        }
    }
    Ok(())
}

/// A calendar as the file gives it, before its base fills it in.
#[derive(Default)]
struct Local {
    name: String,
    base: Option<i64>,
    week: BTreeMap<usize, Vec<m::WorkInterval>>,
    exceptions: BTreeMap<m::Date, Vec<m::WorkInterval>>,
}

fn read_calendar(e: &Element, notes: &mut Notes) -> Result<Local, ImportError> {
    let name = one_line(e.text_of("Name").unwrap_or("Imported calendar"));
    let mut local = Local {
        name: clipped(&name, MAX_NAME, "a calendar", notes),
        base: e.number("BaseCalendarUID").filter(|uid| *uid >= 0),
        ..Local::default()
    };
    if e.child("WorkWeeks").is_some_and(|w| !w.children.is_empty()) {
        notes.add(
            "work-weeks",
            "alternate work weeks were left out, so the normal week applies all year",
            &name,
        );
    }
    if let Some(days) = e.child("WeekDays") {
        for day in days.all("WeekDay") {
            let times = if day.flag("DayWorking") {
                shifts(day, notes)
            } else {
                Vec::new()
            };
            match day.number("DayType").unwrap_or(0) {
                0 => {
                    if let Some(period) = day.child("TimePeriod") {
                        exception_days(period, &times, &mut local.exceptions)?;
                    }
                }
                kind => {
                    if let Some(weekday) = weekday(kind) {
                        local.week.insert(weekday.index(), times);
                    }
                }
            }
        }
    }
    if let Some(list) = e.child("Exceptions") {
        for exception in list.all("Exception") {
            read_exception(exception, &name, &mut local, notes)?;
        }
    }
    Ok(local)
}

fn read_exception(
    exception: &Element,
    calendar: &str,
    local: &mut Local,
    notes: &mut Notes,
) -> Result<(), ImportError> {
    let Some(period) = exception.child("TimePeriod") else {
        return Ok(());
    };
    let times = if exception.flag("DayWorking") {
        shifts(exception, notes)
    } else {
        Vec::new()
    };
    // A repeat's period runs from its first date to its last, which isn't one
    // run of days, and its pattern isn't read. So only the first date holds.
    if exception.number("Occurrences").is_some_and(|n| n > 1) {
        notes.add(
            "repeating-exception",
            "only the first date of a repeating calendar exception was kept",
            calendar,
        );
        if let Some(first) = period.text_of("FromDate") {
            local
                .exceptions
                .insert(date_time("FromDate", first)?.date(), times);
        }
        return Ok(());
    }
    exception_days(period, &times, &mut local.exceptions)
}

/// Fills in what a calendar takes from its base, walking up its bases.
fn effective(
    uid: i64,
    locals: &BTreeMap<i64, Local>,
) -> (m::WorkWeek, BTreeMap<m::Date, Vec<m::WorkInterval>>) {
    let mut chain = Vec::new();
    let mut at = Some(uid);
    while let Some(here) = at.filter(|here| !chain.contains(here)) {
        chain.push(here);
        at = locals.get(&here).and_then(|l| l.base);
    }
    let mut week = m::WorkWeek::default();
    let mut exceptions = BTreeMap::new();
    for here in chain.iter().rev() {
        let Some(local) = locals.get(here) else {
            continue;
        };
        for day in m::Weekday::ALL {
            if let Some(times) = local.week.get(&day.index()) {
                week.day_mut(day).clone_from(times);
            }
        }
        exceptions.extend(local.exceptions.clone());
    }
    (week, exceptions)
}

fn even_day(week: &m::WorkWeek) -> Option<m::Minutes> {
    let lengths: BTreeSet<i64> = m::Weekday::ALL
        .iter()
        .map(|day| week.day(*day).iter().map(|s| s.duration().get()).sum())
        .filter(|length: &i64| *length > 0)
        .collect();
    match lengths.iter().collect::<Vec<_>>().as_slice() {
        [one] => Some(m::Minutes::new(**one)),
        _ => None,
    }
}

fn calendars(
    root: &Element,
    schedule: &mut m::Schedule,
    minutes_per_day: Option<i64>,
    notes: &mut Notes,
) -> Result<(), ImportError> {
    let mut locals = BTreeMap::new();
    if let Some(list) = root.child("Calendars") {
        for calendar in list.all("Calendar") {
            let uid = calendar
                .number("UID")
                .ok_or_else(|| bad("a calendar's UID", ""))?;
            locals.insert(uid, read_calendar(calendar, notes)?);
        }
    }
    for (uid, local) in &locals {
        let (work_week, exceptions) = effective(*uid, &locals);
        let id = m::CalendarId(id_of(*uid)?);
        let hours = even_day(&work_week).or(minutes_per_day.map(m::Minutes::new));
        schedule.calendars.insert(
            id,
            m::Calendar {
                id,
                name: local.name.clone(),
                // Bases are already folded in, so each stands alone.
                kind: m::CalendarType::Global,
                project: None,
                base: None,
                is_default: false,
                hours_per_period: m::HoursPerPeriod {
                    day: hours,
                    ..m::HoursPerPeriod::default()
                },
                work_week,
                exceptions,
                source: None,
            },
        );
    }
    Ok(())
}

fn id_of(uid: i64) -> Result<u64, ImportError> {
    u64::try_from(uid)
        .ok()
        .and_then(|uid| uid.checked_add(1))
        .filter(|id| *id < ELAPSED_CALENDAR)
        .ok_or_else(|| bad("a UID", &uid.to_string()))
}

fn clipped(text: &str, limit: usize, what: &'static str, notes: &mut Notes) -> String {
    if text.chars().count() <= limit {
        return text.to_owned();
    }
    notes.add(
        "name-shortened",
        "a name was longer than P6 holds, so it was shortened",
        what,
    );
    text.chars().take(limit).collect()
}

fn one_line(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .trim()
        .to_owned()
}

/// A code from a title: what the flags and P6 can both hold.
fn project_code(title: &str) -> String {
    let code: String = one_line(title)
        .chars()
        .filter(|c| !matches!(c, '=' | '>' | ';'))
        .take(MAX_CODE)
        .collect();
    let code = code.trim().to_owned();
    if code.is_empty() {
        "IMPORTED".to_owned()
    } else {
        code
    }
}

fn constraint(
    task: &Element,
    name: &str,
    notes: &mut Notes,
) -> Result<Option<m::Constraint>, ImportError> {
    use m::ConstraintType as C;
    let kind = match task.number("ConstraintType").unwrap_or(0) {
        0 => return Ok(None),
        1 => C::AsLateAsPossible,
        2 => C::MandatoryStart,
        3 => C::MandatoryFinish,
        4 => C::StartOnOrAfter,
        5 => C::StartOnOrBefore,
        6 => C::FinishOnOrAfter,
        7 => C::FinishOnOrBefore,
        other => return Err(bad("a constraint type", &other.to_string())),
    };
    if !kind.requires_date() {
        return Ok(Some(m::Constraint { kind, date: None }));
    }
    let Some(date) = optional_date(task, "ConstraintDate")? else {
        notes.add(
            "constraint-no-date",
            "a constraint had no date, so it was left off",
            name,
        );
        return Ok(None);
    };
    if kind == C::StartOnOrAfter {
        notes.add(
            "start-no-earlier-than",
            "Start No Earlier Than constraints kept; check each one is meant",
            name,
        );
    }
    Ok(Some(m::Constraint {
        kind,
        date: Some(date),
    }))
}

/// Reads what progress the file records.
fn progress(task: &Element, activity: &mut m::Activity) -> Result<(), ImportError> {
    activity.actual_start = optional_date(task, "ActualStart")?;
    activity.actual_finish = optional_date(task, "ActualFinish")?;
    if activity.activity_type.is_milestone() {
        activity.actual_start = activity.actual_start.or(activity.actual_finish);
    }
    let remaining = task
        .text_of("RemainingDuration")
        .map(|text| duration("RemainingDuration", text))
        .transpose()?;
    activity.status = if activity.actual_finish.is_some() {
        m::ActivityStatus::Completed
    } else if activity.actual_start.is_some() {
        m::ActivityStatus::InProgress
    } else {
        m::ActivityStatus::NotStarted
    };
    activity.remaining_duration = match activity.status {
        m::ActivityStatus::Completed => m::Minutes::ZERO,
        m::ActivityStatus::InProgress => remaining.unwrap_or(activity.original_duration),
        m::ActivityStatus::NotStarted => activity.original_duration,
    };
    Ok(())
}

/// Tasks and WBS nodes, with the summary tasks and left-out tasks noted.
struct TaskTable {
    /// Every UID with an activity behind it.
    activities: BTreeMap<i64, m::ActivityId>,
    /// Summary tasks, whose links aren't carried.
    summaries: BTreeSet<i64>,
}

/// A calendar that works every hour of every day, for elapsed work.
fn elapsed_calendar(schedule: &mut m::Schedule) -> m::CalendarId {
    let id = m::CalendarId(ELAPSED_CALENDAR);
    schedule.calendars.entry(id).or_insert_with(|| {
        let day = m::WorkInterval::new(m::TimeOfDay::MIDNIGHT, m::TimeOfDay::END_OF_DAY);
        let mut week = m::WorkWeek::default();
        for weekday in m::Weekday::ALL {
            week.day_mut(weekday).extend(day);
        }
        m::Calendar {
            id,
            name: "Elapsed 24-Hour".to_owned(),
            kind: m::CalendarType::Global,
            project: None,
            base: None,
            is_default: false,
            hours_per_period: m::HoursPerPeriod {
                day: Some(m::Minutes::new(1440)),
                ..m::HoursPerPeriod::default()
            },
            work_week: week,
            exceptions: BTreeMap::new(),
            source: None,
        }
    });
    id
}

fn note_what_scheduling_loses(task: &Element, name: &str, notes: &mut Notes) {
    let mut note = |field: &str, kind, message| {
        if task.flag(field) {
            notes.add(kind, message, name);
        }
    };
    note(
        "Manual",
        "manual-task",
        "a manually scheduled task is now calculated by its logic",
    );
    note(
        "EffortDriven",
        "effort-driven",
        "an effort-driven task keeps its duration, so its work won't drive it",
    );
    if task.text_of("Deadline").is_some() {
        notes.add("deadline", "a task deadline was left off", name);
    }
    if task.text_of("Stop").is_some() || task.text_of("Resume").is_some() {
        notes.add("split-task", "a split in a task was left out", name);
    }
    if task.number("LevelingDelay").unwrap_or(0) != 0 {
        notes.add("leveling-delay", "a leveling delay was left out", name);
    }
}

/// One non-summary task as an activity.
fn activity(
    task: &Element,
    uid: i64,
    (project, wbs): (m::ProjectId, m::WbsId),
    default_calendar: m::CalendarId,
    schedule: &mut m::Schedule,
    notes: &mut Notes,
) -> Result<m::Activity, ImportError> {
    let name = one_line(task.text_of("Name").unwrap_or("Unnamed task"));
    let id = m::ActivityId(id_of(uid)?);
    let mut calendar = task
        .number("CalendarUID")
        .filter(|uid| *uid >= 0)
        .map(id_of)
        .transpose()?
        .map(m::CalendarId)
        .filter(|c| schedule.calendars.contains_key(c))
        .unwrap_or(default_calendar);
    if task
        .number("DurationFormat")
        .is_some_and(|f| ELAPSED.contains(&f))
    {
        notes.add(
            "elapsed-duration",
            "an elapsed duration now runs on the Elapsed 24-Hour calendar",
            &name,
        );
        calendar = elapsed_calendar(schedule);
    }
    let code = task
        .text_of("ID")
        .map_or_else(|| format!("A{uid}"), |row| format!("A{row}"));
    let mut activity = m::Activity::new(
        id,
        project,
        wbs,
        calendar,
        code,
        clipped(&name, MAX_NAME, "an activity", notes),
    );
    activity.original_duration = task
        .text_of("Duration")
        .map(|text| duration("Duration", text))
        .transpose()?
        .unwrap_or(m::Minutes::ZERO);
    if task.flag("Milestone") {
        if activity.original_duration != m::Minutes::ZERO {
            notes.add(
                "milestone-with-duration",
                "a task marked as a milestone had a duration, which was dropped",
                &name,
            );
        }
        activity.activity_type = m::ActivityType::FinishMilestone;
        activity.original_duration = m::Minutes::ZERO;
    }
    note_what_scheduling_loses(task, &name, notes);
    activity.planned_start = optional_date(task, "Start")?;
    activity.planned_finish = optional_date(task, "Finish")?;
    activity.primary_constraint = constraint(task, &name, notes)?;
    progress(task, &mut activity)?;
    Ok(activity)
}

fn tasks(
    root: &Element,
    schedule: &mut m::Schedule,
    default_calendar: m::CalendarId,
    notes: &mut Notes,
) -> Result<TaskTable, ImportError> {
    let mut table = TaskTable {
        activities: BTreeMap::new(),
        summaries: BTreeSet::new(),
    };
    let project = schedule.project.id;
    let mut outline: Vec<(i64, m::WbsId)> = Vec::new();
    let mut sequence = 0;
    let Some(list) = root.child("Tasks") else {
        return Ok(table);
    };
    for task in list.all("Task") {
        let uid = task.number("UID").ok_or_else(|| bad("a task's UID", ""))?;
        // Project's own summary row isn't a task.
        if uid == 0 || task.flag("IsNull") {
            continue;
        }
        let name = one_line(task.text_of("Name").unwrap_or("Unnamed task"));
        if task.text_of("Active") == Some("0") {
            notes.add("inactive-task", "an inactive task was left out", &name);
            continue;
        }
        let level = task.number("OutlineLevel").unwrap_or(1);
        while outline.last().is_some_and(|(at, _)| *at >= level) {
            outline.pop();
        }
        let parent = outline.last().map_or(m::WbsId(ROOT_WBS), |(_, id)| *id);
        sequence += 1;
        if task.flag("Summary") {
            if task.number("ConstraintType").unwrap_or(0) != 0 {
                notes.add(
                    "summary-constraint",
                    "a constraint on a summary task was left out",
                    &name,
                );
            }
            let id = m::WbsId(id_of(uid)? + ROOT_WBS);
            let code = task
                .text_of("OutlineNumber")
                .map_or_else(|| uid.to_string(), one_line);
            schedule.wbs.insert(
                id,
                m::Wbs {
                    id,
                    project,
                    parent: Some(parent),
                    code,
                    name: clipped(&name, MAX_WBS_NAME, "a WBS node", notes),
                    sequence,
                    is_project_node: false,
                    earned_value: m::EarnedValue::default(),
                    obs: None,
                    notes: std::collections::BTreeMap::new(),
                    udfs: BTreeMap::new(),
                    source: None,
                },
            );
            outline.push((level, id));
            table.summaries.insert(uid);
            continue;
        }
        let built = activity(
            task,
            uid,
            (project, parent),
            default_calendar,
            schedule,
            notes,
        )?;
        table.activities.insert(uid, built.id);
        schedule.activities.insert(built.id, built);
    }
    Ok(table)
}

fn link_kind(code: i64) -> Option<m::RelationshipType> {
    match code {
        0 => Some(m::RelationshipType::FinishToFinish),
        1 => Some(m::RelationshipType::FinishToStart),
        2 => Some(m::RelationshipType::StartToFinish),
        3 => Some(m::RelationshipType::StartToStart),
        _ => None,
    }
}

fn lag(link: &Element, name: &str, minutes_per_day: i64, notes: &mut Notes) -> m::Minutes {
    let format = link.number("LagFormat").unwrap_or(7);
    if PERCENT.contains(&format) {
        notes.add(
            "percent-lag",
            "a lag given as a percent of its predecessor was left as no lag",
            name,
        );
        return m::Minutes::ZERO;
    }
    let minutes = link.number("LinkLag").unwrap_or(0) / 10;
    if ELAPSED.contains(&format) {
        notes.add(
            "elapsed-lag",
            "an elapsed lag is now the same number of working days",
            name,
        );
        return m::Minutes::new(minutes.saturating_mul(minutes_per_day) / (24 * 60));
    }
    m::Minutes::new(minutes)
}

fn links(
    root: &Element,
    schedule: &mut m::Schedule,
    table: &TaskTable,
    minutes_per_day: i64,
    notes: &mut Notes,
) {
    let Some(list) = root.child("Tasks") else {
        return;
    };
    let project = schedule.project.id;
    let mut next = 1;
    for task in list.all("Task") {
        let uid = task.number("UID").unwrap_or(0);
        let name = task.text_of("Name").unwrap_or("Unnamed task");
        for link in task.all("PredecessorLink") {
            let from = link.number("PredecessorUID").unwrap_or(-1);
            if table.summaries.contains(&uid) || table.summaries.contains(&from) {
                notes.add(
                    "summary-logic",
                    "a link on a summary task was left out, because summary tasks become WBS nodes, which hold no logic",
                    name,
                );
                continue;
            }
            let (Some(predecessor), Some(successor)) =
                (table.activities.get(&from), table.activities.get(&uid))
            else {
                notes.add(
                    "link-to-left-out-task",
                    "a link to a task that was left out is gone, so check what it held back",
                    name,
                );
                continue;
            };
            let id = m::RelationshipId(next);
            next += 1;
            let kind = link
                .number("Type")
                .and_then(link_kind)
                .unwrap_or(m::RelationshipType::FinishToStart);
            schedule.relationships.insert(
                id,
                m::Relationship {
                    id,
                    predecessor: *predecessor,
                    successor: *successor,
                    predecessor_project: project,
                    successor_project: project,
                    kind,
                    lag: lag(link, name, minutes_per_day, notes),
                    source: None,
                },
            );
        }
    }
}

/// A short name that's this resource's alone, because it's P6's key.
fn unique_short_name(wanted: &str, taken: &mut BTreeSet<String>, notes: &mut Notes) -> String {
    let mut name = wanted.to_owned();
    let mut next = 2;
    while !taken.insert(name.to_lowercase()) {
        name = format!("{wanted}-{next}");
        next += 1;
    }
    if name != wanted {
        notes.add(
            "resource-id",
            "two resources shared initials, so one was numbered",
            &name,
        );
    }
    name
}

fn read_resource(
    resource: &Element,
    schedule: &m::Schedule,
    taken: &mut BTreeSet<String>,
    notes: &mut Notes,
) -> Option<m::Resource> {
    let (uid, name) = (resource.number("UID")?, resource.text_of("Name")?);
    let id = m::ResourceId(id_of(uid).ok()?);
    let short = resource
        .text_of("Initials")
        .map_or_else(|| project_code(name), project_code);
    let kind = match resource.number("Type") {
        Some(0) => m::ResourceType::Material,
        Some(2) => {
            notes.add(
                "cost-resource",
                "a cost resource came across as nonlabor",
                name,
            );
            m::ResourceType::Nonlabor
        }
        _ => m::ResourceType::Labor,
    };
    Some(m::Resource {
        id,
        short_name: unique_short_name(&short, taken, notes),
        name: one_line(name),
        kind,
        calendar: resource
            .number("CalendarUID")
            .and_then(|uid| id_of(uid).ok())
            .map(m::CalendarId)
            .filter(|c| schedule.calendars.contains_key(c)),
        parent: None,
        default_units_per_hour: None,
        default_cost_follows_units: false,
        rates: Vec::new(),
        roles: Vec::new(),
        udfs: BTreeMap::new(),
        source: None,
    })
}

/// The dates MS Project XML holds for "NA", a period open at that end, as
/// MPXJ documents them (TF-003).
pub(crate) const NA_FROM: (i32, u8, u8) = (1984, 1, 1);
pub(crate) const NA_TO: (i32, u8, u8) = (2049, 12, 31);

/// A limit and when it takes effect; `None` is before every dated one.
type Limit = (Option<m::DateTime>, Option<f64>);

fn units(e: &Element, name: &str) -> Option<f64> {
    e.text_of(name)?
        .parse::<f64>()
        .ok()
        .filter(|n| n.is_finite() && *n >= 0.0)
}

/// A work resource's limit over time, in units per hour (TF-003): its
/// availability periods, with none outside them, or else `MaxUnits`.
/// Material and cost resources are skipped: in the corpus files they hold a
/// `MaxUnits` of 1 throughout (TF-003).
fn limits_of(resource: &Element) -> Vec<Limit> {
    if !matches!(resource.number("Type"), None | Some(1)) {
        return Vec::new();
    }
    let periods = resource.child("AvailabilityPeriods");
    let mut periods: Vec<_> = periods
        .into_iter()
        .flat_map(|p| p.all("AvailabilityPeriod"))
        .map(|period| {
            // An unreadable date counts as none, as for a rate (TF-001 item 5).
            let at = |name| optional_date(period, name).ok().flatten();
            let from = at("AvailableFrom").filter(|d| d.date().ymd() > NA_FROM);
            let to = at("AvailableTo").filter(|d| d.date().ymd() < NA_TO);
            (from, to, units(period, "AvailableUnits"))
        })
        .collect();
    if periods.is_empty() {
        return units(resource, "MaxUnits")
            .map(|limit| vec![(None, Some(limit))])
            .unwrap_or_default();
    }
    periods.sort_by_key(|(from, ..)| *from);
    let mut limits = Vec::new();
    if periods[0].0.is_some() {
        limits.push((None, Some(0.0)));
    }
    for (at, (from, to, limit)) in periods.iter().enumerate() {
        limits.push((*from, *limit));
        // After its last day, none until the day the next period begins.
        let after = to.and_then(|to| to.date().checked_add_days(1));
        let next = periods.get(at + 1).map(|(from, ..)| *from);
        if let Some(after) = after
            && next.is_none_or(|next| next.is_some_and(|next| next.date() > after))
            && let Some(after) = after.at(m::TimeOfDay::MIDNIGHT)
        {
            limits.push((Some(after), Some(0.0)));
        }
    }
    limits
}

/// Rates holding these limits, with IDs from `next`. A limit before every
/// dated one starts on the project's opening day, or else the day before
/// the first dated one, so it stays first (TF-001 item 7).
fn rates_of(
    limits: Vec<Limit>,
    opening: Option<m::DateTime>,
    next: &mut u64,
) -> Vec<m::ResourceRate> {
    let first = limits.iter().filter_map(|(from, _)| *from).min();
    let undated = match first {
        None => opening,
        Some(first) => opening.filter(|o| *o < first).or_else(|| {
            first
                .date()
                .checked_add_days(-1)?
                .at(m::TimeOfDay::MIDNIGHT)
        }),
    };
    limits
        .into_iter()
        .map(|(from, limit)| {
            let id = m::ResourceRateId(*next);
            *next += 1;
            m::ResourceRate {
                id,
                effective: from.or(undated),
                prices: [None; 5],
                max_units_per_hour: limit,
                source: None,
            }
        })
        .collect()
}

fn resources(root: &Element, schedule: &mut m::Schedule, table: &TaskTable, notes: &mut Notes) {
    let mut known = BTreeMap::new();
    let mut taken = BTreeSet::new();
    let opening = schedule
        .project
        .planned_start
        .or(schedule.project.data_date)
        .and_then(|d| d.date().at(m::TimeOfDay::MIDNIGHT));
    let mut next_rate = 1;
    if let Some(list) = root.child("Resources") {
        for resource in list.all("Resource") {
            if resource.number("UID").is_some_and(|uid| uid > 0)
                && let Some(mut read) = read_resource(resource, schedule, &mut taken, notes)
            {
                read.rates = rates_of(limits_of(resource), opening, &mut next_rate);
                known.insert(resource.number("UID").unwrap_or(0), read);
            }
        }
    }
    let project = schedule.project.id;
    let list = root.child("Assignments");
    for (index, assignment) in (1_u64..).zip(list.into_iter().flat_map(|l| l.all("Assignment"))) {
        let task = assignment.number("TaskUID").unwrap_or(-1);
        let resource = assignment.number("ResourceUID").unwrap_or(-1);
        if table.summaries.contains(&task) && known.contains_key(&resource) {
            let wbs = id_of(task).ok().map(|id| m::WbsId(id + ROOT_WBS));
            let name = wbs
                .and_then(|id| schedule.wbs.get(&id))
                .map(|w| w.name.clone());
            notes.add(
                "summary-assignment",
                "an assignment on a summary task was left out, because a WBS node holds no assignments in P6",
                &name.unwrap_or_default(),
            );
            continue;
        }
        let (Some(activity), Some(resource)) = (table.activities.get(&task), known.get(&resource))
        else {
            continue;
        };
        let amounts = amounts_of(assignment, resource.kind);
        if amounts.budgeted_units.is_none() {
            let task = schedule.activities.get(activity).map(|a| a.name.clone());
            notes.add(
                "assignment-units",
                "an assignment came across without its units",
                &task.unwrap_or_default(),
            );
        }
        let id = m::AssignmentId(index);
        schedule.assignments.insert(
            id,
            m::Assignment {
                id,
                activity: *activity,
                resource: Some(resource.id),
                role: None,
                cost_account: None,
                project,
                curve: None,
                typed_spread: None,
                amounts,
                udfs: BTreeMap::new(),
                source: None,
            },
        );
        schedule.resources.insert(resource.id, resource.clone());
    }
    // Resources with no assignments too: the file lists them, and P6 holds them.
    for resource in known.into_values() {
        schedule.resources.entry(resource.id).or_insert(resource);
    }
}

/// An assignment's work, units and costs. Work is hours for labor and
/// nonlabor; a material resource's work isn't read, since the file gives it
/// as a duration. Costs are in hundredths, as the public samples show: an
/// assignment of 24 hours at 50 an hour costs `120000` (docs/rules/msproject-xml.md).
fn amounts_of(assignment: &Element, kind: m::ResourceType) -> m::Amounts {
    let hours = |name: &str| {
        if kind == m::ResourceType::Material {
            return None;
        }
        // To the second, since work need not be whole minutes.
        let seconds = seconds_of("assignment work", assignment.text_of(name)?).ok()?;
        Some(seconds / 3_600.0)
    };
    let decimal = |name: &str| {
        assignment
            .text_of(name)?
            .parse::<f64>()
            .ok()
            .filter(|n| n.is_finite())
    };
    let money = |name: &str| decimal(name).map(|n| n / 100.0);
    let units = decimal("Units");
    m::Amounts {
        budgeted_units: hours("Work"),
        actual_units: hours("ActualWork"),
        remaining_units: hours("RemainingWork"),
        budgeted_units_per_hour: units,
        remaining_units_per_hour: units,
        budgeted_cost: money("Cost"),
        actual_cost: money("ActualCost"),
        remaining_cost: money("RemainingCost"),
        ..m::Amounts::default()
    }
}

/// The latest date any task records work on, for a file with no status date.
fn latest_actual(root: &Element) -> Result<Option<m::DateTime>, ImportError> {
    let mut latest = None;
    let Some(list) = root.child("Tasks") else {
        return Ok(None);
    };
    for task in list.all("Task") {
        for field in ["ActualStart", "ActualFinish"] {
            latest = latest.max(optional_date(task, field)?);
        }
    }
    Ok(latest)
}

/// The data date: the status date, or else the last recorded work.
fn data_date(root: &Element, title: &str, notes: &mut Notes) -> Result<m::DateTime, ImportError> {
    let latest = latest_actual(root)?;
    if let Some(status) = optional_date(root, "StatusDate")? {
        if latest.is_some_and(|latest| latest > status) {
            notes.add(
                "actuals-after-status-date",
                "a task records work after the file's status date",
                title,
            );
        }
        return Ok(status);
    }
    if let Some(latest) = latest {
        notes.add(
            "no-status-date",
            "the file has no status date, so the data date is its last recorded work",
            title,
        );
        return Ok(latest);
    }
    optional_date(root, "StartDate")?.ok_or_else(|| bad("the project's start date", ""))
}

/// Reads an MS Project XML document.
///
/// # Errors
///
/// [`ImportError`] if it isn't one, or lacks what a schedule needs.
pub fn read(text: &str) -> Result<Imported, ImportError> {
    let root = xml::parse(text)?;
    if root.name != "Project" || root.child("Tasks").is_none() {
        return Err(ImportError::WrongFormat("an MS Project XML file"));
    }
    let mut notes = Notes::default();
    let title = root
        .text_of("Title")
        .or_else(|| root.text_of("Name"))
        .unwrap_or("Imported project");
    let id = m::ProjectId(1);
    let mut project = m::Project::new(id, project_code(title));
    let name = clipped(&one_line(title), MAX_WBS_NAME, "the project", &mut notes);
    project.name = Some(name.clone());
    let date = data_date(&root, title, &mut notes)?;
    project.data_date = Some(date);
    project.planned_start = optional_date(&root, "StartDate")?.or(Some(date));
    if root.text_of("ScheduleFromStart") == Some("0") {
        notes.add(
            "schedule-from-finish",
            "the file schedules back from its finish date, and TotalFloat schedules forward",
            title,
        );
        project.must_finish_by = optional_date(&root, "FinishDate")?;
    }
    if root.number("CriticalSlackLimit").unwrap_or(0) != 0 {
        notes.add(
            "critical-slack",
            "the file's critical slack limit was left as zero",
            title,
        );
    }
    let code = project.short_name.clone();
    let minutes_per_day = root.number("MinutesPerDay").unwrap_or(480);
    let mut schedule = m::Schedule::new(project);
    calendars(&root, &mut schedule, Some(minutes_per_day), &mut notes)?;
    let default_calendar = root
        .number("CalendarUID")
        .map(id_of)
        .transpose()?
        .map(m::CalendarId)
        .filter(|c| schedule.calendars.contains_key(c))
        .or_else(|| schedule.calendars.keys().next().copied())
        .ok_or_else(|| bad("a calendar", "the file holds none"))?;
    if let Some(calendar) = schedule.calendars.get_mut(&default_calendar) {
        calendar.kind = m::CalendarType::Global;
        calendar.is_default = true;
    }
    schedule.project.default_calendar = Some(default_calendar);
    schedule.wbs.insert(
        m::WbsId(ROOT_WBS),
        m::Wbs {
            id: m::WbsId(ROOT_WBS),
            project: id,
            parent: None,
            code,
            name,
            sequence: 0,
            is_project_node: true,
            earned_value: m::EarnedValue::default(),
            obs: None,
            notes: std::collections::BTreeMap::new(),
            udfs: BTreeMap::new(),
            source: None,
        },
    );
    let table = tasks(&root, &mut schedule, default_calendar, &mut notes)?;
    links(&root, &mut schedule, &table, minutes_per_day, &mut notes);
    resources(&root, &mut schedule, &table, &mut notes);
    Ok(Imported {
        schedule,
        notes: notes.finish(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLANT: &str = include_str!("../../../fixtures/msproject/plant.xml");

    fn plant() -> Imported {
        read(PLANT).unwrap()
    }

    fn named<'a>(imported: &'a Imported, name: &str) -> &'a m::Activity {
        let found = imported.schedule.activities.values();
        found.into_iter().find(|a| a.name == name).unwrap()
    }

    fn note<'a>(imported: &'a Imported, kind: &str) -> &'a Note {
        imported.notes.iter().find(|n| n.kind == kind).unwrap()
    }

    #[test]
    fn durations_read_as_written() {
        let read = |text| duration("test", text).map(m::Minutes::get);
        assert_eq!(read("PT8H0M0S"), Ok(480));
        assert_eq!(read("PT7H30M0S"), Ok(450));
        assert_eq!(read("P1DT2H0M0S"), Ok(1560));
        assert_eq!(read("PT0H0M0S"), Ok(0));
        assert_eq!(read("PT0.5H"), Ok(30));
        for bad in ["", "8h", "PT8", "PTXH"] {
            assert!(read(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn the_project_comes_across() {
        let imported = plant();
        let project = &imported.schedule.project;
        assert_eq!(project.short_name, "Plant & Yard Upgrade");
        assert_eq!(project.data_date.unwrap().to_string(), "2026-03-09 08:00");
        assert_eq!(imported.schedule.validate(), []);
    }

    #[test]
    fn summary_tasks_become_wbs_nodes_and_work_sits_under_them() {
        let imported = plant();
        let wbs = &imported.schedule.wbs;
        assert_eq!(wbs.len(), 3);
        let yard = wbs.values().find(|w| w.name == "Yard").unwrap();
        assert_eq!(yard.parent, Some(m::WbsId(ROOT_WBS)));
        assert_eq!(named(&imported, "Lighting").wbs, yard.id);
        assert_eq!(named(&imported, "Turnover").wbs, m::WbsId(ROOT_WBS));
    }

    #[test]
    fn a_task_carries_its_duration_progress_and_constraint() {
        let imported = plant();
        let dig = named(&imported, "Excavate");
        assert_eq!(dig.status, m::ActivityStatus::InProgress);
        assert_eq!(dig.original_duration, m::Minutes::new(2400));
        assert_eq!(dig.remaining_duration, m::Minutes::new(960));
        let pour = named(&imported, "Pour slab");
        let held = pour.primary_constraint.unwrap();
        assert_eq!(held.kind, m::ConstraintType::StartOnOrAfter);
        let lighting = named(&imported, "Lighting");
        assert_eq!(
            lighting.primary_constraint.unwrap().kind,
            m::ConstraintType::FinishOnOrBefore
        );
        let turnover = named(&imported, "Turnover");
        assert_eq!(turnover.activity_type, m::ActivityType::FinishMilestone);
    }

    #[test]
    fn links_keep_their_type_and_lag_and_summary_logic_is_noted() {
        let imported = plant();
        let links = &imported.schedule.relationships;
        assert_eq!(links.len(), 4);
        let pour = named(&imported, "Pour slab").id;
        let in_pour = links.values().find(|l| l.successor == pour).unwrap();
        assert_eq!(in_pour.kind, m::RelationshipType::FinishToStart);
        assert_eq!(in_pour.lag, m::Minutes::new(480));
        let fence = named(&imported, "Install fence").id;
        let in_fence = links.values().find(|l| l.successor == fence).unwrap();
        assert_eq!(in_fence.kind, m::RelationshipType::StartToStart);
        assert_eq!(note(&imported, "summary-logic").count, 1);
        assert_eq!(note(&imported, "elapsed-lag").examples, ["Install fence"]);
        assert_eq!(note(&imported, "percent-lag").examples, ["Lighting"]);
    }

    #[test]
    fn calendars_inherit_and_keep_their_holidays() {
        let imported = plant();
        let six = imported
            .schedule
            .calendars
            .values()
            .find(|c| c.name == "Six Day")
            .unwrap();
        assert_eq!(six.work_week.day(m::Weekday::Monday).len(), 2);
        assert_eq!(six.work_week.day(m::Weekday::Saturday).len(), 2);
        assert_eq!(six.work_week.day(m::Weekday::Sunday), []);
        let friday: m::Date = "2026-04-03".parse().unwrap();
        assert_eq!(six.exceptions[&friday], []);
        assert_eq!(six.hours_per_period.day, Some(m::Minutes::new(480)));
    }

    #[test]
    fn every_kind_of_loss_is_reported() {
        let imported = plant();
        for kind in [
            "inactive-task",
            "manual-task",
            "effort-driven",
            "start-no-earlier-than",
            "assignment-units",
        ] {
            assert!(imported.notes.iter().any(|n| n.kind == kind), "{kind}");
        }
        assert_eq!(imported.schedule.assignments.len(), 1);
        assert_eq!(imported.schedule.resources.len(), 1);
    }

    fn plan(body: &str) -> String {
        format!(
            "<Project><Title>T</Title><StartDate>2026-03-02T08:00:00</StartDate>\
             <CalendarUID>1</CalendarUID><Calendars><Calendar><UID>1</UID><Name>S</Name>\
             </Calendar></Calendars><Tasks>{body}</Tasks></Project>"
        )
    }

    #[test]
    fn an_assignment_on_a_summary_task_is_reported() {
        let text = plan(
            "<Task><UID>1</UID><Name>Phase</Name><OutlineLevel>1</OutlineLevel><Summary>1</Summary></Task>\
             <Task><UID>2</UID><Name>A</Name><OutlineLevel>2</OutlineLevel><Duration>PT8H0M0S</Duration></Task>",
        )
        .replace(
            "</Project>",
            "<Resources><Resource><UID>1</UID><Name>Crane</Name></Resource></Resources>\
             <Assignments><Assignment><TaskUID>1</TaskUID><ResourceUID>1</ResourceUID>\
             <Work>PT8H0M0S</Work></Assignment></Assignments></Project>",
        );
        let imported = read(&text).unwrap();
        assert!(imported.schedule.assignments.is_empty());
        assert_eq!(note(&imported, "summary-assignment").examples, ["Phase"]);
    }

    #[test]
    fn a_link_to_a_left_out_task_is_reported() {
        let imported = read(&plan(
            "<Task><UID>1</UID><Name>A</Name><Duration>PT8H0M0S</Duration></Task>\
             <Task><UID>2</UID><Name>Mid</Name><Active>0</Active></Task>\
             <Task><UID>3</UID><Name>B</Name><PredecessorLink><PredecessorUID>2</PredecessorUID>\
             </PredecessorLink></Task>",
        ))
        .unwrap();
        assert_eq!(note(&imported, "link-to-left-out-task").examples, ["B"]);
    }

    #[test]
    fn with_no_status_date_the_data_date_follows_recorded_work() {
        let imported = read(&plan(
            "<Task><UID>1</UID><Name>A</Name><ActualStart>2026-03-10T08:00:00</ActualStart>\
             <RemainingDuration>PT8H0M0S</RemainingDuration></Task>",
        ))
        .unwrap();
        let date = imported.schedule.project.data_date.unwrap();
        assert_eq!(date.to_string(), "2026-03-10 08:00");
        assert_eq!(note(&imported, "no-status-date").count, 1);
    }

    #[test]
    fn elapsed_work_runs_on_a_calendar_of_every_hour() {
        let imported = read(&plan(
            "<Task><UID>1</UID><Name>Cure</Name><Duration>PT240H0M0S</Duration>\
             <DurationFormat>8</DurationFormat></Task>",
        ))
        .unwrap();
        let cure = named(&imported, "Cure");
        let calendar = &imported.schedule.calendars[&cure.calendar];
        assert_eq!(calendar.name, "Elapsed 24-Hour");
        assert_eq!(calendar.work_week.day(m::Weekday::Sunday).len(), 1);
    }

    #[test]
    fn lag_formats_translate_or_are_reported() {
        let lag_of = |format: i64, value: i64| {
            let body = format!(
                "<Task><UID>1</UID><Name>A</Name></Task><Task><UID>2</UID><Name>B</Name>\
                 <PredecessorLink><PredecessorUID>1</PredecessorUID><LinkLag>{value}</LinkLag>\
                 <LagFormat>{format}</LagFormat></PredecessorLink></Task>"
            );
            let imported = read(&plan(&body)).unwrap();
            let link = imported.schedule.relationships.values().next().unwrap().lag;
            (link, imported.notes)
        };
        assert_eq!(lag_of(7, 4800).0, m::Minutes::new(480));
        let (two_elapsed_days, notes) = lag_of(8, 28800);
        assert_eq!(two_elapsed_days, m::Minutes::new(960));
        assert!(notes.iter().any(|n| n.kind == "elapsed-lag"));
        for format in PERCENT {
            let (lag, notes) = lag_of(format, 25000);
            assert_eq!(lag, m::Minutes::ZERO);
            assert!(notes.iter().any(|n| n.kind == "percent-lag"));
        }
    }

    #[test]
    fn a_repeating_exception_keeps_only_its_first_date() {
        let text = "<Project><Title>T</Title><StartDate>2026-03-02T08:00:00</StartDate>\
             <Calendars><Calendar><UID>1</UID><Name>S</Name><Exceptions><Exception>\
             <Occurrences>21</Occurrences><TimePeriod><FromDate>2026-12-25T00:00:00</FromDate>\
             <ToDate>2046-12-25T23:59:00</ToDate></TimePeriod></Exception></Exceptions>\
             </Calendar></Calendars><Tasks/></Project>";
        let imported = read(text).unwrap();
        let calendar = imported.schedule.calendars.values().next().unwrap();
        assert_eq!(calendar.exceptions.len(), 1);
        assert_eq!(note(&imported, "repeating-exception").count, 1);
    }

    /// Every resource the file lists comes across, assigned or not: the
    /// file holds them, and the done definition asks for them intact.
    #[test]
    fn resources_with_no_work_come_across_and_shared_initials_are_numbered() {
        let text =
            plan("<Task><UID>1</UID><Name>A</Name></Task><Task><UID>2</UID><Name>B</Name></Task>");
        let text = text.replace(
            "</Project>",
            "<Resources><Resource><UID>1</UID><Name>Crane</Name><Initials>C</Initials></Resource>\
             <Resource><UID>2</UID><Name>Carpenter</Name><Initials>C</Initials></Resource>\
             <Resource><UID>3</UID><Name>Spare</Name><Initials>S</Initials></Resource></Resources>\
             <Assignments><Assignment><TaskUID>1</TaskUID><ResourceUID>1</ResourceUID></Assignment>\
             <Assignment><TaskUID>2</TaskUID><ResourceUID>2</ResourceUID></Assignment></Assignments>\
             </Project>",
        );
        let imported = read(&text).unwrap();
        let names: Vec<&str> = imported
            .schedule
            .resources
            .values()
            .map(|r| r.short_name.as_str())
            .collect();
        assert_eq!(names, ["C", "C-2", "S"]);
        assert!(imported.notes.iter().all(|n| n.kind != "unused-resource"));
    }

    fn limits_read(resources: &str) -> Vec<Vec<(Option<String>, Option<f64>)>> {
        let text = plan("<Task><UID>1</UID><Name>A</Name></Task>").replace(
            "</Project>",
            &format!("<Resources>{resources}</Resources></Project>"),
        );
        let imported = read(&text).unwrap();
        imported
            .schedule
            .resources
            .values()
            .map(|r| {
                r.rates
                    .iter()
                    .map(|rate| {
                        (
                            rate.effective.map(|e| e.to_string()),
                            rate.max_units_per_hour,
                        )
                    })
                    .collect()
            })
            .collect()
    }

    /// `MaxUnits` is a work resource's limit, from the project's start; MS
    /// Project writes 1 for material and cost resources whatever they hold,
    /// so theirs isn't read (TF-003).
    #[test]
    fn a_work_resource_s_max_units_is_its_limit() {
        let limits = limits_read(
            "<Resource><UID>1</UID><Name>Crew</Name><Type>1</Type><MaxUnits>2.00</MaxUnits></Resource>\
             <Resource><UID>2</UID><Name>Sand</Name><Type>0</Type><MaxUnits>1</MaxUnits></Resource>\
             <Resource><UID>3</UID><Name>Hire</Name><Type>2</Type><MaxUnits>1</MaxUnits></Resource>\
             <Resource><UID>4</UID><Name>Spare</Name></Resource>",
        );
        assert_eq!(
            limits,
            [
                vec![(Some("2026-03-02 00:00".to_owned()), Some(2.0))],
                vec![],
                vec![],
                vec![]
            ]
        );
    }

    /// Availability periods are the limit over time, and outside them the
    /// resource isn't available: none before the first, in a gap, or after
    /// the last (TF-003).
    #[test]
    fn availability_periods_are_a_limit_over_time() {
        let periods = |p: &str| {
            format!(
                "<Resource><UID>1</UID><Name>Crew</Name><MaxUnits>1</MaxUnits>\
                 <AvailabilityPeriods>{p}</AvailabilityPeriods></Resource>"
            )
        };
        // Open at both ends, written by TotalFloat or as MS Project's "NA".
        let open = limits_read(&periods(
            "<AvailabilityPeriod><AvailableFrom>1984-01-01T00:00:00</AvailableFrom>\
             <AvailableTo>2026-03-31T23:59:00</AvailableTo><AvailableUnits>1</AvailableUnits></AvailabilityPeriod>\
             <AvailabilityPeriod><AvailableFrom>2026-04-01T00:00:00</AvailableFrom>\
             <AvailableTo>2049-12-31T23:59:00</AvailableTo><AvailableUnits>3</AvailableUnits></AvailabilityPeriod>",
        ));
        assert_eq!(
            open,
            [vec![
                (Some("2026-03-02 00:00".to_owned()), Some(1.0)),
                (Some("2026-04-01 00:00".to_owned()), Some(3.0)),
            ]]
        );
        // Closed, with a gap: none before, between and after.
        let closed = limits_read(&periods(
            "<AvailabilityPeriod><AvailableFrom>2026-05-01T08:00:00</AvailableFrom>\
             <AvailableTo>2026-05-29T17:00:00</AvailableTo><AvailableUnits>2</AvailableUnits></AvailabilityPeriod>\
             <AvailabilityPeriod><AvailableFrom>2026-03-02T08:00:00</AvailableFrom>\
             <AvailableTo>2026-04-30T17:00:00</AvailableTo><AvailableUnits>0.5</AvailableUnits></AvailabilityPeriod>\
             <AvailabilityPeriod><AvailableFrom>2026-07-01T00:00:00</AvailableFrom>\
             <AvailableTo>2026-07-31T17:00:00</AvailableTo><AvailableUnits>1</AvailableUnits></AvailabilityPeriod>",
        ));
        assert_eq!(
            closed,
            [vec![
                (Some("2026-03-02 00:00".to_owned()), Some(0.0)),
                (Some("2026-03-02 08:00".to_owned()), Some(0.5)),
                // 17:00 to 08:00 the next morning is no gap: periods are by day.
                (Some("2026-05-01 08:00".to_owned()), Some(2.0)),
                (Some("2026-05-30 00:00".to_owned()), Some(0.0)),
                (Some("2026-07-01 00:00".to_owned()), Some(1.0)),
                (Some("2026-08-01 00:00".to_owned()), Some(0.0)),
            ]]
        );
    }

    /// An assignment's work, units and costs come across; costs are in
    /// hundredths, as the public samples show.
    #[test]
    fn an_assignment_s_work_units_and_costs_come_across() {
        let text = plan("<Task><UID>1</UID><Name>A</Name></Task>").replace(
            "</Project>",
            "<Resources><Resource><UID>1</UID><Name>Crew</Name><Initials>C</Initials></Resource>\
             <Resource><UID>2</UID><Name>Sand</Name><Initials>S</Initials><Type>0</Type></Resource></Resources>\
             <Assignments><Assignment><TaskUID>1</TaskUID><ResourceUID>1</ResourceUID>\
             <ActualCost>30000</ActualCost><ActualWork>PT6H0M0S</ActualWork><Cost>120000</Cost>\
             <RemainingCost>90000</RemainingCost><RemainingWork>PT18H0M0S</RemainingWork>\
             <Units>0.5</Units><Work>PT24H0M0S</Work></Assignment>\
             <Assignment><TaskUID>1</TaskUID><ResourceUID>2</ResourceUID><Work>PT10H0M0S</Work></Assignment>\
             </Assignments></Project>",
        );
        let imported = read(&text).unwrap();
        let mut assignments = imported.schedule.assignments.values();
        let crew = assignments.next().unwrap().amounts;
        assert_eq!(crew.budgeted_units, Some(24.0));
        assert_eq!(crew.actual_units, Some(6.0));
        assert_eq!(crew.remaining_units, Some(18.0));
        assert_eq!(crew.budgeted_units_per_hour, Some(0.5));
        assert_eq!(crew.budgeted_cost, Some(1200.0));
        assert_eq!(crew.actual_cost, Some(300.0));
        assert_eq!(crew.remaining_cost, Some(900.0));
        // A material resource's work is a duration in the file, so it isn't read.
        let sand = assignments.next().unwrap().amounts;
        assert_eq!(sand.budgeted_units, None);
        assert_eq!(note(&imported, "assignment-units").count, 1);
        // And they go back out the same.
        let written = crate::msproject_write::write(&imported.schedule).text;
        for field in [
            "<ActualCost>30000</ActualCost>",
            "<ActualWork>PT6H0M0S</ActualWork>",
            "<Cost>120000</Cost>",
            "<RemainingWork>PT18H0M0S</RemainingWork>",
            "<Units>0.5</Units>",
            "<Work>PT24H0M0S</Work>",
            "<SaveVersion>14</SaveVersion>",
        ] {
            assert!(written.contains(field), "{field}");
        }
    }

    /// Work that isn't whole minutes keeps its seconds.
    #[test]
    fn work_of_part_of_a_minute_comes_across_to_the_second() {
        let text = plan("<Task><UID>1</UID><Name>A</Name></Task>").replace(
            "</Project>",
            "<Resources><Resource><UID>1</UID><Name>Crew</Name></Resource></Resources>\
             <Assignments><Assignment><TaskUID>1</TaskUID><ResourceUID>1</ResourceUID>\
             <Work>PT0H19M48S</Work></Assignment></Assignments></Project>",
        );
        let imported = read(&text).unwrap();
        let work = imported.schedule.assignments.values().next().unwrap();
        let hours = work.amounts.budgeted_units.unwrap();
        assert!((hours - 0.33).abs() < 1e-9, "{hours}");
        let written = crate::msproject_write::write(&imported.schedule).text;
        assert!(written.contains("<Work>PT0H19M48S</Work>"), "{written}");
    }

    #[test]
    fn hostile_numbers_are_refused_not_panics() {
        let huge = plan("<Task><UID>9223372036854775807</UID><Name>A</Name></Task>");
        assert!(read(&huge).is_err());
        let negative = plan("<Task><UID>-1</UID><Name>A</Name></Task>");
        assert!(read(&negative).is_err());
    }

    proptest::proptest! {
        /// Whatever a file holds, reading it ends in a result, not a panic.
        #[test]
        fn any_text_reads_or_is_refused(text in "\\PC{0,200}") {
            let _ = read(&text);
            let _ = read(&plan(&text));
        }
    }

    #[test]
    fn other_files_are_refused() {
        assert!(matches!(
            read("<Project><Name>x</Name></Project>"),
            Err(ImportError::WrongFormat(_))
        ));
        assert!(matches!(read("<a/>"), Err(ImportError::WrongFormat(_))));
        assert!(matches!(read("nope"), Err(ImportError::Xml(_))));
        assert!(matches!(
            read("<Project><Tasks/></Project>"),
            Err(ImportError::BadValue { .. })
        ));
    }
}
