use super::*;
use totalfloat_model::{
    Activity, Calendar, CalendarId, CalendarType, HoursPerPeriod, Project, ProjectId, Relationship,
    RelationshipId, RelationshipType, TimeOfDay, WbsId, WorkInterval, WorkWeek,
};

fn at(text: &str) -> DateTime {
    text.parse().unwrap()
}

/// Activities of the given durations, starting together on a calendar that
/// works every minute, so finishes are plain arithmetic.
fn parallel(durations: &[i64]) -> Schedule {
    let mut project = Project::new(ProjectId(1), "AUTHORED");
    project.planned_start = Some(at("2026-01-05 00:00"));
    project.data_date = project.planned_start;
    project.default_calendar = Some(CalendarId(1));
    let mut s = Schedule::new(project);
    let all_day = WorkInterval::new(
        TimeOfDay::from_minutes(0).unwrap(),
        TimeOfDay::from_minutes(1440).unwrap(),
    )
    .unwrap();
    s.calendars.insert(
        CalendarId(1),
        Calendar {
            id: CalendarId(1),
            name: "Always".to_owned(),
            kind: CalendarType::Global,
            project: None,
            base: None,
            is_default: true,
            hours_per_period: HoursPerPeriod::default(),
            work_week: WorkWeek(std::array::from_fn(|_| vec![all_day])),
            exceptions: BTreeMap::new(),
            source: None,
        },
    );
    for (i, d) in durations.iter().enumerate() {
        let id = ActivityId(u64::try_from(i).unwrap() + 1);
        let mut a = Activity::new(
            id,
            ProjectId(1),
            WbsId(1),
            CalendarId(1),
            format!("A{}", id.0),
            "Authored",
        );
        a.remaining_duration = Minutes::new(*d);
        a.original_duration = Minutes::new(*d);
        s.activities.insert(id, a);
    }
    s
}

fn uniform(min: i64, max: i64) -> Estimate {
    Estimate {
        min: Minutes::new(min),
        likely: Minutes::new(min),
        max: Minutes::new(max),
        distribution: Distribution::Uniform,
    }
}

fn settings(iterations: u32, threads: usize) -> Settings {
    Settings {
        iterations,
        seed: 1,
        threads,
    }
}

/// Minutes after the start of 2026-01-05.
fn minutes(date: DateTime) -> i64 {
    date.minutes_since(at("2026-01-05 00:00")).get()
}

#[test]
fn collapsed_estimates_give_the_deterministic_finish_every_time() {
    let s = parallel(&[600, 900]);
    let mut inputs = Inputs::default();
    for id in [1, 2] {
        let d = s.activities[&ActivityId(id)].remaining_duration;
        inputs.estimates.insert(
            ActivityId(id),
            Estimate {
                min: d,
                likely: d,
                max: d,
                distribution: Distribution::Pert,
            },
        );
    }
    let sim = simulate(&s, &[], &inputs, settings(200, 3)).unwrap();
    assert_eq!(sim.deterministic_finish, at("2026-01-05 15:00"));
    assert!(sim.finishes.iter().all(|f| *f == sim.deterministic_finish));
    assert_eq!(sim.percentile(50), sim.deterministic_finish);
    assert!((sim.probability_by(sim.deterministic_finish) - 1.0).abs() < f64::EPSILON);
    assert_eq!(sim.sensitivity, []);
}

#[test]
fn the_thread_count_changes_nothing() {
    let s = parallel(&[600, 900, 300]);
    let inputs = Inputs {
        default: Some(Range {
            min_percent: -20.0,
            max_percent: 50.0,
            distribution: Distribution::Pert,
        }),
        correlations: vec![Correlation {
            activities: vec![ActivityId(1), ActivityId(3)],
            coefficient: 0.6,
        }],
        ..Inputs::default()
    };
    let one = simulate(&s, &[], &inputs, settings(301, 1)).unwrap();
    let eight = simulate(&s, &[], &inputs, settings(301, 8)).unwrap();
    assert_eq!(one, eight);
    let other_seed = simulate(
        &s,
        &[],
        &inputs,
        Settings {
            seed: 2,
            ..settings(301, 8)
        },
    )
    .unwrap();
    assert_ne!(one.finishes, other_seed.finishes);
}

