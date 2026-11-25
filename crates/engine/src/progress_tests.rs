use super::*;

const DATA_DATE: &str = "2026-01-05 08:00";

/// Three tasks in a finish-to-start chain on a continuous calendar.
fn chain(durations: &[i64]) -> Schedule {
    let mut s = network(durations);
    for i in 1..durations.len() {
        let i = u64::try_from(i).unwrap();
        edge(&mut s, i, i, i + 1, RelationshipType::FinishToStart, 0);
    }
    s
}
fn get(s: &mut Schedule, id: u64) -> &mut Activity {
    s.activities.get_mut(&ActivityId(id)).unwrap()
}
fn complete(s: &mut Schedule, id: u64, start: &str, finish: &str) {
    let a = get(s, id);
    a.status = ActivityStatus::Completed;
    a.actual_start = Some(at(start));
    a.actual_finish = Some(at(finish));
    a.remaining_duration = Minutes::ZERO;
}
fn begin(s: &mut Schedule, id: u64, start: &str, remaining: i64) {
    let a = get(s, id);
    a.status = ActivityStatus::InProgress;
    a.actual_start = Some(at(start));
    a.remaining_duration = Minutes(remaining);
}
fn weekday_calendar(s: &mut Schedule) {
    s.calendars
        .insert(CalendarId(1), calendar(1, &[1, 2, 3, 4, 5], 480, 1020));
}

#[test]
fn completed_work_collapses_to_the_data_date() {
    let mut s = chain(&[60, 60]);
    complete(&mut s, 1, "2026-01-02 08:00", "2026-01-02 09:00");
    let r = run(&s);
    let done = &activity(&r, 1).values;
    assert_eq!(done.early_start, Some(at(DATA_DATE)));
    assert_eq!(done.early_finish, Some(at(DATA_DATE)));
    assert_eq!(done.late_start, Some(at(DATA_DATE)));
    assert_eq!(done.late_finish, Some(at(DATA_DATE)));
    assert_eq!(done.remaining_early_start, None);
    assert_eq!(done.remaining_late_finish, None);
    assert_eq!(done.total_float, None);
    assert_eq!(done.free_float, None);
    assert!(
        !activity(&r, 1)
            .provenance
            .contains_key(&DateField::RemainingEarlyStart)
    );
    assert_eq!(activity(&r, 2).values.early_start, Some(at(DATA_DATE)));
}

#[test]
fn a_nonworking_data_date_is_stored_as_it_is_on_completed_work() {
    // Sunday midnight. Remaining and future work move to Monday, finished work doesn't.
    let mut s = chain(&[60, 60]);
    weekday_calendar(&mut s);
    s.project.data_date = Some(at("2026-01-04 00:00"));
    s.project.planned_start = Some(at("2026-01-02 08:00"));
    complete(&mut s, 1, "2026-01-02 08:00", "2026-01-02 09:00");
    let r = run(&s);
    assert_eq!(
        activity(&r, 1).values.early_start,
        Some(at("2026-01-04 00:00"))
    );
    assert_eq!(
        activity(&r, 2).values.early_start,
        Some(at("2026-01-05 08:00"))
    );
}

#[test]
fn a_lone_completed_activity_ends_at_the_data_date() {
    let mut s = network(&[60]);
    complete(&mut s, 1, "2026-01-02 08:00", "2026-01-02 09:00");
    let r = run(&s);
    assert_eq!(r.project.scheduled_finish, Some(at(DATA_DATE)));
    assert_eq!(activity(&r, 1).values.late_finish, Some(at(DATA_DATE)));
}

#[test]
fn in_progress_work_reports_only_what_remains() {
    let mut s = chain(&[600, 60]);
    begin(&mut s, 1, "2026-01-04 08:00", 120);
    let r = run(&s);
    let active = &activity(&r, 1).values;
    assert_eq!(active.early_start, Some(at(DATA_DATE)));
    assert_eq!(active.remaining_early_start, Some(at(DATA_DATE)));
    assert_eq!(active.early_finish, Some(at("2026-01-05 10:00")));
    assert_eq!(active.remaining_early_finish, Some(at("2026-01-05 10:00")));
    assert_eq!(active.remaining_late_start, active.late_start);
    assert_eq!(active.total_float, Some(Minutes::ZERO));
    assert_eq!(
        activity(&r, 2).values.early_start,
        Some(at("2026-01-05 10:00"))
    );
    assert_eq!(r.project.scheduled_finish, Some(at("2026-01-05 11:00")));
}

#[test]
fn remaining_work_starts_at_the_next_work_after_the_data_date() {
    let mut s = chain(&[600]);
    weekday_calendar(&mut s);
    s.project.data_date = Some(at("2026-01-04 00:00"));
    s.project.planned_start = Some(at("2026-01-02 08:00"));
    begin(&mut s, 1, "2026-01-02 08:00", 60);
    let r = run(&s);
    assert_eq!(
        activity(&r, 1).values.early_start,
        Some(at("2026-01-05 08:00"))
    );
    assert_eq!(
        activity(&r, 1).provenance[&DateField::EarlyStart],
        DateProvenance::CalendarAdjustment {
            unadjusted: at("2026-01-04 00:00"),
            calendar: CalendarId(1),
            cause: Box::new(DateProvenance::DataDate),
        }
    );
}

