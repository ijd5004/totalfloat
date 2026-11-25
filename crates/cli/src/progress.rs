//! Progress as planned: a status update that records the work the schedule
//! expected to happen before a new data date (TF-020).
//!
//! Each activity whose calculated early start is before the new data date
//! starts then. One whose early finish is on or before it finishes then. One
//! still underway keeps the working time left from the new data date to its
//! early finish, on its own calendar. Finished work, level of effort and WBS
//! summary activities are left as they are.

use std::collections::{BTreeMap, BTreeSet};

use totalfloat_calendar::WorkCalendar;
use totalfloat_edit::{Edit, Span};
use totalfloat_model::{
    ActivityId, ActivityStatus, ActivityType, CalendarId, DateTime, Minutes, Schedule,
};

use crate::error::CliError;

/// The progress edits for one project, and what they leave.
#[derive(Debug, Default)]
pub struct Planned {
    /// One `set_progress` per activity that moves.
    pub edits: Vec<Edit>,
    /// Activities that start.
    pub started: usize,
    /// Activities that finish.
    pub finished: usize,
    /// Finished work and spanning activities, left as they were.
    pub left: usize,
    /// Activities the last calculation gave no dates, also left as they were.
    pub undated: usize,
}

fn refused(message: impl Into<String>) -> CliError {
    CliError::new(
        "as-planned-refused",
        message,
        "record that activity's progress with --actual-start and --remaining instead",
    )
}

/// The calendar each resource-dependent activity's work runs on: its
/// resources' calendar, which `edit` keeps the same for all of them.
fn resource_calendars(schedule: &Schedule) -> BTreeMap<ActivityId, CalendarId> {
    let mut calendars = BTreeMap::new();
    for assignment in schedule.assignments.values() {
        let Some(activity) = schedule.activities.get(&assignment.activity) else {
            continue;
        };
        if activity.activity_type != ActivityType::ResourceDependent {
            continue;
        }
        if let Some(calendar) = assignment
            .resource
            .and_then(|r| schedule.resources.get(&r))
            .and_then(|r| r.calendar)
        {
            calendars.entry(activity.id).or_insert(calendar);
        }
    }
    calendars
}

#[allow(clippy::cast_precision_loss, reason = "minutes are far below 2^52")]
fn span(minutes: Minutes) -> Result<Span, CliError> {
    // Three decimals of an hour round back to the same minute.
    format!("{:.3}h", minutes.get() as f64 / 60.0)
        .parse()
        .map_err(|_| refused("a remaining duration can't be written"))
}

/// Works out the progress edits that bring `schedule`, as last calculated, up
/// to `data_date`, leaving out the activities in `skip`, whose progress the
/// caller gives itself.
///
/// # Errors
///
/// `as-planned-refused` for an activity whose calendar can't be read.
pub fn as_planned(
    schedule: &Schedule,
    data_date: DateTime,
    skip: &BTreeSet<&str>,
) -> Result<Planned, CliError> {
    let mut planned = Planned::default();
    let resources = resource_calendars(schedule);
    let mut calendars: BTreeMap<CalendarId, WorkCalendar> = BTreeMap::new();
    for activity in schedule.activities.values() {
        if skip.contains(activity.code.as_str()) {
            continue;
        }
        if activity.status == ActivityStatus::Completed || activity.activity_type.is_span() {
            planned.left += 1;
            continue;
        }
        let stored = &activity.stored;
        let (Some(start), Some(finish)) = (stored.early_start, stored.early_finish) else {
            planned.undated += 1;
            continue;
        };
        // Zero-duration work stores its start after its finish, across the
        // nonworking time between (TF-020), so it happens at its finish.
        let start = start.min(finish);
        let started = activity.status == ActivityStatus::InProgress;
        if !started && start >= data_date {
            continue;
        }
        let code = &activity.code;
        let mut remaining = None;
        let mut finishes = finish <= data_date;
        if !finishes {
            let id = resources
                .get(&activity.id)
                .copied()
                .unwrap_or(activity.calendar);
            let calendar = match calendars.entry(id) {
                std::collections::btree_map::Entry::Occupied(held) => held.into_mut(),
                std::collections::btree_map::Entry::Vacant(slot) => {
                    let model = schedule
                        .calendars
                        .get(&id)
                        .ok_or_else(|| refused(format!("{code}'s calendar isn't in the file")))?;
                    let compiled = WorkCalendar::new(model)
                        .map_err(|e| refused(format!("{code}'s calendar can't be read: {e}")))?;
                    slot.insert(compiled)
                }
            };
            let left = calendar.work_between(data_date, finish);
            // No working time left before its finish: it's done by the data date.
            finishes = left <= Minutes::ZERO;
            remaining = (!finishes).then_some(left);
        }
        let milestone = activity.activity_type.is_milestone();
        let actual_start = (!started).then(|| start.to_string());
        let actual_finish = finishes.then(|| finish.min(data_date).to_string());
        planned.started += usize::from(!started);
        planned.finished += usize::from(finishes);
        planned.edits.push(Edit::SetProgress {
            activity: code.clone(),
            actual_start,
            actual_finish: if milestone { None } else { actual_finish },
            remaining: if milestone {
                None
            } else {
                remaining.map(span).transpose()?
            },
            percent: None,
        });
    }
    Ok(planned)
}

