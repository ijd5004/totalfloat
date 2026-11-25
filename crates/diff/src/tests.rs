//! Synthetic update pairs, one per change category.

use std::collections::BTreeMap;

use totalfloat_model::{
    Activity, ActivityCode, ActivityCodeId, ActivityCodeType, ActivityCodeTypeId, ActivityId,
    ActivityStatus, ActivityType, Calendar, CalendarId, CalendarType, CodeScope, Constraint,
    ConstraintType, DateTime, HoursPerPeriod, Minutes, Project, ProjectId, Relationship,
    RelationshipId, RelationshipType, Schedule, WbsId, WorkWeek,
};

use super::{Category, Suspicion, compare};

const DAY: i64 = 480;

fn at(text: &str) -> DateTime {
    text.parse().unwrap()
}

fn calendar(id: u64, name: &str) -> Calendar {
    Calendar {
        id: CalendarId(id),
        name: name.to_owned(),
        kind: CalendarType::Global,
        project: None,
        base: None,
        is_default: false,
        hours_per_period: HoursPerPeriod::default(),
        work_week: WorkWeek::default(),
        exceptions: BTreeMap::new(),
        source: None,
    }
}

/// A1 to A2 to A3, with A1 and A2 critical and A3 carrying float.
fn base() -> Schedule {
    let mut project = Project::new(ProjectId(1), "AUTHORED");
    project.data_date = Some(at("2026-01-05 08:00"));
    project.stored.scheduled_finish = Some(at("2026-02-02 17:00"));
    let mut s = Schedule::new(project);
    s.calendars.insert(CalendarId(1), calendar(1, "Five day"));
    s.calendars.insert(CalendarId(2), calendar(2, "Six day"));
    for n in 1..=3 {
        let mut a = Activity::new(
            ActivityId(n),
            ProjectId(1),
            WbsId(1),
            CalendarId(1),
            format!("A{n}"),
            format!("Authored {n}"),
        );
        a.original_duration = Minutes(5 * DAY);
        a.remaining_duration = Minutes(5 * DAY);
        a.stored.total_float = Some(Minutes(if n == 3 { 5 * DAY } else { 0 }));
        s.activities.insert(a.id, a);
    }
    link(&mut s, 1, 1, 2, 0);
    link(&mut s, 2, 2, 3, 0);
    s
}

fn link(s: &mut Schedule, id: u64, from: u64, to: u64, lag: i64) {
    s.relationships.insert(
        RelationshipId(id),
        Relationship {
            id: RelationshipId(id),
            predecessor: ActivityId(from),
            successor: ActivityId(to),
            predecessor_project: ProjectId(1),
            successor_project: ProjectId(1),
            kind: RelationshipType::FinishToStart,
            lag: Minutes(lag),
            source: None,
        },
    );
}

fn activity(s: &mut Schedule, n: u64) -> &mut Activity {
    s.activities.get_mut(&ActivityId(n)).unwrap()
}

#[test]
fn identical_schedules_have_no_differences() {
    let comparison = compare(&base(), &base());
    assert_eq!(comparison.counts, super::Counts::default());
    assert_eq!(comparison.project.finish_moved_hours, Some(0.0));
}

#[test]
fn added_and_deleted_activities_are_listed() {
    let mut later = base();
    later.activities.remove(&ActivityId(3));
    later.relationships.remove(&RelationshipId(2));
    let mut new = Activity::new(
        ActivityId(9),
        ProjectId(1),
        WbsId(1),
        CalendarId(1),
        "A9",
        "New work",
    );
    new.original_duration = Minutes(DAY);
    later.activities.insert(new.id, new);
    let comparison = compare(&base(), &later);
    assert_eq!(comparison.added[0].code, "A9");
    assert_eq!(comparison.deleted[0].code, "A3");
}

#[test]
fn a_renumbered_activity_is_matched_by_activity_id() {
    let mut later = base();
    let mut moved = later.activities.remove(&ActivityId(2)).unwrap();
    moved.id = ActivityId(20);
    later.activities.insert(moved.id, moved);
    let link = later.relationships.get_mut(&RelationshipId(1)).unwrap();
    link.successor = ActivityId(20);
    let link = later.relationships.get_mut(&RelationshipId(2)).unwrap();
    link.predecessor = ActivityId(20);
    let comparison = compare(&base(), &later);
    assert_eq!(comparison.counts, super::Counts::default());
}

