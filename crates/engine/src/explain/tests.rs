use super::*;
use crate::schedule;
use std::collections::BTreeMap;
use totalfloat_model::CalendarId;
use totalfloat_model::{
    Activity, ActivityStatus, Calendar, CalendarType, Constraint, HoursPerPeriod,
    OutOfSequenceMode, Project, ProjectId, Relationship, RelationshipId, TimeOfDay, WbsId,
    WorkInterval, WorkWeek,
};

fn at(text: &str) -> DateTime {
    text.parse().unwrap()
}

fn weekday_calendar(id: u64) -> Calendar {
    Calendar {
        id: CalendarId(id),
        name: "Authored".into(),
        kind: CalendarType::Global,
        project: None,
        base: None,
        is_default: false,
        hours_per_period: HoursPerPeriod::default(),
        work_week: WorkWeek(std::array::from_fn(|day| {
            if (1..=5).contains(&day) {
                vec![
                    WorkInterval::new(
                        TimeOfDay::from_minutes(480).unwrap(),
                        TimeOfDay::from_minutes(1020).unwrap(),
                    )
                    .unwrap(),
                ]
            } else {
                vec![]
            }
        })),
        exceptions: BTreeMap::new(),
        source: None,
    }
}

/// Tasks of the given minutes, in an FS chain, coded `A1`, `A2`, ...
fn chain(durations: &[i64], lag: i64) -> Schedule {
    let mut project = Project::new(ProjectId(1), "AUTHORED");
    project.planned_start = Some(at("2026-01-05 08:00"));
    project.data_date = project.planned_start;
    project.default_calendar = Some(CalendarId(1));
    let mut s = Schedule::new(project);
    s.calendars.insert(CalendarId(1), weekday_calendar(1));
    for (i, duration) in durations.iter().enumerate() {
        let id = u64::try_from(i).unwrap() + 1;
        let mut a = Activity::new(
            ActivityId(id),
            ProjectId(1),
            WbsId(1),
            CalendarId(1),
            format!("A{id}"),
            "Authored",
        );
        a.remaining_duration = Minutes(*duration);
        a.original_duration = Minutes(*duration);
        s.activities.insert(ActivityId(id), a);
        if id > 1 {
            s.relationships.insert(
                RelationshipId(id),
                Relationship {
                    id: RelationshipId(id),
                    predecessor: ActivityId(id - 1),
                    successor: ActivityId(id),
                    predecessor_project: ProjectId(1),
                    successor_project: ProjectId(1),
                    kind: RelationshipType::FinishToStart,
                    lag: Minutes(lag),
                    source: None,
                },
            );
        }
    }
    s
}

fn why(s: &Schedule, id: u64, field: DateField) -> Explanation {
    let result = schedule(s, &s.project.options).unwrap();
    explain(s, &result, ActivityId(id), field).unwrap()
}

#[test]
fn a_relationship_chain_walks_back_to_the_data_date() {
    let s = chain(&[60, 60, 60], 0);
    let e = why(&s, 3, DateField::EarlyFinish);
    assert_eq!(e.origin(), Some(&DateProvenance::DataDate));
    assert_eq!(
        e.render(),
        "A3 early finish 2026-01-05 11:00: its early start and its duration\n\
         A3 early start 2026-01-05 10:00: FS from A2 early finish\n\
         A2 early finish 2026-01-05 10:00: its early start and its duration\n\
         A2 early start 2026-01-05 09:00: FS from A1 early finish\n\
         A1 early finish 2026-01-05 09:00: its early start and its duration\n\
         A1 early start 2026-01-05 08:00: data date\n"
    );
}

#[test]
fn lag_and_a_calendar_move_are_stated() {
    let s = chain(&[480, 60], 90);
    let e = why(&s, 2, DateField::EarlyStart);
    let text = e.render();
    assert!(
        text.contains("FS with 1h30m lag on the predecessor calendar"),
        "{text}"
    );
    assert_eq!(e.origin(), Some(&DateProvenance::DataDate));

    // Saturday's data date moves to Monday's working time.
    let mut s = chain(&[60], 0);
    s.project.data_date = Some(at("2026-01-03 12:00"));
    s.project.planned_start = s.project.data_date;
    let text = why(&s, 1, DateField::EarlyStart).render();
    assert!(
        text.contains("data date, moved from 2026-01-03 12:00 to working time on calendar 1"),
        "{text}"
    );
}

#[test]
fn a_constraint_is_an_origin() {
    let mut s = chain(&[60, 60], 0);
    s.activities
        .get_mut(&ActivityId(1))
        .unwrap()
        .primary_constraint = Some(Constraint {
        kind: ConstraintType::StartOnOrAfter,
        date: Some(at("2026-01-07 08:00")),
    });
    let e = why(&s, 2, DateField::EarlyStart);
    assert!(matches!(
        e.origin(),
        Some(DateProvenance::Constraint { .. })
    ));
    assert!(
        e.render()
            .contains("start on or after constraint at 2026-01-07 08:00")
    );
}