#[test]
fn in_progress_work_with_nothing_remaining_is_one_point() {
    let mut s = chain(&[600, 60]);
    weekday_calendar(&mut s);
    begin(&mut s, 1, "2026-01-02 08:00", 0);
    let r = run(&s);
    let active = &activity(&r, 1).values;
    assert_eq!(active.early_start, active.early_finish);
    assert_eq!(active.late_start, active.late_finish);
}

#[test]
fn a_later_resume_date_holds_remaining_work() {
    let mut s = chain(&[600, 60]);
    begin(&mut s, 1, "2026-01-02 08:00", 60);
    get(&mut s, 1).suspend = Some(at("2026-01-03 08:00"));
    get(&mut s, 1).resume = Some(at("2026-01-06 08:00"));
    let r = run(&s);
    assert_eq!(
        activity(&r, 1).values.early_start,
        Some(at("2026-01-06 08:00"))
    );
    assert_eq!(
        activity(&r, 2).values.early_start,
        Some(at("2026-01-06 09:00"))
    );
    assert_eq!(
        activity(&r, 1).provenance[&DateField::EarlyStart],
        DateProvenance::Resume
    );

    // A resume date already passed has no effect.
    get(&mut s, 1).resume = Some(at("2026-01-04 08:00"));
    assert_eq!(
        activity(&run(&s), 1).values.early_start,
        Some(at(DATA_DATE))
    );
}

#[test]
fn a_lag_that_ends_before_the_data_date_has_no_effect() {
    let mut s = chain(&[60, 60]);
    complete(&mut s, 1, "2026-01-01 08:00", "2026-01-01 09:00");
    let r = run(&s);
    assert_eq!(activity(&r, 2).values.early_start, Some(at(DATA_DATE)));
    assert_eq!(
        activity(&r, 2).provenance[&DateField::EarlyStart],
        DateProvenance::DataDate
    );
}

#[test]
fn an_actual_start_link_leaves_remaining_work_free() {
    // With lag from actual starts, B's start owes nothing to A's remaining work,
    // so A's floats come from the project finish, not from that link.
    let mut s = network(&[600, 60]);
    edge(&mut s, 1, 1, 2, RelationshipType::StartToStart, 2 * 1440);
    begin(&mut s, 1, "2026-01-04 08:00", 60);
    s.project.options.ss_lag_from_early_start = false;
    let r = run(&s);
    assert_eq!(
        activity(&r, 2).values.early_start,
        Some(at("2026-01-06 08:00"))
    );
    assert_eq!(
        activity(&r, 1).values.late_finish,
        Some(at("2026-01-06 09:00"))
    );
    let float = Some(Minutes(24 * 60));
    assert_eq!(activity(&r, 1).values.total_float, float);
    assert_eq!(activity(&r, 1).values.free_float, float);
}

#[test]
fn unstarted_predecessors_stay_on_their_early_dates() {
    // The option only concerns work with an actual start to measure from.
    let mut s = network(&[60, 60]);
    edge(&mut s, 1, 1, 2, RelationshipType::StartToStart, 60);
    s.project.options.ss_lag_from_early_start = false;
    let r = run(&s);
    let chain = r.driving_chain(ActivityId(2), DateField::EarlyStart);
    let steps: Vec<_> = chain.iter().map(|l| (l.activity.0, l.field)).collect();
    assert_eq!(
        steps,
        [(2, DateField::EarlyStart), (1, DateField::EarlyStart)]
    );
}

#[test]
fn a_lag_tie_with_the_data_date_is_still_the_data_date() {
    // Actual finish Friday 17:00 and the data date Sunday both reach Monday
    // 08:00. The finished work had no effect, so the data date is the cause.
    let mut s = chain(&[60, 60]);
    weekday_calendar(&mut s);
    s.project.data_date = Some(at("2026-01-04 00:00"));
    s.project.planned_start = Some(at("2026-01-02 08:00"));
    complete(&mut s, 1, "2026-01-02 08:00", "2026-01-02 17:00");
    let r = run(&s);
    assert_eq!(
        activity(&r, 2).values.early_start,
        Some(at("2026-01-05 08:00"))
    );
    assert_eq!(
        activity(&r, 2).provenance[&DateField::EarlyStart].driver(),
        &DateProvenance::DataDate
    );
}