#[test]
fn two_independent_paths_finish_at_the_later_of_two_draws() {
    // A1 and A2 each take a uniform 0 to 1,000 minutes, side by side. The
    // finish is the later: P(max <= x) = (x / 1000)^2, so the median is
    // 1000 × sqrt(0.5) = 707 and P80 is 1000 × sqrt(0.8) = 894.
    let s = parallel(&[500, 500]);
    let mut inputs = Inputs::default();
    inputs.estimates.insert(ActivityId(1), uniform(0, 1_000));
    inputs.estimates.insert(ActivityId(2), uniform(0, 1_000));
    let sim = simulate(&s, &[], &inputs, settings(20_000, 4)).unwrap();
    let p50 = minutes(sim.percentile(50));
    let p80 = minutes(sim.percentile(80));
    assert!((p50 - 707).abs() <= 10, "{p50}");
    assert!((p80 - 894).abs() <= 10, "{p80}");
    // The deterministic finish, 500, is reached a quarter of the time.
    let by_500 = sim.probability_by(at("2026-01-05 08:20"));
    assert!((by_500 - 0.25).abs() < 0.015, "{by_500}");
}

#[test]
fn full_correlation_moves_a_group_together() {
    // In step, the two draws are the same, so the finish is one uniform:
    // median 500.
    let s = parallel(&[500, 500]);
    let mut inputs = Inputs::default();
    inputs.estimates.insert(ActivityId(1), uniform(0, 1_000));
    inputs.estimates.insert(ActivityId(2), uniform(0, 1_000));
    inputs.correlations.push(Correlation {
        activities: vec![ActivityId(1), ActivityId(2)],
        coefficient: 1.0,
    });
    let sim = simulate(&s, &[], &inputs, settings(20_000, 4)).unwrap();
    let p50 = minutes(sim.percentile(50));
    assert!((p50 - 500).abs() <= 10, "{p50}");
}

#[test]
fn criticality_and_sensitivity_follow_the_longer_path() {
    // A1 takes a uniform 0 to 1,000 minutes; A2 a fixed 600. A1 is critical
    // when it runs longer than 600, 40% of the time; A2 the rest.
    let s = parallel(&[500, 600]);
    let mut inputs = Inputs::default();
    inputs.estimates.insert(ActivityId(1), uniform(0, 1_000));
    let sim = simulate(&s, &[], &inputs, settings(20_000, 4)).unwrap();
    let a1 = sim.criticality[&ActivityId(1)];
    let a2 = sim.criticality[&ActivityId(2)];
    assert!((a1 - 0.4).abs() < 0.015, "{a1}");
    assert!((a2 - 0.6).abs() < 0.015, "{a2}");
    assert_eq!(sim.sampled, 1);
    assert_eq!(sim.sensitivity.len(), 1);
    assert_eq!(sim.sensitivity[0].0, ActivityId(1));
    assert!(sim.sensitivity[0].1 > 0.5);
}

#[test]
fn a_finish_milestone_gets_its_own_distribution() {
    let mut s = parallel(&[500, 0]);
    let m = s.activities.get_mut(&ActivityId(2)).unwrap();
    m.activity_type = ActivityType::FinishMilestone;
    s.relationships.insert(
        RelationshipId(1),
        Relationship {
            id: RelationshipId(1),
            predecessor: ActivityId(1),
            successor: ActivityId(2),
            predecessor_project: ProjectId(1),
            successor_project: ProjectId(1),
            kind: RelationshipType::FinishToStart,
            lag: Minutes::new(0),
            source: None,
        },
    );
    let mut inputs = Inputs::default();
    inputs.estimates.insert(ActivityId(1), uniform(0, 1_000));
    let sim = simulate(&s, &[], &inputs, settings(2_000, 2)).unwrap();
    assert_eq!(sim.milestones[&ActivityId(2)], sim.finishes);
}

