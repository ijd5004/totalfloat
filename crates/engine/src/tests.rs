use super::*;
use std::collections::BTreeMap;
use totalfloat_model::*;
fn at(text: &str) -> DateTime {
    text.parse().unwrap()
}
fn calendar(id: u64, days: &[usize], start: u16, finish: u16) -> Calendar {
    Calendar {
        id: CalendarId(id),
        name: format!("Authored calendar {id}"),
        kind: CalendarType::Global,
        project: None,
        base: None,
        is_default: false,
        hours_per_period: HoursPerPeriod::default(),
        work_week: WorkWeek(std::array::from_fn(|i| {
            if days.contains(&i) {
                vec![
                    WorkInterval::new(
                        TimeOfDay::from_minutes(start).unwrap(),
                        TimeOfDay::from_minutes(finish).unwrap(),
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
fn network(durations: &[i64]) -> Schedule {
    let mut project = Project::new(ProjectId(1), "AUTHORED");
    project.planned_start = Some(at("2026-01-05 08:00"));
    project.data_date = project.planned_start;
    project.default_calendar = Some(CalendarId(1));
    let mut s = Schedule::new(project);
    s.calendars
        .insert(CalendarId(1), calendar(1, &[0, 1, 2, 3, 4, 5, 6], 0, 1440));
    for (i, d) in durations.iter().enumerate() {
        let id = ActivityId(u64::try_from(i).unwrap() + 1);
        let mut a = Activity::new(
            id,
            ProjectId(1),
            WbsId(1),
            CalendarId(1),
            format!("A{}", id.0),
            "Authored task",
        );
        a.remaining_duration = Minutes(*d);
        a.original_duration = Minutes(*d);
        s.activities.insert(id, a);
    }
    s
}
fn edge(s: &mut Schedule, id: u64, a: u64, b: u64, kind: RelationshipType, lag: i64) {
    s.relationships.insert(
        RelationshipId(id),
        Relationship {
            id: RelationshipId(id),
            predecessor: ActivityId(a),
            successor: ActivityId(b),
            predecessor_project: s.project.id,
            successor_project: s.project.id,
            kind,
            lag: Minutes(lag),
            source: None,
        },
    );
}
fn run(s: &Schedule) -> ScheduleResult {
    schedule(s, &s.project.options).unwrap()
}
fn activity(r: &ScheduleResult, id: u64) -> &ActivityResult {
    &r.activities[&ActivityId(id)]
}
#[test]
fn all_relationship_types_and_signed_lags() {
    // Minute offsets are authored arithmetic expectations on a continuous calendar.
    for (kind, positive_start, negative_start) in [
        (RelationshipType::FinishToStart, 300, 180),
        (RelationshipType::StartToStart, 180, 60),
        (RelationshipType::FinishToFinish, 270, 150),
        (RelationshipType::StartToFinish, 150, 30),
    ] {
        for (lag, expected) in [(60, positive_start), (-60, negative_start)] {
            let mut s = network(&[120, 120, 30]);
            edge(&mut s, 1, 1, 2, RelationshipType::FinishToStart, 0);
            edge(&mut s, 2, 2, 3, kind, lag);
            let r = run(&s);
            let start = s.project.data_date.unwrap();
            assert_eq!(
                activity(&r, 3).values.early_start,
                start.checked_add(Minutes(expected)),
                "{kind:?} {lag}"
            );
            assert_eq!(
                activity(&r, 3).values.early_finish,
                start.checked_add(Minutes(expected + 30))
            );
            let a = activity(&r, 2);
            let b = activity(&r, 3);
            let pred = if matches!(
                kind,
                RelationshipType::FinishToStart | RelationshipType::FinishToFinish
            ) {
                a.values.late_finish
            } else {
                a.values.late_start
            }
            .unwrap();
            let succ = if matches!(
                kind,
                RelationshipType::FinishToFinish | RelationshipType::StartToFinish
            ) {
                b.values.late_finish
            } else {
                b.values.late_start
            }
            .unwrap();
            assert!(pred.checked_add(Minutes(lag)).unwrap() <= succ);
            assert_eq!(run(&s), r);
        }
    }
}
#[test]
fn each_lag_calendar_and_mixed_workweeks() {
    let mut s = network(&[480, 60]);
    s.project.data_date = Some(at("2026-01-09 08:00"));
    s.calendars
        .insert(CalendarId(1), calendar(1, &[1, 2, 3, 4, 5], 480, 960));
    s.calendars
        .insert(CalendarId(2), calendar(2, &[0, 1, 2, 3, 4, 5, 6], 0, 1440));
    s.calendars
        .insert(CalendarId(3), calendar(3, &[6], 600, 720));
    s.activities.get_mut(&ActivityId(2)).unwrap().calendar = CalendarId(2);
    s.project.default_calendar = Some(CalendarId(3));
    edge(&mut s, 1, 1, 2, RelationshipType::FinishToStart, 60);
    for (mode, expected) in [
        (LagCalendar::Predecessor, "2026-01-12 09:00"),
        (LagCalendar::Successor, "2026-01-09 17:00"),
        (LagCalendar::TwentyFourHour, "2026-01-09 17:00"),
        (LagCalendar::ProjectDefault, "2026-01-10 11:00"),
    ] {
        s.project.options.lag_calendar = mode;
        assert_eq!(activity(&run(&s), 2).values.early_start, Some(at(expected)));
    }
    // A working exception supersedes the recurring weekend.
    s.project.options.lag_calendar = LagCalendar::Predecessor;
    let saturday = at("2026-01-10 00:00").date();
    s.calendars
        .get_mut(&CalendarId(1))
        .unwrap()
        .exceptions
        .insert(
            saturday,
            vec![
                WorkInterval::new(
                    TimeOfDay::from_minutes(480).unwrap(),
                    TimeOfDay::from_minutes(600).unwrap(),
                )
                .unwrap(),
            ],
        );
    assert_eq!(
        activity(&run(&s), 2).values.early_start,
        Some(at("2026-01-10 09:00"))
    );
}
/// docs/rules/TF-006-lag-calendar.md, items 1, 3 and 4: on the successor's
/// calendar, lag counts the same calendar in both passes.
#[test]
fn tf_006_successor_lag_runs_on_the_successors_calendar_both_ways() {
    // Activity 1 works weekdays 08:00 to 16:00, activity 2 around the clock,
    // and activity 3 sets a later project finish so activity 1 has float.
    let mut s = network(&[480, 60, 7200]);
    s.project.data_date = Some(at("2026-01-09 08:00"));
    s.calendars
        .insert(CalendarId(2), calendar(2, &[1, 2, 3, 4, 5], 480, 960));
    s.activities.get_mut(&ActivityId(1)).unwrap().calendar = CalendarId(2);
    edge(&mut s, 1, 1, 2, RelationshipType::FinishToStart, 1440);
    s.project.options.lag_calendar = LagCalendar::Successor;
    let r = run(&s);
    // Forward: Friday 16:00 plus a day around the clock.
    assert_eq!(
        activity(&r, 2).values.early_start,
        Some(at("2026-01-10 16:00"))
    );
    // Backward: activity 2's late start, Wednesday 07:00, less a day around the
    // clock is Tuesday 07:00, before activity 1's calendar works that day, so
    // activity 1 finishes by Monday 16:00: one working day of float.
    assert_eq!(
        activity(&r, 2).values.late_start,
        Some(at("2026-01-14 07:00"))
    );
    let first = &activity(&r, 1).values;
    assert_eq!(first.late_finish, Some(at("2026-01-12 16:00")));
    assert_eq!(first.total_float, Some(Minutes(480)));
}
#[test]
fn branched_network_float_open_ends_and_provenance() {
    let mut s = network(&[120, 60, 60, 30]);
    edge(&mut s, 9, 1, 3, RelationshipType::FinishToStart, 0);
    edge(&mut s, 2, 2, 3, RelationshipType::FinishToStart, 0);
    // Lowest relationship ID wins a tie, even when it is visited via another predecessor.
    s.activities
        .get_mut(&ActivityId(2))
        .unwrap()
        .remaining_duration = Minutes(120);
    edge(&mut s, 1, 1, 3, RelationshipType::FinishToStart, 0);
    let r = run(&s);
    let end = at("2026-01-05 11:00");
    assert_eq!(r.project.scheduled_finish, Some(end));
    let open = activity(&r, 4);
    assert_eq!(open.values.late_finish, Some(end));
    assert_eq!(open.values.total_float, Some(Minutes(150)));
    assert_eq!(open.values.free_float, Some(Minutes(150)));
    assert_eq!(activity(&r, 1).values.free_float, Some(Minutes(0)));
    let chain = r.driving_chain(ActivityId(3), DateField::EarlyFinish);
    assert_eq!(
        chain[1].provenance.unwrap().driver(),
        &DateProvenance::Relationship {
            relationship: RelationshipId(1),
            from: ActivityId(1),
            from_date: DateField::EarlyFinish,
            kind: RelationshipType::FinishToStart,
            lag: Minutes(0),
            lag_calendar: LagCalendar::Predecessor
        }
    );
    for a in r.activities.values() {
        for f in [
            DateField::EarlyStart,
            DateField::EarlyFinish,
            DateField::LateStart,
            DateField::LateFinish,
            DateField::RemainingEarlyStart,
            DateField::RemainingEarlyFinish,
            DateField::RemainingLateStart,
            DateField::RemainingLateFinish,
        ] {
            assert!(a.date(f).is_some());
            assert!(a.provenance.contains_key(&f));
        }
    }
    for mode in [
        FloatType::StartFloat,
        FloatType::FinishFloat,
        FloatType::SmallestFloat,
    ] {
        s.project.options.float_type = mode;
        assert_eq!(activity(&run(&s), 4).values.total_float, Some(Minutes(150)));
    }
    // Stored output cannot affect a deterministic recalculation.
    for a in s.activities.values_mut() {
        a.stored.early_start = Some(at("1999-01-01 00:00"));
        a.stored.total_float = Some(Minutes(999));
    }
    assert_eq!(run(&s), r);
}
#[test]
fn cycles_report_the_full_loop_and_do_not_recurse() {
    let mut s = network(&[1, 1, 1, 1]);
    edge(&mut s, 1, 1, 2, RelationshipType::FinishToStart, 0);
    edge(&mut s, 2, 2, 3, RelationshipType::FinishToStart, 0);
    edge(&mut s, 3, 3, 1, RelationshipType::FinishToStart, 0);
    edge(&mut s, 4, 3, 4, RelationshipType::FinishToStart, 0);
    assert_eq!(
        schedule(&s, &s.project.options),
        Err(EngineError::Cycle(vec![
            ActivityId(1),
            ActivityId(2),
            ActivityId(3),
            ActivityId(1)
        ]))
    );
    let mut s = network(&[0]);
    edge(&mut s, 1, 1, 1, RelationshipType::StartToStart, 0);
    assert_eq!(
        schedule(&s, &s.project.options),
        Err(EngineError::Cycle(vec![ActivityId(1), ActivityId(1)]))
    );
}
#[test]
fn project_anchors_milestones_and_calendar_adjustment() {
    for kind in [ActivityType::StartMilestone, ActivityType::FinishMilestone] {
        let mut s = network(&[0]);
        s.activities.get_mut(&ActivityId(1)).unwrap().activity_type = kind;
        let r = run(&s);
        let a = activity(&r, 1);
        assert_eq!(a.values.early_start, a.values.late_start);
        assert_eq!(a.values.total_float, Some(Minutes(0)));
    }
    let mut s = network(&[480, 0]);
    s.calendars
        .insert(CalendarId(1), calendar(1, &[1, 2, 3, 4, 5], 480, 960));
    s.project.data_date = Some(at("2026-01-10 00:00"));
    s.activities.get_mut(&ActivityId(2)).unwrap().activity_type = ActivityType::FinishMilestone;
    edge(&mut s, 1, 1, 2, RelationshipType::FinishToFinish, 0);
    let r = run(&s);
    assert_eq!(
        activity(&r, 1).values.early_start,
        Some(at("2026-01-12 08:00"))
    );
    assert_eq!(
        activity(&r, 2).values.early_finish,
        Some(at("2026-01-12 16:00"))
    );
    assert!(matches!(
        activity(&r, 1).provenance[&DateField::EarlyStart],
        DateProvenance::CalendarAdjustment { .. }
    ));
    s.project.data_date = None;
    s.project.planned_start = Some(at("2026-01-13 08:00"));
    assert_eq!(
        activity(&run(&s), 1).provenance[&DateField::EarlyStart],
        DateProvenance::ProjectStart
    );
}
#[test]
fn unsupported_and_invalid_inputs_are_errors_not_partial_results() {
    let mut s = network(&[1]);
    s.activities.get_mut(&ActivityId(1)).unwrap().status = ActivityStatus::InProgress;
    assert_eq!(
        schedule(&s, &s.project.options),
        Err(EngineError::InvalidInput(
            "in-progress activity needs an actual start and no actual finish"
        ))
    );
    let mut s = network(&[1]);
    s.activities
        .get_mut(&ActivityId(1))
        .unwrap()
        .remaining_duration = Minutes(-1);
    assert!(matches!(
        schedule(&s, &s.project.options),
        Err(EngineError::InvalidInput(_))
    ));
    let mut s = network(&[1]);
    s.project.data_date = None;
    s.project.planned_start = None;
    assert!(matches!(
        schedule(&s, &s.project.options),
        Err(EngineError::InvalidInput(_))
    ));
    let mut s = network(&[1]);
    s.calendars.clear();
    assert!(matches!(
        schedule(&s, &s.project.options),
        Err(EngineError::InvalidInput(_))
    ));
    let s = network(&[]);
    assert!(run(&s).activities.is_empty());
    assert_eq!(run(&s).project.scheduled_finish, None);
}

#[test]
fn mixed_calendar_relationships_remain_feasible_in_both_passes() {
    use totalfloat_calendar::WorkCalendar;
    // Cross every endpoint type, lag sign and lag-calendar option. Calendar 1
    // has lunch/weekends; calendar 2 works through both. No stored results claimed.
    for kind in [
        RelationshipType::FinishToStart,
        RelationshipType::StartToStart,
        RelationshipType::FinishToFinish,
        RelationshipType::StartToFinish,
    ] {
        for lag in [-600, -480, -60, -1, 0, 1, 60, 480, 600] {
            for mode in [
                LagCalendar::Predecessor,
                LagCalendar::Successor,
                LagCalendar::TwentyFourHour,
                LagCalendar::ProjectDefault,
            ] {
                for swap in [false, true] {
                    let mut input = network(&[480, 120]);
                    let mut split = calendar(1, &[1, 2, 3, 4, 5], 480, 720);
                    for day in 1..=5 {
                        split.work_week.0[day].push(
                            WorkInterval::new(
                                TimeOfDay::from_minutes(780).unwrap(),
                                TimeOfDay::from_minutes(1020).unwrap(),
                            )
                            .unwrap(),
                        );
                    }
                    input.calendars.insert(CalendarId(1), split);
                    input
                        .calendars
                        .insert(CalendarId(2), calendar(2, &[0, 1, 2, 3, 4, 5, 6], 0, 1440));
                    input
                        .activities
                        .get_mut(&ActivityId(if swap { 1 } else { 2 }))
                        .unwrap()
                        .calendar = CalendarId(2);
                    input.project.data_date = Some(at("2026-01-09 08:00"));
                    input.project.options.lag_calendar = mode;
                    edge(&mut input, 1, 1, 2, kind, lag);
                    let output = run(&input);
                    let pred = input.activities[&ActivityId(1)].calendar;
                    let succ = input.activities[&ActivityId(2)].calendar;
                    let lc = match mode {
                        LagCalendar::Predecessor => {
                            WorkCalendar::new(&input.calendars[&pred]).unwrap()
                        }
                        LagCalendar::Successor => {
                            WorkCalendar::new(&input.calendars[&succ]).unwrap()
                        }
                        LagCalendar::ProjectDefault => {
                            WorkCalendar::new(&input.calendars[&CalendarId(1)]).unwrap()
                        }
                        LagCalendar::TwentyFourHour => WorkCalendar::continuous().unwrap(),
                    };
                    for late in [false, true] {
                        let pred_values = &activity(&output, 1).values;
                        let succ_values = &activity(&output, 2).values;
                        let pf = matches!(
                            kind,
                            RelationshipType::FinishToStart | RelationshipType::FinishToFinish
                        );
                        let sf = matches!(
                            kind,
                            RelationshipType::FinishToFinish | RelationshipType::StartToFinish
                        );
                        let pred_endpoint = match (late, pf) {
                            (false, false) => pred_values.early_start,
                            (false, true) => pred_values.early_finish,
                            (true, false) => pred_values.late_start,
                            (true, true) => pred_values.late_finish,
                        }
                        .unwrap();
                        let succ_endpoint = match (late, sf) {
                            (false, false) => succ_values.early_start,
                            (false, true) => succ_values.early_finish,
                            (true, false) => succ_values.late_start,
                            (true, true) => succ_values.late_finish,
                        }
                        .unwrap();
                        assert!(
                            lc.add_work(pred_endpoint, Minutes(lag)).unwrap() <= succ_endpoint,
                            "{kind:?} {lag} {mode:?} swap={swap} late={late}"
                        );
                    }
                    assert_work_spans(&input, &output);
                }
            }
        }
    }
}

fn assert_work_spans(input: &Schedule, output: &ScheduleResult) {
    use totalfloat_calendar::WorkCalendar;
    for (id, pred_values) in &input.activities {
        let v = &output.activities[id].values;
        let c = WorkCalendar::new(&input.calendars[&pred_values.calendar]).unwrap();
        assert_eq!(
            c.work_between(v.early_start.unwrap(), v.early_finish.unwrap()),
            pred_values.remaining_duration
        );
        assert_eq!(
            c.work_between(v.late_start.unwrap(), v.late_finish.unwrap()),
            pred_values.remaining_duration
        );
        assert!(v.total_float.unwrap().get() >= 0, "nonnegative float");
        assert!(v.free_float.unwrap().get() >= 0, "nonnegative float");
    }
}

#[test]
fn zero_duration_task_has_distinct_start_and_finish_roles() {
    let mut input = network(&[0]);
    input
        .calendars
        .insert(CalendarId(1), calendar(1, &[1, 2, 3, 4, 5], 480, 1020));
    let output = run(&input);
    let v = &activity(&output, 1).values;
    assert_eq!(v.early_start, Some(at("2026-01-05 08:00")));
    assert_eq!(v.early_finish, Some(at("2026-01-02 17:00")));
    assert_eq!(v.late_start, v.early_start);
    assert_eq!(v.late_finish, v.early_finish);
    assert_eq!(v.total_float, Some(Minutes(0)));
    assert_work_spans(&input, &output);
}

#[test]
fn deep_cycles_and_invalid_relationships_return_structured_errors() {
    let mut input = network(&vec![1; 5_000]);
    for id in 1..5_000 {
        edge(
            &mut input,
            id,
            id,
            id + 1,
            RelationshipType::FinishToStart,
            0,
        );
    }
    edge(
        &mut input,
        5_000,
        5_000,
        1,
        RelationshipType::FinishToStart,
        0,
    );
    let Err(EngineError::Cycle(cycle)) = schedule(&input, &input.project.options) else {
        panic!("expected full cycle");
    };
    assert_eq!(cycle.len(), 5_001);
    assert_eq!(cycle.first(), cycle.last());
    let mut input = network(&[1]);
    edge(&mut input, 1, 1, 99, RelationshipType::FinishToStart, 0);
    assert!(matches!(
        schedule(&input, &input.project.options),
        Err(EngineError::InvalidInput(_))
    ));
    input
        .relationships
        .get_mut(&RelationshipId(1))
        .unwrap()
        .successor_project = ProjectId(2);
    assert!(matches!(
        schedule(&input, &input.project.options),
        Err(EngineError::Unsupported(_))
    ));
    let options = ScheduleOptions {
        ignore_external_relationships: true,
        ..input.project.options.clone()
    };
    assert!(schedule(&input, &options).is_ok());
    let mut input = network(&[1, 1]);
    edge(
        &mut input,
        1,
        1,
        2,
        RelationshipType::FinishToStart,
        i64::MIN,
    );
    assert!(schedule(&input, &input.project.options).is_err());
}

#[test]
fn negative_lag_inverse_allows_the_entire_lag_calendar_plateau() {
    let mut input = network(&[60, 60, 480, 180]);
    let mut split = calendar(2, &[1, 2, 3, 4, 5], 480, 720);
    for day in 1..=5 {
        split.work_week.0[day].push(
            WorkInterval::new(
                TimeOfDay::from_minutes(780).unwrap(),
                TimeOfDay::from_minutes(1020).unwrap(),
            )
            .unwrap(),
        );
    }
    input.calendars.insert(CalendarId(2), split);
    input.activities.get_mut(&ActivityId(2)).unwrap().calendar = CalendarId(2);
    input.project.options.lag_calendar = LagCalendar::Successor;
    edge(&mut input, 1, 1, 2, RelationshipType::FinishToStart, -60);
    edge(&mut input, 2, 4, 2, RelationshipType::FinishToStart, 0);
    edge(&mut input, 3, 2, 3, RelationshipType::StartToStart, 180);
    let output = run(&input);
    assert_eq!(
        activity(&output, 2).values.late_start,
        Some(at("2026-01-05 11:00"))
    );
    // Any predecessor finish through 13:00 maps back to 11:00 under -60
    // successor-calendar work minutes. Stopping at 12:00 loses an hour.
    assert_eq!(
        activity(&output, 1).values.late_finish,
        Some(at("2026-01-05 13:00"))
    );
    assert_eq!(
        activity(&output, 1).values.late_start,
        Some(at("2026-01-05 12:00"))
    );
}

#[path = "constraint_tests.rs"]
mod constraints;
#[path = "progress_tests.rs"]
mod progress;

/// Two tasks on a weekday 08:00-17:00 calendar, linked as given.
fn weekday_pair(durations: &[i64], kind: RelationshipType, lag: i64) -> Schedule {
    let mut s = network(durations);
    s.calendars
        .insert(CalendarId(1), calendar(1, &[1, 2, 3, 4, 5], 480, 1020));
    edge(&mut s, 1, 1, 2, kind, lag);
    s
}

#[test]
fn a_finish_bound_on_a_new_work_period_needs_no_extra_time() {
    // SF +9h from A's Monday 08:00 start ends at Monday 17:00, so B needs nine
    // hours back from there: Monday 08:00, with not a minute added.
    let mut s = weekday_pair(&[60, 540], RelationshipType::StartToFinish, 540);
    s.project.data_date = Some(at("2026-01-05 08:00"));
    let r = run(&s);
    assert_eq!(
        activity(&r, 2).values.early_start,
        Some(at("2026-01-05 08:00"))
    );
    assert_eq!(
        activity(&r, 2).values.early_finish,
        Some(at("2026-01-05 17:00"))
    );
}

#[test]
fn a_start_bound_at_the_end_of_a_period_is_kept() {
    // A zero-lag SF link to a milestone finishing Friday 17:00 lets A start at that
    // very moment. C runs later, so only the link limits A.
    let mut s = network(&[60, 0, 60]);
    s.calendars
        .insert(CalendarId(1), calendar(1, &[1, 2, 3, 4, 5], 480, 1020));
    edge(&mut s, 1, 1, 2, RelationshipType::StartToFinish, 0);
    let b = s.activities.get_mut(&ActivityId(2)).unwrap();
    b.activity_type = ActivityType::FinishMilestone;
    b.primary_constraint = Some(Constraint {
        kind: ConstraintType::MandatoryFinish,
        date: Some(at("2026-01-09 17:00")),
    });
    s.activities
        .get_mut(&ActivityId(3))
        .unwrap()
        .primary_constraint = Some(Constraint {
        kind: ConstraintType::StartOnOrAfter,
        date: Some(at("2026-01-14 08:00")),
    });
    let r = run(&s);
    assert_eq!(
        activity(&r, 1).values.late_start,
        Some(at("2026-01-09 17:00"))
    );
}

#[test]
fn a_finish_milestone_ends_the_work_period_before_its_successor() {
    let mut s = weekday_pair(&[0, 60], RelationshipType::FinishToStart, 0);
    let a = s.activities.get_mut(&ActivityId(1)).unwrap();
    a.activity_type = ActivityType::FinishMilestone;
    let r = run(&s);
    assert_eq!(
        activity(&r, 2).values.late_start,
        Some(at("2026-01-05 08:00"))
    );
    assert_eq!(
        activity(&r, 1).values.late_finish,
        Some(at("2026-01-02 17:00"))
    );
    assert_eq!(
        activity(&r, 1).values.late_start,
        Some(at("2026-01-02 17:00"))
    );
}

#[test]
fn relationships_compare_clock_moments_across_calendars() {
    // A works to 17:00 and B to 16:00. A's extra hour is real time on the lag
    // calendar, so FF +2h from B's 16:00 finish leaves A's late finish at 14:00.
    // Measuring from B's next morning instead would give 16:00 the same day. C
    // sets the project finish, so only B's deadline limits the chain.
    let mut s = network(&[60, 60, 1440]);
    s.calendars
        .insert(CalendarId(1), calendar(1, &[1, 2, 3, 4, 5], 480, 1020));
    s.calendars
        .insert(CalendarId(2), calendar(2, &[1, 2, 3, 4, 5], 480, 960));
    s.activities.get_mut(&ActivityId(2)).unwrap().calendar = CalendarId(2);
    edge(&mut s, 1, 1, 2, RelationshipType::FinishToFinish, 120);
    s.activities
        .get_mut(&ActivityId(2))
        .unwrap()
        .primary_constraint = Some(Constraint {
        kind: ConstraintType::FinishOnOrBefore,
        date: Some(at("2026-01-05 16:00")),
    });
    let r = run(&s);
    assert_eq!(
        activity(&r, 2).values.late_finish,
        Some(at("2026-01-05 16:00"))
    );
    assert_eq!(
        activity(&r, 1).values.late_finish,
        Some(at("2026-01-05 14:00"))
    );
}

#[test]
fn the_calculated_end_is_a_moment_a_deadline_normalizes_back() {
    // B's bound falls in the weekend, so its finish reads Monday 08:00, and that
    // calculated end is itself the late finish. Float stays zero.
    let mut s = network(&[480, 120]);
    s.calendars
        .insert(CalendarId(1), calendar(1, &[1, 2, 3, 4, 5], 480, 1020));
    s.calendars
        .insert(CalendarId(2), calendar(2, &[0, 1, 2, 3, 4, 5, 6], 0, 1440));
    s.activities.get_mut(&ActivityId(1)).unwrap().calendar = CalendarId(2);
    s.project.data_date = Some(at("2026-01-09 08:00"));
    edge(&mut s, 1, 1, 2, RelationshipType::FinishToFinish, 480);
    let r = run(&s);
    let b = &activity(&r, 2).values;
    assert_eq!(b.early_finish, Some(at("2026-01-12 08:00")));
    assert_eq!(b.late_finish, b.early_finish);
    assert_eq!(b.total_float, Some(Minutes::ZERO));

    // A Monday-morning deadline is an input, and backs up to Friday's end.
    let mut d = network(&[60]);
    d.calendars
        .insert(CalendarId(1), calendar(1, &[1, 2, 3, 4, 5], 480, 1020));
    d.project.must_finish_by = Some(at("2026-01-12 08:00"));
    assert_eq!(
        activity(&run(&d), 1).values.late_finish,
        Some(at("2026-01-09 17:00"))
    );
}

#[test]
fn finish_bounds_at_one_position_keep_the_later_moment() {
    // A and B both finish at C's position, but B's is later on the clock, and C
    // can't finish before either. Link order must not matter.
    for (first, second) in [(1, 2), (2, 1)] {
        let mut s = network(&[60, 60, 60]);
        s.calendars
            .insert(CalendarId(1), calendar(1, &[1, 2, 3, 4, 5], 480, 1020));
        s.calendars
            .insert(CalendarId(2), calendar(2, &[0, 1, 2, 3, 4, 5, 6], 0, 1440));
        s.activities.get_mut(&ActivityId(2)).unwrap().calendar = CalendarId(2);
        s.project.data_date = Some(at("2026-01-09 16:00"));
        s.activities
            .get_mut(&ActivityId(2))
            .unwrap()
            .primary_constraint = Some(Constraint {
            kind: ConstraintType::FinishOnOrAfter,
            date: Some(at("2026-01-12 08:00")),
        });
        edge(&mut s, 1, first, 3, RelationshipType::FinishToFinish, 0);
        edge(&mut s, 2, second, 3, RelationshipType::FinishToFinish, 0);
        let r = run(&s);
        assert_eq!(
            activity(&r, 3).values.early_start,
            Some(at("2026-01-09 16:00"))
        );
        assert_eq!(
            activity(&r, 3).values.early_finish,
            Some(at("2026-01-12 08:00"))
        );
        assert_eq!(activity(&r, 2).values.free_float, Some(Minutes::ZERO));
    }
}

/// Stored dates in a real export: a milestone holding a duration takes no time.
#[test]
fn a_milestone_with_a_duration_in_the_file_takes_no_time() {
    let mut s = network(&[60, 480, 60]);
    typed(&mut s, 2, ActivityType::FinishMilestone);
    edge(&mut s, 1, 1, 2, RelationshipType::FinishToStart, 0);
    edge(&mut s, 2, 2, 3, RelationshipType::FinishToStart, 0);
    let r = run(&s);
    assert_eq!(
        activity(&r, 2).values.early_finish,
        activity(&r, 1).values.early_finish
    );
    assert_eq!(
        activity(&r, 3).values.early_finish,
        Some(at("2026-01-05 10:00"))
    );
    assert_eq!(activity(&r, 2).values.total_float, Some(Minutes(0)));
    assert_eq!(
        activity(&r, 2).values.late_finish,
        activity(&r, 2).values.early_finish
    );

    typed(&mut s, 2, ActivityType::StartMilestone);
    let r = run(&s);
    assert_eq!(
        activity(&r, 2).values.early_start,
        activity(&r, 2).values.early_finish
    );
}

#[test]
fn a_milestone_with_a_negative_duration_in_the_file_takes_no_time_either() {
    let mut s = network(&[60]);
    typed(&mut s, 1, ActivityType::FinishMilestone);
    s.activities
        .get_mut(&ActivityId(1))
        .unwrap()
        .remaining_duration = Minutes(-60);
    assert!(schedule(&s, &s.project.options).is_ok());
}

#[test]
fn a_lone_finish_milestone_stays_one_point() {
    let mut s = network(&[0]);
    s.calendars
        .insert(CalendarId(1), calendar(1, &[1, 2, 3, 4, 5], 480, 1020));
    s.activities.get_mut(&ActivityId(1)).unwrap().activity_type = ActivityType::FinishMilestone;
    let r = run(&s);
    let v = &activity(&r, 1).values;
    assert_eq!(v.early_start, Some(at("2026-01-05 08:00")));
    assert_eq!(v.late_start, v.early_start);
    assert_eq!(v.late_finish, v.early_finish);
    assert_eq!(
        activity(&r, 1).provenance[&DateField::LateFinish],
        DateProvenance::ProjectFinish
    );
}

#[test]
fn a_project_end_outside_working_time_backs_up_on_each_calendar() {
    // The 24-hour activity sets a Saturday end. The weekday activity can't finish
    // then, so its late finish is the end of Friday.
    let mut s = network(&[60, 60]);
    s.calendars
        .insert(CalendarId(1), calendar(1, &[1, 2, 3, 4, 5], 480, 1020));
    s.calendars
        .insert(CalendarId(2), calendar(2, &[0, 1, 2, 3, 4, 5, 6], 0, 1440));
    s.activities.get_mut(&ActivityId(2)).unwrap().calendar = CalendarId(2);
    s.activities
        .get_mut(&ActivityId(2))
        .unwrap()
        .primary_constraint = Some(Constraint {
        kind: ConstraintType::StartOnOrAfter,
        date: Some(at("2026-01-10 12:00")),
    });
    let r = run(&s);
    assert_eq!(
        activity(&r, 1).values.late_finish,
        Some(at("2026-01-09 17:00"))
    );
}

#[test]
fn as_late_as_possible_work_reaches_the_same_project_end() {
    let mut s = network(&[60, 480]);
    s.calendars
        .insert(CalendarId(1), calendar(1, &[1, 2, 3, 4, 5], 480, 1020));
    s.calendars
        .insert(CalendarId(2), calendar(2, &[0, 1, 2, 3, 4, 5, 6], 0, 1440));
    s.activities.get_mut(&ActivityId(2)).unwrap().calendar = CalendarId(2);
    s.activities
        .get_mut(&ActivityId(2))
        .unwrap()
        .primary_constraint = Some(Constraint {
        kind: ConstraintType::StartOnOrAfter,
        date: Some(at("2026-01-12 00:00")),
    });
    s.activities
        .get_mut(&ActivityId(1))
        .unwrap()
        .primary_constraint = Some(Constraint {
        kind: ConstraintType::AsLateAsPossible,
        date: None,
    });
    let r = run(&s);
    let v = &activity(&r, 1).values;
    assert_eq!(v.early_finish, v.late_finish);
    assert_eq!(v.total_float, Some(Minutes::ZERO));
}

#[test]
fn an_expected_finish_at_a_finish_bounds_position_is_no_conflict() {
    // The expected finish is Friday's end, and the FF bound is Monday's start on
    // the 24-hour predecessor: the same position on the successor's calendar.
    let mut s = network(&[60, 60]);
    s.calendars
        .insert(CalendarId(1), calendar(1, &[1, 2, 3, 4, 5], 480, 1020));
    s.calendars
        .insert(CalendarId(2), calendar(2, &[0, 1, 2, 3, 4, 5, 6], 0, 1440));
    s.activities.get_mut(&ActivityId(1)).unwrap().calendar = CalendarId(2);
    s.project.data_date = Some(at("2026-01-09 08:00"));
    s.activities
        .get_mut(&ActivityId(1))
        .unwrap()
        .primary_constraint = Some(Constraint {
        kind: ConstraintType::FinishOnOrAfter,
        date: Some(at("2026-01-12 08:00")),
    });
    s.activities
        .get_mut(&ActivityId(2))
        .unwrap()
        .expected_finish = Some(at("2026-01-09 17:00"));
    edge(&mut s, 1, 1, 2, RelationshipType::FinishToFinish, 0);
    let r = run(&s);
    assert_eq!(
        activity(&r, 2).values.early_finish,
        Some(at("2026-01-12 08:00"))
    );
}

fn longest(r: &ScheduleResult, id: u64) -> Option<bool> {
    activity(r, id).values.on_longest_path
}

#[test]
fn the_longest_path_runs_back_through_driving_links() {
    // A drives C, and B finishes early with float, so B stays off the path.
    let mut s = network(&[120, 60, 60]);
    edge(&mut s, 1, 1, 3, RelationshipType::FinishToStart, 0);
    edge(&mut s, 2, 2, 3, RelationshipType::FinishToStart, 0);
    let r = run(&s);
    assert_eq!(longest(&r, 1), Some(true));
    assert_eq!(longest(&r, 2), Some(false));
    assert_eq!(longest(&r, 3), Some(true));
    assert_eq!(activity(&r, 3).driving_relationships, [RelationshipId(1)]);
    assert_eq!(activity(&r, 1).critical, Some(true));
    assert_eq!(activity(&r, 2).critical, Some(false));
}

#[test]
fn equally_driving_predecessors_are_all_on_the_path() {
    let mut s = network(&[120, 120, 60]);
    edge(&mut s, 2, 1, 3, RelationshipType::FinishToStart, 0);
    edge(&mut s, 1, 2, 3, RelationshipType::FinishToStart, 0);
    let r = run(&s);
    assert_eq!(longest(&r, 1), Some(true));
    assert_eq!(longest(&r, 2), Some(true));
    assert_eq!(
        activity(&r, 3).driving_relationships,
        [RelationshipId(1), RelationshipId(2)]
    );
}

#[test]
fn a_longest_path_can_carry_float_and_a_deadline_can_leave_it() {
    // A deadline after the finish gives the whole path float, and it's still the
    // longest path. Critical by float and by longest path now differ.
    let mut s = network(&[60, 60]);
    edge(&mut s, 1, 1, 2, RelationshipType::FinishToStart, 0);
    s.project.must_finish_by = Some(at("2026-01-06 08:00"));
    let r = run(&s);
    assert_eq!(longest(&r, 1), Some(true));
    assert!(activity(&r, 1).values.total_float.unwrap().get() > 0);
    assert_eq!(activity(&r, 1).critical, Some(false));

    s.project.options.critical = CriticalDefinition::LongestPath;
    let r = run(&s);
    assert_eq!(activity(&r, 1).critical, Some(true));
}

#[test]
fn critical_uses_the_float_threshold() {
    let mut s = network(&[120, 60]);
    s.project.options.critical = CriticalDefinition::TotalFloatAtMost(Minutes(60));
    assert_eq!(activity(&run(&s), 2).critical, Some(true));
    s.project.options.critical = CriticalDefinition::TotalFloatAtMost(Minutes(59));
    assert_eq!(activity(&run(&s), 2).critical, Some(false));
}

#[test]
fn a_link_with_slack_on_the_lag_calendar_does_not_drive() {
    // B can't work Tuesday, so it starts Wednesday either way, but Tuesday is a
    // working day on A's lag calendar. The link has a day of slack.
    let mut s = network(&[60, 60]);
    s.calendars
        .insert(CalendarId(1), calendar(1, &[1, 2, 3, 4, 5], 480, 1020));
    s.calendars
        .insert(CalendarId(2), calendar(2, &[1, 3, 4, 5], 480, 1020));
    s.activities.get_mut(&ActivityId(2)).unwrap().calendar = CalendarId(2);
    let a = s.activities.get_mut(&ActivityId(1)).unwrap();
    a.remaining_duration = Minutes(540);
    a.original_duration = Minutes(540);
    edge(&mut s, 1, 1, 2, RelationshipType::FinishToStart, 0);
    s.activities
        .get_mut(&ActivityId(1))
        .unwrap()
        .primary_constraint = Some(Constraint {
        kind: ConstraintType::StartOnOrAfter,
        date: Some(at("2026-01-05 08:00")),
    });
    s.relationships.get_mut(&RelationshipId(1)).unwrap().lag = Minutes(480);
    let r = run(&s);
    assert_eq!(
        activity(&r, 2).values.early_start,
        Some(at("2026-01-07 08:00"))
    );
    assert_eq!(
        activity(&r, 2).driving_relationships,
        Vec::<RelationshipId>::new()
    );
    assert_eq!(longest(&r, 1), Some(false));
}

#[test]
fn the_path_runs_through_finished_work_but_not_a_broken_link() {
    let mut s = network(&[60, 60]);
    edge(&mut s, 1, 1, 2, RelationshipType::FinishToStart, 0);
    let a = s.activities.get_mut(&ActivityId(1)).unwrap();
    a.status = ActivityStatus::Completed;
    a.actual_start = Some(at("2026-01-02 08:00"));
    a.actual_finish = Some(at("2026-01-02 09:00"));
    assert_eq!(longest(&run(&s), 1), Some(true));

    // Progress Override drops the broken link, so it can't drive.
    let mut s = network(&[120, 60]);
    edge(&mut s, 1, 1, 2, RelationshipType::FinishToStart, 0);
    let b = s.activities.get_mut(&ActivityId(2)).unwrap();
    b.status = ActivityStatus::InProgress;
    b.actual_start = Some(at("2026-01-04 08:00"));
    s.project.options.out_of_sequence = OutOfSequenceMode::ProgressOverride;
    let r = run(&s);
    assert_eq!(
        activity(&r, 2).driving_relationships,
        Vec::<RelationshipId>::new()
    );
}

#[test]
fn open_ended_work_can_be_made_critical() {
    // B has no successors and finishes before the project does.
    let mut s = network(&[120, 60]);
    assert!(activity(&run(&s), 2).values.total_float.unwrap().get() > 0);
    s.project.options.make_open_ended_critical = true;
    let r = run(&s);
    assert_eq!(activity(&r, 2).values.total_float, Some(Minutes::ZERO));
    assert_eq!(activity(&r, 2).critical, Some(true));
}

#[test]
fn an_overnight_gap_is_not_slack_on_a_24_hour_lag_calendar() {
    let mut s = network(&[540, 60]);
    s.calendars
        .insert(CalendarId(1), calendar(1, &[1, 2, 3, 4, 5], 480, 1020));
    edge(&mut s, 1, 1, 2, RelationshipType::FinishToStart, 0);
    s.project.options.lag_calendar = LagCalendar::TwentyFourHour;
    let r = run(&s);
    assert_eq!(activity(&r, 2).driving_relationships, [RelationshipId(1)]);
    assert_eq!(longest(&r, 1), Some(true));
}

#[test]
fn only_reaching_the_finish_on_the_clock_starts_the_path() {
    // A ends Friday and the 7-day B ends Sunday. A has no work left before Sunday,
    // but it still finished two days before the project did.
    let mut s = network(&[540, 60]);
    s.calendars
        .insert(CalendarId(1), calendar(1, &[1, 2, 3, 4, 5], 480, 1020));
    s.calendars
        .insert(CalendarId(2), calendar(2, &[0, 1, 2, 3, 4, 5, 6], 0, 1440));
    s.project.data_date = Some(at("2026-01-09 08:00"));
    s.activities.get_mut(&ActivityId(2)).unwrap().calendar = CalendarId(2);
    s.activities
        .get_mut(&ActivityId(2))
        .unwrap()
        .primary_constraint = Some(Constraint {
        kind: ConstraintType::StartOnOrAfter,
        date: Some(at("2026-01-11 16:00")),
    });
    let r = run(&s);
    assert_eq!(longest(&r, 1), Some(false));
    assert_eq!(longest(&r, 2), Some(true));
}

#[test]
fn an_open_end_keeps_the_deadline_and_its_own_provenance() {
    // Past the deadline, the deadline still wins, so float stays negative.
    let mut s = network(&[120, 60]);
    s.project.options.make_open_ended_critical = true;
    s.project.must_finish_by = Some(at("2026-01-05 08:30"));
    let r = run(&s);
    assert!(activity(&r, 2).values.total_float.unwrap().get() < 0);

    let mut s = network(&[120, 60]);
    s.project.options.make_open_ended_critical = true;
    let r = run(&s);
    let b = activity(&r, 2);
    assert_eq!(
        b.provenance[&DateField::LateFinish],
        DateProvenance::OpenEnd
    );
    assert_eq!(b.values.free_float, Some(Minutes::ZERO));

    // Critical whatever the definition, because that's what the option asks for.
    s.project.options.critical = CriticalDefinition::LongestPath;
    assert_eq!(activity(&run(&s), 2).critical, Some(true));
    s.project.options.critical = CriticalDefinition::TotalFloatAtMost(Minutes(-60));
    assert_eq!(activity(&run(&s), 2).critical, Some(true));
}

#[test]
fn a_dropped_link_leaves_its_predecessor_an_open_end() {
    let mut s = network(&[60, 60, 600]);
    edge(&mut s, 1, 1, 2, RelationshipType::FinishToStart, 0);
    let b = s.activities.get_mut(&ActivityId(2)).unwrap();
    b.status = ActivityStatus::InProgress;
    b.actual_start = Some(at("2026-01-04 08:00"));
    s.project.options.out_of_sequence = OutOfSequenceMode::ProgressOverride;
    s.project.options.make_open_ended_critical = true;
    let r = run(&s);
    assert_eq!(activity(&r, 1).values.total_float, Some(Minutes::ZERO));
}

fn typed(s: &mut Schedule, id: u64, kind: ActivityType) {
    s.activities.get_mut(&ActivityId(id)).unwrap().activity_type = kind;
}

#[test]
fn level_of_effort_spans_its_links_and_drives_nothing() {
    // L starts with A and finishes with B, and it can't delay C.
    let mut s = network(&[60, 180, 60, 0]);
    typed(&mut s, 4, ActivityType::LevelOfEffort);
    edge(&mut s, 1, 1, 2, RelationshipType::FinishToStart, 0);
    edge(&mut s, 2, 1, 4, RelationshipType::StartToStart, 0);
    edge(&mut s, 3, 2, 4, RelationshipType::FinishToFinish, 0);
    edge(&mut s, 4, 4, 3, RelationshipType::FinishToStart, 0);
    let r = run(&s);
    let l = activity(&r, 4);
    assert_eq!(l.values.early_start, Some(at("2026-01-05 08:00")));
    assert_eq!(l.values.early_finish, Some(at("2026-01-05 12:00")));
    assert_eq!(l.values.free_float, Some(Minutes::ZERO));
    assert_eq!(
        l.provenance[&DateField::EarlyStart],
        DateProvenance::LoeSpan
    );
    // No float, because L spans the chain that sets the project finish.
    assert_eq!(l.values.total_float, Some(Minutes::ZERO));
    assert_eq!(l.values.on_longest_path, Some(true));
    assert_eq!(
        activity(&r, 3).values.early_start,
        Some(at("2026-01-05 08:00"))
    );
    assert_eq!(
        activity(&r, 3).driving_relationships,
        Vec::<RelationshipId>::new()
    );
}

#[test]
fn a_level_of_effort_takes_its_earliest_start_and_latest_finish() {
    let mut s = network(&[60, 180, 60, 0]);
    typed(&mut s, 4, ActivityType::LevelOfEffort);
    edge(&mut s, 1, 1, 3, RelationshipType::FinishToStart, 0);
    edge(&mut s, 2, 1, 4, RelationshipType::StartToStart, 0);
    edge(&mut s, 3, 3, 4, RelationshipType::StartToStart, 0);
    edge(&mut s, 4, 3, 4, RelationshipType::FinishToFinish, 0);
    edge(&mut s, 5, 2, 4, RelationshipType::FinishToFinish, 0);
    let r = run(&s);
    let l = &activity(&r, 4).values;
    assert_eq!(l.early_start, Some(at("2026-01-05 08:00")));
    assert_eq!(l.early_finish, Some(at("2026-01-05 11:00")));
}

#[test]
fn a_level_of_effort_late_dates_come_from_late_neighbors() {
    let mut s = network(&[60, 180, 0]);
    typed(&mut s, 3, ActivityType::LevelOfEffort);
    edge(&mut s, 1, 1, 3, RelationshipType::StartToStart, 0);
    edge(&mut s, 2, 1, 3, RelationshipType::FinishToFinish, 0);
    let r = run(&s);
    let (a, l) = (&activity(&r, 1).values, &activity(&r, 3).values);
    assert_eq!(l.late_start, a.late_start);
    assert_eq!(l.late_finish, a.late_finish);
}

#[test]
fn each_pass_holds_its_own_end_when_a_level_of_effort_crosses() {
    // L starts with B and finishes with A, which comes first, so its bounds
    // cross. Forward holds the start and backward holds the finish.
    let mut s = network(&[60, 60, 180, 0]);
    typed(&mut s, 4, ActivityType::LevelOfEffort);
    edge(&mut s, 1, 1, 2, RelationshipType::FinishToStart, 60);
    edge(&mut s, 2, 2, 4, RelationshipType::StartToStart, 0);
    edge(&mut s, 3, 4, 1, RelationshipType::FinishToFinish, 0);
    let r = run(&s);
    assert_eq!(
        activity(&r, 1).values.late_finish,
        Some(at("2026-01-05 09:00"))
    );
    assert_eq!(
        activity(&r, 2).values.late_start,
        Some(at("2026-01-05 10:00"))
    );
    let l = activity(&r, 4);
    assert_eq!(l.values.early_start, Some(at("2026-01-05 10:00")));
    assert_eq!(l.values.early_finish, Some(at("2026-01-05 10:00")));
    assert_eq!(l.values.late_start, Some(at("2026-01-05 09:00")));
    assert_eq!(l.values.late_finish, Some(at("2026-01-05 09:00")));
    // Crossing makes the float negative, which isn't the path's float.
    assert!(l.values.total_float.unwrap() < Minutes::ZERO);
    assert_eq!(l.values.on_longest_path, Some(false));
}

#[test]
fn a_level_of_effort_with_no_start_link_measures_back_from_its_finish() {
    let mut s = network(&[60, 300, 120]);
    typed(&mut s, 3, ActivityType::LevelOfEffort);
    edge(&mut s, 1, 3, 1, RelationshipType::FinishToFinish, 0);
    let r = run(&s);
    let l = activity(&r, 3);
    assert_eq!(l.values.late_finish, activity(&r, 1).values.late_finish);
    let finish = l.values.late_finish.unwrap();
    assert_eq!(
        finish.minutes_since(l.values.late_start.unwrap()),
        Minutes(120)
    );
}

#[test]
fn a_span_on_a_path_with_float_stays_on_it() {
    // A deadline after the finish gives the whole path float, spans included.
    let mut s = network(&[60, 180, 0]);
    s.project.must_finish_by = Some(at("2026-01-05 15:00"));
    typed(&mut s, 3, ActivityType::LevelOfEffort);
    edge(&mut s, 1, 1, 2, RelationshipType::FinishToStart, 0);
    edge(&mut s, 2, 1, 3, RelationshipType::StartToStart, 0);
    edge(&mut s, 3, 2, 3, RelationshipType::FinishToFinish, 0);
    let r = run(&s);
    let l = activity(&r, 3);
    assert_eq!(l.values.total_float, activity(&r, 2).values.total_float);
    assert!(l.values.total_float.unwrap() > Minutes::ZERO);
    assert_eq!(l.values.on_longest_path, Some(true));
}

/// A to B to C, with D also into C but finishing an hour sooner, and E apart.
fn float_path_network() -> Schedule {
    let mut s = network(&[60, 60, 60, 60, 60]);
    s.project.options.float_paths.enabled = true;
    edge(&mut s, 1, 1, 2, RelationshipType::FinishToStart, 0);
    edge(&mut s, 2, 2, 3, RelationshipType::FinishToStart, 0);
    edge(&mut s, 3, 4, 3, RelationshipType::FinishToStart, 0);
    s
}

fn on_path(r: &ScheduleResult, id: u64) -> Option<(u32, u32)> {
    let values = &activity(r, id).values;
    values.float_path.zip(values.float_path_order)
}

#[test]
fn paths_run_from_the_lowest_relative_float() {
    let r = run(&float_path_network());
    assert_eq!(on_path(&r, 1), Some((1, 1)));
    assert_eq!(on_path(&r, 2), Some((1, 2)));
    assert_eq!(on_path(&r, 3), Some((1, 3)));
    assert_eq!(on_path(&r, 4), Some((2, 1)));
    assert_eq!(on_path(&r, 5), None);
}

#[test]
fn paths_stop_at_the_limit_and_need_the_option() {
    let mut s = float_path_network();
    s.project.options.float_paths.max_paths = Some(1);
    assert_eq!(on_path(&run(&s), 4), None);
    assert_eq!(on_path(&run(&s), 3), Some((1, 3)));

    s.project.options.float_paths.enabled = false;
    assert_eq!(on_path(&run(&s), 1), None);
}

#[test]
fn paths_end_at_the_chosen_activity() {
    let mut s = float_path_network();
    s.project.options.float_paths.end_activity = Some(ActivityId(2));
    let r = run(&s);
    assert_eq!(on_path(&r, 1), Some((1, 1)));
    assert_eq!(on_path(&r, 2), Some((1, 2)));
    assert_eq!(on_path(&r, 3), None);
    assert_eq!(on_path(&r, 4), None);
}

#[test]
fn a_finished_activity_is_on_no_path() {
    let mut s = float_path_network();
    let a = s.activities.get_mut(&ActivityId(1)).unwrap();
    a.status = ActivityStatus::Completed;
    a.actual_start = Some(at("2026-01-01 08:00"));
    a.actual_finish = Some(at("2026-01-01 09:00"));
    let r = run(&s);
    assert_eq!(on_path(&r, 1), None);
    assert_eq!(on_path(&r, 2), Some((1, 1)));
}

/// A (600) to X (60) drives; B (420) into X has 180 minutes of slack; E (390)
/// finishes 210 before A, by an FF link; C (360) finishes 60 before B. So the
/// relative floats are B 180, E 210, C 240, and C is 60 from B.
fn free_float_network() -> Schedule {
    let mut s = network(&[600, 60, 420, 390, 360]);
    s.project.options.float_paths.enabled = true;
    edge(&mut s, 1, 1, 2, RelationshipType::FinishToStart, 0);
    edge(&mut s, 2, 3, 2, RelationshipType::FinishToStart, 0);
    edge(&mut s, 3, 4, 1, RelationshipType::FinishToFinish, 0);
    edge(&mut s, 4, 5, 3, RelationshipType::FinishToFinish, 0);
    s
}

#[test]
fn float_paths_by_total_float_rise_with_relative_float() {
    let r = run(&free_float_network());
    assert_eq!(on_path(&r, 1), Some((1, 1)));
    assert_eq!(on_path(&r, 2), Some((1, 2)));
    assert_eq!(on_path(&r, 3), Some((2, 1)));
    assert_eq!(on_path(&r, 4), Some((3, 1)));
    assert_eq!(on_path(&r, 5), Some((4, 1)));
}

#[test]
fn float_paths_by_free_float_take_the_least_slack_to_a_path_found() {
    let mut s = free_float_network();
    s.project.options.float_paths.mode = FloatPathMode::FreeFloat;
    let r = run(&s);
    assert_eq!(on_path(&r, 1), Some((1, 1)));
    assert_eq!(on_path(&r, 2), Some((1, 2)));
    // B is 180 from path 1 and E 210; once B is a path, C is 60 from it.
    assert_eq!(on_path(&r, 3), Some((2, 1)));
    assert_eq!(on_path(&r, 5), Some((3, 1)));
    assert_eq!(on_path(&r, 4), Some((4, 1)));
}

#[test]
fn float_paths_that_tie_go_to_the_later_finish_then_the_earlier_position() {
    // B (300, lag 120), F (420) and G (420) are each 180 from X.
    let mut s = network(&[600, 60, 300, 420, 420]);
    s.project.options.float_paths.enabled = true;
    edge(&mut s, 1, 1, 2, RelationshipType::FinishToStart, 0);
    edge(&mut s, 2, 3, 2, RelationshipType::FinishToStart, 120);
    edge(&mut s, 3, 4, 2, RelationshipType::FinishToStart, 0);
    edge(&mut s, 4, 5, 2, RelationshipType::FinishToStart, 0);
    for mode in [FloatPathMode::TotalFloat, FloatPathMode::FreeFloat] {
        s.project.options.float_paths.mode = mode;
        let r = run(&s);
        assert_eq!(on_path(&r, 4), Some((2, 1)), "{mode:?}");
        assert_eq!(on_path(&r, 5), Some((3, 1)), "{mode:?}");
        assert_eq!(on_path(&r, 3), Some((4, 1)), "{mode:?}");
    }
}

#[test]
fn float_paths_start_at_the_end_activity_even_when_a_predecessor_ties_with_it() {
    // A finish milestone finishes with its predecessor, which comes first in
    // the file; path 1 still starts at the milestone, the end activity.
    let mut s = network(&[480, 0]);
    s.activities.get_mut(&ActivityId(2)).unwrap().activity_type = ActivityType::FinishMilestone;
    s.project.options.float_paths.enabled = true;
    s.project.options.float_paths.end_activity = Some(ActivityId(2));
    edge(&mut s, 1, 1, 2, RelationshipType::FinishToStart, 0);
    for mode in [FloatPathMode::TotalFloat, FloatPathMode::FreeFloat] {
        s.project.options.float_paths.mode = mode;
        let r = run(&s);
        assert_eq!(on_path(&r, 1), Some((1, 1)), "{mode:?}");
        assert_eq!(on_path(&r, 2), Some((1, 2)), "{mode:?}");
    }
}

/// Project 2, whose one activity 9 has the stored dates a link reads.
fn other_project() -> Schedule {
    let mut other = network(&[60]);
    other.project.id = ProjectId(2);
    let mut a = other.activities.remove(&ActivityId(1)).unwrap();
    a.id = ActivityId(9);
    a.project = ProjectId(2);
    a.stored.early_start = Some(at("2026-01-05 11:00"));
    a.stored.early_finish = Some(at("2026-01-05 12:00"));
    a.stored.late_start = Some(at("2026-01-05 10:00"));
    a.stored.late_finish = Some(at("2026-01-05 11:00"));
    other.activities.insert(a.id, a);
    other
}

fn external_edge(s: &mut Schedule, from: u64, to: u64, kind: RelationshipType, lag: i64) {
    let project = |id| if id == 9 { ProjectId(2) } else { ProjectId(1) };
    s.relationships.insert(
        RelationshipId(50),
        Relationship {
            id: RelationshipId(50),
            predecessor: ActivityId(from),
            successor: ActivityId(to),
            predecessor_project: project(from),
            successor_project: project(to),
            kind,
            lag: Minutes(lag),
            source: None,
        },
    );
}

fn with_other(s: &Schedule, other: Schedule) -> ScheduleResult {
    schedule_with_projects(s, &s.project.options, &[s.clone(), other]).unwrap()
}

#[test]
fn an_external_early_start_holds_back_work_not_started() {
    let mut s = network(&[60]);
    let a = s.activities.get_mut(&ActivityId(1)).unwrap();
    a.external_early_start = Some(at("2026-01-05 10:00"));
    let r = run(&s);
    let a = activity(&r, 1);
    assert_eq!(a.values.early_start, Some(at("2026-01-05 10:00")));
    assert_eq!(
        a.provenance[&DateField::EarlyStart],
        DateProvenance::ExternalDate {
            date: at("2026-01-05 10:00")
        }
    );

    s.project.options.ignore_external_relationships = true;
    let r = run(&s);
    assert_eq!(
        activity(&r, 1).values.early_start,
        Some(at("2026-01-05 08:00"))
    );
}

#[test]
fn an_external_late_finish_limits_late_dates() {
    let mut s = network(&[60, 180]);
    let a = s.activities.get_mut(&ActivityId(1)).unwrap();
    a.external_late_finish = Some(at("2026-01-05 09:30"));
    let r = run(&s);
    let a = activity(&r, 1);
    assert_eq!(a.values.late_finish, Some(at("2026-01-05 09:30")));
    assert_eq!(a.values.total_float, Some(Minutes(30)));
}

#[test]
fn a_link_from_another_project_reads_its_stored_dates() {
    let mut s = network(&[60]);
    external_edge(&mut s, 9, 1, RelationshipType::FinishToStart, 30);
    assert_eq!(
        schedule(&s, &s.project.options),
        Err(EngineError::Unsupported("external relationships"))
    );
    let r = with_other(&s, other_project());
    let a = activity(&r, 1);
    assert_eq!(a.values.early_start, Some(at("2026-01-05 12:30")));
    assert!(matches!(
        a.provenance[&DateField::EarlyStart],
        DateProvenance::Relationship {
            from: ActivityId(9),
            from_date: DateField::EarlyFinish,
            ..
        }
    ));

    s.project.options.ignore_external_relationships = true;
    let r = run(&s);
    assert_eq!(
        activity(&r, 1).values.early_start,
        Some(at("2026-01-05 08:00"))
    );
}

#[test]
fn a_link_from_finished_work_counts_from_its_actual_date() {
    let mut s = network(&[60]);
    external_edge(&mut s, 9, 1, RelationshipType::FinishToStart, 0);
    let mut other = other_project();
    let a = other.activities.get_mut(&ActivityId(9)).unwrap();
    a.status = ActivityStatus::Completed;
    a.actual_start = Some(at("2026-01-05 08:00"));
    a.actual_finish = Some(at("2026-01-05 09:00"));
    let r = with_other(&s, other);
    assert_eq!(
        activity(&r, 1).values.early_start,
        Some(at("2026-01-05 09:00"))
    );
}

/// Lag on a link from resource-dependent work elsewhere runs on the calendar
/// that work runs on: 16:00 plus an hour, on a day ending at 16:30.
#[test]
fn lag_from_another_project_uses_its_resource_calendar() {
    let mut s = network(&[60]);
    external_edge(&mut s, 9, 1, RelationshipType::FinishToStart, 60);
    let mut other = other_project();
    other
        .calendars
        .insert(CalendarId(2), calendar(2, &[1, 2, 3, 4, 5], 600, 990));
    let a = other.activities.get_mut(&ActivityId(9)).unwrap();
    a.activity_type = ActivityType::ResourceDependent;
    a.stored.early_finish = Some(at("2026-01-05 16:00"));
    other.resources.insert(
        ResourceId(1),
        Resource {
            id: ResourceId(1),
            short_name: "R".into(),
            name: "Authored".into(),
            kind: ResourceType::Labor,
            calendar: Some(CalendarId(2)),
            parent: None,
            default_units_per_hour: None,
            default_cost_follows_units: false,
            rates: Vec::new(),
            roles: Vec::new(),
            udfs: BTreeMap::new(),
            source: None,
        },
    );
    other.assignments.insert(
        AssignmentId(1),
        Assignment {
            id: AssignmentId(1),
            activity: ActivityId(9),
            resource: Some(ResourceId(1)),
            role: None,
            cost_account: None,
            project: ProjectId(2),
            curve: None,
            typed_spread: None,
            amounts: totalfloat_model::Amounts::default(),
            udfs: BTreeMap::new(),
            source: None,
        },
    );
    let r = with_other(&s, other);
    assert_eq!(
        activity(&r, 1).values.early_start,
        Some(at("2026-01-06 10:30"))
    );
}

/// A mandatory date breaks the link, so the link has negative slack and drives nothing.
#[test]
fn a_link_broken_by_a_mandatory_date_doesnt_drive() {
    let mut s = network(&[60, 60]);
    let a = s.activities.get_mut(&ActivityId(2)).unwrap();
    a.primary_constraint = Some(Constraint {
        kind: ConstraintType::MandatoryStart,
        date: Some(at("2026-01-05 08:00")),
    });
    let r = run(&s);
    assert_eq!(
        activity(&r, 2).values.early_start,
        Some(at("2026-01-05 08:00"))
    );
    assert_eq!(activity(&r, 2).driving_relationships.len(), 0);
}

#[test]
fn a_link_to_another_project_limits_late_dates_through_its_stored_dates() {
    let mut s = network(&[60, 180]);
    external_edge(&mut s, 1, 9, RelationshipType::FinishToStart, 0);
    let r = with_other(&s, other_project());
    let a = activity(&r, 1);
    assert_eq!(a.values.late_finish, Some(at("2026-01-05 10:00")));
    assert!(matches!(
        a.provenance[&DateField::LateFinish].driver(),
        DateProvenance::Relationship {
            from: ActivityId(9),
            from_date: DateField::LateStart,
            ..
        }
    ));
}

#[test]
fn an_external_date_stands_in_for_a_project_not_loaded() {
    let mut s = network(&[60]);
    external_edge(&mut s, 9, 1, RelationshipType::FinishToStart, 0);
    let a = s.activities.get_mut(&ActivityId(1)).unwrap();
    a.external_early_start = Some(at("2026-01-05 10:00"));
    let r = run(&s);
    assert_eq!(
        activity(&r, 1).values.early_start,
        Some(at("2026-01-05 10:00"))
    );
}

#[test]
fn links_the_engine_cannot_honor_are_refused() {
    let refused = |s: &Schedule| {
        schedule_with_projects(s, &s.project.options, &[s.clone(), other_project()]).unwrap_err()
    };
    let mut span = network(&[60]);
    typed(&mut span, 1, ActivityType::LevelOfEffort);
    external_edge(&mut span, 9, 1, RelationshipType::StartToStart, 0);
    assert_eq!(
        refused(&span),
        EngineError::Unsupported("span linked to another project")
    );

    let mut alap = network(&[60]);
    external_edge(&mut alap, 1, 9, RelationshipType::FinishToStart, 0);
    alap.activities
        .get_mut(&ActivityId(1))
        .unwrap()
        .primary_constraint = Some(Constraint {
        kind: ConstraintType::AsLateAsPossible,
        date: None,
    });
    assert_eq!(
        refused(&alap),
        EngineError::Unsupported("as-late-as-possible activity linked to another project")
    );

    let mut expected = network(&[60]);
    expected.project.options.use_expected_finish = true;
    external_edge(&mut expected, 9, 1, RelationshipType::FinishToFinish, 0);
    expected
        .activities
        .get_mut(&ActivityId(1))
        .unwrap()
        .expected_finish = Some(at("2026-01-05 09:00"));
    assert_eq!(
        refused(&expected),
        EngineError::Unsupported("expected finish with a finish link from another project")
    );

    let mut bare = network(&[60]);
    external_edge(&mut bare, 9, 1, RelationshipType::FinishToStart, 0);
    let mut other = other_project();
    other.activities.get_mut(&ActivityId(9)).unwrap().stored = StoredResults::default();
    assert_eq!(
        schedule_with_projects(&bare, &bare.project.options, &[other]).unwrap_err(),
        EngineError::Unsupported("linked activity in another project has no stored dates")
    );
}

#[test]
fn a_finished_successor_in_another_project_limits_nothing() {
    let mut s = network(&[60, 180]);
    external_edge(&mut s, 1, 9, RelationshipType::FinishToStart, 0);
    let mut other = other_project();
    let a = other.activities.get_mut(&ActivityId(9)).unwrap();
    a.status = ActivityStatus::Completed;
    a.actual_start = Some(at("2026-01-05 08:00"));
    a.actual_finish = Some(at("2026-01-05 09:00"));
    let r = with_other(&s, other);
    assert_eq!(
        activity(&r, 1).values.late_finish,
        Some(at("2026-01-05 11:00"))
    );
}

#[test]
fn a_successor_in_another_project_is_not_an_open_end() {
    let mut s = network(&[60, 180]);
    s.project.options.make_open_ended_critical = true;
    external_edge(&mut s, 1, 9, RelationshipType::FinishToStart, 0);
    let r = with_other(&s, other_project());
    assert_eq!(
        activity(&r, 1).values.late_finish,
        Some(at("2026-01-05 10:00"))
    );
}

#[test]
fn other_projects_stay_out_of_the_results() {
    let mut s = network(&[60]);
    external_edge(&mut s, 9, 1, RelationshipType::FinishToStart, 0);
    let r = with_other(&s, other_project());
    assert_eq!(r.activities.len(), 1);
    assert_eq!(r.project.id, ProjectId(1));
}

#[test]
fn float_from_the_opened_projects_finish() {
    // A1 runs 08:00 to 09:00. Project 2's activity 9 finishes 12:00.
    let mut s = network(&[60]);
    s.project.options.float_from_project_finish = false;
    let both = [s.clone(), other_project()];
    let opened = Some(at("2026-01-05 12:00"));
    let r = schedule_open_together(&s, &s.project.options, &both, opened).unwrap();
    let a = activity(&r, 1);
    assert_eq!(a.values.late_finish, Some(at("2026-01-05 12:00")));
    assert_eq!(a.values.total_float, Some(Minutes(180)));
    assert_eq!(a.values.free_float, Some(Minutes(180)));
    assert_eq!(r.project.scheduled_finish, Some(at("2026-01-05 12:00")));
    assert_eq!(r.project.network_finish, Some(at("2026-01-05 09:00")));
    // The longest path still runs back from the project's own finish.
    assert_eq!(a.values.on_longest_path, Some(true));
    // `schedule_with_projects` takes the other projects' stored finish.
    assert_eq!(
        schedule_with_projects(&s, &s.project.options, &both).unwrap(),
        r
    );

    // No other project open: only this project's finish counts.
    let r = schedule_open_together(&s, &s.project.options, &both, None).unwrap();
    assert_eq!(
        activity(&r, 1).values.late_finish,
        Some(at("2026-01-05 09:00"))
    );
    assert_eq!(r.project.scheduled_finish, Some(at("2026-01-05 09:00")));

    // Float from each project's own finish.
    s.project.options.float_from_project_finish = true;
    let r = schedule_open_together(&s, &s.project.options, &both, opened).unwrap();
    assert_eq!(activity(&r, 1).values.total_float, Some(Minutes(0)));
    assert_eq!(r.project.scheduled_finish, Some(at("2026-01-05 09:00")));
}

#[test]
fn an_earlier_opened_project_leaves_the_finish_alone() {
    let mut s = network(&[600]);
    s.project.options.float_from_project_finish = false;
    let both = [s.clone(), other_project()];
    let r = schedule_with_projects(&s, &s.project.options, &both).unwrap();
    assert_eq!(r.project.scheduled_finish, Some(at("2026-01-05 18:00")));
    assert_eq!(activity(&r, 1).values.total_float, Some(Minutes(0)));
}

#[test]
fn free_float_counts_a_successor_in_another_project() {
    // A1 08:00 to 09:00 links to activity 9 in project 2, whose early start
    // is 11:00 and late start 12:00. A2 runs to 13:00, the project's finish.
    let mut s = network(&[60, 300]);
    external_edge(&mut s, 1, 9, RelationshipType::FinishToStart, 0);
    let mut other = other_project();
    let nine = other.activities.get_mut(&ActivityId(9)).unwrap();
    nine.stored.late_start = Some(at("2026-01-05 12:00"));
    nine.stored.late_finish = Some(at("2026-01-05 13:00"));
    let r = with_other(&s, other.clone());
    let a = activity(&r, 1);
    assert_eq!(a.values.late_finish, Some(at("2026-01-05 12:00")));
    assert_eq!(a.values.total_float, Some(Minutes(180)));
    assert_eq!(a.values.free_float, Some(Minutes(120)));

    // A link with lag leaves that much less.
    external_edge(&mut s, 1, 9, RelationshipType::FinishToStart, 30);
    let r = with_other(&s, other.clone());
    assert_eq!(activity(&r, 1).values.free_float, Some(Minutes(90)));

    // A finished successor limits nothing.
    let nine = other.activities.get_mut(&ActivityId(9)).unwrap();
    nine.status = ActivityStatus::Completed;
    nine.actual_start = Some(at("2026-01-01 08:00"));
    nine.actual_finish = Some(at("2026-01-01 09:00"));
    let r = with_other(&s, other);
    assert_eq!(activity(&r, 1).values.free_float, Some(Minutes(240)));
}

#[test]
fn finished_work_reads_no_successor_in_another_project() {
    // A1 is finished, and its successor in project 2 has no stored dates.
    let mut s = network(&[60, 120]);
    external_edge(&mut s, 1, 9, RelationshipType::FinishToStart, 0);
    let a = s.activities.get_mut(&ActivityId(1)).unwrap();
    a.status = ActivityStatus::Completed;
    a.actual_start = Some(at("2026-01-01 08:00"));
    a.actual_finish = Some(at("2026-01-01 09:00"));
    let mut other = other_project();
    other.activities.get_mut(&ActivityId(9)).unwrap().stored = StoredResults::default();
    let r = with_other(&s, other);
    assert_eq!(activity(&r, 1).values.free_float, None);
}

#[test]
fn a_successor_with_no_early_start_leaves_free_float_alone() {
    // Activity 9 has stored late dates but no early start: free float leaves
    // the link out rather than refusing the project.
    let mut s = network(&[60, 300]);
    external_edge(&mut s, 1, 9, RelationshipType::FinishToStart, 0);
    let mut other = other_project();
    let nine = other.activities.get_mut(&ActivityId(9)).unwrap();
    nine.stored.early_start = None;
    nine.stored.late_start = Some(at("2026-01-05 12:00"));
    let r = with_other(&s, other);
    let a = activity(&r, 1);
    assert_eq!(a.values.late_finish, Some(at("2026-01-05 12:00")));
    assert_eq!(a.values.free_float, Some(Minutes(240)));
}

#[test]
fn a_finished_span_is_off_the_longest_path() {
    let mut s = network(&[60, 180, 0]);
    typed(&mut s, 3, ActivityType::LevelOfEffort);
    edge(&mut s, 1, 1, 3, RelationshipType::StartToStart, 0);
    edge(&mut s, 2, 3, 2, RelationshipType::FinishToFinish, 0);
    let l = s.activities.get_mut(&ActivityId(3)).unwrap();
    l.status = ActivityStatus::Completed;
    l.actual_start = Some(at("2026-01-01 08:00"));
    l.actual_finish = Some(at("2026-01-01 09:00"));
    s.project.data_date = Some(at("2026-01-05 00:00"));
    let r = run(&s);
    assert_eq!(activity(&r, 3).values.on_longest_path, Some(false));
}

#[test]
fn a_wbs_summary_with_float_is_off_the_longest_path() {
    // W's only member is A, which has float beside the longer B.
    let mut s = network(&[60, 180, 0]);
    typed(&mut s, 3, ActivityType::WbsSummary);
    let wbs = |id| Wbs {
        id: WbsId(id),
        project: ProjectId(1),
        parent: None,
        code: "W".into(),
        name: "Authored".into(),
        sequence: 0,
        is_project_node: false,
        earned_value: totalfloat_model::EarnedValue::default(),
        obs: None,
        notes: std::collections::BTreeMap::new(),
        udfs: BTreeMap::new(),
        source: None,
    };
    s.wbs.insert(WbsId(2), wbs(2));
    s.wbs.insert(WbsId(3), wbs(3));
    s.activities.get_mut(&ActivityId(1)).unwrap().wbs = WbsId(2);
    s.activities.get_mut(&ActivityId(3)).unwrap().wbs = WbsId(2);
    s.activities.get_mut(&ActivityId(2)).unwrap().wbs = WbsId(3);
    let r = run(&s);
    let w = activity(&r, 3);
    assert!(w.values.total_float.unwrap() > Minutes::ZERO);
    assert_eq!(w.values.on_longest_path, Some(false));
}

#[test]
fn a_span_with_float_is_off_the_longest_path() {
    // A is off the path, so L, which measures only A, has float.
    let mut s = network(&[60, 180, 0]);
    typed(&mut s, 3, ActivityType::LevelOfEffort);
    edge(&mut s, 1, 1, 3, RelationshipType::StartToStart, 0);
    edge(&mut s, 2, 3, 1, RelationshipType::FinishToFinish, 0);
    let r = run(&s);
    let l = activity(&r, 3);
    assert!(l.values.total_float.unwrap() > Minutes::ZERO);
    assert_eq!(l.values.on_longest_path, Some(false));
}

#[test]
fn a_wbs_summary_spans_its_subtree() {
    let mut s = network(&[60, 180, 0]);
    typed(&mut s, 3, ActivityType::WbsSummary);
    let wbs = |id, parent| Wbs {
        id: WbsId(id),
        project: ProjectId(1),
        parent,
        code: "W".into(),
        name: "Authored".into(),
        sequence: 0,
        is_project_node: false,
        earned_value: totalfloat_model::EarnedValue::default(),
        obs: None,
        notes: std::collections::BTreeMap::new(),
        udfs: BTreeMap::new(),
        source: None,
    };
    s.wbs.insert(WbsId(1), wbs(1, None));
    s.wbs.insert(WbsId(2), wbs(2, Some(WbsId(1))));
    s.activities.get_mut(&ActivityId(2)).unwrap().wbs = WbsId(2);
    s.activities.get_mut(&ActivityId(3)).unwrap().wbs = WbsId(1);
    edge(&mut s, 1, 1, 2, RelationshipType::FinishToStart, 0);
    let r = run(&s);
    let w = activity(&r, 3);
    assert_eq!(w.values.early_start, Some(at("2026-01-05 08:00")));
    assert_eq!(w.values.early_finish, Some(at("2026-01-05 12:00")));
    assert_eq!(w.values.on_longest_path, Some(true));
    assert_eq!(w.values.total_float, Some(Minutes::ZERO));
    assert_eq!(
        w.provenance[&DateField::EarlyFinish],
        DateProvenance::WbsSummarySpan
    );
}

#[test]
fn a_wbs_summary_spans_what_is_unfinished() {
    let mut s = network(&[60, 180, 0]);
    typed(&mut s, 3, ActivityType::WbsSummary);
    edge(&mut s, 1, 1, 2, RelationshipType::FinishToStart, 0);
    let a = s.activities.get_mut(&ActivityId(1)).unwrap();
    a.status = ActivityStatus::Completed;
    a.actual_start = Some(at("2026-01-01 08:00"));
    a.actual_finish = Some(at("2026-01-01 09:00"));
    s.project.data_date = Some(at("2026-01-05 00:00"));
    let r = run(&s);
    assert_eq!(
        activity(&r, 3).values.early_start,
        activity(&r, 2).values.early_start
    );
}

#[test]
fn resource_dependent_work_runs_on_its_resources_calendar() {
    let resource = |id, calendar| Resource {
        id: ResourceId(id),
        short_name: "R".into(),
        name: "Authored".into(),
        kind: ResourceType::Labor,
        calendar: Some(CalendarId(calendar)),
        parent: None,
        default_units_per_hour: None,
        default_cost_follows_units: false,
        rates: Vec::new(),
        roles: Vec::new(),
        udfs: BTreeMap::new(),
        source: None,
    };
    let assign = |id, r| Assignment {
        id: AssignmentId(id),
        activity: ActivityId(1),
        resource: Some(ResourceId(r)),
        role: None,
        cost_account: None,
        project: ProjectId(1),
        curve: None,
        typed_spread: None,
        amounts: totalfloat_model::Amounts::default(),
        udfs: BTreeMap::new(),
        source: None,
    };
    let mut s = network(&[120]);
    typed(&mut s, 1, ActivityType::ResourceDependent);
    s.calendars
        .insert(CalendarId(2), calendar(2, &[1, 2, 3, 4, 5], 600, 1020));
    s.resources.insert(ResourceId(1), resource(1, 2));
    s.assignments.insert(AssignmentId(1), assign(1, 1));
    let r = run(&s);
    assert_eq!(
        activity(&r, 1).values.early_start,
        Some(at("2026-01-05 10:00"))
    );
    assert_eq!(
        activity(&r, 1).values.early_finish,
        Some(at("2026-01-05 12:00"))
    );

    s.calendars
        .insert(CalendarId(3), calendar(3, &[1, 2, 3, 4, 5], 480, 960));
    s.resources.insert(ResourceId(2), resource(2, 3));
    s.assignments.insert(AssignmentId(2), assign(2, 2));
    assert_eq!(
        schedule(&s, &s.project.options).err(),
        Some(EngineError::Unsupported(
            "resource-dependent activity with several resource calendars"
        ))
    );
    s.resources.get_mut(&ResourceId(2)).unwrap().calendar = None;
    assert!(matches!(
        schedule(&s, &s.project.options),
        Err(EngineError::Unsupported(_))
    ));
    s.resources.get_mut(&ResourceId(1)).unwrap().calendar = None;
    assert_eq!(
        activity(&run(&s), 1).values.early_start,
        Some(at("2026-01-05 08:00"))
    );
    s.resources.remove(&ResourceId(1));
    assert!(matches!(
        schedule(&s, &s.project.options),
        Err(EngineError::InvalidInput(_))
    ));
}

#[test]
fn span_links_do_not_create_network_cycles() {
    for kind in [ActivityType::LevelOfEffort, ActivityType::WbsSummary] {
        let mut s = network(&[60, 0]);
        typed(&mut s, 2, kind);
        edge(&mut s, 1, 1, 2, RelationshipType::StartToStart, 0);
        edge(&mut s, 2, 2, 1, RelationshipType::FinishToFinish, 0);
        let result = run(&s);
        assert_eq!(
            activity(&result, 1).values.early_start,
            Some(at("2026-01-05 08:00"))
        );
    }
}

/// A finish-side lag on a seven-day calendar, into work on a five-day one.
/// A2 runs Friday, its lag reaches Saturday, and A3 can't finish until Monday.
fn weekend_gap() -> Schedule {
    let mut s = network(&[480, 480, 480, 480]);
    s.project.data_date = Some(at("2026-01-08 08:00"));
    s.calendars
        .insert(CalendarId(1), calendar(1, &[1, 2, 3, 4, 5], 480, 960));
    s.calendars
        .insert(CalendarId(2), calendar(2, &[0, 1, 2, 3, 4, 5, 6], 480, 960));
    for id in [1, 2, 4] {
        s.activities.get_mut(&ActivityId(id)).unwrap().calendar = CalendarId(2);
    }
    edge(&mut s, 1, 1, 2, RelationshipType::FinishToStart, 0);
    edge(&mut s, 2, 2, 3, RelationshipType::FinishToFinish, 480);
    edge(&mut s, 3, 3, 4, RelationshipType::FinishToStart, 0);
    s
}
#[test]
fn true_float_keeps_the_time_total_float_loses_to_a_calendar_gap() {
    let r = run(&weekend_gap());
    for id in [1, 2] {
        let a = activity(&r, id);
        assert_eq!(a.values.total_float, Some(Minutes(-480)));
        assert_eq!(a.true_total_float, Some(Minutes(480)));
    }
    assert_eq!(activity(&r, 4).true_total_float, None);
}
#[test]
fn a_link_with_no_lag_loses_the_same_time() {
    let mut s = weekend_gap();
    s.activities
        .get_mut(&ActivityId(2))
        .unwrap()
        .remaining_duration = Minutes(960);
    s.relationships.get_mut(&RelationshipId(2)).unwrap().lag = Minutes::ZERO;
    let r = run(&s);
    let a = activity(&r, 2);
    assert_eq!(a.values.total_float, Some(Minutes(-480)));
    assert_eq!(a.true_total_float, Some(Minutes(480)));
}
#[test]
fn lateness_against_a_set_date_is_real() {
    let mut s = weekend_gap();
    s.activities.remove(&ActivityId(4));
    s.relationships.remove(&RelationshipId(3));
    s.project.must_finish_by = Some(at("2026-01-09 16:00"));
    let r = run(&s);
    let a = activity(&r, 2);
    assert_eq!(a.values.total_float, Some(Minutes(-480)));
    assert_eq!(a.true_total_float, None);
}
#[test]
fn true_float_is_absent_when_nothing_differs() {
    let mut s = weekend_gap();
    for id in [1, 2, 4] {
        s.activities.get_mut(&ActivityId(id)).unwrap().calendar = CalendarId(1);
    }
    let r = run(&s);
    assert!(r.activities.values().all(|a| a.true_total_float.is_none()));
    let mut s = weekend_gap();
    s.relationships.get_mut(&RelationshipId(2)).unwrap().kind = RelationshipType::FinishToStart;
    let r = run(&s);
    assert!(r.activities.values().all(|a| a.true_total_float.is_none()));
}

#[test]
fn a_link_out_with_a_stand_in_date_is_not_an_open_end() {
    let mut s = network(&[60, 180]);
    s.project.options.make_open_ended_critical = true;
    external_edge(&mut s, 1, 9, RelationshipType::FinishToStart, 0);
    let a = s.activities.get_mut(&ActivityId(1)).unwrap();
    a.external_late_finish = Some(at("2026-01-05 10:00"));
    let r = run(&s);
    assert_eq!(activity(&r, 1).critical, Some(false));
    // The date limits it with or without the link.
    s.relationships.clear();
    let r = run(&s);
    assert_eq!(activity(&r, 1).critical, Some(false));
}
#[test]
fn finished_work_linked_to_a_project_not_loaded_can_be_scheduled() {
    let mut s = network(&[60, 60]);
    s.project.data_date = Some(at("2026-01-06 08:00"));
    let a = s.activities.get_mut(&ActivityId(1)).unwrap();
    a.status = ActivityStatus::Completed;
    a.actual_start = Some(at("2026-01-05 08:00"));
    a.actual_finish = Some(at("2026-01-05 09:00"));
    a.remaining_duration = Minutes::ZERO;
    external_edge(&mut s, 1, 9, RelationshipType::FinishToStart, 0);
    run(&s);
}
#[test]
fn a_level_of_effort_waits_for_a_project_that_starts_after_the_data_date() {
    let mut s = network(&[60, 0]);
    typed(&mut s, 2, ActivityType::LevelOfEffort);
    s.project.data_date = Some(at("2026-01-05 08:00"));
    s.project.planned_start = Some(at("2026-01-06 08:00"));
    edge(&mut s, 1, 2, 1, RelationshipType::FinishToFinish, 0);
    let r = run(&s);
    assert_eq!(
        activity(&r, 2).values.early_start,
        Some(at("2026-01-06 08:00"))
    );
}
#[test]
fn as_late_as_possible_stops_at_an_external_late_finish() {
    let mut s = network(&[60, 480]);
    let a = s.activities.get_mut(&ActivityId(1)).unwrap();
    a.primary_constraint = Some(Constraint {
        kind: ConstraintType::AsLateAsPossible,
        date: None,
    });
    a.external_late_finish = Some(at("2026-01-05 10:00"));
    let r = run(&s);
    assert_eq!(
        activity(&r, 1).values.early_finish,
        Some(at("2026-01-05 10:00"))
    );
}
#[test]
fn late_dates_through_another_project_use_the_lag_that_is_left() {
    // Four hours of lag ran out by the data date, so none comes off the late date.
    let mut s = network(&[480, 2400]);
    s.project.data_date = Some(at("2026-01-05 12:00"));
    let a = s.activities.get_mut(&ActivityId(1)).unwrap();
    a.status = ActivityStatus::InProgress;
    a.actual_start = Some(at("2026-01-05 08:00"));
    a.remaining_duration = Minutes(240);
    external_edge(&mut s, 1, 9, RelationshipType::StartToStart, 240);
    let mut other = other_project();
    let target = other.activities.get_mut(&ActivityId(9)).unwrap();
    target.stored.late_start = Some(at("2026-01-05 16:00"));
    let r = with_other(&s, other);
    assert_eq!(
        activity(&r, 1).values.late_start,
        Some(at("2026-01-05 16:00"))
    );
}
#[test]
fn finished_work_links_to_another_project_whatever_its_constraints_were() {
    let mut s = network(&[60]);
    s.project.data_date = Some(at("2026-01-06 08:00"));
    let a = s.activities.get_mut(&ActivityId(1)).unwrap();
    a.status = ActivityStatus::Completed;
    a.actual_start = Some(at("2026-01-05 08:00"));
    a.actual_finish = Some(at("2026-01-05 09:00"));
    a.remaining_duration = Minutes::ZERO;
    a.primary_constraint = Some(Constraint {
        kind: ConstraintType::AsLateAsPossible,
        date: None,
    });
    external_edge(&mut s, 1, 9, RelationshipType::FinishToStart, 0);
    with_other(&s, other_project());
}
#[test]
fn pushed_finished_work_at_the_project_finish_starts_the_longest_path() {
    let mut s = network(&[480, 60]);
    s.project.data_date = Some(at("2026-01-05 08:00"));
    let a = s.activities.get_mut(&ActivityId(2)).unwrap();
    a.status = ActivityStatus::Completed;
    a.actual_start = Some(at("2026-01-01 08:00"));
    a.actual_finish = Some(at("2026-01-01 09:00"));
    a.remaining_duration = Minutes::ZERO;
    edge(&mut s, 1, 1, 2, RelationshipType::FinishToFinish, 60);
    let r = run(&s);
    assert_eq!(activity(&r, 2).values.on_longest_path, Some(true));
}
#[test]
fn a_finished_level_of_effort_has_one_late_point() {
    let mut s = network(&[480, 0]);
    typed(&mut s, 2, ActivityType::LevelOfEffort);
    s.project.data_date = Some(at("2026-01-05 08:00"));
    let a = s.activities.get_mut(&ActivityId(2)).unwrap();
    a.status = ActivityStatus::Completed;
    a.actual_start = Some(at("2026-01-01 08:00"));
    a.actual_finish = Some(at("2026-01-01 09:00"));
    edge(&mut s, 1, 1, 2, RelationshipType::StartToStart, 0);
    edge(&mut s, 2, 1, 2, RelationshipType::FinishToFinish, 0);
    let r = run(&s);
    let values = &activity(&r, 2).values;
    assert_eq!(values.late_start, values.late_finish);
    assert_eq!(values.late_finish, activity(&r, 1).values.late_finish);
}

#[test]
fn lag_still_running_past_the_data_date_holds_work_after_finished_work() {
    // The data date is a Saturday, and the lag runs on into Sunday on a
    // seven-day calendar, short of the Monday the predecessor's calendar reaches.
    let mut s = network(&[60, 60]);
    s.project.data_date = Some(at("2026-01-10 08:00"));
    s.project.options.lag_calendar = LagCalendar::Successor;
    s.calendars
        .insert(CalendarId(1), calendar(1, &[1, 2, 3, 4, 5], 480, 960));
    s.calendars
        .insert(CalendarId(2), calendar(2, &[0, 1, 2, 3, 4, 5, 6], 480, 960));
    let a = s.activities.get_mut(&ActivityId(1)).unwrap();
    a.status = ActivityStatus::Completed;
    a.actual_start = Some(at("2026-01-09 14:00"));
    a.actual_finish = Some(at("2026-01-09 16:00"));
    a.remaining_duration = Minutes::ZERO;
    s.activities.get_mut(&ActivityId(2)).unwrap().calendar = CalendarId(2);
    edge(&mut s, 1, 1, 2, RelationshipType::FinishToStart, 720);
    let r = run(&s);
    assert_eq!(
        activity(&r, 2).values.early_start,
        Some(at("2026-01-11 12:00"))
    );
}
#[test]
fn a_constraint_on_a_span_binds_nothing() {
    let mut s = network(&[60, 0]);
    typed(&mut s, 2, ActivityType::LevelOfEffort);
    let a = s.activities.get_mut(&ActivityId(2)).unwrap();
    a.primary_constraint = Some(Constraint {
        kind: ConstraintType::StartOnOrAfter,
        date: Some(at("2026-02-02 08:00")),
    });
    edge(&mut s, 1, 1, 2, RelationshipType::StartToStart, 0);
    edge(&mut s, 2, 1, 2, RelationshipType::FinishToFinish, 0);
    let r = run(&s);
    assert_eq!(
        activity(&r, 2).values.early_start,
        Some(at("2026-01-05 08:00"))
    );
}
#[test]
fn progress_override_drops_a_broken_link_from_a_span_too() {
    let mut s = network(&[60, 60, 0]);
    typed(&mut s, 3, ActivityType::LevelOfEffort);
    s.project.data_date = Some(at("2026-01-05 08:00"));
    s.project.options.out_of_sequence = OutOfSequenceMode::ProgressOverride;
    let a = s.activities.get_mut(&ActivityId(3)).unwrap();
    a.status = ActivityStatus::InProgress;
    a.actual_start = Some(at("2026-01-02 08:00"));
    edge(&mut s, 1, 1, 3, RelationshipType::FinishToStart, 240);
    edge(&mut s, 2, 2, 3, RelationshipType::FinishToFinish, 0);
    let dropped = run(&s);
    s.project.options.out_of_sequence = OutOfSequenceMode::RetainedLogic;
    let retained = run(&s);
    assert_ne!(
        activity(&dropped, 3).values.early_start,
        activity(&retained, 3).values.early_start
    );
}
