use super::*;
use totalfloat_model::{CalendarId, CalendarType, HoursPerPeriod};
fn date(value: &str) -> DateTime {
    value.parse().unwrap()
}
fn interval(start: u16, finish: u16) -> WorkInterval {
    WorkInterval::new(
        TimeOfDay::from_minutes(start).unwrap(),
        TimeOfDay::from_minutes(finish).unwrap(),
    )
    .unwrap()
}
fn model() -> Calendar {
    Calendar {
        id: CalendarId(1),
        name: "Synthetic workweek".to_owned(),
        kind: CalendarType::Global,
        project: None,
        base: None,
        is_default: false,
        hours_per_period: HoursPerPeriod::default(),
        work_week: WorkWeek(std::array::from_fn(|day| {
            if (1..=5).contains(&day) {
                vec![interval(480, 720), interval(780, 1020)]
            } else {
                vec![]
            }
        })),
        exceptions: BTreeMap::new(),
        source: None,
    }
}
#[test]
fn split_shifts_and_distinct_start_finish_boundaries() {
    let c = WorkCalendar::new(&model()).unwrap();
    let morning = date("2026-01-05 08:00");
    let lunch = date("2026-01-05 12:00");
    let afternoon = date("2026-01-05 13:00");
    let end = date("2026-01-05 17:00");
    assert_eq!(c.add_work(morning, Minutes(240)).unwrap(), lunch);
    assert_eq!(c.add_work(end, Minutes(-240)).unwrap(), afternoon);
    assert_eq!(c.next_work_instant(lunch).unwrap(), afternoon);
    assert_eq!(c.prev_work_instant(afternoon).unwrap(), lunch);
    assert!(!c.is_work(lunch));
    assert!(c.is_work(afternoon));
    assert_eq!(c.work_between(morning, end), Minutes(480));
    assert_eq!(c.add_work(end, Minutes(0)).unwrap(), end);
    assert_eq!(
        c.add_work(date("2026-01-09 17:00"), Minutes(60)).unwrap(),
        date("2026-01-12 09:00")
    );
}
#[test]
fn exceptions_replace_days_and_cache_patterns_without_stale_mutations() {
    let mut input = model();
    input
        .exceptions
        .insert("2026-01-05".parse().unwrap(), vec![]);
    input
        .exceptions
        .insert("2026-01-10".parse().unwrap(), vec![interval(600, 720)]);
    let c = WorkCalendar::new(&input).unwrap();
    assert_eq!(
        c.next_work_instant(date("2026-01-05 08:00")).unwrap(),
        date("2026-01-06 08:00")
    );
    assert_eq!(
        c.work_between(date("2026-01-10 00:00"), date("2026-01-11 00:00")),
        Minutes(120)
    );
    assert!(Arc::ptr_eq(&c.week[1], &c.week[2]));
    assert!(Arc::ptr_eq(&c.week[0], &c.exceptions[0].pattern));
    input.work_week = WorkWeek::default();
    assert!(c.is_work(date("2026-01-06 09:00")));
}
#[test]
fn invalid_days_empty_weeks_range_edges_and_huge_spans_are_errors() {
    let mut input = model();
    input.work_week = WorkWeek::default();
    assert!(matches!(
        WorkCalendar::new(&input),
        Err(CalendarError::EmptyWeek)
    ));
    let mut input = model();
    input.work_week.0[1] = vec![interval(480, 720), interval(600, 780)];
    assert!(matches!(
        WorkCalendar::new(&input),
        Err(CalendarError::InvalidIntervals)
    ));
    let c = WorkCalendar::continuous().unwrap();
    let first = date("0001-01-01 00:00");
    let last = date("9999-12-31 23:59");
    assert_eq!(c.add_work(first, last.minutes_since(first)).unwrap(), last);
    for amount in [Minutes(-1), Minutes(i64::MIN), Minutes(i64::MAX)] {
        assert!(c.add_work(first, amount).is_err());
    }
    assert!(c.add_work(last, Minutes(1)).is_err());
    assert!(c.prev_work_instant(first).is_err());
    assert_eq!(c.next_work_instant(first).unwrap(), first);
}
proptest::proptest! {
    #[test]
    fn signed_addition_and_work_coordinates_round_trip(
        offset in 0_i64..50_000, amount in -5_000_i64..5_000,
    ) {
        let c = WorkCalendar::new(&model()).unwrap();
        let start = date("2026-01-05 08:00").checked_add(Minutes(offset)).unwrap();
        let end = c.add_work(start, Minutes(amount)).unwrap();
        proptest::prop_assert_eq!(c.work_between(start, end), Minutes(amount));
        let back = c.add_work(end, Minutes(-amount)).unwrap();
        proptest::prop_assert_eq!(c.work_between(start, back), Minutes(0));
        if amount > 0 {
            let normalized = c.next_work_instant(start).unwrap();
            let e = c.add_work(normalized, Minutes(amount)).unwrap();
            proptest::prop_assert_eq!(c.add_work(e, Minutes(-amount)).unwrap(), normalized);
        }
        if amount < 0 {
            let normalized = c.prev_work_instant(start).unwrap();
            let e = c.add_work(normalized, Minutes(amount)).unwrap();
            proptest::prop_assert_eq!(c.add_work(e, Minutes(-amount)).unwrap(), normalized);
        }
    }
    #[test]
    fn elapsed_calendar_and_signed_interval_counts_agree(
        a in 0_i64..500_000, b in 0_i64..500_000,
    ) {
        let c = WorkCalendar::continuous().unwrap();
        let base = date("2026-01-01 00:00");
        let start = base.checked_add(Minutes(a)).unwrap();
        let end = base.checked_add(Minutes(b)).unwrap();
        proptest::prop_assert_eq!(c.work_between(start, end), Minutes(b - a));
        proptest::prop_assert_eq!(c.add_work(start, Minutes(b - a)).unwrap(), end);
    }
}