#[test]
fn every_problem_with_the_inputs_is_listed() {
    let mut s = parallel(&[500, 0]);
    s.activities.get_mut(&ActivityId(2)).unwrap().activity_type = ActivityType::StartMilestone;
    let mut inputs = Inputs::default();
    inputs.estimates.insert(ActivityId(1), uniform(900, 100));
    inputs.estimates.insert(ActivityId(2), uniform(0, 100));
    inputs.estimates.insert(ActivityId(9), uniform(0, 100));
    inputs.correlations.push(Correlation {
        activities: vec![ActivityId(1)],
        coefficient: 1.5,
    });
    let Err(RiskError::Invalid(problems)) = simulate(&s, &[], &inputs, settings(10, 1)) else {
        panic!("inputs accepted");
    };
    let reasons: Vec<_> = problems.iter().map(|p| (p.activity, p.reason)).collect();
    assert_eq!(
        reasons,
        [
            (
                Some(ActivityId(1)),
                "minimum, most likely and maximum out of order"
            ),
            (
                Some(ActivityId(2)),
                "finished, a milestone or a span: nothing to draw"
            ),
            (Some(ActivityId(9)), "not an activity in this project"),
            (None, "a correlation coefficient outside 0 to 1"),
        ]
    );
}

#[test]
fn percentiles_take_the_nearest_rank_exactly() {
    // 100 distinct finishes, one a minute apart: P55 is the 55th, which
    // floating point would round up to the 56th.
    let start = at("2026-01-05 00:00");
    let values: Vec<DateTime> = (0..100)
        .map(|i| start.checked_add(Minutes::new(i)).unwrap())
        .collect();
    for p in [1, 5, 50, 55, 80, 95, 100] {
        assert_eq!(
            minutes(percentile(&values, p).unwrap()),
            i64::from(p) - 1,
            "P{p}"
        );
    }
    assert_eq!(percentile(&values, 0), Some(start));
    assert_eq!(percentile(&[], 50), None);
}

#[test]
fn ranks_share_ties() {
    assert_eq!(ranks(&[30, 10, 30, 20]), [3.5, 1.0, 3.5, 2.0]);
    assert_eq!(correlation(&[1.0, 2.0, 3.0], &[2.0, 4.0, 6.0]), Some(1.0));
    assert_eq!(correlation(&[1.0, 1.0], &[2.0, 4.0]), None);
}

/// Collapsed estimates for every activity, so each iteration is the
/// deterministic schedule.
fn collapsed(s: &Schedule) -> Inputs {
    let mut inputs = Inputs::default();
    for a in s.activities.values().filter(|a| is_sampled(a)) {
        let d = a.remaining_duration;
        inputs.estimates.insert(
            a.id,
            Estimate {
                min: d,
                likely: d,
                max: d,
                distribution: Distribution::Triangle,
            },
        );
    }
    inputs
}

#[test]
fn the_finish_is_the_project_s_own_not_where_alap_work_moves() {
    // Float from the opened projects' finish, another project finishing days
    // later, and an as-late-as-possible activity with nothing after it: the
    // engine moves A2 out to the other project's finish, but the project's
    // own finish is A1's, 10:00.
    let mut s = parallel(&[600, 60]);
    s.project.options.float_from_project_finish = false;
    s.activities
        .get_mut(&ActivityId(2))
        .unwrap()
        .primary_constraint = Some(totalfloat_model::Constraint {
        kind: totalfloat_model::ConstraintType::AsLateAsPossible,
        date: None,
    });
    let mut other = Schedule::new(Project::new(ProjectId(2), "OTHER"));
    let mut late = Activity::new(
        ActivityId(9),
        ProjectId(2),
        WbsId(1),
        CalendarId(1),
        "Z1",
        "Authored",
    );
    late.stored.early_finish = Some(at("2026-01-10 00:00"));
    other.activities.insert(late.id, late);
    let projects = [s.clone(), other];
    let sim = simulate(&s, &projects, &collapsed(&s), settings(20, 2)).unwrap();
    assert_eq!(sim.deterministic_finish, at("2026-01-05 10:00"));
    assert!(sim.finishes.iter().all(|f| *f == at("2026-01-05 10:00")));
}