#[test]
fn a_changed_activity_id_falls_back_to_the_internal_id() {
    let mut later = base();
    activity(&mut later, 2).code = "B2".to_owned();
    let comparison = compare(&base(), &later);
    assert_eq!(comparison.counts.added, 0);
    assert_eq!(comparison.counts.deleted, 0);
    let change = &comparison.changed[0];
    assert_eq!(change.code, "B2");
    assert_eq!(change.changes[0].category, Category::Identity);
    assert_eq!(change.changes[0].from, "A2");
    assert_eq!(comparison.counts.links_added, 0);
    assert_eq!(comparison.counts.links_deleted, 0);
}

fn category(later: &Schedule, wanted: Category) -> Vec<&'static str> {
    compare(&base(), later)
        .changed
        .iter()
        .flat_map(|change| change.changes.iter())
        .filter(|change| change.category == wanted)
        .map(|change| change.field)
        .collect()
}

#[test]
fn identity_changes_cover_name_type_and_wbs() {
    let mut later = base();
    let a = activity(&mut later, 3);
    a.name = "Renamed".to_owned();
    a.activity_type = ActivityType::ResourceDependent;
    assert_eq!(category(&later, Category::Identity), ["name", "type"]);
}

#[test]
fn durations_are_their_own_category() {
    let mut later = base();
    let a = activity(&mut later, 3);
    a.original_duration = Minutes(6 * DAY);
    a.remaining_duration = Minutes(6 * DAY);
    assert_eq!(
        category(&later, Category::Duration),
        ["original_duration", "remaining_duration"]
    );
}

#[test]
fn progress_covers_status_actuals_and_percent() {
    let mut later = base();
    let a = activity(&mut later, 1);
    a.status = ActivityStatus::Completed;
    a.actual_start = Some(at("2026-01-05 08:00"));
    a.actual_finish = Some(at("2026-01-09 17:00"));
    a.physical_percent_complete = Some(100.0);
    assert_eq!(
        category(&later, Category::Progress),
        [
            "status",
            "actual_start",
            "actual_finish",
            "physical_percent_complete"
        ]
    );
}

#[test]
fn constraints_calendars_and_codes_are_categories() {
    let mut later = base();
    let a = activity(&mut later, 3);
    a.primary_constraint = Some(Constraint {
        kind: ConstraintType::StartOnOrAfter,
        date: Some(at("2026-01-20 08:00")),
    });
    a.calendar = CalendarId(2);
    a.codes.insert(ActivityCodeTypeId(1), ActivityCodeId(1));
    later.activity_code_types.insert(
        ActivityCodeTypeId(1),
        ActivityCodeType {
            id: ActivityCodeTypeId(1),
            name: "Phase".to_owned(),
            scope: CodeScope::Global,
            project: None,
            max_length: None,
            sequence: 0,
            source: None,
        },
    );
    later.activity_codes.insert(
        ActivityCodeId(1),
        ActivityCode {
            id: ActivityCodeId(1),
            code_type: ActivityCodeTypeId(1),
            parent: None,
            value: "ONE".to_owned(),
            description: String::new(),
            sequence: 0,
            source: None,
        },
    );
    assert_eq!(
        category(&later, Category::Constraint),
        ["primary_constraint"]
    );
    assert_eq!(category(&later, Category::Calendar), ["calendar"]);
    let comparison = compare(&base(), &later);
    let codes = comparison.changed[0].changes.last().unwrap();
    assert_eq!(codes.to, "Phase=ONE");
}

#[test]
fn relationship_changes_are_added_deleted_or_lag() {
    let mut later = base();
    later.relationships.remove(&RelationshipId(2));
    link(&mut later, 3, 1, 3, 0);
    later.relationships.get_mut(&RelationshipId(1)).unwrap().lag = Minutes(90);
    let comparison = compare(&base(), &later);
    assert_eq!(comparison.links_deleted[0].successor, "A3");
    assert_eq!(comparison.links_added[0].predecessor, "A1");
    let lag = &comparison.links_changed[0];
    assert_eq!(lag.from_lag.as_deref(), Some("0h"));
    assert_eq!(lag.to_lag.as_deref(), Some("1h30m"));
}

#[test]
fn milestones_and_the_project_finish_report_movement() {
    let (mut earlier, mut later) = (base(), base());
    for (s, date) in [
        (&mut earlier, "2026-02-02 17:00"),
        (&mut later, "2026-02-03 17:00"),
    ] {
        let a = activity(s, 3);
        a.activity_type = ActivityType::FinishMilestone;
        a.stored.early_finish = Some(at(date));
    }
    later.project.stored.scheduled_finish = Some(at("2026-02-03 17:00"));
    let comparison = compare(&earlier, &later);
    assert_eq!(comparison.milestones_moved[0].hours, Some(24.0));
    assert_eq!(comparison.project.finish_moved_hours, Some(24.0));
}