proptest::proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(96))]
    #[test]
    fn cached_week_and_exception_coordinates_match_minute_oracle(
        offset in -50_000_000_i64..50_000_000, span in -2_000_i64..2_000,
        holiday in 0_i32..35, shortened in 0_i32..35,
        lengths in proptest::array::uniform7(0_u16..300),
    ) {
        let mut input = model();
        // Vary every weekday, including weekends; always retain some work.
        for (day, length) in lengths.into_iter().enumerate() {
            input.work_week.0[day] = if length == 0 {
                vec![]
            } else {
                vec![interval(480, 480 + length)]
            };
        }
        input.work_week.0[1].push(interval(900, 960));
        let base = date("2026-01-05 00:00").checked_add(Minutes(offset)).unwrap();
        input.exceptions.insert(base.date().checked_add_days(holiday).unwrap(), vec![]);
        input.exceptions.insert(
            base.date().checked_add_days(shortened).unwrap(), vec![interval(600, 720)],
        );
        let c = WorkCalendar::new(&input).unwrap();
        let a = base;
        let b = a.checked_add(Minutes(span)).unwrap();
        let (start, end, sign) = if a <= b { (a, b, 1) } else { (b, a, -1) };
        let mut current = start;
        let mut count = 0;
        while current < end {
            let intervals = input.exceptions.get(&current.date()).map_or_else(
                || input.work_week.day(current.weekday()), Vec::as_slice,
            );
            if intervals.iter().any(|i| i.start <= current.time() && current.time() < i.finish) {
                count += 1;
            }
            current = current.checked_add(Minutes(1)).unwrap();
        }
        proptest::prop_assert_eq!(c.work_between(a, b), Minutes(sign * count));
        let amount = c.work_between(a, b);
        let selected = c.add_work(a, amount).unwrap();
        proptest::prop_assert_eq!(c.work_between(selected, b), Minutes(0));
    }
}

proptest::proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(256))]
    /// Finding the day and walking its shifts gives exactly what a search over
    /// every minute of the calendar gives.
    #[test]
    fn selecting_a_coordinate_matches_searching_every_minute(
        anchor in 0_i64..4_000_000, offset in -20_000_i64..20_000,
        days in proptest::collection::vec(0_i32..120, 0..12),
        lengths in proptest::array::uniform7(0_u16..300),
        second in proptest::bool::ANY, midnight in proptest::bool::ANY,
    ) {
        let mut input = model();
        for (day, length) in lengths.into_iter().enumerate() {
            let shift = interval(480, 480 + length.max(1));
            input.work_week.0[day] = match (length, second, midnight) {
                (0, ..) => vec![],
                (_, true, _) => vec![shift, interval(800, 900)],
                (_, _, true) => vec![interval(1200, 1440)],
                _ => vec![shift],
            };
        }
        input.work_week.0[1] = vec![interval(480, 720)];
        let c = WorkCalendar::new(&input).unwrap();
        let base = c.select(anchor * 60, false).unwrap().date();
        for (index, day) in days.iter().enumerate() {
            let pattern = match index % 3 {
                0 => vec![],
                1 => vec![interval(600, 720)],
                _ => vec![interval(0, 1440)],
            };
            input.exceptions.insert(base.checked_add_days(*day).unwrap(), pattern);
        }
        let c = WorkCalendar::new(&input).unwrap();
        let coordinate = (anchor * 60 + offset).max(0);
        for latest in [false, true] {
            proptest::prop_assert_eq!(
                c.select(coordinate, latest),
                c.select_by_search(coordinate, latest),
                "latest {}", latest
            );
        }
    }
}

