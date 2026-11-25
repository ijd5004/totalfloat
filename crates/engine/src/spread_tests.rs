use super::*;
use proptest::prelude::*;
use std::collections::BTreeMap;
use totalfloat_model::{
    Activity, ActivityId, AssignmentId, Calendar, CalendarType, HoursPerPeriod, Minutes, Project,
    ProjectId, Resource, ResourceCurveId, ResourceId, ResourceType, TypedSpread, WbsId,
    WorkInterval, WorkWeek,
};

fn at(text: &str) -> DateTime {
    text.parse().unwrap()
}

fn day(text: &str) -> Date {
    text.parse().unwrap()
}

/// Monday to Friday, 08:00 to 16:00, with Wednesday 2026-03-04 a holiday.
fn calendar_model() -> Calendar {
    let shift = WorkInterval::new(
        TimeOfDay::from_hm(8, 0).unwrap(),
        TimeOfDay::from_hm(16, 0).unwrap(),
    )
    .unwrap();
    Calendar {
        id: CalendarId(1),
        name: "Week with a holiday".into(),
        kind: CalendarType::Global,
        project: None,
        base: None,
        is_default: false,
        hours_per_period: HoursPerPeriod::default(),
        work_week: WorkWeek(std::array::from_fn(|i| {
            if (1..=5).contains(&i) {
                vec![shift]
            } else {
                vec![]
            }
        })),
        exceptions: BTreeMap::from([(day("2026-03-04"), vec![])]),
        source: None,
    }
}

fn calendar() -> WorkCalendar {
    WorkCalendar::new(&calendar_model()).unwrap()
}

fn curve(points: [f64; CURVE_POINTS]) -> ResourceCurve {
    ResourceCurve {
        id: ResourceCurveId(1),
        name: "Authored".into(),
        default: true,
        points,
        source: None,
    }
}

/// Point 0, then points 1 to 20 given in fives.
fn shape(fives: [[f64; 5]; 4]) -> ResourceCurve {
    let mut points = [0.0; CURVE_POINTS];
    for (n, value) in fives.iter().flatten().enumerate() {
        points[n + 1] = *value;
    }
    curve(points)
}

// Seven curves as corpus files hold them, under the names those files give
// them (TF-039).
const LINEAR: [[f64; 5]; 4] = [[5.0; 5]; 4];
const BELL: [[f64; 5]; 4] = [
    [0.5, 0.5, 1.5, 1.5, 4.0],
    [4.0, 7.5, 7.5, 11.5, 11.5],
    [11.5, 11.5, 7.5, 7.5, 4.0],
    [4.0, 1.5, 1.5, 0.5, 0.5],
];
const FRONT_LOADED: [[f64; 5]; 4] = [[6.5; 5], [6.5; 5], [3.5; 5], [3.5; 5]];
const BACK_LOADED: [[f64; 5]; 4] = [[3.5; 5], [3.5; 5], [6.5; 5], [6.5; 5]];
const TRAPEZOIDAL: [[f64; 5]; 4] = [
    [1.0, 1.0, 3.5, 3.5, 5.5],
    [5.5, 7.5, 7.5, 7.5, 7.5],
    [7.5, 7.5, 7.5, 7.5, 5.5],
    [5.5, 3.5, 3.5, 1.0, 1.0],
];
const TRIANGULAR_INCREASE: [[f64; 5]; 4] = [
    [1.0, 1.0, 1.5, 1.5, 3.0],
    [3.0, 3.5, 3.5, 4.5, 4.5],
    [5.5, 5.5, 6.5, 6.5, 7.0],
    [7.0, 8.5, 8.5, 9.0, 9.0],
];
const TRIANGULAR_DECREASE: [[f64; 5]; 4] = [
    [9.0, 9.0, 8.5, 8.5, 7.0],
    [7.0, 6.5, 6.5, 5.5, 5.5],
    [4.5, 4.5, 3.5, 3.5, 3.0],
    [3.0, 1.5, 1.5, 1.0, 1.0],
];

fn units(periods: &[PeriodUnits]) -> Vec<f64> {
    periods.iter().map(|p| p.units).collect()
}

fn starts(periods: &[PeriodUnits]) -> Vec<Date> {
    periods.iter().map(|p| p.start).collect()
}

fn assert_close(actual: &[f64], expected: &[f64]) {
    assert_eq!(actual.len(), expected.len(), "{actual:?} vs {expected:?}");
    for (a, e) in actual.iter().zip(expected) {
        assert!((a - e).abs() < 1e-9, "{actual:?} vs {expected:?}");
    }
}