#[test]
fn remaining_work_is_never_placed_before_the_data_date() {
    let mut s = chain(&[600, 60]);
    begin(&mut s, 1, "2026-01-02 08:00", 60);
    get(&mut s, 1).expected_finish = Some(at("2026-01-02 08:00"));
    let r = schedule(&s, &s.project.options).unwrap();
    assert_eq!(
        r.activities[&ActivityId(1)].values.early_finish,
        Some(at("2026-01-05 09:00")),
        "an expected finish before the data date is ignored (TF-018)"
    );

    // A mandatory finish before the data date doesn't pull the remaining
    // work back either; its late finish keeps the date (TF-005, item 7).
    get(&mut s, 1).expected_finish = None;
    get(&mut s, 1).primary_constraint = Some(Constraint {
        kind: ConstraintType::MandatoryFinish,
        date: Some(at("2026-01-02 08:00")),
    });
    let r = run(&s);
    let started = &activity(&r, 1).values;
    assert_eq!(started.remaining_early_start, Some(at(DATA_DATE)));
    assert_eq!(started.early_finish, Some(at("2026-01-05 09:00")));
    assert_eq!(started.late_finish, Some(at("2026-01-02 08:00")));
    assert_eq!(started.total_float, Some(Minutes(-(3 * 1440 + 60))));
}

/// docs/rules/TF-005-no-forecast-before-the-data-date.md, item 7: started work
/// whose mandatory finish leaves too little time for what remains keeps the
/// early dates its links and the data date give. The late dates keep the
/// mandatory finish, so it and the work driving it show negative float.
#[test]
fn tf_005_started_work_keeps_its_logic_when_its_mandatory_finish_is_too_early() {
    let mandatory = |date| {
        Some(Constraint {
            kind: ConstraintType::MandatoryFinish,
            date: Some(at(date)),
        })
    };
    // A predecessor finishing Tue 12:00 pushes B's eight remaining hours past
    // its mandatory finish of Tue 16:00, which would need them to start at
    // Tue 08:00. C follows B.
    let mut s = chain(&[28 * 60, 8 * 60, 60]);
    begin(&mut s, 2, "2026-01-02 08:00", 8 * 60);
    get(&mut s, 2).primary_constraint = mandatory("2026-01-06 16:00");
    let r = run(&s);
    let pushed = activity(&r, 2);
    assert_eq!(
        pushed.values.remaining_early_start,
        Some(at("2026-01-06 12:00"))
    );
    assert_eq!(pushed.values.early_finish, Some(at("2026-01-06 20:00")));
    assert!(matches!(
        pushed.provenance[&DateField::EarlyStart].driver(),
        DateProvenance::Relationship {
            from: ActivityId(1),
            ..
        }
    ));
    assert_eq!(pushed.values.late_start, Some(at("2026-01-06 08:00")));
    assert_eq!(pushed.values.late_finish, Some(at("2026-01-06 16:00")));
    assert_eq!(
        pushed.provenance[&DateField::LateFinish].driver(),
        &DateProvenance::Constraint {
            kind: ConstraintType::MandatoryFinish,
            date: at("2026-01-06 16:00"),
        }
    );
    assert_eq!(pushed.values.total_float, Some(Minutes(-240)));
    // The driving predecessor shows the same miss; the successor measures
    // from B's early finish, and its float isn't negative.
    let driver = &activity(&r, 1).values;
    assert_eq!(driver.early_finish, Some(at("2026-01-06 12:00")));
    assert_eq!(driver.late_finish, Some(at("2026-01-06 08:00")));
    assert_eq!(driver.total_float, Some(Minutes(-240)));
    let successor = &activity(&r, 3).values;
    assert_eq!(successor.early_start, Some(at("2026-01-06 20:00")));
    assert_eq!(successor.total_float, Some(Minutes::ZERO));

    // With no predecessor, the data date holds the remaining work: ten hours
    // from Mon 08:00 can't finish by the mandatory Mon 12:00.
    let mut s = chain(&[10 * 60]);
    begin(&mut s, 1, "2026-01-02 08:00", 10 * 60);
    get(&mut s, 1).primary_constraint = mandatory("2026-01-05 12:00");
    let r = run(&s);
    let held = activity(&r, 1);
    assert_eq!(held.values.remaining_early_start, Some(at(DATA_DATE)));
    assert_eq!(
        held.provenance[&DateField::EarlyStart].driver(),
        &DateProvenance::DataDate
    );
    assert_eq!(held.values.early_finish, Some(at("2026-01-05 18:00")));
    assert_eq!(held.values.late_finish, Some(at("2026-01-05 12:00")));
    assert_eq!(held.values.total_float, Some(Minutes(-360)));

    // Item 5 still holds for started work: a mandatory finish that leaves
    // room sets the early finish, later than the remaining work alone would.
    get(&mut s, 1).primary_constraint = mandatory("2026-01-06 12:00");
    let r = run(&s);
    let fixed = &activity(&r, 1).values;
    assert_eq!(fixed.early_finish, Some(at("2026-01-06 12:00")));
    assert_eq!(fixed.total_float, Some(Minutes::ZERO));
}