#[test]
fn selecting_at_the_ends_of_the_calendar_matches_searching_every_minute() {
    let mut input = model();
    input
        .exceptions
        .insert("0001-01-02".parse().unwrap(), vec![]);
    input
        .exceptions
        .insert("9999-12-30".parse().unwrap(), vec![]);
    for c in [
        WorkCalendar::new(&input).unwrap(),
        WorkCalendar::continuous().unwrap(),
    ] {
        let end = c.coordinate(c.last);
        for coordinate in (0..2_000).chain(end - 2_000..=end + 2) {
            for latest in [false, true] {
                assert_eq!(
                    c.select(coordinate, latest),
                    c.select_by_search(coordinate, latest),
                    "{coordinate} {latest}"
                );
            }
        }
        assert_eq!(c.select(-1, true), Err(CalendarError::OutOfRange));
    }
}

#[test]
fn midnight_spans_sorted_patterns_and_display_factors_are_explicit() {
    let mut input = model();
    input.work_week = WorkWeek::default();
    input.work_week.0[5] = vec![interval(1320, 1440)];
    input.work_week.0[6] = vec![interval(0, 120)];
    input.hours_per_period.day = Some(Minutes(3));
    let c = WorkCalendar::new(&input).unwrap();
    let start = date("2026-01-09 22:00");
    let finish = date("2026-01-10 02:00");
    assert_eq!(c.add_work(start, Minutes(240)).unwrap(), finish);
    assert_eq!(c.add_work(finish, Minutes(-240)).unwrap(), start);
    assert!(c.is_work(date("2026-01-10 00:00")));
    let mut input = model();
    input.work_week.0[1] = vec![interval(720, 1020), interval(480, 720)];
    let c = WorkCalendar::new(&input).unwrap();
    assert_eq!(c.week[1].intervals, vec![interval(480, 1020)]);
    input.exceptions.insert(
        "2026-01-06".parse().unwrap(),
        vec![WorkInterval {
            start: TimeOfDay::END_OF_DAY,
            finish: TimeOfDay::MIDNIGHT,
        }],
    );
    assert!(matches!(
        WorkCalendar::new(&input),
        Err(CalendarError::InvalidIntervals)
    ));
    assert_eq!(
        c.next_work_instant(date("9999-12-31 23:59")),
        Err(CalendarError::OutOfRange)
    );
}

proptest::proptest! {
    #[test]
    fn a_spot_names_the_same_instants_as_the_older_methods(
        days in 0_i64..4000,
        minute in 0_i64..1440,
        amount in -20_000_i64..20_000,
    ) {
        let calendar = WorkCalendar::new(&model()).unwrap();
        let instant = DateTime::from_ymd_hm(2024, 1, 1, 0, 0)
            .unwrap()
            .checked_add(Minutes(days * 1440 + minute))
            .unwrap();
        let spot = calendar.spot(instant);
        proptest::prop_assert_eq!(spot.start(), calendar.next_work_instant(instant));
        proptest::prop_assert_eq!(spot.finish(), calendar.prev_work_instant(instant));
        if amount != 0 {
            let moved = spot.plus(Minutes(amount)).unwrap();
            let expected = calendar.add_work(instant, Minutes(amount));
            proptest::prop_assert_eq!(if amount > 0 { moved.finish() } else { moved.start() }, expected);
        }
    }
}

#[test]
fn a_spot_matches_the_older_methods_at_the_ends_of_the_calendar() {
    let calendar = WorkCalendar::new(&model()).unwrap();
    for text in ["0001-01-01 00:00", "0001-01-01 07:00", "9999-12-31 23:59"] {
        let instant = date(text);
        assert_eq!(
            calendar.spot(instant).finish(),
            calendar.prev_work_instant(instant),
            "{text}"
        );
        assert_eq!(
            calendar.spot(instant).start(),
            calendar.next_work_instant(instant),
            "{text}"
        );
    }
}

proptest::proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(128))]
    /// Long runs of exceptions put the weekly estimate many days off, so
    /// `find_day` walks the whole way or falls back to bisecting. Either way
    /// it gives what a search over every minute gives.
    #[test]
    fn selecting_past_long_exception_runs_matches_searching_every_minute(
        runs in proptest::collection::vec((0_i32..400, 1_i32..60, proptest::bool::ANY), 1..6),
        offset in 0_i64..400_000,
    ) {
        let mut input = model();
        let base: totalfloat_model::Date = "2026-01-05".parse().unwrap();
        for (start, length, work) in runs {
            for day in start..start + length {
                let pattern = if work { vec![interval(0, 1440)] } else { vec![] };
                input.exceptions.insert(base.checked_add_days(day).unwrap(), pattern);
            }
        }
        let c = WorkCalendar::new(&input).unwrap();
        let coordinate = c.coordinate(base.at(totalfloat_model::TimeOfDay::MIDNIGHT).unwrap()) + offset;
        for latest in [false, true] {
            proptest::prop_assert_eq!(
                c.select(coordinate, latest),
                c.select_by_search(coordinate, latest),
                "latest {}", latest
            );
        }
    }
}