#[cfg(test)]
mod tests {
    use totalfloat_model::{
        Activity, ActivityId, Calendar, CalendarType, HoursPerPeriod, Project, ProjectId,
        TimeOfDay, WbsId, Weekday, WorkInterval, WorkWeek,
    };

    use super::*;

    fn at(text: &str) -> DateTime {
        text.parse().unwrap()
    }

    /// Weekdays 08:00 to 16:00.
    fn weekdays() -> Calendar {
        let day = WorkInterval::new(
            TimeOfDay::from_hm(8, 0).unwrap(),
            TimeOfDay::from_hm(16, 0).unwrap(),
        )
        .unwrap();
        let mut week = WorkWeek::default();
        for day_of in [
            Weekday::Monday,
            Weekday::Tuesday,
            Weekday::Wednesday,
            Weekday::Thursday,
            Weekday::Friday,
        ] {
            *week.day_mut(day_of) = vec![day];
        }
        Calendar {
            id: CalendarId(1),
            name: "Weekdays".to_owned(),
            kind: CalendarType::Global,
            project: None,
            base: None,
            is_default: true,
            hours_per_period: HoursPerPeriod::default(),
            work_week: week,
            exceptions: BTreeMap::new(),
            source: None,
        }
    }

    fn schedule(rows: &[(&str, ActivityType, ActivityStatus, &str, &str)]) -> Schedule {
        let mut s = Schedule::new(Project::new(ProjectId(1), "P"));
        s.calendars.insert(CalendarId(1), weekdays());
        for (n, (code, kind, status, start, finish)) in (1..).zip(rows) {
            let mut a = Activity::new(
                ActivityId(n),
                ProjectId(1),
                WbsId(1),
                CalendarId(1),
                (*code).to_owned(),
                (*code).to_owned(),
            );
            a.activity_type = *kind;
            a.status = *status;
            a.stored.early_start = Some(at(start));
            a.stored.early_finish = Some(at(finish));
            if *status == ActivityStatus::InProgress {
                a.actual_start = Some(at("2026-03-02 08:00"));
            }
            s.activities.insert(a.id, a);
        }
        s
    }

    fn progress(edit: &Edit) -> (Option<&str>, Option<&str>, Option<String>) {
        let Edit::SetProgress {
            actual_start,
            actual_finish,
            remaining,
            ..
        } = edit
        else {
            panic!("{edit:?}");
        };
        (
            actual_start.as_deref(),
            actual_finish.as_deref(),
            remaining.map(|r| r.to_string()),
        )
    }

    const TASK: ActivityType = ActivityType::TaskDependent;
    const NOT: ActivityStatus = ActivityStatus::NotStarted;