/// docs/rules/TF-005-no-forecast-before-the-data-date.md, items 1 to 4: a
/// mandatory date before the data date can't pull unstarted work back past it.
/// The late dates keep the mandatory date, so total float goes negative.
#[test]
fn tf_005_a_mandatory_date_before_the_data_date_holds_unstarted_work_at_it() {
    let fix = |kind| {
        Some(Constraint {
            kind,
            date: Some(at("2026-01-02 08:00")),
        })
    };
    // Item 1, a mandatory finish: the task starts at the data date and runs
    // its whole duration; its successor follows (item 3).
    let mut s = chain(&[60, 60]);
    get(&mut s, 1).primary_constraint = fix(ConstraintType::MandatoryFinish);
    let r = run(&s);
    let held = activity(&r, 1);
    assert_eq!(held.values.early_start, Some(at(DATA_DATE)));
    assert_eq!(held.values.early_finish, Some(at("2026-01-05 09:00")));
    assert_eq!(held.values.remaining_early_start, Some(at(DATA_DATE)));
    assert_eq!(
        held.provenance[&DateField::EarlyStart].driver(),
        &DateProvenance::DataDate
    );
    // Item 4: the late finish is the mandatory date, three days and an hour
    // before the early finish on this continuous calendar.
    assert_eq!(held.values.late_finish, Some(at("2026-01-02 08:00")));
    assert_eq!(held.values.total_float, Some(Minutes(-(3 * 1440 + 60))));
    assert_eq!(
        activity(&r, 2).values.early_start,
        Some(at("2026-01-05 09:00"))
    );

    // Item 1, a mandatory start.
    get(&mut s, 1).primary_constraint = fix(ConstraintType::MandatoryStart);
    let r = run(&s);
    let held = &activity(&r, 1).values;
    assert_eq!(held.early_start, Some(at(DATA_DATE)));
    assert_eq!(held.late_start, Some(at("2026-01-02 08:00")));

    // Item 2, a finish milestone due before the data date is at the data date.
    let mut s = chain(&[0, 60]);
    get(&mut s, 1).activity_type = ActivityType::FinishMilestone;
    get(&mut s, 1).primary_constraint = fix(ConstraintType::MandatoryFinish);
    let r = run(&s);
    let milestone = &activity(&r, 1).values;
    assert_eq!(milestone.early_start, Some(at(DATA_DATE)));
    assert_eq!(milestone.early_finish, Some(at(DATA_DATE)));
    assert_eq!(milestone.total_float, Some(Minutes(-(3 * 1440))));
}

/// docs/rules/TF-005-no-forecast-before-the-data-date.md, item 5: a mandatory
/// date on or after the data date still overrides logic, as TF-016 says.
#[test]
fn tf_005_a_mandatory_date_after_the_data_date_still_overrides_logic() {
    let mut s = chain(&[600, 60]);
    get(&mut s, 2).primary_constraint = Some(Constraint {
        kind: ConstraintType::MandatoryStart,
        date: Some(at("2026-01-05 09:00")),
    });
    let r = run(&s);
    assert_eq!(
        activity(&r, 2).values.early_start,
        Some(at("2026-01-05 09:00"))
    );
}

#[test]
fn constraints_stop_applying_as_work_progresses() {
    let fix = |kind, date_text: &str| {
        Some(Constraint {
            kind,
            date: Some(at(date_text)),
        })
    };
    // A start date can't hold work that has started.
    let mut s = chain(&[600, 60]);
    begin(&mut s, 1, "2026-01-04 08:00", 60);
    get(&mut s, 1).primary_constraint = fix(ConstraintType::StartOnOrAfter, "2026-01-08 08:00");
    assert_eq!(
        activity(&run(&s), 1).values.early_start,
        Some(at(DATA_DATE))
    );

    // Its finish can still be held.
    get(&mut s, 1).primary_constraint = fix(ConstraintType::FinishOnOrAfter, "2026-01-08 08:00");
    assert_eq!(
        activity(&run(&s), 1).values.early_finish,
        Some(at("2026-01-08 08:00"))
    );

    // Finished work takes no constraint at all.
    let mut s = chain(&[60, 60]);
    complete(&mut s, 1, "2026-01-02 08:00", "2026-01-02 09:00");
    get(&mut s, 1).primary_constraint = fix(ConstraintType::MandatoryStart, "2026-01-08 08:00");
    assert_eq!(
        activity(&run(&s), 1).values.early_start,
        Some(at(DATA_DATE))
    );
}

