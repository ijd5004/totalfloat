use super::*;
fn constrain(input: &mut Schedule, id: u64, kind: ConstraintType, date: Option<&str>) {
    input
        .activities
        .get_mut(&ActivityId(id))
        .unwrap()
        .primary_constraint = Some(Constraint {
        kind,
        date: date.map(at),
    });
}
#[test]
fn each_dated_constraint_type_and_provenance() {
    for (kind, early, late) in [
        (
            ConstraintType::StartOn,
            "2026-01-05 10:00",
            "2026-01-05 10:00",
        ),
        (
            ConstraintType::StartOnOrAfter,
            "2026-01-05 10:00",
            "2026-01-05 10:00",
        ),
        (
            ConstraintType::StartOnOrBefore,
            "2026-01-05 08:00",
            "2026-01-05 08:00",
        ),
        (
            ConstraintType::FinishOn,
            "2026-01-05 09:00",
            "2026-01-05 09:00",
        ),
        (
            ConstraintType::FinishOnOrAfter,
            "2026-01-05 09:00",
            "2026-01-05 09:00",
        ),
        (
            ConstraintType::FinishOnOrBefore,
            "2026-01-05 08:00",
            "2026-01-05 08:00",
        ),
        (
            ConstraintType::MandatoryStart,
            "2026-01-05 10:00",
            "2026-01-05 10:00",
        ),
        (
            ConstraintType::MandatoryFinish,
            "2026-01-05 09:00",
            "2026-01-05 09:00",
        ),
    ] {
        let mut input = network(&[60]);
        constrain(&mut input, 1, kind, Some("2026-01-05 10:00"));
        let output = run(&input);
        let a = activity(&output, 1);
        assert_eq!(a.values.early_start, Some(at(early)), "{kind:?}");
        assert_eq!(a.values.late_start, Some(at(late)), "{kind:?}");
        if !matches!(
            kind,
            ConstraintType::StartOnOrBefore | ConstraintType::FinishOnOrBefore
        ) {
            assert!(
                a.provenance.values().any(
                    |p| matches!(p.driver(),DateProvenance::Constraint {kind:k,..} if *k==kind)
                )
            );
        }
    }
}
#[test]
fn soft_deadlines_create_negative_float_and_both_constraints_are_kept() {
    let mut input = network(&[60, 60]);
    edge(&mut input, 1, 1, 2, RelationshipType::FinishToStart, 0);
    constrain(
        &mut input,
        2,
        ConstraintType::StartOnOrBefore,
        Some("2026-01-05 08:30"),
    );
    input
        .activities
        .get_mut(&ActivityId(2))
        .unwrap()
        .secondary_constraint = Some(Constraint {
        kind: ConstraintType::StartOnOrAfter,
        date: Some(at("2026-01-05 10:00")),
    });
    let output = run(&input);
    let a = activity(&output, 2);
    assert_eq!(a.values.early_start, Some(at("2026-01-05 10:00")));
    assert_eq!(a.values.late_start, Some(at("2026-01-05 08:30")));
    assert_eq!(a.values.total_float, Some(Minutes(-90)));
    assert!(matches!(
        a.provenance[&DateField::EarlyStart],
        DateProvenance::Constraint {
            kind: ConstraintType::StartOnOrAfter,
            ..
        }
    ));
    assert!(matches!(
        a.provenance[&DateField::LateStart],
        DateProvenance::Constraint {
            kind: ConstraintType::StartOnOrBefore,
            ..
        }
    ));
    for kind in [
        ConstraintType::StartOn,
        ConstraintType::FinishOn,
        ConstraintType::MandatoryStart,
        ConstraintType::MandatoryFinish,
    ] {
        constrain(&mut input, 2, kind, Some("2026-01-05 08:00"));
        assert!(matches!(
            schedule(&input, &input.project.options),
            Err(EngineError::InvalidInput(_))
        ));
    }
}
#[test]
fn mandatory_dates_override_logic_and_propagate_to_neighbors() {
    let mut input = network(&[120, 60, 60]);
    edge(&mut input, 1, 1, 2, RelationshipType::FinishToStart, 0);
    edge(&mut input, 2, 2, 3, RelationshipType::FinishToStart, 0);
    constrain(
        &mut input,
        2,
        ConstraintType::MandatoryStart,
        Some("2026-01-05 09:00"),
    );
    let output = run(&input);
    assert_eq!(
        activity(&output, 2).values.early_start,
        Some(at("2026-01-05 09:00"))
    );
    assert_eq!(
        activity(&output, 2).values.late_start,
        Some(at("2026-01-05 09:00"))
    );
    assert_eq!(activity(&output, 1).values.total_float, Some(Minutes(-60)));
    assert_eq!(
        activity(&output, 3).values.early_start,
        Some(at("2026-01-05 10:00"))
    );
    constrain(
        &mut input,
        2,
        ConstraintType::MandatoryFinish,
        Some("2026-01-05 09:00"),
    );
    let output = run(&input);
    assert_eq!(
        activity(&output, 2).values.early_finish,
        Some(at("2026-01-05 09:00"))
    );
    assert_eq!(
        activity(&output, 2).values.late_finish,
        Some(at("2026-01-05 09:00"))
    );
    assert_eq!(
        activity(&output, 3).values.early_start,
        Some(at("2026-01-05 09:00"))
    );
}
/// docs/rules/TF-005-no-forecast-before-the-data-date.md, item 6: a mandatory
/// date that breaks a link leaves its predecessor zero free float.
#[test]
fn tf_005_a_link_broken_by_a_mandatory_date_leaves_zero_free_float() {
    let mut input = network(&[120, 60, 60]);
    edge(&mut input, 1, 1, 2, RelationshipType::FinishToStart, 0);
    edge(&mut input, 2, 1, 3, RelationshipType::FinishToStart, 0);
    constrain(
        &mut input,
        2,
        ConstraintType::MandatoryStart,
        Some("2026-01-05 09:00"),
    );
    let output = run(&input);
    let broken = &activity(&output, 1).values;
    // Activity 1 finishes at 10:00, an hour after the mandatory start it
    // precedes: its total float is negative, and its free float is zero.
    assert_eq!(broken.early_finish, Some(at("2026-01-05 10:00")));
    assert_eq!(broken.total_float, Some(Minutes(-60)));
    assert_eq!(broken.free_float, Some(Minutes::ZERO));
}
#[test]
fn alap_consumes_free_float_without_delaying_successors() {
    let mut input = network(&[60, 180, 60]);
    edge(&mut input, 1, 1, 3, RelationshipType::FinishToStart, 0);
    edge(&mut input, 2, 2, 3, RelationshipType::FinishToStart, 0);
    let before = run(&input);
    constrain(&mut input, 1, ConstraintType::AsLateAsPossible, None);
    let output = run(&input);
    assert_eq!(
        activity(&output, 1).values.early_start,
        Some(at("2026-01-05 10:00"))
    );
    assert_eq!(activity(&output, 1).values.free_float, Some(Minutes(0)));
    assert_eq!(
        activity(&output, 3).values.early_start,
        activity(&before, 3).values.early_start
    );
    assert_eq!(output.project, before.project);
    let chain = output.driving_chain(ActivityId(1), DateField::EarlyStart);
    assert!(chain.len() > 1);
    assert!(
        activity(&output, 1)
            .provenance
            .values()
            .any(|p| matches!(p, DateProvenance::AsLateAsPossible { .. }))
    );
}
#[test]
fn expected_finish_recomputes_work_span_only_when_enabled() {
    let mut input = network(&[60, 60]);
    edge(&mut input, 1, 1, 2, RelationshipType::FinishToStart, 0);
    input
        .activities
        .get_mut(&ActivityId(1))
        .unwrap()
        .expected_finish = Some(at("2026-01-05 11:00"));
    let output = run(&input);
    assert_eq!(
        activity(&output, 1).values.early_start,
        Some(at("2026-01-05 08:00"))
    );
    assert_eq!(
        activity(&output, 1).values.early_finish,
        Some(at("2026-01-05 11:00"))
    );
    assert_eq!(
        activity(&output, 2).values.early_start,
        Some(at("2026-01-05 11:00"))
    );
    assert_eq!(
        activity(&output, 1).values.late_start,
        Some(at("2026-01-05 08:00"))
    );
    assert_eq!(
        activity(&output, 1).provenance[&DateField::EarlyFinish],
        DateProvenance::ExpectedFinish
    );
    assert_eq!(
        input.activities[&ActivityId(1)].remaining_duration,
        Minutes(60)
    );
    input.project.options.use_expected_finish = false;
    assert_eq!(
        activity(&run(&input), 1).values.early_finish,
        Some(at("2026-01-05 09:00"))
    );
}
#[test]
fn project_deadline_controls_late_dates_not_early_finish() {
    let mut input = network(&[120]);
    input.project.must_finish_by = Some(at("2026-01-05 09:00"));
    for option in [false, true] {
        input.project.options.float_from_project_finish = option;
        let output = run(&input);
        assert_eq!(
            output.project.scheduled_finish,
            Some(at("2026-01-05 10:00"))
        );
        assert_eq!(
            activity(&output, 1).values.late_finish,
            Some(at("2026-01-05 09:00"))
        );
        assert_eq!(activity(&output, 1).values.total_float, Some(Minutes(-60)));
        assert_eq!(
            activity(&output, 1).provenance[&DateField::LateFinish],
            DateProvenance::MustFinishBy
        );
    }
    input.project.must_finish_by = Some(at("2026-01-05 12:00"));
    assert_eq!(
        activity(&run(&input), 1).values.total_float,
        Some(Minutes(120))
    );
}
#[test]
fn nonworking_mandatory_dates_normalize_in_opposite_pass_directions() {
    let mut input = network(&[60]);
    input
        .calendars
        .insert(CalendarId(1), calendar(1, &[1, 2, 3, 4, 5], 480, 1020));
    constrain(
        &mut input,
        1,
        ConstraintType::MandatoryStart,
        Some("2026-01-10 12:00"),
    );
    let output = run(&input);
    assert_eq!(
        activity(&output, 1).values.early_start,
        Some(at("2026-01-12 08:00"))
    );
    assert_eq!(
        activity(&output, 1).values.late_start,
        Some(at("2026-01-09 17:00"))
    );
    // Friday's end and Monday's start are the same work position.
    assert_eq!(activity(&output, 1).values.total_float, Some(Minutes(0)));
}