#[test]
fn an_expected_finish_sets_the_duration_so_it_isn_t_drawn() {
    let mut s = parallel(&[600, 300]);
    s.project.options.use_expected_finish = true;
    s.activities
        .get_mut(&ActivityId(1))
        .unwrap()
        .expected_finish = Some(at("2026-01-05 12:00"));
    let range = Inputs {
        default: Some(Range {
            min_percent: -10.0,
            max_percent: 25.0,
            distribution: Distribution::Triangle,
        }),
        ..Inputs::default()
    };
    let sim = simulate(&s, &[], &range, settings(10, 1)).unwrap();
    assert_eq!(sim.sampled, 1);
    assert_eq!(sim.set_by_expected_finish, [ActivityId(1)]);
    // An estimate for A1 isn't drawn either, and says so.
    let mut given = Inputs::default();
    given.estimates.insert(ActivityId(1), uniform(0, 1_000));
    let sim = simulate(&s, &[], &given, settings(10, 1)).unwrap();
    assert_eq!(sim.sampled, 0);
    assert_eq!(sim.set_by_expected_finish, [ActivityId(1)]);
    // Without the option, the estimate counts.
    s.project.options.use_expected_finish = false;
    assert_eq!(
        simulate(&s, &[], &given, settings(10, 1)).unwrap().sampled,
        1
    );
    // An expected finish the engine ignores, here before the activity can
    // start, leaves the duration to count, so it's drawn.
    s.project.options.use_expected_finish = true;
    s.activities
        .get_mut(&ActivityId(1))
        .unwrap()
        .expected_finish = Some(at("2026-01-01 12:00"));
    let sim = simulate(&s, &[], &given, settings(10, 1)).unwrap();
    assert_eq!(sim.sampled, 1);
    assert_eq!(
        sim.sensitivity.first().map(|(id, _)| *id),
        Some(ActivityId(1))
    );
}

#[test]
fn a_range_out_of_bounds_and_no_iterations_are_refused() {
    let s = parallel(&[600]);
    for max_percent in [f64::INFINITY, f64::NAN, 1e9, -1.0] {
        let inputs = Inputs {
            default: Some(Range {
                min_percent: -10.0,
                max_percent,
                distribution: Distribution::Triangle,
            }),
            ..Inputs::default()
        };
        assert!(
            matches!(
                simulate(&s, &[], &inputs, settings(10, 1)),
                Err(RiskError::Invalid(_))
            ),
            "{max_percent}"
        );
    }
    let inputs = collapsed(&s);
    assert_eq!(
        simulate(&s, &[], &inputs, settings(0, 1)),
        Err(RiskError::NoIterations)
    );
    assert_eq!(
        simulate(&s, &[], &inputs, settings(10, 0)),
        Err(RiskError::NoIterations)
    );
}

fn fixed(minutes: i64) -> Estimate {
    uniform(minutes, minutes)
}

fn risk(name: &str, probability: f64, impacts: &[(u64, Estimate)]) -> RiskEvent {
    RiskEvent {
        name: name.to_owned(),
        probability,
        impacts: impacts
            .iter()
            .map(|(id, e)| (ActivityId(*id), *e))
            .collect(),
    }
}

#[test]
fn a_risk_that_never_happens_changes_nothing() {
    let s = parallel(&[500, 500]);
    let mut inputs = Inputs::default();
    inputs.estimates.insert(ActivityId(1), uniform(0, 1_000));
    let plain = simulate(&s, &[], &inputs, settings(500, 2)).unwrap();
    inputs.risks.push(risk("R0", 0.0, &[(1, fixed(240))]));
    let with = simulate(&s, &[], &inputs, settings(500, 2)).unwrap();
    assert_eq!(with.finishes, plain.finishes);
    assert_eq!(
        with.risks,
        [RiskEffect {
            name: "R0".to_owned(),
            probability: 0.0,
            occurred: 0.0,
            p50: Minutes::ZERO,
            p80: Minutes::ZERO,
        }]
    );
}

#[test]
fn a_certain_risk_moves_every_finish_by_its_delay() {
    // One path: A1 takes 500 minutes, and the risk always adds 240.
    let s = parallel(&[500]);
    let mut inputs = collapsed(&s);
    inputs.risks.push(risk("R1", 1.0, &[(1, fixed(240))]));
    let sim = simulate(&s, &[], &inputs, settings(100, 2)).unwrap();
    assert!(sim.finishes.iter().all(|f| minutes(*f) == 740));
    assert_eq!(sim.risks[0].occurred, 1.0);
    assert_eq!(sim.risks[0].p80, Minutes::new(240));
    // A risk on an activity with no estimate adds to its remaining duration,
    // and doesn't build up from one iteration to the next.
    let mut inputs = Inputs::default();
    inputs.risks.push(risk("R1", 1.0, &[(1, fixed(240))]));
    let sim = simulate(&s, &[], &inputs, settings(50, 1)).unwrap();
    assert!(sim.finishes.iter().all(|f| minutes(*f) == 740));
}