#[test]
fn impossible_progress_is_refused() {
    type Change = fn(&mut Activity);
    let cases: [(Change, &str); 5] = [
        (
            |a| a.actual_start = Some(at("2026-01-02 08:00")),
            "not-started activity with actual, suspend or resume dates",
        ),
        (
            |a| a.resume = Some(at("2026-01-02 08:00")),
            "not-started activity with actual, suspend or resume dates",
        ),
        (
            |a| a.status = ActivityStatus::InProgress,
            "in-progress activity needs an actual start and no actual finish",
        ),
        (
            |a| a.status = ActivityStatus::Completed,
            "completed activity without an actual finish",
        ),
        (
            |a| {
                a.status = ActivityStatus::InProgress;
                a.actual_start = Some(at("2026-01-02 08:00"));
                a.resume = Some(at("2026-01-03 08:00"));
            },
            "resume date without a suspend date",
        ),
    ];
    for (change, message) in cases {
        let mut s = network(&[60]);
        change(get(&mut s, 1));
        assert_eq!(
            schedule(&s, &s.project.options).err(),
            Some(EngineError::InvalidInput(message))
        );
    }

    let mut s = network(&[60]);
    begin(&mut s, 1, "2026-01-02 08:00", 60);
    s.project.data_date = None;
    assert_eq!(
        schedule(&s, &s.project.options).err(),
        Some(EngineError::InvalidInput("progress without a data date"))
    );
}

#[test]
fn unsupported_progress_is_refused_rather_than_approximated() {
    let mut s = network(&[60]);
    begin(&mut s, 1, "2026-01-02 08:00", 60);
    get(&mut s, 1).suspend = Some(at("2026-01-03 08:00"));
    assert_eq!(
        schedule(&s, &s.project.options).err(),
        Some(EngineError::Unsupported(
            "suspended activity without a resume date"
        ))
    );

    let mut s = network(&[60]);
    begin(&mut s, 1, "2026-01-02 08:00", 60);
    s.project.options.out_of_sequence = OutOfSequenceMode::ActualDates;
    assert_eq!(
        schedule(&s, &s.project.options).err(),
        Some(EngineError::Unsupported("actual dates scheduling"))
    );
}

fn mode(s: &mut Schedule, mode: OutOfSequenceMode) {
    s.project.options.out_of_sequence = mode;
}

#[test]
fn a_completed_late_point_is_the_bound_as_it_falls() {
    // One point, not moved to working time: Saturday on a weekday calendar.
    for kind in [
        RelationshipType::FinishToStart,
        RelationshipType::StartToStart,
    ] {
        let mut s = network(&[60, 60]);
        edge(&mut s, 1, 1, 2, kind, 0);
        s.calendars
            .insert(CalendarId(2), calendar(2, &[1, 2, 3, 4, 5], 480, 1020));
        get(&mut s, 1).calendar = CalendarId(2);
        s.project.data_date = Some(at("2026-01-02 10:00"));
        s.project.planned_start = s.project.data_date;
        get(&mut s, 2).primary_constraint = Some(Constraint {
            kind: ConstraintType::MandatoryStart,
            date: Some(at("2026-01-03 10:00")),
        });
        complete(&mut s, 1, "2026-01-01 08:00", "2026-01-01 09:00");
        let r = run(&s);
        let done = &activity(&r, 1).values;
        assert_eq!(done.late_start, Some(at("2026-01-03 10:00")), "{kind:?}");
        assert_eq!(done.late_finish, done.late_start, "{kind:?}");
    }
}

#[test]
fn lag_from_work_that_happened_counts_down_to_the_data_date() {
    // Three days of lag from an actual finish one day before the data date
    // leaves two days to run.
    let mut s = chain(&[60, 60]);
    complete(&mut s, 1, "2026-01-03 08:00", "2026-01-04 08:00");
    s.relationships.get_mut(&RelationshipId(1)).unwrap().lag = Minutes(3 * 1440);
    assert_eq!(
        activity(&run(&s), 2).values.early_start,
        Some(at("2026-01-07 08:00"))
    );

    // Lag that ran out before the data date leaves nothing, on either side.
    let mut s = network(&[60, 60]);
    edge(&mut s, 1, 1, 2, RelationshipType::StartToStart, 2 * 1440);
    complete(&mut s, 1, "2026-01-02 08:00", "2026-01-02 09:00");
    assert_eq!(
        activity(&run(&s), 2).values.early_start,
        Some(at(DATA_DATE))
    );
}

#[test]
fn what_is_left_of_a_lag_runs_from_where_the_predecessor_stands() {
    // A has run three days of a five-day SS lag but is suspended until Jan 8, so
    // the last two days run from there. Straight from the actual start, B would
    // start on Jan 7.
    let mut s = network(&[600, 60]);
    edge(&mut s, 1, 1, 2, RelationshipType::StartToStart, 5 * 1440);
    begin(&mut s, 1, "2026-01-02 08:00", 60);
    get(&mut s, 1).suspend = Some(at("2026-01-03 08:00"));
    get(&mut s, 1).resume = Some(at("2026-01-08 08:00"));
    assert_eq!(
        activity(&run(&s), 2).values.early_start,
        Some(at("2026-01-10 08:00"))
    );

    // With the option off it does run straight from the actual start.
    s.project.options.ss_lag_from_early_start = false;
    assert_eq!(
        activity(&run(&s), 2).values.early_start,
        Some(at("2026-01-07 08:00"))
    );
}