/// 100 units by `fives` over the holiday week, by day. The four work days
/// are five twentieths each, so each day takes five points.
fn by_day(fives: [[f64; 5]; 4]) -> Vec<f64> {
    let periods = spread(
        100.0,
        at("2026-03-02 08:00"),
        at("2026-03-06 16:00"),
        Distribution::Curve(&shape(fives)),
        &calendar(),
        Period::Day,
    )
    .unwrap();
    assert_eq!(
        starts(&periods),
        [
            "2026-03-02",
            "2026-03-03",
            "2026-03-04",
            "2026-03-05",
            "2026-03-06"
        ]
        .map(day)
    );
    units(&periods)
}

#[test]
fn each_built_in_curve_over_a_week_with_a_holiday() {
    assert_close(&by_day(LINEAR), &[25.0, 25.0, 0.0, 25.0, 25.0]);
    assert_close(&by_day(BELL), &[8.0, 42.0, 0.0, 42.0, 8.0]);
    assert_close(&by_day(FRONT_LOADED), &[32.5, 32.5, 0.0, 17.5, 17.5]);
    assert_close(&by_day(BACK_LOADED), &[17.5, 17.5, 0.0, 32.5, 32.5]);
    assert_close(&by_day(TRAPEZOIDAL), &[14.5, 35.5, 0.0, 35.5, 14.5]);
    assert_close(&by_day(TRIANGULAR_INCREASE), &[8.0, 19.0, 0.0, 31.0, 42.0]);
    assert_close(&by_day(TRIANGULAR_DECREASE), &[42.0, 31.0, 0.0, 19.0, 8.0]);
}

#[test]
fn no_curve_spreads_evenly_over_work_time() {
    let periods = spread(
        100.0,
        at("2026-03-02 08:00"),
        at("2026-03-06 16:00"),
        Distribution::Even,
        &calendar(),
        Period::Day,
    )
    .unwrap();
    assert_close(&units(&periods), &[25.0, 25.0, 0.0, 25.0, 25.0]);
}

/// From Monday noon there are 28 work hours, so a twentieth is 1.4 hours and
/// day boundaries fall inside twentieths.
#[test]
fn a_twentieth_split_by_a_day_divides_by_work_time() {
    let periods = spread(
        100.0,
        at("2026-03-02 12:00"),
        at("2026-03-06 16:00"),
        Distribution::Curve(&shape(FRONT_LOADED)),
        &calendar(),
        Period::Day,
    )
    .unwrap();
    // Monday: 4/1.4 twentieths at 6.5; Tuesday: to 12/1.4 at 6.5; Thursday:
    // the rest of the 6.5s and 20/1.4 - 10 at 3.5; Friday: 8/1.4 at 3.5.
    assert_close(
        &units(&periods),
        &[130.0 / 7.0, 260.0 / 7.0, 0.0, 170.0 / 7.0, 20.0],
    );
    let even = spread(
        100.0,
        at("2026-03-02 12:00"),
        at("2026-03-06 16:00"),
        Distribution::Even,
        &calendar(),
        Period::Day,
    )
    .unwrap();
    assert_close(
        &units(&even),
        &[100.0 / 7.0, 200.0 / 7.0, 0.0, 200.0 / 7.0, 200.0 / 7.0],
    );
}

#[test]
fn weeks_begin_on_the_day_asked_for() {
    let spread_from = |starts_on| {
        spread(
            100.0,
            at("2026-03-05 08:00"),
            at("2026-03-11 16:00"),
            Distribution::Even,
            &calendar(),
            Period::Week { starts_on },
        )
        .unwrap()
    };
    let monday = spread_from(Weekday::Monday);
    assert_eq!(starts(&monday), [day("2026-03-02"), day("2026-03-09")]);
    assert_close(&units(&monday), &[40.0, 60.0]);
    // Thursday to Wednesday is one week that begins on Thursday.
    let thursday = spread_from(Weekday::Thursday);
    assert_eq!(starts(&thursday), [day("2026-03-05")]);
    assert_close(&units(&thursday), &[100.0]);
}

#[test]
fn months_split_at_the_first() {
    let periods = spread(
        100.0,
        at("2026-03-30 08:00"),
        at("2026-04-02 16:00"),
        Distribution::Curve(&shape(BACK_LOADED)),
        &calendar(),
        Period::Month,
    )
    .unwrap();
    assert_eq!(starts(&periods), [day("2026-03-01"), day("2026-04-01")]);
    assert_close(&units(&periods), &[35.0, 65.0]);
    let year_end = spread(
        8.0,
        at("2026-12-31 08:00"),
        at("2027-01-01 16:00"),
        Distribution::Even,
        &calendar(),
        Period::Month,
    )
    .unwrap();
    assert_eq!(starts(&year_end), [day("2026-12-01"), day("2027-01-01")]);
    assert_close(&units(&year_end), &[4.0, 4.0]);
}