#[test]
fn invalid_dates_and_conflicts_are_explicit_and_expected_estimates_do_not_override_mandatory() {
    let mut input = network(&[60]);
    constrain(&mut input, 1, ConstraintType::FinishOn, None);
    assert!(matches!(
        schedule(&input, &input.project.options),
        Err(EngineError::InvalidInput(_))
    ));
    input
        .activities
        .get_mut(&ActivityId(1))
        .unwrap()
        .primary_constraint = None;
    input
        .activities
        .get_mut(&ActivityId(1))
        .unwrap()
        .secondary_constraint = Some(Constraint {
        kind: ConstraintType::StartOnOrAfter,
        date: Some(at("2026-01-05 09:00")),
    });
    assert!(matches!(
        schedule(&input, &input.project.options),
        Err(EngineError::InvalidInput(_))
    ));
    input
        .activities
        .get_mut(&ActivityId(1))
        .unwrap()
        .secondary_constraint = None;
    input
        .activities
        .get_mut(&ActivityId(1))
        .unwrap()
        .expected_finish = Some(at("2026-01-04 08:00"));
    // Before the calculated start, an estimate is ignored (TF-018).
    let ignored = schedule(&input, &input.project.options).unwrap();
    assert_eq!(
        ignored.activities[&ActivityId(1)].values.early_finish,
        ignored.activities[&ActivityId(1)]
            .values
            .early_start
            .and_then(
                |start| start.checked_add(input.activities[&ActivityId(1)].remaining_duration)
            )
    );
    constrain(
        &mut input,
        1,
        ConstraintType::MandatoryFinish,
        Some("2026-01-05 10:00"),
    );
    assert_eq!(
        activity(&run(&input), 1).values.early_finish,
        Some(at("2026-01-05 10:00"))
    );
}
#[test]
fn mandatory_finish_keeps_the_normalized_endpoint_even_on_a_work_plateau() {
    let mut input = network(&[60]);
    input
        .calendars
        .insert(CalendarId(1), calendar(1, &[1, 2, 3, 4, 5], 480, 1020));
    constrain(
        &mut input,
        1,
        ConstraintType::MandatoryFinish,
        Some("2026-01-10 12:00"),
    );
    let output = run(&input);
    assert_eq!(
        activity(&output, 1).values.early_finish,
        Some(at("2026-01-12 08:00"))
    );
    assert_eq!(
        activity(&output, 1).values.late_finish,
        Some(at("2026-01-09 17:00"))
    );
    assert_eq!(
        activity(&output, 1).values.early_start,
        Some(at("2026-01-09 16:00"))
    );
}