#[test]
fn an_actual_after_the_data_date_adds_no_lag() {
    let mut s = chain(&[60, 60]);
    complete(&mut s, 1, "2026-01-02 08:00", "2026-01-05 12:00");
    let r = run(&s);
    assert_eq!(activity(&r, 2).values.early_start, Some(at(DATA_DATE)));
}

#[test]
fn a_finished_successor_still_passes_on_its_late_point() {
    // C is what sets the project finish, so only the link through B can pull A's
    // late start back to the data date.
    let mut s = network(&[60, 60, 600]);
    edge(&mut s, 1, 1, 2, RelationshipType::StartToStart, 60);
    edge(&mut s, 2, 2, 3, RelationshipType::FinishToStart, 0);
    begin(&mut s, 1, "2026-01-02 08:00", 60);
    complete(&mut s, 2, "2026-01-03 08:00", "2026-01-03 09:00");
    let r = run(&s);
    assert_eq!(activity(&r, 1).values.late_start, Some(at(DATA_DATE)));
    assert_eq!(
        activity(&r, 1).values.late_start,
        activity(&r, 2).values.late_start
    );
}

#[test]
fn lag_from_a_completed_activity_needs_its_actual_start() {
    let mut s = network(&[60, 60]);
    edge(&mut s, 1, 1, 2, RelationshipType::StartToStart, 0);
    complete(&mut s, 1, "2026-01-02 08:00", "2026-01-02 09:00");
    get(&mut s, 1).actual_start = None;
    assert_eq!(
        activity(&run(&s), 2).values.early_start,
        Some(at(DATA_DATE))
    );

    s.relationships.get_mut(&RelationshipId(1)).unwrap().lag = Minutes(60);
    assert_eq!(
        schedule(&s, &s.project.options).err(),
        Some(EngineError::InvalidInput(
            "lag from a completed activity with no actual start"
        ))
    );
}

#[test]
fn an_actual_start_link_ends_a_chain_on_the_actual_date() {
    let mut s = network(&[600, 60]);
    edge(&mut s, 1, 1, 2, RelationshipType::StartToStart, 3 * 1440);
    begin(&mut s, 1, "2026-01-04 08:00", 60);
    s.project.options.ss_lag_from_early_start = false;
    let r = run(&s);
    let chain = r.driving_chain(ActivityId(2), DateField::EarlyStart);
    let steps: Vec<_> = chain.iter().map(|l| (l.activity.0, l.field)).collect();
    assert_eq!(
        steps,
        [(2, DateField::EarlyStart), (1, DateField::ActualStart)]
    );
    assert_eq!(chain[1].date, Some(at("2026-01-04 08:00")));
    assert_eq!(chain[1].provenance, Some(&DateProvenance::Actual));
}

#[test]
fn each_relationship_type_detects_broken_logic() {
    type Progress = fn(&mut Schedule);
    let started: Progress = |s| begin(s, 2, "2026-01-02 08:00", 60);
    let finished: Progress = |s| complete(s, 2, "2026-01-02 08:00", "2026-01-02 09:00");
    let unstarted: Progress = |_| {};
    for (kind, first_started, second, broken) in [
        (RelationshipType::FinishToStart, true, started, true),
        (RelationshipType::FinishToStart, false, started, true),
        (RelationshipType::StartToStart, false, started, true),
        (RelationshipType::StartToStart, true, started, false),
        (RelationshipType::FinishToFinish, true, finished, true),
        (RelationshipType::FinishToFinish, true, started, false),
        (RelationshipType::StartToFinish, false, finished, true),
        (RelationshipType::StartToFinish, true, finished, false),
        (RelationshipType::FinishToStart, true, unstarted, false),
    ] {
        let mut s = network(&[60, 60]);
        edge(&mut s, 7, 1, 2, kind, 0);
        if first_started {
            begin(&mut s, 1, "2026-01-02 08:00", 60);
        }
        second(&mut s);
        let expected = if broken {
            vec![RelationshipId(7)]
        } else {
            vec![]
        };
        assert_eq!(out_of_sequence(&s), expected, "{kind:?}");
        assert!(schedule(&s, &s.project.options).is_ok(), "{kind:?}");
    }
}

/// A runs 120 minutes from the data date and B has started out of sequence.
fn broken_pair(kind: RelationshipType) -> Schedule {
    let mut s = network(&[120, 60]);
    edge(&mut s, 1, 1, 2, kind, 0);
    if kind == RelationshipType::FinishToStart {
        begin(&mut s, 1, "2026-01-04 08:00", 120);
    }
    begin(&mut s, 2, "2026-01-04 09:00", 60);
    s
}