#[test]
fn an_actual_start_is_an_origin() {
    let mut s = chain(&[600, 60], 3 * 1440);
    s.relationships.get_mut(&RelationshipId(2)).unwrap().kind = RelationshipType::StartToStart;
    let a = s.activities.get_mut(&ActivityId(1)).unwrap();
    a.status = ActivityStatus::InProgress;
    a.actual_start = Some(at("2026-01-04 08:00"));
    s.project.options.ss_lag_from_early_start = false;
    let e = why(&s, 2, DateField::EarlyStart);
    assert_eq!(e.origin(), Some(&DateProvenance::Actual));
    assert!(
        e.render()
            .contains("A1 actual start 2026-01-04 08:00: actual date")
    );
}

#[test]
fn an_open_end_is_an_origin() {
    let mut s = chain(&[60], 0);
    let mut b = s.activities[&ActivityId(1)].clone();
    b.id = ActivityId(2);
    b.code = "B".into();
    b.remaining_duration = Minutes(240);
    s.activities.insert(ActivityId(2), b);
    s.project.options.make_open_ended_critical = true;
    let e = why(&s, 1, DateField::LateFinish);
    assert_eq!(e.origin(), Some(&DateProvenance::OpenEnd));
    assert!(
        e.render()
            .contains("A1 late finish 2026-01-05 09:00: open end")
    );
}

#[test]
fn late_dates_walk_forward_to_the_project_finish() {
    let s = chain(&[60, 60], 0);
    let e = why(&s, 1, DateField::LateStart);
    assert_eq!(e.origin(), Some(&DateProvenance::ProjectFinish));
    assert_eq!(e.steps.last().unwrap().activity, ActivityId(2));
}

#[test]
fn as_late_as_possible_work_keeps_walking_past_the_move() {
    let mut s = chain(&[60, 60], 0);
    s.activities
        .get_mut(&ActivityId(2))
        .unwrap()
        .primary_constraint = Some(Constraint {
        kind: ConstraintType::AsLateAsPossible,
        date: None,
    });
    let mut last = s.activities[&ActivityId(1)].clone();
    last.id = ActivityId(3);
    last.code = "A3".into();
    last.remaining_duration = Minutes(240);
    s.activities.insert(ActivityId(3), last);
    let e = why(&s, 2, DateField::EarlyFinish);
    assert!(
        e.render()
            .contains("as late as possible, up to: project finish"),
        "{}",
        e.render()
    );
    assert_eq!(e.origin(), Some(&DateProvenance::ProjectFinish));
}

#[test]
fn a_long_chain_shows_both_ends_and_counts_the_middle() {
    let s = chain(&[10; 30], 0);
    let e = why(&s, 30, DateField::EarlyStart);
    assert_eq!(e.steps.len(), 59);
    let text = e.render();
    assert_eq!(text.lines().count(), 2 * RENDERED_ENDS + 1);
    assert!(text.starts_with("A30 early start"));
    assert!(text.contains(&format!("... {} more steps ...", 59 - 2 * RENDERED_ENDS)));
    assert!(text.ends_with("A1 early start 2026-01-05 08:00: data date\n"));
}

#[test]
fn a_chain_of_exactly_both_ends_isn_t_summarized() {
    let s = chain(&[10; 4], 0);
    let e = why(&s, 4, DateField::EarlyFinish);
    assert_eq!(e.steps.len(), 2 * RENDERED_ENDS);
    assert!(!e.render().contains("more steps"));
}

#[test]
fn a_link_to_another_project_is_named_as_outside_it() {
    let s = chain(&[60], 0);
    let mut result = schedule(&s, &s.project.options).unwrap();
    result
        .activities
        .get_mut(&ActivityId(1))
        .unwrap()
        .provenance
        .insert(
            DateField::EarlyStart,
            DateProvenance::Relationship {
                relationship: RelationshipId(9),
                from: ActivityId(99),
                from_date: DateField::EarlyFinish,
                kind: RelationshipType::FinishToStart,
                lag: Minutes(0),
                lag_calendar: LagCalendar::Predecessor,
            },
        );
    let e = explain(&s, &result, ActivityId(1), DateField::EarlyStart).unwrap();
    assert_eq!(e.origin(), None);
    assert!(e.render().contains("activity 99 (outside this project)"));
}

#[test]
fn every_field_answers_or_says_why_not() {
    let mut s = chain(&[60, 60], 0);
    let a = s.activities.get_mut(&ActivityId(1)).unwrap();
    a.status = ActivityStatus::Completed;
    a.actual_start = Some(at("2026-01-02 08:00"));
    a.actual_finish = Some(at("2026-01-02 09:00"));
    a.remaining_duration = Minutes(0);
    s.project.options.out_of_sequence = OutOfSequenceMode::RetainedLogic;
    let result = schedule(&s, &s.project.options).unwrap();
    assert_eq!(
        explain(&s, &result, ActivityId(1), DateField::RemainingEarlyStart),
        Err(ExplainError::NoSuchDate(
            ActivityId(1),
            DateField::RemainingEarlyStart
        ))
    );
    assert_eq!(
        explain(&s, &result, ActivityId(7), DateField::EarlyStart),
        Err(ExplainError::NoSuchActivity(ActivityId(7)))
    );
    assert!(explain(&s, &result, ActivityId(1), DateField::EarlyStart).is_ok());
}

#[test]
fn a_lead_under_an_hour_keeps_its_sign() {
    let text = why(&chain(&[60, 60], -30), 2, DateField::EarlyStart).render();
    assert!(text.contains("-0h30m"), "{text}");
}