#[test]
fn a_finish_at_midnight_ends_the_day_before() {
    let periods = spread(
        16.0,
        at("2026-03-02 08:00"),
        at("2026-03-04 00:00"),
        Distribution::Even,
        &calendar(),
        Period::Day,
    )
    .unwrap();
    assert_eq!(starts(&periods), [day("2026-03-02"), day("2026-03-03")]);
    assert_close(&units(&periods), &[8.0, 8.0]);
}

#[test]
fn point_zero_falls_where_work_begins() {
    let mut points = [0.0; CURVE_POINTS];
    points[0] = 50.0;
    points[20] = 50.0;
    // Saturday and Sunday hold no work, so point 0 lands on Monday.
    let periods = spread(
        100.0,
        at("2026-02-28 08:00"),
        at("2026-03-03 16:00"),
        Distribution::Curve(&curve(points)),
        &calendar(),
        Period::Day,
    )
    .unwrap();
    assert_close(&units(&periods), &[0.0, 0.0, 50.0, 50.0]);
}

#[test]
fn no_work_puts_everything_in_the_first_period() {
    let weekend = spread(
        10.0,
        at("2026-02-28 08:00"),
        at("2026-03-01 16:00"),
        Distribution::Curve(&shape(BELL)),
        &calendar(),
        Period::Day,
    )
    .unwrap();
    assert_close(&units(&weekend), &[10.0, 0.0]);
    let instant = spread(
        10.0,
        at("2026-03-02 08:00"),
        at("2026-03-02 08:00"),
        Distribution::Even,
        &calendar(),
        Period::Day,
    )
    .unwrap();
    assert_eq!(starts(&instant), [day("2026-03-02")]);
    assert_close(&units(&instant), &[10.0]);
}

/// Stretches of `units` over `hours` of work.
fn typed(stretches: &[(f64, i64)]) -> Vec<SpreadSegment> {
    stretches
        .iter()
        .map(|&(units, hours)| SpreadSegment {
            units,
            work: Minutes::new(hours * 60),
        })
        .collect()
}

fn typed_by_day(units: f64, stretches: &[(f64, i64)]) -> Vec<f64> {
    let periods = spread(
        units,
        at("2026-03-02 08:00"),
        at("2026-03-06 16:00"),
        Distribution::Typed(&typed(stretches)),
        &calendar(),
        Period::Day,
    )
    .unwrap();
    assert_eq!(periods.len(), 5);
    self::units(&periods)
}

/// Hours typed for each day of the holiday week come back day by day;
/// a stretch over two work days splits by work time, and the holiday gets
/// nothing.
#[test]
fn a_typed_spread_gives_the_typed_units_per_period() {
    let stretches = [(10.0, 8), (2.0, 8), (5.0, 16)];
    assert_close(&typed_by_day(17.0, &stretches), &[10.0, 2.0, 0.0, 2.5, 2.5]);
    // By week from Monday, the whole week is one period.
    let week = spread(
        17.0,
        at("2026-03-02 08:00"),
        at("2026-03-06 16:00"),
        Distribution::Typed(&typed(&stretches)),
        &calendar(),
        Period::Week {
            starts_on: Weekday::Monday,
        },
    )
    .unwrap();
    assert_close(&units(&week), &[17.0]);
}

/// A stored spread whose work or units differ from the assignment's
/// gives the shape: stretched to the work and scaled to the units.
#[test]
fn a_typed_spread_that_differs_is_scaled_to_the_work_and_units() {
    // Half the units in the first half of the work, typed over 16 hours and
    // as percents.
    assert_close(
        &typed_by_day(10.0, &[(50.0, 8), (50.0, 8)]),
        &[2.5, 2.5, 0.0, 2.5, 2.5],
    );
    // A stretch ending mid-day: 3 units in the first 12 of 32 hours, then 1
    // in the other 20, so Tuesday takes 4 hours of each.
    assert_close(
        &typed_by_day(4.0, &[(3.0, 12), (1.0, 20)]),
        &[2.0, 1.2, 0.0, 0.4, 0.4],
    );
    // Nothing typed, or no units in it, spreads evenly.
    assert_close(&typed_by_day(8.0, &[]), &[2.0, 2.0, 0.0, 2.0, 2.0]);
    assert_close(&typed_by_day(8.0, &[(0.0, 8)]), &[2.0, 2.0, 0.0, 2.0, 2.0]);
}