#[test]
fn retained_logic_holds_remaining_work_for_each_broken_link() {
    for (kind, start) in [
        (RelationshipType::FinishToStart, "2026-01-05 10:00"),
        (RelationshipType::StartToStart, "2026-01-05 08:00"),
    ] {
        let r = run(&broken_pair(kind));
        assert_eq!(
            activity(&r, 2).values.early_start,
            Some(at(start)),
            "{kind:?}"
        );
        assert_eq!(
            activity(&r, 2).values.remaining_early_start,
            Some(at(start))
        );
    }

    // Lag on a broken link still has to run from the unfinished predecessor.
    let mut s = broken_pair(RelationshipType::FinishToStart);
    s.relationships.get_mut(&RelationshipId(1)).unwrap().lag = Minutes(60);
    assert_eq!(
        activity(&run(&s), 2).values.early_start,
        Some(at("2026-01-05 11:00"))
    );
}

#[test]
fn progress_override_drops_a_broken_link_in_both_passes() {
    for kind in [
        RelationshipType::FinishToStart,
        RelationshipType::StartToStart,
    ] {
        let mut s = broken_pair(kind);
        mode(&mut s, OutOfSequenceMode::ProgressOverride);
        let r = run(&s);
        // B carries on from the data date, and A's late dates owe it nothing.
        assert_eq!(
            activity(&r, 2).values.early_start,
            Some(at(DATA_DATE)),
            "{kind:?}"
        );
        assert_eq!(
            activity(&r, 1).values.late_finish,
            r.project.scheduled_finish
        );
        assert_eq!(activity(&r, 1).values.free_float, Some(Minutes::ZERO));
    }
}

#[test]
fn progress_override_keeps_links_progress_has_not_broken() {
    let mut s = network(&[60, 60, 60]);
    edge(&mut s, 1, 1, 2, RelationshipType::FinishToStart, 0);
    edge(&mut s, 2, 2, 3, RelationshipType::FinishToStart, 0);
    begin(&mut s, 1, "2026-01-04 08:00", 60);
    begin(&mut s, 2, "2026-01-04 09:00", 60);
    mode(&mut s, OutOfSequenceMode::ProgressOverride);
    assert_eq!(
        activity(&run(&s), 3).values.early_start,
        Some(at("2026-01-05 09:00"))
    );
}

/// B finished while A, its predecessor, is still running.
fn finished_early() -> Schedule {
    let mut s = network(&[120, 60, 60]);
    edge(&mut s, 1, 1, 2, RelationshipType::FinishToStart, 0);
    edge(&mut s, 2, 2, 3, RelationshipType::FinishToStart, 0);
    begin(&mut s, 1, "2026-01-03 08:00", 120);
    complete(&mut s, 2, "2026-01-03 09:00", "2026-01-03 10:00");
    s
}

#[test]
fn retained_logic_pushes_finished_work_and_on_through_it() {
    let r = run(&finished_early());
    let pushed = Some(at("2026-01-05 10:00"));
    assert_eq!(activity(&r, 2).values.early_start, pushed);
    assert_eq!(activity(&r, 2).values.early_finish, pushed);
    assert_eq!(activity(&r, 2).values.remaining_early_start, None);
    assert_eq!(activity(&r, 3).values.early_start, pushed);

    // Further finished work inherits the push, without any lag between them.
    let mut s = finished_early();
    edge(&mut s, 3, 2, 4, RelationshipType::StartToStart, 7 * 1440);
    let mut extra = s.activities[&ActivityId(3)].clone();
    extra.id = ActivityId(4);
    s.activities.insert(ActivityId(4), extra);
    complete(&mut s, 4, "2026-01-03 11:00", "2026-01-03 12:00");
    assert_eq!(activity(&run(&s), 4).values.early_start, pushed);
}

#[test]
fn progress_override_leaves_finished_work_on_the_data_date() {
    let mut s = finished_early();
    mode(&mut s, OutOfSequenceMode::ProgressOverride);
    let r = run(&s);
    assert_eq!(activity(&r, 2).values.early_start, Some(at(DATA_DATE)));
    assert_eq!(activity(&r, 3).values.early_start, Some(at(DATA_DATE)));
}

#[test]
fn something_still_holding_finished_work_moves_it_to_working_time() {
    // A's SS lag ran out long ago, so it doesn't push B, but A is unfinished. The
    // data date is Sunday midnight, and that puts B on Monday morning.
    let mut s = network(&[600, 60]);
    weekday_calendar(&mut s);
    edge(&mut s, 1, 1, 2, RelationshipType::StartToStart, 60);
    s.project.data_date = Some(at("2026-01-04 00:00"));
    s.project.planned_start = Some(at("2026-01-01 08:00"));
    begin(&mut s, 1, "2026-01-01 08:00", 600);
    complete(&mut s, 2, "2026-01-02 08:00", "2026-01-02 09:00");
    assert_eq!(
        activity(&run(&s), 2).values.early_start,
        Some(at("2026-01-05 08:00"))
    );
}

/// B finished while its predecessor A hasn't reached the side the link needs.
fn broken_finish(kind: RelationshipType) -> Schedule {
    let mut s = network(&[120, 60]);
    edge(&mut s, 1, 1, 2, kind, 0);
    if kind == RelationshipType::FinishToFinish {
        begin(&mut s, 1, "2026-01-03 08:00", 120);
    }
    complete(&mut s, 2, "2026-01-03 09:00", "2026-01-03 10:00");
    s
}