#[test]
fn critical_membership_changes_both_ways() {
    let mut later = base();
    activity(&mut later, 1).stored.total_float = Some(Minutes(DAY));
    activity(&mut later, 3).stored.total_float = Some(Minutes(-DAY));
    let comparison = compare(&base(), &later);
    assert_eq!(comparison.left_critical, ["A1"]);
    assert_eq!(comparison.became_critical, ["A3"]);
}

#[test]
fn a_duration_cut_on_critical_work_without_progress_is_flagged() {
    let mut later = base();
    activity(&mut later, 2).remaining_duration = Minutes(2 * DAY);
    let comparison = compare(&base(), &later);
    assert_eq!(comparison.findings[0].kind, Suspicion::CriticalDurationCut);
    assert_eq!(comparison.findings[0].subject, "A2");
}

#[test]
fn a_duration_cut_with_progress_or_off_the_path_is_not_flagged() {
    let mut later = base();
    let a = activity(&mut later, 2);
    a.remaining_duration = Minutes(2 * DAY);
    a.status = ActivityStatus::InProgress;
    a.actual_start = Some(at("2026-01-05 08:00"));
    activity(&mut later, 3).remaining_duration = Minutes(DAY);
    assert_eq!(compare(&base(), &later).findings, []);
}

#[test]
fn a_finished_milestone_moves_only_if_its_actual_date_does() {
    let (mut earlier, mut later) = (base(), base());
    for (s, stored) in [
        (&mut earlier, "2026-02-02 17:00"),
        (&mut later, "2026-02-09 17:00"),
    ] {
        let a = activity(s, 3);
        a.activity_type = ActivityType::FinishMilestone;
        a.status = ActivityStatus::Completed;
        a.actual_finish = Some(at("2026-02-02 17:00"));
        a.stored.early_finish = Some(at(stored));
    }
    assert_eq!(compare(&earlier, &later).milestones_moved, []);
    activity(&mut later, 3).actual_finish = Some(at("2026-02-03 17:00"));
    assert_eq!(
        compare(&earlier, &later).milestones_moved[0].hours,
        Some(24.0)
    );
}

/// A2 under way in both updates, with a week between their data dates.
fn work_under_way() -> (Schedule, Schedule) {
    let (mut earlier, mut later) = (base(), base());
    earlier.project.data_date = Some(at("2026-01-05 08:00"));
    later.project.data_date = Some(at("2026-01-12 08:00"));
    for s in [&mut earlier, &mut later] {
        let a = activity(s, 2);
        a.status = ActivityStatus::InProgress;
        a.actual_start = Some(at("2026-01-02 08:00"));
    }
    (earlier, later)
}

#[test]
fn work_used_up_between_updates_is_not_a_duration_cut() {
    let (earlier, mut later) = work_under_way();
    let before = activity(&mut later, 2).remaining_duration;
    activity(&mut later, 2).remaining_duration = Minutes(before.get() - DAY);
    assert_eq!(compare(&earlier, &later).findings, []);
}

#[test]
fn a_cut_bigger_than_the_time_between_updates_is_flagged() {
    let (mut earlier, mut later) = work_under_way();
    activity(&mut earlier, 2).remaining_duration = Minutes(40 * DAY);
    activity(&mut later, 2).remaining_duration = Minutes(DAY);
    let comparison = compare(&earlier, &later);
    assert_eq!(comparison.findings[0].kind, Suspicion::CriticalDurationCut);
}

#[test]
fn a_new_secondary_constraint_is_flagged() {
    let mut later = base();
    activity(&mut later, 2).secondary_constraint = Some(Constraint {
        kind: ConstraintType::FinishOnOrBefore,
        date: Some(at("2026-02-02 17:00")),
    });
    let comparison = compare(&base(), &later);
    assert_eq!(comparison.findings[0].kind, Suspicion::NewConstraint);
}

#[test]
fn a_calendar_change_on_critical_work_is_flagged() {
    let mut later = base();
    activity(&mut later, 2).calendar = CalendarId(2);
    let comparison = compare(&base(), &later);
    assert_eq!(
        comparison.findings[0].kind,
        Suspicion::CriticalCalendarChange
    );
    assert_eq!(
        comparison.findings[0].detail,
        "calendar went from Five day to Six day"
    );
}

#[test]
fn a_calendar_change_off_the_path_is_not_flagged() {
    let mut later = base();
    activity(&mut later, 3).calendar = CalendarId(2);
    assert_eq!(compare(&base(), &later).findings, []);
}