/// The remaining spread of an assignment is its typed remaining
/// spread, else its curve, else even.
#[test]
fn an_assignments_remaining_distribution() {
    let mut s = Schedule::new(Project::new(ProjectId(1), "AUTHORED"));
    let held = curve([5.0; CURVE_POINTS]);
    s.resource_curves.insert(held.id, held.clone());
    let mut a = Assignment {
        id: AssignmentId(1),
        activity: ActivityId(1),
        resource: None,
        role: None,
        cost_account: None,
        project: ProjectId(1),
        curve: None,
        typed_spread: None,
        amounts: totalfloat_model::Amounts::default(),
        udfs: BTreeMap::new(),
        source: None,
    };
    assert_eq!(remaining_distribution(&s, &a), Distribution::Even);
    a.curve = Some(held.id);
    assert_eq!(remaining_distribution(&s, &a), Distribution::Curve(&held));
    a.curve = Some(ResourceCurveId(9));
    assert_eq!(remaining_distribution(&s, &a), Distribution::Even);
    a.curve = None;
    let spread = TypedSpread {
        planned: typed(&[(4.0, 8)]),
        remaining: typed(&[(3.0, 8)]),
        actual: Vec::new(),
    };
    a.typed_spread = Some(spread.clone());
    assert_eq!(
        remaining_distribution(&s, &a),
        Distribution::Typed(&spread.remaining)
    );
    a.typed_spread = Some(TypedSpread {
        remaining: Vec::new(),
        ..spread
    });
    assert_eq!(remaining_distribution(&s, &a), Distribution::Typed(&[]));
}

#[test]
fn bad_inputs_are_refused() {
    let cal = calendar();
    let (start, finish) = (at("2026-03-02 08:00"), at("2026-03-06 16:00"));
    assert_eq!(
        spread(1.0, finish, start, Distribution::Even, &cal, Period::Day),
        Err(SpreadError::Backwards)
    );
    assert_eq!(
        spread(
            f64::NAN,
            start,
            finish,
            Distribution::Even,
            &cal,
            Period::Day
        ),
        Err(SpreadError::Units)
    );
    let mut negative = [5.0; CURVE_POINTS];
    negative[3] = -1.0;
    assert_eq!(
        spread(
            1.0,
            start,
            finish,
            Distribution::Curve(&curve(negative)),
            &cal,
            Period::Day
        ),
        Err(SpreadError::Curve)
    );
    assert_eq!(
        spread(
            1.0,
            start,
            finish,
            Distribution::Curve(&curve([0.0; CURVE_POINTS])),
            &cal,
            Period::Day
        ),
        Err(SpreadError::Curve)
    );
    for bad in [[(-1.0, 8)], [(f64::NAN, 8)], [(1.0, 0)]] {
        assert_eq!(
            spread(
                1.0,
                start,
                finish,
                Distribution::Typed(&typed(&bad)),
                &cal,
                Period::Day
            ),
            Err(SpreadError::Typed)
        );
    }
}

#[test]
fn an_assignment_works_on_its_resource_calendar_only_when_resource_dependent() {
    let mut project = Project::new(ProjectId(1), "AUTHORED");
    project.default_calendar = Some(CalendarId(1));
    let mut s = Schedule::new(project);
    let activity = Activity::new(
        ActivityId(1),
        ProjectId(1),
        WbsId(1),
        CalendarId(1),
        "A1",
        "Authored",
    );
    s.activities.insert(activity.id, activity);
    let resource = |id, calendar| Resource {
        id: ResourceId(id),
        short_name: format!("R{id}"),
        name: "Authored".into(),
        kind: ResourceType::Labor,
        calendar,
        parent: None,
        default_units_per_hour: None,
        rates: Vec::new(),
        roles: Vec::new(),
        default_cost_follows_units: false,
        udfs: BTreeMap::new(),
        source: None,
    };
    s.resources
        .insert(ResourceId(1), resource(1, Some(CalendarId(2))));
    s.resources.insert(ResourceId(2), resource(2, None));
    let assignment = |resource| Assignment {
        id: AssignmentId(1),
        activity: ActivityId(1),
        resource: Some(ResourceId(resource)),
        role: None,
        cost_account: None,
        project: ProjectId(1),
        curve: None,
        typed_spread: None,
        amounts: totalfloat_model::Amounts::default(),
        udfs: BTreeMap::new(),
        source: None,
    };
    assert_eq!(assignment_calendar(&s, &assignment(1)), Some(CalendarId(1)));
    s.activities.get_mut(&ActivityId(1)).unwrap().activity_type = ActivityType::ResourceDependent;
    assert_eq!(assignment_calendar(&s, &assignment(1)), Some(CalendarId(2)));
    assert_eq!(assignment_calendar(&s, &assignment(2)), Some(CalendarId(1)));
    let mut elsewhere = assignment(1);
    elsewhere.activity = ActivityId(9);
    assert_eq!(assignment_calendar(&s, &elsewhere), None);
    assert_eq!(assignment_calendar(&s, &assignment(3)), None);
}