#[test]
fn finish_side_links_push_finished_work_under_retained_logic() {
    for (kind, point) in [
        (RelationshipType::FinishToFinish, "2026-01-05 10:00"),
        (RelationshipType::StartToFinish, "2026-01-05 08:00"),
    ] {
        let r = run(&broken_finish(kind));
        assert_eq!(
            activity(&r, 2).values.early_start,
            Some(at(point)),
            "{kind:?}"
        );
        assert_eq!(
            activity(&r, 2).values.early_finish,
            Some(at(point)),
            "{kind:?}"
        );
    }
}

#[test]
fn progress_override_drops_finish_side_links_too() {
    for kind in [
        RelationshipType::FinishToFinish,
        RelationshipType::StartToFinish,
    ] {
        let mut s = broken_finish(kind);
        mode(&mut s, OutOfSequenceMode::ProgressOverride);
        let r = run(&s);
        assert_eq!(
            activity(&r, 2).values.early_start,
            Some(at(DATA_DATE)),
            "{kind:?}"
        );
        assert_eq!(
            activity(&r, 1).values.late_finish,
            r.project.scheduled_finish
        );
    }
}

#[test]
fn a_dropped_link_does_not_hold_back_as_late_as_possible_work() {
    // A is ALAP with a broken link into started B. The link is gone, so C's
    // finish is what lets A move late.
    let mut s = network(&[60, 60, 600]);
    edge(&mut s, 1, 1, 2, RelationshipType::FinishToStart, 0);
    begin(&mut s, 2, "2026-01-04 08:00", 60);
    get(&mut s, 1).primary_constraint = Some(Constraint {
        kind: ConstraintType::AsLateAsPossible,
        date: None,
    });
    mode(&mut s, OutOfSequenceMode::ProgressOverride);
    assert_eq!(
        activity(&run(&s), 1).values.early_finish,
        Some(at("2026-01-05 18:00"))
    );
}

#[test]
fn a_lead_on_a_side_that_has_happened_is_used_up() {
    // A started, so SS -1d carries A's position with no lead in either pass. Tempting
    // to keep it, but that loses corpus matches in both (TF-022).
    let mut s = network(&[60, 2 * 1440]);
    edge(&mut s, 1, 1, 2, RelationshipType::StartToStart, -1440);
    begin(&mut s, 1, "2026-01-04 08:00", 60);
    let r = run(&s);
    assert_eq!(activity(&r, 2).values.early_start, Some(at(DATA_DATE)));
    assert_eq!(activity(&r, 1).values.late_start, Some(at(DATA_DATE)));
}

#[test]
fn lag_between_finished_work_needs_no_actual_start() {
    let mut s = network(&[60, 60]);
    edge(&mut s, 1, 1, 2, RelationshipType::StartToStart, 60);
    complete(&mut s, 1, "2026-01-02 08:00", "2026-01-02 09:00");
    complete(&mut s, 2, "2026-01-02 09:00", "2026-01-02 10:00");
    get(&mut s, 1).actual_start = None;
    assert!(schedule(&s, &s.project.options).is_ok());
}

#[test]
fn moving_to_working_time_is_not_a_push() {
    // B is held at Sunday's data date and moves to Monday on its weekday calendar.
    // C works every day, so B has not pushed it past Sunday.
    let mut s = network(&[600, 60, 60]);
    s.calendars
        .insert(CalendarId(2), calendar(2, &[1, 2, 3, 4, 5], 480, 1020));
    edge(&mut s, 1, 1, 2, RelationshipType::StartToStart, 60);
    edge(&mut s, 2, 2, 3, RelationshipType::FinishToStart, 0);
    get(&mut s, 2).calendar = CalendarId(2);
    s.project.data_date = Some(at("2026-01-04 00:00"));
    s.project.planned_start = Some(at("2026-01-01 08:00"));
    begin(&mut s, 1, "2026-01-01 08:00", 600);
    complete(&mut s, 2, "2026-01-02 08:00", "2026-01-02 09:00");
    let r = run(&s);
    assert_eq!(
        activity(&r, 2).values.early_start,
        Some(at("2026-01-05 08:00"))
    );
    assert_eq!(
        activity(&r, 3).values.early_start,
        Some(at("2026-01-04 00:00"))
    );
}

#[test]
fn provenance_records_the_lag_that_was_applied() {
    let mut s = chain(&[60, 60]);
    complete(&mut s, 1, "2026-01-03 08:00", "2026-01-04 08:00");
    s.relationships.get_mut(&RelationshipId(1)).unwrap().lag = Minutes(3 * 1440);
    let r = run(&s);
    let DateProvenance::Relationship { lag, .. } =
        activity(&r, 2).provenance[&DateField::EarlyStart]
    else {
        panic!("expected a relationship");
    };
    assert_eq!(lag, Minutes(2 * 1440));
}