#[test]
fn conflicting_expected_finish_cannot_silently_break_finish_bounds() {
    let mut input = network(&[120, 120]);
    edge(&mut input, 1, 1, 2, RelationshipType::FinishToFinish, 0);
    input
        .activities
        .get_mut(&ActivityId(2))
        .unwrap()
        .expected_finish = Some(at("2026-01-05 09:00"));
    assert!(matches!(
        schedule(&input, &input.project.options),
        Err(EngineError::InvalidInput(_))
    ));
    input.relationships.clear();
    constrain(
        &mut input,
        2,
        ConstraintType::FinishOnOrAfter,
        Some("2026-01-05 10:00"),
    );
    assert!(matches!(
        schedule(&input, &input.project.options),
        Err(EngineError::InvalidInput(_))
    ));
}

/// Stored dates in a real export: an expected finish before the activity can
/// start leaves its remaining duration standing.
#[test]
fn an_expected_finish_before_the_start_is_ignored() {
    let mut s = network(&[120, 60]);
    edge(&mut s, 1, 1, 2, RelationshipType::FinishToStart, 0);
    s.activities
        .get_mut(&ActivityId(2))
        .unwrap()
        .expected_finish = Some(at("2026-01-05 09:00"));
    let r = run(&s);
    assert_eq!(
        activity(&r, 2).values.early_start,
        Some(at("2026-01-05 10:00"))
    );
    assert_eq!(
        activity(&r, 2).values.early_finish,
        Some(at("2026-01-05 11:00"))
    );
}

