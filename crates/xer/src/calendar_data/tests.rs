use super::*;
use crate::{
    raw,
    typed::{RowRef, TypedXer},
};
use proptest::prelude::*;
use totalfloat_model::{CalendarId, CalendarType, Minutes};

fn node(name: &str, payload: &str, children: &str) -> String {
    format!("(0||{name}({payload})({children}))")
}

fn shift(start: &str, finish: &str) -> String {
    node("0", &format!("s|{start}|f|{finish}"), "")
}

fn week() -> String {
    let days: String = (1..=7)
        .map(|day| {
            node(
                &day.to_string(),
                "",
                &if (2..=6).contains(&day) {
                    format!("{}{}", shift("08:00", "12:00"), shift("13:00", "17:00"))
                } else {
                    String::new()
                },
            )
        })
        .collect();
    node("DaysOfWeek", "", &days)
}

fn calendar(week: &str, exceptions: &str) -> String {
    node(
        "CalendarData",
        "",
        &format!("{week}{}", node("Exceptions", "", exceptions)),
    )
}

fn raw_calendars(rows: Vec<(u64, Option<u64>, &str, String)>) -> raw::RawXer {
    let mut raw = raw::RawXer::new(
        &["synthetic"],
        raw::Encoding::Utf8 { bom: false },
        raw::LineEnding::Lf,
    )
    .unwrap();
    let table = raw
        .push_table(
            "CALENDAR",
            [
                "clndr_id",
                "clndr_type",
                "clndr_name",
                "base_clndr_id",
                "clndr_data",
                "week_hr_cnt",
                "default_flag",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect(),
        )
        .unwrap();
    for (id, base, kind, data) in rows {
        raw.push_row(
            table,
            &[
                id.to_string(),
                kind.to_owned(),
                "synthetic".into(),
                base.map_or_else(String::new, |id| id.to_string()),
                data,
                "40".into(),
                "N".into(),
            ],
        )
        .unwrap();
    }
    raw
}

#[test]
fn weekdays_split_shifts_and_exact_layout_retention() {
    let source = calendar(&week(), "");
    let variants = [
        source.clone(),
        format!("\"\n{}\n\"", source.replace(")(", ")\n(")),
        source.replace(")(", ")\x7f\x7f ("),
        source.replacen("CalendarData", "", 1),
    ];
    for source in variants {
        let data = CalendarData::decode(&source);
        assert!(data.is_valid(), "{:?}", data.diagnostics());
        assert_eq!(data.encode(), source);
        assert_eq!(data.weekday(Weekday::Sunday), Some([].as_slice()));
        assert_eq!(data.weekday(Weekday::Monday).unwrap().len(), 2);
        let total: i64 = data
            .week
            .iter()
            .flatten()
            .flatten()
            .map(|interval| interval.duration().get())
            .sum();
        assert_eq!(Minutes::new(total), Minutes::from_hours(40).unwrap());
    }
}

#[test]
fn serial_day_epoch_and_working_exceptions() {
    assert_eq!(exception_date(0), Date::from_ymd(1899, 12, 30));
    assert_eq!(exception_date(25569), Date::from_ymd(1970, 1, 1));
    assert_eq!(exception_date(i32::MAX), None);
    let holiday = Date::from_ymd(2026, 12, 25).unwrap();
    let serial = holiday.days_since(Date::from_ymd(1899, 12, 30).unwrap());
    let payload = format!(
        "{}{}",
        node("0", &format!("d|{serial}"), ""),
        node("1", &format!("d|{}", serial + 1), &shift("08:00", "24:00"))
    );
    let source = calendar(&week(), &payload);
    let data = CalendarData::decode(&source);
    assert!(data.is_valid());
    assert_eq!(data.exceptions()[&holiday], []);
    assert_eq!(
        data.exceptions()[&holiday.checked_add_days(1).unwrap()][0]
            .finish
            .minutes(),
        1440
    );
}

#[test]
fn base_inheritance_keeps_local_week_and_overrides_exceptions() {
    let date = Date::from_ymd(2026, 12, 25).unwrap();
    let serial = date.days_since(Date::from_ymd(1899, 12, 30).unwrap());
    let base = calendar(&week(), &node("0", &format!("d|{serial}"), ""));
    let local_week = week().replace(
        &node(
            "2",
            "",
            &format!("{}{}", shift("08:00", "12:00"), shift("13:00", "17:00")),
        ),
        &node("2", "", ""),
    );
    let child = calendar(
        &local_week,
        &node("0", &format!("d|{serial}"), &shift("09:00", "11:00")),
    );
    for child_first in [true, false] {
        let mut rows = vec![
            (1, None, "CA_Base", base.clone()),
            (2, Some(1), "CA_Rsrc", child.clone()),
        ];
        if child_first {
            rows.reverse();
        }
        let raw = raw_calendars(rows);
        let typed = TypedXer::decode(&raw);
        let set = decode_calendars(&typed);
        let child = set.calendar(CalendarId(2)).unwrap();
        assert_eq!(child.kind, CalendarType::Resource);
        assert_eq!(child.base, Some(CalendarId(1)));
        assert_eq!(child.work_week.day(Weekday::Monday), []);
        assert_eq!(child.work_week.day(Weekday::Tuesday).len(), 2);
        assert_eq!(child.exceptions[&date][0].duration().get(), 120);
        assert_eq!(set.calendar(CalendarId(1)).unwrap().exceptions[&date], []);
        let source = RowRef::from_source_ref(child.source.unwrap()).unwrap();
        assert_eq!(
            raw.tables()[source.table].rows[source.row].get(0),
            Some("2")
        );
    }
}

#[test]
fn empty_payload_does_not_inherit_standard_week_or_fabricate_work() {
    let raw = raw_calendars(vec![
        (2, Some(1), "CA_Rsrc", String::new()),
        (1, None, "CA_Base", calendar(&week(), "")),
        (3, None, "CA_Base", String::new()),
    ]);
    let typed = TypedXer::decode(&raw);
    let set = decode_calendars(&typed);
    assert!(set.calendar(CalendarId(2)).is_none());
    assert!(set.calendar(CalendarId(3)).is_none());
    assert!(
        set.diagnostics
            .iter()
            .any(|d| d.kind == DiagnosticKind::UnresolvedWeek)
    );
    assert_eq!(set.entries[0].data.encode(), "");
}

#[test]
fn missing_bases_cycles_duplicates_and_depth_limit_are_diagnostics() {
    let empty = calendar("", "");
    let raw = raw_calendars(vec![
        (1, Some(2), "CA_Rsrc", empty.clone()),
        (2, Some(1), "CA_Rsrc", empty.clone()),
        (3, Some(99), "CA_Rsrc", empty),
        (1, None, "CA_Base", calendar(&week(), "")),
    ]);
    let typed = TypedXer::decode(&raw);
    let set = decode_calendars(&typed);
    assert!(set.entries.iter().all(|entry| entry.calendar.is_none()));
    for kind in [
        DiagnosticKind::InheritanceCycle,
        DiagnosticKind::MissingBase,
        DiagnosticKind::DuplicateId,
    ] {
        assert!(set.diagnostics.iter().any(|d| d.kind == kind));
    }
    for reverse in [false, true] {
        let mut rows = vec![(1, None, "CA_Base", calendar(&week(), ""))];
        rows.extend((2..=66).map(|id| (id, Some(id - 1), "CA_Rsrc", calendar(&week(), ""))));
        if reverse {
            rows.reverse();
        }
        let raw = raw_calendars(rows);
        let typed = TypedXer::decode(&raw);
        let set = decode_calendars(&typed);
        assert!(set.calendar(CalendarId(64)).is_some());
        assert!(set.calendar(CalendarId(65)).is_none());
        assert!(set.calendar(CalendarId(66)).is_none());
    }
}

#[test]
fn malformed_structure_and_intervals_remain_exact() {
    for source in [
        "(garbage",
        "(0||CalendarData()(",
        "\"unclosed",
        "(0||CalendarData()())trailing",
        "(1||CalendarData()())",
        "(0|unknown|CalendarData()())",
    ] {
        let data = CalendarData::decode(source);
        assert!(!data.is_valid());
        assert_eq!(data.encode(), source);
        assert_eq!(data.diagnostics()[0].kind, DiagnosticKind::Syntax);
    }
    for shifts in [
        shift("17:00", "08:00"),
        format!("{}{}", shift("08:00", "12:00"), shift("11:00", "13:00")),
        shift("25:00", "26:00"),
        node("0", "s|08:00", ""),
    ] {
        let source = calendar(&node("DaysOfWeek", "", &node("2", "", &shifts)), "");
        let data = CalendarData::decode(&source);
        assert!(!data.is_valid());
        assert!(
            data.diagnostics()
                .iter()
                .any(|d| d.kind == DiagnosticKind::InvalidInterval)
        );
        assert_eq!(data.encode(), source);
    }
    let source = calendar(
        &node("DaysOfWeek", "", &node("8", "", "")),
        &node("0", "d|0.5", ""),
    );
    let data = CalendarData::decode(&source);
    assert!(
        data.diagnostics()
            .iter()
            .any(|d| d.kind == DiagnosticKind::InvalidWeekday)
    );
    assert!(
        data.diagnostics()
            .iter()
            .any(|d| d.kind == DiagnosticKind::InvalidExceptionDate)
    );
}

#[test]
fn conversion_factors_are_exact_and_display_week_mismatches_are_reported() {
    for (source, expected) in [
        ("8", Some(480)),
        ("0.05", Some(3)),
        ("0.01", None),
        ("0.1", Some(6)),
        ("0", Some(0)),
        ("1.00", Some(60)),
        ("0.000000000000000000000000000000000000000", Some(0)),
        ("1e38", None),
    ] {
        assert_eq!(
            source
                .parse::<Decimal>()
                .unwrap()
                .checked_minutes()
                .map(Minutes::get),
            expected
        );
    }
    assert_eq!(
        "45000.000".parse::<Decimal>().unwrap().checked_integer(),
        Some(45000)
    );
    assert_eq!(
        "45000.5".parse::<Decimal>().unwrap().checked_integer(),
        None
    );
    let raw = raw_calendars(vec![(1, None, "CA_Base", calendar(&week(), ""))]);
    let typed = TypedXer::decode(&raw);
    let set = decode_calendars(&typed);
    let calendar = set.calendar(CalendarId(1)).unwrap();
    let weekly: i64 = calendar
        .work_week
        .0
        .iter()
        .flatten()
        .map(|i| i.duration().get())
        .sum();
    assert_eq!(Some(Minutes::new(weekly)), calendar.hours_per_period.week);
    assert!(
        !set.diagnostics
            .iter()
            .any(|d| d.kind == DiagnosticKind::WeeklyHoursMismatch)
    );
}

proptest! {
    #[test]
    fn arbitrary_calendar_strings_never_panic_and_remain_exact(source in ".{0,300}") {
        let data = CalendarData::decode(&source);
        prop_assert_eq!(data.encode(), source.as_str());
    }

    #[test]
    fn serial_dates_round_trip_with_civil_day_arithmetic(serial in -100_000i32..1_000_000) {
        if let Some(date) = exception_date(serial) {
            prop_assert_eq!(date.days_since(Date::from_ymd(1899, 12, 30).unwrap()), serial);
        }
    }
}

#[test]
fn midnight_finish_and_identical_exception_repetition() {
    let zero_shift = shift("00:00", "00:00");
    let last_shift = shift("16:00", "00:00");
    for (shift, minutes) in [(zero_shift, 1440), (last_shift, 480)] {
        let source = calendar(&node("DaysOfWeek", "", &node("2", "", &shift)), "");
        let data = CalendarData::decode(&source);
        assert!(data.is_valid());
        assert_eq!(
            data.weekday(Weekday::Monday).unwrap()[0].duration().get(),
            minutes
        );
    }
    let source = calendar(&week(), &node("0", "d|45000", "").repeat(2));
    let data = CalendarData::decode(&source);
    assert!(data.is_valid());
    assert_eq!(data.exceptions().len(), 1);
    assert_eq!(data.encode(), source);
    let conflicting = calendar(
        &week(),
        &format!(
            "{}{}",
            node("0", "d|45000", ""),
            node("1", "d|45000", &shift("08:00", "12:00"))
        ),
    );
    assert!(!CalendarData::decode(&conflicting).is_valid());
}

#[test]
fn unrecognized_sections_and_excessive_nesting_are_retained_with_diagnostics() {
    let unknown = node("CalendarData", "", &node("FutureRule", "", ""));
    let data = CalendarData::decode(&unknown);
    assert!(!data.is_valid());
    assert_eq!(
        data.diagnostics()[0].kind,
        DiagnosticKind::UnsupportedStructure
    );
    let mut nested = node("0", "", "");
    for _ in 0..syntax::MAX_DEPTH + 2 {
        nested = node("0", "", &nested);
    }
    let source = node("CalendarData", "", &nested);
    let data = CalendarData::decode(&source);
    assert!(!data.is_valid());
    assert_eq!(data.diagnostics()[0].kind, DiagnosticKind::Syntax);
    assert_eq!(data.encode(), source);
    for raw in [0, u64::MAX] {
        let source = totalfloat_model::SourceRef(raw);
        assert_eq!(
            RowRef::from_source_ref(source).unwrap().to_source_ref(),
            Some(source)
        );
    }
}

#[test]
fn inherited_exceptions_do_not_require_a_usable_base_standard_week() {
    let base = calendar("", &node("0", "d|45000", ""));
    let raw = raw_calendars(vec![
        (2, Some(1), "CA_Rsrc", calendar(&week(), "")),
        (1, None, "CA_Base", base),
    ]);
    let typed = TypedXer::decode(&raw);
    let set = decode_calendars(&typed);
    assert!(set.calendar(CalendarId(1)).is_none());
    let child = set.calendar(CalendarId(2)).unwrap();
    assert_eq!(child.exceptions[&exception_date(45000).unwrap()], []);
    assert_eq!(child.work_week.day(Weekday::Monday).len(), 2);
}