fn any_period() -> impl Strategy<Value = Period> {
    prop_oneof![
        Just(Period::Day),
        (0_usize..7).prop_map(|d| Period::Week {
            starts_on: Weekday::ALL[d]
        }),
        Just(Period::Month),
    ]
}

/// A start within six weeks of 2026-02-23 and a span of up to 90 days, on
/// any minute.
fn any_span() -> impl Strategy<Value = (DateTime, DateTime)> {
    (0_i64..42 * 1440, 0_i64..90 * 1440).prop_map(|(offset, length)| {
        let start = at("2026-02-23 00:00")
            .checked_add(totalfloat_model::Minutes::new(offset))
            .unwrap();
        let finish = start
            .checked_add(totalfloat_model::Minutes::new(length))
            .unwrap();
        (start, finish)
    })
}

proptest! {
    /// Spread units always sum to the units given, by any curve.
    #[test]
    fn spread_units_sum_to_the_units(
        units in -1.0e6_f64..1.0e6,
        (start, finish) in any_span(),
        halves in proptest::collection::vec(0_u32..40, CURVE_POINTS),
        curved in any::<bool>(),
        period in any_period(),
    ) {
        let mut points = [0.0; CURVE_POINTS];
        for (point, half) in points.iter_mut().zip(&halves) {
            *point = f64::from(*half) / 2.0;
        }
        points[10] += 1.0;
        let curve = curve(points);
        let periods = spread(
            units, start, finish, curved.then_some(&curve).map_or(Distribution::Even, Distribution::Curve), &calendar(), period,
        ).unwrap();
        let sum: f64 = periods.iter().map(|p| p.units).sum();
        prop_assert!((sum - units).abs() <= 1e-9 * units.abs().max(1.0), "{sum} vs {units}");
        // Periods are contiguous, from the start's to the finish's.
        prop_assert!(periods[0].start <= start.date());
        prop_assert!(periods.windows(2).all(|w| w[0].start < w[1].start));
    }

    /// Typed units always sum to the units given, whatever the
    /// stretches.
    #[test]
    fn typed_units_sum_to_the_units(
        units in -1.0e6_f64..1.0e6,
        (start, finish) in any_span(),
        stretches in proptest::collection::vec((0_u32..1000, 1_i64..5000), 0..12),
        period in any_period(),
    ) {
        let segments: Vec<_> = stretches
            .iter()
            .map(|&(u, m)| SpreadSegment { units: f64::from(u) / 8.0, work: Minutes::new(m) })
            .collect();
        let periods = spread(
            units, start, finish, Distribution::Typed(&segments), &calendar(), period,
        ).unwrap();
        let sum: f64 = periods.iter().map(|p| p.units).sum();
        prop_assert!((sum - units).abs() <= 1e-9 * units.abs().max(1.0), "{sum} vs {units}");
        prop_assert!(periods.iter().all(|p| p.units * units.signum() >= -1e-9 * units.abs().max(1.0)));
    }

    /// A flat curve, nothing at the start and the same in every twentieth,
    /// spreads exactly as no curve does.
    #[test]
    fn a_flat_curve_equals_the_even_spread(
        units in 0.0_f64..1.0e6,
        (start, finish) in any_span(),
        level in 1_u32..200,
        period in any_period(),
    ) {
        let mut points = [f64::from(level); CURVE_POINTS];
        points[0] = 0.0;
        let flat = curve(points);
        let cal = calendar();
        let curved = spread(units, start, finish, Distribution::Curve(&flat), &cal, period).unwrap();
        let even = spread(units, start, finish, Distribution::Even, &cal, period).unwrap();
        prop_assert_eq!(starts(&curved), starts(&even));
        for (c, e) in curved.iter().zip(&even) {
            prop_assert!((c.units - e.units).abs() <= 1e-9 * units.max(1.0), "{curved:?} vs {even:?}");
        }
    }
}