#[test]
fn a_coin_flip_risk_happens_about_half_the_time() {
    let s = parallel(&[500]);
    let mut inputs = Inputs::default();
    inputs.risks.push(risk("R1", 0.5, &[(1, uniform(60, 120))]));
    let sim = simulate(&s, &[], &inputs, settings(10_000, 4)).unwrap();
    let share = sim.risks[0].occurred;
    assert!((share - 0.5).abs() < 0.02, "{share}");
    let late = sim.finishes.iter().filter(|f| minutes(**f) > 500).count();
    assert!((f64::from(u32::try_from(late).unwrap()) / 10_000.0 - share).abs() < 1e-9);
}

#[test]
fn risks_are_ranked_by_their_effect_on_p80() {
    // Two parallel paths of 500: R-SMALL may add 60 to A1, R-BIG 600 to A2.
    let s = parallel(&[500, 500]);
    let mut inputs = Inputs::default();
    inputs.risks.push(risk("R-SMALL", 0.5, &[(1, fixed(60))]));
    inputs.risks.push(risk("R-BIG", 0.5, &[(2, fixed(600))]));
    let sim = simulate(&s, &[], &inputs, settings(2_000, 4)).unwrap();
    let names: Vec<_> = sim.risks.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, ["R-BIG", "R-SMALL"]);
    // With both, P80 is 1,100 (R-BIG happens in more than a fifth of
    // iterations); with R-BIG off, A1 decides it: 560.
    assert_eq!(sim.risks[0].p80, Minutes::new(540));
}

#[test]
fn a_risk_on_work_it_can_t_delay_is_refused() {
    let mut s = parallel(&[500, 0]);
    s.activities.get_mut(&ActivityId(2)).unwrap().activity_type = ActivityType::FinishMilestone;
    let mut inputs = Inputs::default();
    inputs.risks.push(risk(
        "R1",
        1.5,
        &[(2, fixed(60)), (9, fixed(60)), (1, uniform(90, 30))],
    ));
    let Err(RiskError::Invalid(problems)) = simulate(&s, &[], &inputs, settings(10, 1)) else {
        panic!("accepted");
    };
    assert_eq!(problems.len(), 4, "{problems:?}");
}

#[test]
fn problems_with_estimates_and_risks_come_back_together() {
    let s = parallel(&[500]);
    let mut inputs = Inputs::default();
    inputs.estimates.insert(ActivityId(1), uniform(900, 100));
    inputs.risks.push(risk("R1", 1.5, &[(1, fixed(60))]));
    let Err(RiskError::Invalid(problems)) = simulate(&s, &[], &inputs, settings(10, 1)) else {
        panic!("accepted");
    };
    assert_eq!(problems.len(), 2, "{problems:?}");
}

#[test]
fn rerunning_only_where_a_risk_happened_gives_the_full_rerun_s_answer() {
    // A risk's effect from rerunning only the iterations it happened in must
    // be what switching it off everywhere gives.
    let s = parallel(&[500, 400]);
    let mut inputs = Inputs::default();
    inputs.estimates.insert(ActivityId(1), uniform(300, 700));
    inputs.estimates.insert(ActivityId(2), uniform(200, 900));
    inputs.risks.push(risk("R1", 0.3, &[(2, uniform(60, 240))]));
    let with = simulate(&s, &[], &inputs, settings(3_000, 3)).unwrap();
    inputs.risks[0].probability = 0.0;
    let without = simulate(&s, &[], &inputs, settings(3_000, 3)).unwrap();
    for p in [50, 80] {
        let expected = with.percentile(p).minutes_since(without.percentile(p));
        let got = if p == 50 {
            with.risks[0].p50
        } else {
            with.risks[0].p80
        };
        assert_eq!(got, expected, "P{p}");
    }
}