#[test]
fn a_rewritten_actual_is_flagged_and_a_new_one_is_not() {
    let mut earlier = base();
    activity(&mut earlier, 1).actual_start = Some(at("2026-01-05 08:00"));
    let mut later = earlier.clone();
    activity(&mut later, 1).actual_finish = Some(at("2026-01-09 17:00"));
    assert_eq!(compare(&earlier, &later).findings, []);
    activity(&mut later, 1).actual_start = Some(at("2026-01-02 08:00"));
    let comparison = compare(&earlier, &later);
    assert_eq!(comparison.findings[0].kind, Suspicion::ActualChanged);
    assert_eq!(
        comparison.findings[0].detail,
        "actual start went from 2026-01-05 08:00 to 2026-01-02 08:00"
    );
}

#[test]
fn a_removed_actual_is_flagged() {
    let mut earlier = base();
    activity(&mut earlier, 1).actual_finish = Some(at("2026-01-09 17:00"));
    let mut later = earlier.clone();
    activity(&mut later, 1).actual_finish = None;
    let comparison = compare(&earlier, &later);
    assert_eq!(comparison.findings[0].kind, Suspicion::ActualChanged);
    assert_eq!(
        comparison.findings[0].detail,
        "actual finish of 2026-01-09 17:00 was removed"
    );
}

#[test]
fn a_renamed_calendar_is_not_a_move() {
    let mut later = base();
    later.calendars.get_mut(&CalendarId(1)).unwrap().name = "Five-day week".to_owned();
    assert_eq!(compare(&base(), &later).findings, []);
}

#[test]
fn a_new_id_for_the_same_calendar_is_not_a_move_but_different_hours_are() {
    let mut later = base();
    let mut copy = later.calendars[&CalendarId(1)].clone();
    copy.id = CalendarId(9);
    later.calendars.insert(CalendarId(9), copy);
    activity(&mut later, 2).calendar = CalendarId(9);
    assert_eq!(compare(&base(), &later).findings, []);
    later
        .calendars
        .get_mut(&CalendarId(9))
        .unwrap()
        .exceptions
        .insert("2026-01-05".parse().unwrap(), Vec::new());
    assert_eq!(
        compare(&base(), &later).findings[0].kind,
        Suspicion::CriticalCalendarChange
    );
}

#[test]
fn deleted_logic_on_critical_work_is_flagged() {
    let mut later = base();
    later.relationships.remove(&RelationshipId(1));
    let comparison = compare(&base(), &later);
    assert_eq!(comparison.findings[0].kind, Suspicion::CriticalLogicDeleted);
    assert_eq!(comparison.findings[0].subject, "A1 to A2");
}

#[test]
fn a_new_constraint_is_flagged() {
    let mut later = base();
    activity(&mut later, 3).primary_constraint = Some(Constraint {
        kind: ConstraintType::MandatoryStart,
        date: Some(at("2026-01-20 08:00")),
    });
    let comparison = compare(&base(), &later);
    assert_eq!(comparison.findings[0].kind, Suspicion::NewConstraint);
    assert_eq!(
        comparison.findings[0].detail,
        "mandatory start 2026-01-20 08:00"
    );
}

#[test]
fn limiting_a_comparison_keeps_its_counts() {
    let mut later = base();
    for n in 1..=3 {
        activity(&mut later, n).name = "Renamed".to_owned();
    }
    let mut comparison = compare(&base(), &later);
    comparison.limit(1);
    assert_eq!(comparison.changed.len(), 1);
    assert_eq!(comparison.counts.changed, 3);
}

#[test]
fn durations_and_lags_keep_their_sign() {
    assert_eq!(super::hours(Minutes(-30)), "-0h30m");
    assert_eq!(super::hours(Minutes(-100)), "-1h40m");
    assert_eq!(super::hours(Minutes(100)), "1h40m");
    assert_eq!(super::hours(Minutes(-120)), "-2h");

    let (mut earlier, mut later) = (base(), base());
    earlier
        .relationships
        .get_mut(&RelationshipId(1))
        .unwrap()
        .lag = Minutes(30);
    later.relationships.get_mut(&RelationshipId(1)).unwrap().lag = Minutes(-30);
    let lag = &compare(&earlier, &later).links_changed[0];
    assert_ne!(lag.from_lag, lag.to_lag);
}

#[test]
fn a_cut_to_original_duration_says_so() {
    let mut later = base();
    activity(&mut later, 2).original_duration = Minutes(2 * DAY);
    let comparison = compare(&base(), &later);
    assert_eq!(
        comparison.findings[0].detail,
        "original duration went from 40h to 16h with no progress"
    );
}