/// Friday's end and Monday's start are one work position, so an estimate at
/// either one gets the same answer.
#[test]
fn an_estimate_at_the_start_position_leaves_no_work_and_is_ignored() {
    let mut s = network(&[120]);
    s.activities
        .get_mut(&ActivityId(1))
        .unwrap()
        .expected_finish = Some(at("2026-01-05 08:00"));
    let r = run(&s);
    assert_eq!(
        activity(&r, 1).values.early_finish,
        Some(at("2026-01-05 10:00"))
    );
}

#[test]
fn an_ignored_estimate_doesnt_stop_as_late_as_possible() {
    let mut s = network(&[60, 480]);
    constrain(&mut s, 1, ConstraintType::AsLateAsPossible, None);
    s.activities
        .get_mut(&ActivityId(1))
        .unwrap()
        .expected_finish = Some(at("2026-01-05 08:00"));
    let r = run(&s);
    assert_eq!(
        activity(&r, 1).values.early_finish,
        Some(at("2026-01-05 16:00"))
    );
}

#[test]
fn a_milestone_has_no_work_for_an_estimate_to_stretch() {
    let mut s = network(&[0, 60]);
    s.activities.get_mut(&ActivityId(1)).unwrap().activity_type = ActivityType::FinishMilestone;
    s.activities
        .get_mut(&ActivityId(1))
        .unwrap()
        .expected_finish = Some(at("2026-01-06 08:00"));
    let r = run(&s);
    assert_eq!(
        activity(&r, 1).values.early_finish,
        Some(at("2026-01-05 08:00"))
    );
}