    #[test]
    fn work_before_the_data_date_starts_or_finishes() {
        // Data date Wednesday 08:00.
        let s = schedule(&[
            ("DONE", TASK, NOT, "2026-03-02 08:00", "2026-03-03 16:00"),
            ("HALF", TASK, NOT, "2026-03-03 08:00", "2026-03-04 16:00"),
            ("LATER", TASK, NOT, "2026-03-04 08:00", "2026-03-05 16:00"),
            (
                "UNDERWAY",
                TASK,
                ActivityStatus::InProgress,
                "2026-03-04 08:00",
                "2026-03-04 12:00",
            ),
        ]);
        let planned = as_planned(&s, at("2026-03-04 08:00"), &BTreeSet::new()).unwrap();
        assert_eq!((planned.started, planned.finished), (2, 1));
        let edits: Vec<_> = planned.edits.iter().map(progress).collect();
        assert_eq!(
            edits,
            [
                (Some("2026-03-02 08:00"), Some("2026-03-03 16:00"), None),
                (Some("2026-03-03 08:00"), None, Some("8h".to_owned())),
                (None, None, Some("4h".to_owned())),
            ]
        );
    }

    #[test]
    fn finishing_exactly_at_the_data_date_finishes() {
        let s = schedule(&[("EXACT", TASK, NOT, "2026-03-03 08:00", "2026-03-04 08:00")]);
        let planned = as_planned(&s, at("2026-03-04 08:00"), &BTreeSet::new()).unwrap();
        assert_eq!(planned.finished, 1);
        assert_eq!(
            progress(&planned.edits[0]),
            (Some("2026-03-03 08:00"), Some("2026-03-04 08:00"), None)
        );
    }

    /// Data date Friday 12:00, finish Monday 12:00: the weekend isn't work.
    #[test]
    fn remaining_work_skips_the_calendar_s_gaps() {
        let s = schedule(&[("GAP", TASK, NOT, "2026-03-05 08:00", "2026-03-09 12:00")]);
        let planned = as_planned(&s, at("2026-03-06 12:00"), &BTreeSet::new()).unwrap();
        assert_eq!(progress(&planned.edits[0]).2.as_deref(), Some("8h"));
    }

    /// P6 stores a zero-duration task at 17:00 starting at 08:00 the next day.
    #[test]
    fn zero_duration_work_starts_and_finishes_at_its_finish() {
        let s = schedule(&[("ZERO", TASK, NOT, "2026-03-04 08:00", "2026-03-03 16:00")]);
        let planned = as_planned(&s, at("2026-03-05 08:00"), &BTreeSet::new()).unwrap();
        assert_eq!(
            progress(&planned.edits[0]),
            (Some("2026-03-03 16:00"), Some("2026-03-03 16:00"), None)
        );
    }

    #[test]
    fn work_the_caller_progresses_and_work_without_dates_are_left() {
        let mut s = schedule(&[
            ("MINE", TASK, NOT, "2026-03-02 08:00", "2026-03-03 16:00"),
            ("NONE", TASK, NOT, "2026-03-02 08:00", "2026-03-03 16:00"),
        ]);
        s.activities
            .get_mut(&ActivityId(2))
            .unwrap()
            .stored
            .early_start = None;
        let skip = BTreeSet::from(["MINE"]);
        let planned = as_planned(&s, at("2026-03-04 08:00"), &skip).unwrap();
        assert!(planned.edits.is_empty(), "{:?}", planned.edits);
        assert_eq!(planned.undated, 1);
    }

    #[test]
    fn milestones_happen_at_one_moment_and_spans_are_left() {
        let s = schedule(&[
            (
                "START",
                ActivityType::StartMilestone,
                NOT,
                "2026-03-03 08:00",
                "2026-03-03 08:00",
            ),
            (
                "AT",
                ActivityType::FinishMilestone,
                NOT,
                "2026-03-04 08:00",
                "2026-03-04 08:00",
            ),
            (
                "LOE",
                ActivityType::LevelOfEffort,
                NOT,
                "2026-03-02 08:00",
                "2026-03-06 16:00",
            ),
            (
                "OLD",
                TASK,
                ActivityStatus::Completed,
                "2026-03-02 08:00",
                "2026-03-02 16:00",
            ),
        ]);
        let planned = as_planned(&s, at("2026-03-04 08:00"), &BTreeSet::new()).unwrap();
        assert_eq!(planned.left, 2);
        assert_eq!(planned.edits.len(), 1);
        assert_eq!(
            progress(&planned.edits[0]),
            (Some("2026-03-03 08:00"), None, None)
        );
    }
}
