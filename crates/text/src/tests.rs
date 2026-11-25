use proptest::prelude::*;

use crate::{
    Activity, ActivityDetail, ActivityKind, Calendar, CalendarDetail, CodeType, CodeValue, Comment,
    ConstraintKind, Date, DateTime, Day, Detail, Diagnostic, Document, Item, LinkType, Predecessor,
    Project, ProjectDetail, Role, RoleDetail, Span, Statement, TypeKind, Value, Wbs, format, parse,
};

/// The example schedule, in the canonical layout.
const EXAMPLE: &str = r#"totalfloat-schedule 1

project PLANT "New Plant"
  data-date 2026-03-02 08:00
  must-finish 2026-06-30 17:00
  calendar "Standard 5-Day"

calendar "Standard 5-Day"
  week mon-fri 08:00-12:00 13:00-17:00
  holiday 2026-05-25

code-type Phase
  value DES "Design"
  value CON "Construction"

wbs PLANT.CIV "Civil works"

activity A1010 "Excavate" 5d
  code Phase=CON
activity A1020 "Pour foundations" 10d after A1010
activity A1030 "Cure" 7d after A1020 ss+2d
activity A1040 "Backfill" 3d after A1030, A1020 ff
milestone M100 "Civil complete" after A1040
  finish-on-or-before 2026-06-30 17:00

wbs PLANT.MEP "Mechanical and electrical"

activity B2010 "Rough-in" 15d after A1020 ss+5d
  calendar "Standard 5-Day"
  resource ELEC units 2
  start-on-or-after 2026-04-06 08:00
"#;

fn problems(source: &str) -> Vec<Diagnostic> {
    parse(source, "plan.tfs").expect_err("should be refused")
}

fn codes(source: &str) -> Vec<&'static str> {
    problems(source).iter().map(|d| d.code).collect()
}

fn rendered(source: &str) -> String {
    problems(source)
        .iter()
        .map(|d| d.render(source))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn the_adr_example_reads_and_formats_to_itself() {
    let doc = parse(EXAMPLE, "plan.tfs").unwrap();
    assert_eq!(format(&doc), EXAMPLE);
    let activities: Vec<&Statement<Activity, ActivityDetail>> = doc
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Activity(a) => Some(a),
            _ => None,
        })
        .collect();
    assert_eq!(activities.len(), 6);
    let backfill = &activities[3].head;
    assert_eq!(backfill.after.len(), 2);
    assert_eq!(backfill.after[1].code.value, "A1020");
    assert_eq!(backfill.after[1].link, Some(LinkType::FF));
    let cure = &activities[2].head.after[0];
    assert_eq!(cure.link, Some(LinkType::SS));
    assert_eq!(cure.lag, Some("2d".parse().unwrap()));
    assert_eq!(activities[4].head.kind, ActivityKind::Milestone);
}

#[test]
fn positions_point_at_the_words() {
    let doc = parse(EXAMPLE, "plan.tfs").unwrap();
    let Some(Item::Activity(cure)) = doc
        .items
        .iter()
        .find(|i| matches!(i, Item::Activity(a) if a.head.code.value == "A1030"))
    else {
        panic!("no A1030");
    };
    assert_eq!(cure.at.line, 21);
    assert_eq!(cure.at.column, 1);
    let predecessor = &cure.head.after[0];
    assert_eq!(
        (predecessor.code.at.column, predecessor.code.at.length),
        (32, 5)
    );
    assert_eq!(predecessor.link_at.map(|a| a.column), Some(38));
}

#[test]
fn a_messy_file_formats_canonically() {
    let messy = "totalfloat-schedule   1\n\n\n\nwbs   P.A  \"A\"   # area\nactivity A1 \"One\" 5d\n\n\n  # about one\n  remaining 3D\nactivity A2 \"Two\" after A1 FS, A1 SS+2d\nactivity A3 \"Three\" after A2 +1d, A1 fs-1d\n";
    let doc = parse(messy, "plan.tfs").unwrap_or_else(|p| panic!("{p:?}"));
    let once = format(&doc);
    let twice = format(&parse(&once, "plan.tfs").unwrap());
    assert_eq!(once, twice);
    insta::assert_snapshot!(once);
}

#[test]
fn every_mistake_is_reported_in_one_pass() {
    let source = r#"totalfloat-schedule 1

wbs PLANT.CIV "Civil works"

activty A1010 "Excavate" 5d
activity A1020 "Pour" 10x after A1010
activity A1030 "Cure" 7d after A1020 ss+2d
  early-start 2026-03-02 08:00
"#;
    let found = problems(source);
    assert_eq!(
        found
            .iter()
            .map(|d| (d.code, d.at.line, d.at.column))
            .collect::<Vec<_>>(),
        [
            ("unknown-keyword", 5, 1),
            ("bad-span", 6, 23),
            ("calculated-value", 8, 3),
        ]
    );
    insta::assert_snapshot!(rendered(source));
}

#[test]
fn a_typo_gets_a_suggestion() {
    let found = problems("totalfloat-schedule 1\nactivity A \"a\"\n  calender Night\n");
    assert_eq!(found[0].code, "unknown-detail");
    assert_eq!(found[0].hint, "did you mean `calendar`?");
}

#[test]
fn reading_stops_at_a_missing_or_wrong_version() {
    assert_eq!(codes("activity A \"a\"\n"), ["missing-version"]);
    assert_eq!(codes(""), ["missing-version"]);
    assert_eq!(
        codes("totalfloat-schedule 2\nactivity A \"a\"\n"),
        ["wrong-version"]
    );
}

#[test]
fn mistakes_have_their_own_codes() {
    let cases = [
        ("activity A \"unclosed\n", "unclosed-quote"),
        ("  remaining 3d\n", "detail-without-statement"),
        ("activity A Excavate\n", "expected-name"),
        ("activity A \"a\" after B sx\n", "bad-link"),
        ("activity A \"a\" after B ss+2q\n", "bad-span"),
        ("milestone M \"m\" 5d\n", "milestone-duration"),
        (
            "activity A \"a\"\n  start-on-or-after 2026-04-06\n",
            "bad-date",
        ),
        (
            "activity A \"a\"\n  start-on-or-after 2026-02-30 08:00\n",
            "bad-date",
        ),
        ("activity A \"a\"\n  percent 120\n", "bad-number"),
        ("activity A \"a\"\n  type level-of-effort\n", "bad-type"),
        ("activity A \"a\"\n  code =CON\n", "bad-code"),
        ("calendar Night\n  week fri-mon 20:00-24:00\n", "bad-day"),
        ("calendar Night\n  week mon 12:00-08:00\n", "bad-shift"),
        ("calendar Night\n  week mon\n", "missing-value"),
        ("calendar Night\n  holiday 2026-13-01\n", "bad-date"),
        ("wbs P.A \"a\"\n  remaining 3d\n", "unexpected-detail"),
        ("project P \"p\" extra\n", "unexpected-word"),
        ("activity A \"a\", B\n", "unexpected-word"),
        ("total-float A 5d\n", "calculated-value"),
    ];
    for (body, code) in cases {
        let source = format!("totalfloat-schedule 1\n{body}");
        assert_eq!(codes(&source), [code], "{body}");
    }
}

#[test]
fn details_of_a_broken_statement_are_skipped() {
    let source = "totalfloat-schedule 1\nactivity A \"a\" 5x\n  remaining 3d\n  bogus\nactivity B \"b\" 9q\n";
    assert_eq!(codes(source), ["bad-span", "bad-span"]);
}

#[test]
fn names_keep_quotes_and_backslashes() {
    let source = "totalfloat-schedule 1\n\nactivity \"A 1\" \"Say \\\"hi\\\" \\\\ there\"\nactivity A2 \"b\" after \"A 1\", \"ss\"\n";
    let doc = parse(source, "plan.tfs").unwrap();
    let Item::Activity(first) = &doc.items[0] else {
        panic!()
    };
    assert_eq!(first.head.code.value, "A 1");
    assert_eq!(first.head.name.value, r#"Say "hi" \ there"#);
    assert_eq!(format(&doc), source);
}

#[test]
fn durations_cant_be_negative_but_leads_can() {
    assert_eq!(
        codes("totalfloat-schedule 1\nactivity A \"a\" -5d\n"),
        ["bad-span"]
    );
    assert_eq!(
        codes("totalfloat-schedule 1\nactivity A \"a\"\n  remaining -2d\n"),
        ["bad-span"]
    );
    assert!(
        parse(
            "totalfloat-schedule 1\nactivity A \"a\" 5d after B fs-2d\n",
            "p"
        )
        .is_ok()
    );
}

#[test]
fn the_version_line_can_carry_a_comment_and_a_byte_order_mark() {
    let source = "\u{feff}totalfloat-schedule 1   # site plan\n";
    let doc = parse(source, "plan.tfs").unwrap();
    assert_eq!(doc.comment.as_deref(), Some("site plan"));
    assert_eq!(format(&doc), "totalfloat-schedule 1 # site plan\n");
}

#[test]
fn a_broken_statement_skips_its_comments_and_details_quietly() {
    let source =
        "totalfloat-schedule 1\nactivty A \"x\" 5d\n  # note\n  calendar Std\nactivity B \"b\"\n";
    assert_eq!(codes(source), ["unknown-keyword"]);
}

#[test]
fn a_broken_detail_doesnt_hide_the_next_one() {
    let source = "totalfloat-schedule 1\nactivity A \"x\" 5d\n  code \"Phase CON\n  percent 150\n";
    assert_eq!(codes(source), ["unclosed-quote", "bad-number"]);
}

#[test]
fn line_breaks_and_tabs_in_names_are_escaped() {
    let mut doc = parse("totalfloat-schedule 1\n\nactivity A \"x\"\n", "p").unwrap();
    let Item::Activity(a) = &mut doc.items[0] else {
        panic!()
    };
    a.head.name.value = "Line one\nLine two\tand\r".to_owned();
    let text = format(&doc);
    assert_eq!(
        text,
        "totalfloat-schedule 1\n\nactivity A \"Line one\\nLine two\\tand\\r\"\n"
    );
    let back = parse(&text, "p").unwrap();
    let Item::Activity(b) = &back.items[0] else {
        panic!()
    };
    assert_eq!(b.head.name.value, "Line one\nLine two\tand\r");
}

#[test]
fn shifts_are_two_digit_times() {
    assert_eq!(
        codes("totalfloat-schedule 1\ncalendar Night\n  week mon +8:00-12:00\n"),
        ["bad-shift"]
    );
}

// Generated documents survive formatting and reading back.

fn ident() -> impl Strategy<Value = String> {
    prop_oneof![
        "[A-Z][A-Z0-9.]{0,6}",
        Just("ss".to_owned()),
        Just("+1d".to_owned()),
        Just("A 1".to_owned()),
        Just("x=y".to_owned()),
        Just("after".to_owned()),
    ]
}

fn name() -> impl Strategy<Value = String> {
    "[ -~]{0,12}"
}

fn word(s: impl Strategy<Value = String>) -> impl Strategy<Value = Value<String>> {
    s.prop_map(Value::new)
}

fn span() -> impl Strategy<Value = Span> {
    (-50_000i32..50_000, any::<bool>()).prop_map(|(n, days)| {
        let unit = if days { 'd' } else { 'h' };
        format!("{}{unit}", f64::from(n) / 1000.0).parse().unwrap()
    })
}

fn duration() -> impl Strategy<Value = Span> {
    span().prop_filter("durations aren't negative", |s| {
        !s.to_string().starts_with('-')
    })
}

fn datetime() -> impl Strategy<Value = Value<DateTime>> {
    (2000i32..2100, 1u8..13, 1u8..29, 0u16..24, 0u16..60).prop_map(|(y, m, d, h, min)| {
        Value::new(
            format!("{y:04}-{m:02}-{d:02} {h:02}:{min:02}")
                .parse()
                .unwrap(),
        )
    })
}

fn date() -> impl Strategy<Value = Value<Date>> {
    (2000i32..2100, 1u8..13, 1u8..29)
        .prop_map(|(y, m, d)| Value::new(format!("{y:04}-{m:02}-{d:02}").parse().unwrap()))
}

fn comment() -> impl Strategy<Value = Option<String>> {
    proptest::option::of("[a-z #]{0,8}".prop_map(|s| s.trim().to_owned()))
}

fn detail<D: std::fmt::Debug>(value: impl Strategy<Value = D>) -> impl Strategy<Value = Detail<D>> {
    prop_oneof![
        4 => (value, comment()).prop_map(|(value, comment)| Detail::Line {
            value,
            at: crate::At::default(),
            comment,
        }),
        1 => "[a-z ]{0,8}".prop_map(|t| Detail::Comment(Comment { text: t.trim().to_owned(), at: crate::At::default() })),
    ]
}

fn statement<H: std::fmt::Debug, D: std::fmt::Debug>(
    head: impl Strategy<Value = H>,
    details: impl Strategy<Value = Detail<D>>,
) -> impl Strategy<Value = Statement<H, D>> {
    (head, comment(), proptest::collection::vec(details, 0..4)).prop_map(
        |(head, comment, details)| Statement {
            head,
            at: crate::At::default(),
            comment,
            details,
        },
    )
}

fn shifts() -> impl Strategy<Value = Vec<Value<String>>> {
    proptest::collection::vec(
        prop_oneof![
            Just("08:00-12:00"),
            Just("13:00-17:00"),
            Just("20:00-24:00")
        ]
        .prop_map(|s| Value::new(s.to_owned())),
        1..3,
    )
}

fn day() -> impl Strategy<Value = Day> {
    (0usize..7).prop_map(|i| crate::parse::DAYS[i].1)
}

fn project() -> impl Strategy<Value = Item> {
    let detail_value = prop_oneof![
        datetime().prop_map(ProjectDetail::DataDate),
        datetime().prop_map(ProjectDetail::MustFinish),
        word(ident()).prop_map(ProjectDetail::Calendar),
    ];
    statement(
        (word(ident()), word(name())).prop_map(|(code, name)| Project { code, name }),
        detail(detail_value),
    )
    .prop_map(Item::Project)
}

fn calendar() -> impl Strategy<Value = Item> {
    let detail_value = prop_oneof![
        (day(), day(), prop_oneof![Just(Vec::new()), shifts()]).prop_map(|(a, b, shifts)| {
            let (i, j) = (day_index(a), day_index(b));
            let (from, to) = if i <= j { (a, b) } else { (b, a) };
            CalendarDetail::Week { from, to, shifts }
        }),
        date().prop_map(CalendarDetail::Holiday),
        (date(), shifts()).prop_map(|(date, shifts)| CalendarDetail::Exception { date, shifts }),
        (1u32..=48).prop_map(|half| CalendarDetail::HoursPerDay(Value::new(f64::from(half) / 2.0))),
    ];
    statement(
        (word(ident()), proptest::option::of(word(ident())))
            .prop_map(|(name, copy_of)| Calendar { name, copy_of }),
        detail(detail_value),
    )
    .prop_map(Item::Calendar)
}

fn day_index(day: Day) -> usize {
    crate::parse::DAYS
        .iter()
        .position(|(_, d)| *d == day)
        .unwrap()
}

fn code_type() -> impl Strategy<Value = Item> {
    statement(
        word(ident()).prop_map(|name| CodeType { name }),
        detail(
            (word(ident()), proptest::option::of(word(name())))
                .prop_map(|(value, description)| CodeValue { value, description }),
        ),
    )
    .prop_map(Item::CodeType)
}

fn constraint() -> impl Strategy<Value = ConstraintKind> {
    prop_oneof![
        Just(ConstraintKind::StartOn),
        Just(ConstraintKind::StartOnOrAfter),
        Just(ConstraintKind::FinishOnOrBefore),
        Just(ConstraintKind::AsLateAsPossible),
        Just(ConstraintKind::MandatoryFinish),
    ]
}

fn activity_detail() -> impl Strategy<Value = ActivityDetail> {
    prop_oneof![
        word(ident()).prop_map(ActivityDetail::Calendar),
        prop_oneof![Just(TypeKind::Task), Just(TypeKind::Resource)].prop_map(ActivityDetail::Type),
        duration().prop_map(|s| ActivityDetail::Remaining(Value::new(s))),
        (any::<bool>(), constraint(), datetime()).prop_map(|(secondary, kind, date)| {
            ActivityDetail::Constraint {
                secondary,
                kind,
                date: (kind != ConstraintKind::AsLateAsPossible).then_some(date),
            }
        }),
        (word(ident()), word(ident()))
            .prop_map(|(code_type, value)| ActivityDetail::Code { code_type, value }),
        (word(ident()), proptest::option::of(0u32..10_000)).prop_map(|(resource, units)| {
            ActivityDetail::Resource {
                resource,
                units: units.map(|u| Value::new(f64::from(u) / 100.0)),
            }
        }),
        (word(ident()), proptest::option::of(0u32..10_000)).prop_map(|(role, units)| {
            ActivityDetail::Role {
                role,
                units: units.map(|u| Value::new(f64::from(u) / 100.0)),
            }
        }),
        (
            word(name()),
            -100_000_000i64..100_000_000,
            proptest::option::of(0u32..10_000_000),
            proptest::option::of(0u32..10_000_000),
            proptest::option::of(proptest::sample::select(vec![
                crate::ExpenseAccrual::Start,
                crate::ExpenseAccrual::End,
                crate::ExpenseAccrual::Uniform,
            ])),
        )
            .prop_map(|(item, budget, actual, remaining, accrual)| {
                #[allow(clippy::cast_precision_loss, reason = "test values are small")]
                let budget = Value::new(budget as f64 / 100.0);
                let cents = |c: u32| Value::new(f64::from(c) / 100.0);
                ActivityDetail::Expense(crate::Expense {
                    item,
                    budget,
                    actual: actual.map(cents),
                    remaining: remaining.map(cents),
                    accrual: accrual.map(Value::new),
                })
            }),
        datetime().prop_map(ActivityDetail::ActualStart),
        datetime().prop_map(ActivityDetail::ActualFinish),
        (0u32..=1000).prop_map(|p| ActivityDetail::Percent(Value::new(f64::from(p) / 10.0))),
        datetime().prop_map(ActivityDetail::ExpectedFinish),
    ]
}

fn predecessor() -> impl Strategy<Value = Predecessor> {
    let link = proptest::option::of(prop_oneof![
        Just(LinkType::FS),
        Just(LinkType::SS),
        Just(LinkType::FF),
        Just(LinkType::SF),
    ]);
    (word(ident()), link, proptest::option::of(span())).prop_map(|(code, link, lag)| Predecessor {
        code,
        link,
        lag,
        link_at: None,
    })
}

fn activity() -> impl Strategy<Value = Item> {
    let kind = prop_oneof![
        Just(ActivityKind::Activity),
        Just(ActivityKind::Milestone),
        Just(ActivityKind::StartMilestone),
    ];
    let head = (
        kind,
        word(ident()),
        word(name()),
        proptest::option::of(duration()),
        proptest::collection::vec(predecessor(), 0..3),
    )
        .prop_map(|(kind, code, name, duration, after)| Activity {
            kind,
            code,
            name,
            duration: duration
                .filter(|_| kind == ActivityKind::Activity)
                .map(Value::new),
            after,
        });
    statement(head, detail(activity_detail())).prop_map(Item::Activity)
}

fn item() -> impl Strategy<Value = Item> {
    prop_oneof![
        1 => Just(Item::Blank),
        1 => "[a-z ]{0,8}".prop_map(|t| Item::Comment(Comment { text: t.trim().to_owned(), at: crate::At::default() })),
        1 => project(),
        1 => calendar(),
        1 => code_type(),
        1 => statement(
            (word(ident()), word(name())).prop_map(|(code, name)| Role { code, name }),
            detail(prop_oneof![
                word(ident()).prop_map(RoleDetail::Parent),
                (0_u32..100).prop_map(|n| RoleDetail::MaxUnitsPerHour(Value::new(f64::from(n) / 4.0))),
            ]),
        )
        .prop_map(Item::Role),
        1 => (word(ident()), word(name()), comment()).prop_map(|(path, name, comment)| Item::Wbs(Statement {
            head: Wbs { path, name },
            at: crate::At::default(),
            comment,
            details: Vec::new(),
        })),
        4 => activity(),
    ]
}

proptest! {
    #[test]
    fn formatting_survives_reading_back(items in proptest::collection::vec(item(), 0..12)) {
        let doc = Document { comment: None, items };
        let once = format(&doc);
        let read = parse(&once, "gen.tfs").map_err(|p| TestCaseError::fail(format!("{once}\n{p:?}")))?;
        prop_assert_eq!(format(&read), once);
    }
}

mod compiling {
    use totalfloat_edit::{Edit, Kind};

    use crate::{STARTER_CALENDAR, compile, parse};

    fn compiled(body: &str) -> crate::Compiled {
        let source = format!(
            "totalfloat-schedule 1\n\nproject P \"Plant\"\n  data-date 2026-03-02 08:00\n{body}"
        );
        let doc = parse(&source, "plan.tfs").unwrap_or_else(|p| panic!("{p:?}"));
        compile(&doc, "plan.tfs").unwrap_or_else(|p| panic!("{p:?}"))
    }

    fn codes(body: &str) -> Vec<&'static str> {
        compiled(body).problems.iter().map(|d| d.code).collect()
    }

    #[test]
    fn commands_come_in_an_order_that_builds() {
        let c = compiled(
            "\ncalendar Night copy-of \"Standard 5-Day\"\n  week mon 20:00-24:00\n\nwbs P.A.B \"Deep\"\nwbs P.A \"Area\"\n\nactivity X \"x\" 1d after Y ss\n  calendar Night\nmilestone Y \"y\"\n",
        );
        assert!(c.problems.is_empty(), "{:?}", c.problems);
        let ops: Vec<String> = c
            .edits
            .iter()
            .map(|p| {
                format!("{:?}", p.edit)
                    .split([' ', '{'])
                    .next()
                    .unwrap()
                    .to_owned()
            })
            .collect();
        assert_eq!(
            ops,
            [
                "AddCalendar",
                "SetWorkWeek",
                "AddWbs",
                "AddWbs",
                "AddActivity",
                "AddActivity",
                "AddLink"
            ]
        );
        assert!(
            matches!(&c.edits[2].edit, Edit::AddWbs { code, parent, .. } if code == "A" && parent == "P")
        );
        assert!(
            matches!(&c.edits[3].edit, Edit::AddWbs { code, parent, .. } if code == "B" && parent == "A")
        );
        assert!(
            matches!(&c.edits[4].edit, Edit::AddActivity { wbs, calendar: Some(cal), .. } if wbs == "A" && cal == "Night")
        );
        assert!(matches!(
            &c.edits[5].edit,
            Edit::AddActivity {
                activity_type: Some(Kind::FinishMilestone),
                ..
            }
        ));
        assert_eq!(c.project.calendar, STARTER_CALENDAR);
    }

    #[test]
    fn the_project_calendar_is_the_one_the_schedule_starts_with() {
        let source = "totalfloat-schedule 1\n\nproject P \"Plant\"\n  data-date 2026-03-02 08:00\n  calendar Shifts\n\ncalendar Shifts\n  week mon-thu 07:00-17:00\n";
        let c = compile(&parse(source, "p").unwrap(), "p").unwrap();
        assert_eq!(c.project.calendar, "Shifts");
        // No add_calendar: the starter takes its name. Its days are all 10
        // hours, so a day is 10 hours, set before the week.
        assert!(
            matches!(&c.edits[0].edit, Edit::SetHoursPerDay { hours, .. } if (*hours - 10.0).abs() < 1e-9),
            "{:?}",
            c.edits[0].edit
        );
        // Working days are set first, then the rest of the week is made
        // non-work.
        let days: Vec<(String, bool)> = c.edits[1..]
            .iter()
            .map(|p| match &p.edit {
                Edit::SetWorkWeek { day, shifts, .. } => (format!("{day:?}"), shifts.is_empty()),
                other => panic!("{other:?}"),
            })
            .collect();
        assert_eq!(days.len(), 7);
        assert_eq!(days[0], ("Monday".to_owned(), false));
        assert_eq!(days[4], ("Friday".to_owned(), true));
    }

    #[test]
    fn names_are_checked_against_the_file() {
        assert_eq!(
            codes("\nactivity A \"a\" 1d after B\n"),
            ["unknown-activity"]
        );
        assert_eq!(
            codes("\nactivity A \"a\" 1d\n  calendar Nite\n"),
            ["unknown-calendar"]
        );
        assert_eq!(codes("\nwbs Q.A \"a\"\n"), ["wrong-wbs-root"]);
        assert_eq!(codes("\nwbs P.A.B \"b\"\n"), ["unknown-wbs"]);
        assert_eq!(
            codes("\nwbs P.A \"a\"\nwbs P.A \"again\"\n"),
            ["duplicate-wbs"]
        );
        assert_eq!(
            codes("\nwbs P.A \"a\"\nwbs P.B \"b\"\nwbs P.B.A \"c\"\n"),
            ["repeated-wbs-code"]
        );
        assert_eq!(
            codes("\nactivity A \"a\" 1d\nactivity A \"again\" 1d\n"),
            ["duplicate-activity"]
        );
        assert_eq!(
            codes("\ncalendar Night copy-of Day\n"),
            ["unknown-calendar"]
        );
        assert_eq!(
            codes("\ncode-type Phase\n  value CON\nactivity A \"a\" 1d\n  code Phase=CON\n"),
            Vec::<&str>::new()
        );
        assert_eq!(
            codes("\ncode-type Phase\n  value CON\ncode-type PHASE\n"),
            ["defined-twice"]
        );
        assert_eq!(
            codes("\nresource R \"r\"\n  calendar Nite\n"),
            ["unknown-calendar"]
        );
    }

    #[test]
    fn a_text_resource_can_only_use_the_project_s_calendar() {
        let body = "\ncalendar Night\n  week mon-fri 20:00-24:00\n\nresource NIGHT \"Night crew\"\n  calendar Night\nresource DAY \"Day crew\"\n  calendar \"Standard 5-Day\"\n";
        let c = compiled(body);
        let found: Vec<_> = c.problems.iter().map(|d| (d.code, d.at.line)).collect();
        assert_eq!(found, [("resource-calendar", 10)]);
        let calendars: Vec<_> = c
            .edits
            .iter()
            .filter_map(|p| match &p.edit {
                Edit::AddResource { calendar, .. } => Some(calendar.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(calendars, [None, Some("Standard 5-Day".to_owned())]);
    }

    /// A resource's limit reads, formats back as written, and becomes
    /// the new resource's `max_units_per_hour`. A negative one is refused.
    #[test]
    fn a_resource_s_limit_reads_formats_and_compiles() {
        let body = "\nresource ELEC \"Electricians\" labor\n  units-per-hour 1.5\n  max-units-per-hour 2\n";
        let source = format!(
            "totalfloat-schedule 1\n\nproject P \"Plant\"\n  data-date 2026-03-02 08:00\n{body}"
        );
        let doc = parse(&source, "plan.tfs").unwrap_or_else(|p| panic!("{p:?}"));
        assert_eq!(crate::format(&doc), source);
        let c = compiled(body);
        let limits: Vec<_> = c
            .edits
            .iter()
            .filter_map(|p| match &p.edit {
                Edit::AddResource {
                    units_per_hour,
                    max_units_per_hour,
                    ..
                } => Some((*units_per_hour, *max_units_per_hour)),
                _ => None,
            })
            .collect();
        assert_eq!(limits, [(Some(1.5), Some(2.0))]);
        let negative = source.replace("max-units-per-hour 2", "max-units-per-hour -1");
        let refused = parse(&negative, "plan.tfs").expect_err("a negative limit");
        assert_eq!(refused[0].code, "bad-number");
    }

    /// A role's limit reads, formats back as written, and becomes the
    /// new role's `max_units_per_hour`.
    #[test]
    fn a_role_s_limit_reads_formats_and_compiles() {
        let body = "\nrole ENG \"Engineer\"\n  max-units-per-hour 2\n";
        let source = format!(
            "totalfloat-schedule 1\n\nproject P \"Plant\"\n  data-date 2026-03-02 08:00\n{body}"
        );
        let doc = parse(&source, "plan.tfs").unwrap_or_else(|p| panic!("{p:?}"));
        assert_eq!(crate::format(&doc), source);
        let c = compiled(body);
        let limits: Vec<_> = c
            .edits
            .iter()
            .filter_map(|p| match &p.edit {
                Edit::AddRole {
                    max_units_per_hour, ..
                } => Some(*max_units_per_hour),
                _ => None,
            })
            .collect();
        assert_eq!(limits, [Some(2.0)]);
    }

    #[test]
    fn a_long_code_value_makes_room_for_itself() {
        let c = compiled("\ncode-type Area\n  value NORTH-EAST-WING\n");
        assert!(
            matches!(
                &c.edits[0].edit,
                Edit::AddCodeType {
                    max_length: Some(15),
                    ..
                }
            ),
            "{:?}",
            c.edits
        );
    }

    #[test]
    fn a_suggestion_comes_from_the_file() {
        let c = compiled("\nactivity A1020 \"a\" 1d\nactivity A1030 \"b\" 1d after A1025\n");
        assert_eq!(c.problems[0].hint, "did you mean A1020?");
    }

    #[test]
    fn without_a_project_or_data_date_nothing_compiles() {
        let doc = parse("totalfloat-schedule 1\n\nactivity A \"a\"\n", "p").unwrap();
        let codes: Vec<_> = compile(&doc, "p")
            .unwrap_err()
            .iter()
            .map(|d| d.code)
            .collect();
        assert_eq!(codes, ["missing-project"]);
        let doc = parse("totalfloat-schedule 1\n\nproject P \"p\"\n", "p").unwrap();
        let codes: Vec<_> = compile(&doc, "p")
            .unwrap_err()
            .iter()
            .map(|d| d.code)
            .collect();
        assert_eq!(codes, ["missing-data-date"]);
    }
}

mod rendering {
    use std::collections::BTreeMap;

    use totalfloat_model::{
        Activity, ActivityId, Calendar, CalendarId, CalendarType, EarnedValue, HoursPerPeriod,
        Minutes, Project, ProjectId, Schedule, TimeOfDay, Wbs, WbsId, Weekday, WorkInterval,
        WorkWeek,
    };

    use crate::{compile, format, parse, render};

    fn calendar(id: u64, name: &str) -> Calendar {
        let shift = WorkInterval::new(
            TimeOfDay::from_hm(8, 0).unwrap(),
            TimeOfDay::from_hm(16, 0).unwrap(),
        )
        .unwrap();
        let mut week = WorkWeek::default();
        for day in [Weekday::Monday, Weekday::Tuesday, Weekday::Wednesday] {
            week.day_mut(day).push(shift);
        }
        Calendar {
            id: CalendarId(id),
            name: name.to_owned(),
            kind: CalendarType::Global,
            project: None,
            base: None,
            is_default: false,
            hours_per_period: HoursPerPeriod::default(),
            work_week: week,
            exceptions: BTreeMap::new(),
            source: None,
        }
    }

    fn wbs(id: u64, parent: Option<u64>, code: &str, root: bool) -> Wbs {
        Wbs {
            id: WbsId(id),
            project: ProjectId(1),
            parent: parent.map(WbsId),
            code: code.to_owned(),
            name: format!("Node {code}"),
            sequence: i64::try_from(id).unwrap(),
            is_project_node: root,
            earned_value: EarnedValue::default(),
            obs: None,
            notes: std::collections::BTreeMap::new(),
            udfs: BTreeMap::new(),
            source: None,
        }
    }

    fn odd_schedule() -> Schedule {
        let mut project = Project::new(ProjectId(1), "P");
        project.default_calendar = Some(CalendarId(1));
        let mut s = Schedule::new(project);
        for c in [
            calendar(1, "Day"),
            calendar(2, "Night"),
            calendar(3, "night"),
        ] {
            s.calendars.insert(c.id, c);
        }
        for node in [
            wbs(1, None, "P", true),
            wbs(2, Some(1), "AREA1", false),
            wbs(3, Some(2), "ENG", false),
            wbs(4, Some(1), "AREA2", false),
            wbs(5, Some(4), "ENG", false),
            wbs(6, Some(99), "LOST", false),
        ] {
            s.wbs.insert(node.id, node);
        }
        for (n, wbs, calendar) in [(1, 3, 3), (2, 6, 1), (3, 5, 2)] {
            let mut a = Activity::new(
                ActivityId(n),
                ProjectId(1),
                WbsId(wbs),
                CalendarId(calendar),
                format!("A{n}"),
                format!("Activity {n}"),
            );
            a.original_duration = Minutes::new(480);
            a.remaining_duration = Minutes::new(480);
            s.activities.insert(a.id, a);
        }
        s
    }

    #[test]
    fn awkward_schedules_are_written_with_notes() {
        let rendered = render(&odd_schedule());
        let text = format(&rendered.document);
        let kinds: Vec<&str> = rendered.left_out.iter().map(|l| l.kind).collect();
        for kind in ["calendar-name-repeated", "wbs-orphan", "wbs-code-repeated"] {
            assert!(kinds.contains(&kind), "{kind} missing: {kinds:?}");
        }
        // A1 is on the second "night", written as the first "Night".
        assert!(
            text.contains("activity A1 \"Activity 1\" 1d\n  calendar Night\n"),
            "{text}"
        );
        // A2's node has a lost parent, so A2 is written at project level,
        // before any WBS line.
        let a2 = text.find("activity A2").unwrap();
        assert!(a2 < text.find("wbs ").unwrap(), "{text}");
        assert!(text.contains("wbs P.AREA1.ENG \"Node ENG\""), "{text}");
        insta::assert_snapshot!(text);
    }

    /// A role on an activity twice with no resource is written once, with
    /// the units of both, and noted, as the text can assign a role only once.
    #[test]
    fn a_role_twice_on_one_activity_is_one_line_and_noted() {
        use totalfloat_model as m;
        let mut schedule = odd_schedule();
        let role = m::RoleId(1);
        schedule.roles.insert(
            role,
            m::Role {
                id: role,
                short_name: "ENG".to_owned(),
                name: "Engineer".to_owned(),
                parent: None,
                rates: Vec::new(),
                default_cost_follows_units: true,
                source: None,
            },
        );
        for (id, units) in [(1, 8.0), (2, 4.0)] {
            let assignment = m::AssignmentId(id);
            schedule.assignments.insert(
                assignment,
                m::Assignment {
                    id: assignment,
                    activity: ActivityId(1),
                    resource: None,
                    role: Some(role),
                    cost_account: None,
                    project: ProjectId(1),
                    curve: None,
                    typed_spread: None,
                    amounts: m::Amounts {
                        budgeted_units: Some(units),
                        ..m::Amounts::default()
                    },
                    udfs: std::collections::BTreeMap::new(),
                    source: None,
                },
            );
        }
        let rendered = render(&schedule);
        let text = format(&rendered.document);
        assert_eq!(text.matches("  role ENG").count(), 1, "{text}");
        assert!(text.contains("  role ENG units 12\n"), "{text}");
        assert!(
            rendered.left_out.iter().any(|l| l.kind == "repeated-roles"),
            "{:?}",
            rendered.left_out
        );
        // Unchanged, it asks for nothing.
        let diff = applied(&schedule, &text);
        assert!(diff.edits.is_empty(), "{:?}", ops(&diff));
    }

    /// The text has no place for the account an assignment's costs are
    /// booked to, so it's noted, and applying the text keeps it.
    #[test]
    fn an_assignment_s_cost_account_is_noted_and_kept() {
        use totalfloat_model as m;
        let mut schedule = odd_schedule();
        let role = m::RoleId(1);
        schedule.roles.insert(
            role,
            m::Role {
                id: role,
                short_name: "ENG".to_owned(),
                name: "Engineer".to_owned(),
                parent: None,
                rates: Vec::new(),
                default_cost_follows_units: true,
                source: None,
            },
        );
        schedule.assignments.insert(
            m::AssignmentId(1),
            m::Assignment {
                id: m::AssignmentId(1),
                activity: ActivityId(1),
                resource: None,
                role: Some(role),
                cost_account: Some(m::CostAccountId(1)),
                project: ProjectId(1),
                curve: None,
                typed_spread: None,
                amounts: m::Amounts::default(),
                udfs: std::collections::BTreeMap::new(),
                source: None,
            },
        );
        let rendered = render(&schedule);
        let noted: Vec<_> = rendered.left_out.iter().map(|l| l.kind).collect();
        assert!(noted.contains(&"assignment-accounts"), "{noted:?}");
        let diff = applied(&schedule, &format(&rendered.document));
        assert!(diff.edits.is_empty(), "{:?}", ops(&diff));
    }

    /// A loop of parents, which a file could hold, is written with one parent
    /// left out, and applying it unchanged asks for nothing.
    #[test]
    fn a_loop_of_parent_roles_is_noted_and_applies_back_unchanged() {
        use totalfloat_model as m;
        let mut schedule = odd_schedule();
        for (id, parent) in [(1, 2), (2, 1), (3, 1)] {
            schedule.roles.insert(
                m::RoleId(id),
                m::Role {
                    id: m::RoleId(id),
                    short_name: format!("R{id}"),
                    name: format!("Role {id}"),
                    parent: Some(m::RoleId(parent)),
                    rates: Vec::new(),
                    default_cost_follows_units: true,
                    source: None,
                },
            );
        }
        let rendered = render(&schedule);
        let text = format(&rendered.document);
        assert!(
            rendered
                .left_out
                .iter()
                .any(|l| l.kind == "role-parent-loops"),
            "{:?}",
            rendered.left_out
        );
        let diff = applied(&schedule, &text);
        assert!(diff.edits.is_empty(), "{:?}", ops(&diff));
        assert!(diff.problems.is_empty(), "{:?}\n{text}", diff.problems);
    }

    /// `odd_schedule()` with expenses on its first activity: one as a build
    /// makes it, one with details the text can't hold, partly spent.
    fn with_expenses() -> Schedule {
        use totalfloat_model as m;
        let mut schedule = odd_schedule();
        let expense = |id: u64, name: &str, budget: f64| m::Expense {
            id: m::ExpenseId(id),
            activity: ActivityId(1),
            project: ProjectId(1),
            name: name.to_owned(),
            cost_account: None,
            category: None,
            accrual: Some(m::Accrual::Uniform),
            budgeted_cost: Some(budget),
            actual_cost: Some(0.0),
            remaining_cost: Some(budget),
            price_per_unit: Some(budget),
            budgeted_units: Some(1.0),
            unit_of_measure: String::new(),
            auto_compute_actuals: false,
            po_number: String::new(),
            vendor: String::new(),
            description: String::new(),
            source: None,
        };
        let permit = expense(1, "Building permit", 1500.0);
        let mut skip = expense(2, "Skip hire", 1200.0);
        skip.actual_cost = Some(400.0);
        skip.remaining_cost = Some(900.0);
        skip.price_per_unit = Some(300.0);
        skip.budgeted_units = Some(4.0);
        skip.vendor = "Skips Ltd".to_owned();
        skip.accrual = Some(m::Accrual::End);
        for x in [permit, skip] {
            schedule.expenses.insert(x.id, x);
        }
        schedule
    }

    /// Text has no curves, so an assignment's curve is noted, and
    /// applying the text unchanged leaves it alone.
    #[test]
    fn an_assignment_s_curve_is_noted() {
        use totalfloat_model as m;
        let mut schedule = odd_schedule();
        let role = m::RoleId(1);
        schedule.roles.insert(
            role,
            m::Role {
                id: role,
                short_name: "ENG".to_owned(),
                name: "Engineer".to_owned(),
                parent: None,
                rates: Vec::new(),
                default_cost_follows_units: true,
                source: None,
            },
        );
        let curve = m::ResourceCurveId(1);
        schedule.resource_curves.insert(
            curve,
            m::ResourceCurve {
                id: curve,
                name: "Bell".to_owned(),
                default: true,
                points: [5.0; m::CURVE_POINTS],
                source: None,
            },
        );
        let assignment = m::AssignmentId(1);
        schedule.assignments.insert(
            assignment,
            m::Assignment {
                id: assignment,
                activity: ActivityId(1),
                resource: None,
                role: Some(role),
                cost_account: None,
                project: ProjectId(1),
                curve: Some(curve),
                typed_spread: None,
                amounts: m::Amounts::default(),
                udfs: std::collections::BTreeMap::new(),
                source: None,
            },
        );
        let rendered = render(&schedule);
        assert!(
            rendered.left_out.iter().any(|l| l.kind == "curves"),
            "{:?}",
            rendered.left_out
        );
        // A hand-typed spread is noted the same way.
        let mut typed = schedule.clone();
        let a = typed.assignments.get_mut(&assignment).unwrap();
        a.curve = None;
        a.typed_spread = Some(m::TypedSpread {
            remaining: vec![m::SpreadSegment {
                units: 8.0,
                work: m::Minutes::new(480),
            }],
            ..m::TypedSpread::default()
        });
        assert!(
            render(&typed)
                .left_out
                .iter()
                .any(|l| l.kind == "typed-spreads"),
        );
        let diff = applied(&schedule, &format(&rendered.document));
        assert!(diff.edits.is_empty(), "{:?}", ops(&diff));
    }

    /// Expenses are written by item with what isn't the default, and
    /// what a line can't hold is noted; unchanged, the text asks for nothing.
    #[test]
    fn expenses_are_written_noted_and_apply_back_unchanged() {
        let schedule = with_expenses();
        let rendered = render(&schedule);
        let text = format(&rendered.document);
        assert!(
            text.contains("  expense \"Building permit\" 1500\n"),
            "{text}"
        );
        assert!(
            text.contains("  expense \"Skip hire\" 1200 actual 400 remaining 900 accrual end\n"),
            "{text}"
        );
        let noted: Vec<_> = rendered.left_out.iter().map(|l| l.kind).collect();
        assert!(noted.contains(&"expense-details"), "{noted:?}");
        let diff = applied(&schedule, &text);
        assert!(diff.edits.is_empty(), "{:?}", ops(&diff));
        assert!(diff.problems.is_empty(), "{:?}", diff.problems);
    }

    /// A changed line sets the expense's costs, a new one adds it, and one
    /// taken out deletes it.
    #[test]
    fn expense_lines_changed_added_and_removed_are_edits() {
        let schedule = with_expenses();
        let text = format(&render(&schedule).document)
            .replace(
                "  expense \"Building permit\" 1500\n",
                "  expense \"Survey\" 650 actual 100\n",
            )
            .replace(
                "1200 actual 400 remaining 900 accrual end",
                "1200 actual 700",
            );
        let diff = applied(&schedule, &text);
        let ops = ops(&diff);
        assert_eq!(ops.len(), 4, "{ops:?}");
        assert!(ops[0].starts_with("DeleteExpense"), "{ops:?}");
        assert!(ops[0].contains("Building permit"), "{ops:?}");
        assert!(
            ops.iter()
                .any(|o| o.starts_with("AddExpense") && o.contains("Survey"))
        );
        assert!(
            ops.iter().any(|o| o.starts_with("SetExpense")
                && o.contains("Survey")
                && o.contains("actual_cost: Some(100.0)")
                && o.contains("remaining_cost: Some(550.0)")),
            "{ops:?}"
        );
        assert!(
            ops.iter().any(|o| o.starts_with("SetExpense")
                && o.contains("Skip hire")
                && o.contains("budgeted_cost: None")
                && o.contains("remaining_cost: Some(500.0)")
                && o.contains("accrual: Some(Uniform)")),
            "{ops:?}"
        );
    }

    /// A second line for an item is added, which the edits refuse as a
    /// duplicate, rather than left out.
    #[test]
    fn a_second_line_for_an_expense_is_not_dropped() {
        let schedule = with_expenses();
        let text = format(&render(&schedule).document).replace(
            "  expense \"Building permit\" 1500\n",
            "  expense \"Building permit\" 1500\n  expense \"building permit\" 2500\n",
        );
        let ops = ops(&applied(&schedule, &text));
        assert!(
            ops.iter().any(|o| o.starts_with("AddExpense")
                && o.contains("building permit")
                && o.contains("cost: 2500.0")),
            "{ops:?}"
        );
    }

    fn applied(schedule: &Schedule, text: &str) -> crate::Difference {
        let doc = parse(text, "p.tfs").unwrap_or_else(|p| panic!("{p:?}"));
        crate::difference(schedule, &doc, "p.tfs", crate::Leaving::Keep)
    }

    fn ops(diff: &crate::Difference) -> Vec<String> {
        diff.edits.iter().map(|p| format!("{:?}", p.edit)).collect()
    }

    #[test]
    fn an_unchanged_export_of_an_awkward_schedule_asks_for_nothing() {
        let schedule = odd_schedule();
        let text = format(&render(&schedule).document);
        let diff = applied(&schedule, &text);
        assert!(diff.edits.is_empty(), "{:?}", ops(&diff));
        assert!(diff.missing.is_empty(), "{:?}", diff.missing);
        assert!(diff.problems.is_empty(), "{:?}", diff.problems);
    }

    #[test]
    fn a_new_node_sharing_a_code_is_added_not_moved() {
        let schedule = odd_schedule();
        let text = format(&render(&schedule).document).replace(
            "wbs P.AREA2 \"Node AREA2\"\n",
            "wbs P.AREA2 \"Node AREA2\"\n\nwbs P.AREA2.AREA1X \"New\"\n",
        );
        let diff = applied(&schedule, &text);
        assert_eq!(diff.edits.len(), 1, "{:?}", ops(&diff));
        assert!(ops(&diff)[0].starts_with("AddWbs"), "{:?}", ops(&diff));
        // A node moved: the only one with its code, gone from its old path.
        let text = format(&render(&schedule).document)
            .replace(
                "wbs P.AREA1 \"Node AREA1\"",
                "wbs P.AREA2.AREA1 \"Node AREA1\"",
            )
            .replace("wbs P.AREA1.ENG", "wbs P.AREA2.AREA1.ENG");
        let diff = applied(&schedule, &text);
        assert!(
            ops(&diff).iter().any(|o| o.starts_with("MoveWbs")),
            "{:?}",
            ops(&diff)
        );
        assert!(diff.missing.is_empty(), "{:?}", diff.missing);
    }

    #[test]
    fn new_calendars_come_in_the_order_written_and_calendar_before_duration() {
        let schedule = odd_schedule();
        let text = format(&render(&schedule).document)
            .replace(
                "\ncalendar Day\n",
                "\ncalendar Zed\n  week mon 08:00-18:00\n\ncalendar Alt copy-of Zed\n\ncalendar Day\n",
            )
            .replace("activity A2 \"Activity 2\" 1d\n", "activity A2 \"Activity 2\" 2d\n  calendar Night\n");
        let diff = applied(&schedule, &text);
        let ops = ops(&diff);
        let zed = ops
            .iter()
            .position(|o| o.contains("AddCalendar") && o.contains("Zed"))
            .unwrap();
        let alt = ops
            .iter()
            .position(|o| o.contains("AddCalendar") && o.contains("Alt"))
            .unwrap();
        assert!(zed < alt, "{ops:?}");
        let calendar = ops
            .iter()
            .position(|o| o.starts_with("SetCalendar"))
            .unwrap();
        let duration = ops
            .iter()
            .position(|o| o.starts_with("SetDuration"))
            .unwrap();
        assert!(calendar < duration, "{ops:?}");
    }

    #[test]
    fn an_activity_defined_twice_is_a_problem() {
        let schedule = odd_schedule();
        let text = format(&render(&schedule).document) + "activity A3 \"Again\" 2d\n";
        let diff = applied(&schedule, &text);
        let codes: Vec<_> = diff.problems.iter().map(|d| d.code).collect();
        assert_eq!(codes, ["defined-twice"]);
    }

    #[test]
    fn a_code_type_defined_twice_counts_once_as_when_building() {
        let schedule = odd_schedule();
        let text = format(&render(&schedule).document)
            + "\ncode-type Area\n  value EAST\n\ncode-type area\n  value WEST\n";
        let diff = applied(&schedule, &text);
        let codes: Vec<_> = diff.problems.iter().map(|d| d.code).collect();
        assert_eq!(codes, ["defined-twice"]);
        let values: Vec<_> = ops(&diff)
            .into_iter()
            .filter(|op| op.starts_with("AddCodeValue"))
            .collect();
        assert_eq!(values.len(), 1);
        assert!(values[0].contains("EAST"), "{values:?}");
    }

    #[test]
    fn code_types_sharing_a_name_are_written_once() {
        let mut schedule = odd_schedule();
        for id in [7, 8] {
            let id = totalfloat_model::ActivityCodeTypeId(id);
            schedule.activity_code_types.insert(
                id,
                totalfloat_model::ActivityCodeType {
                    id,
                    name: "Phase".to_owned(),
                    scope: totalfloat_model::CodeScope::default(),
                    project: None,
                    max_length: None,
                    sequence: 0,
                    source: None,
                },
            );
        }
        let rendered = render(&schedule);
        let text = format(&rendered.document);
        assert_eq!(text.matches("code-type Phase").count(), 1, "{text}");
        assert!(
            rendered
                .left_out
                .iter()
                .any(|n| n.kind == "code-type-name-repeated")
        );
    }

    #[test]
    fn a_code_type_no_activity_uses_yet_takes_values() {
        let mut schedule = odd_schedule();
        let id = totalfloat_model::ActivityCodeTypeId(7);
        schedule.activity_code_types.insert(
            id,
            totalfloat_model::ActivityCodeType {
                id,
                name: "Phase".to_owned(),
                scope: totalfloat_model::CodeScope::default(),
                project: None,
                max_length: None,
                sequence: 0,
                source: None,
            },
        );
        // Written out, it has no values.
        let text = format(&render(&schedule).document)
            .replace("code-type Phase\n", "code-type Phase\n  value CON\n");
        let diff = applied(&schedule, &text);
        assert!(diff.problems.is_empty(), "{:?}", diff.problems);
        assert!(
            ops(&diff)[0].starts_with("AddCodeValue"),
            "{:?}",
            ops(&diff)
        );
    }

    #[test]
    fn a_project_code_with_a_dot_builds_its_wbs() {
        let source = "totalfloat-schedule 1\n\nproject PRJ.001 \"p\"\n  data-date 2026-03-02 08:00\n\nwbs PRJ.001.CIV \"Civil\"\nwbs PRJ.001.CIV.A \"A\"\n";
        let c = compile(&parse(source, "p").unwrap(), "p").unwrap();
        assert!(c.problems.is_empty(), "{:?}", c.problems);
        assert_eq!(c.edits.len(), 2);
        let source = "totalfloat-schedule 1\n\nproject PRJ.001 \"p\"\n  data-date 2026-03-02 08:00\n\nwbs PRJ.002.CIV \"Civil\"\nwbs PRJ.001 \"Root again\"\n";
        let c = compile(&parse(source, "p").unwrap(), "p").unwrap();
        let codes: Vec<_> = c.problems.iter().map(|d| d.code).collect();
        assert_eq!(codes, ["wrong-wbs-root", "wrong-wbs-root"]);
    }
}

mod user_defined_fields {
    use totalfloat_edit::{Edit, UdfFor, UdfKind};
    use totalfloat_model::{
        Activity, ActivityId, CalendarId, Project, ProjectId, Schedule, UdfDataType, UdfSubject,
        UdfType, UdfTypeId, UdfValue, Wbs, WbsId,
    };

    use crate::{compile, format, parse, render};

    const TEXT: &str = "totalfloat-schedule 1\n\nproject P \"Plant\"\n  data-date 2026-03-02 08:00\n\nfield \"Crew size\" integer\nfield Risk indicator\n\nactivity A1 \"Dig\" 1d\n  udf \"Crew size\" 4\n  udf Risk=UDF_G2\n";

    #[test]
    fn fields_and_values_read_and_format_to_themselves() {
        let doc = parse(TEXT, "p.tfs").unwrap_or_else(|p| panic!("{p:?}"));
        assert_eq!(format(&doc), TEXT);
    }

    #[test]
    fn a_built_schedule_adds_the_fields_then_the_values() {
        let c = compile(&parse(TEXT, "p.tfs").unwrap(), "p.tfs").unwrap();
        assert!(c.problems.is_empty(), "{:?}", c.problems);
        let edits: Vec<&Edit> = c.edits.iter().map(|p| &p.edit).collect();
        assert!(edits.contains(&&Edit::AddUdfType {
            field: "Crew size".to_owned(),
            applies_to: UdfFor::Activity,
            data_type: UdfKind::Integer,
        }));
        let set = edits
            .iter()
            .position(|e| matches!(e, Edit::SetUdf { field, value, .. } if field == "Crew size" && value == "4"));
        let added = edits
            .iter()
            .position(|e| matches!(e, Edit::AddUdfType { field, .. } if field == "Crew size"));
        assert!(added < set, "{edits:?}");
        let unknown = TEXT.replace("udf Risk=UDF_G2", "udf Rsk=UDF_G2");
        let c = compile(&parse(&unknown, "p.tfs").unwrap(), "p.tfs").unwrap();
        let problems: Vec<_> = c
            .problems
            .iter()
            .map(|d| (d.code, d.hint.clone()))
            .collect();
        assert_eq!(
            problems,
            [("unknown-field", "did you mean Risk?".to_owned())]
        );
    }

    fn with_fields() -> Schedule {
        let mut s = Schedule::new(Project::new(ProjectId(1), "P"));
        s.wbs.insert(
            WbsId(1),
            Wbs {
                id: WbsId(1),
                project: ProjectId(1),
                parent: None,
                code: "P".to_owned(),
                name: "Plant".to_owned(),
                sequence: 0,
                is_project_node: true,
                earned_value: totalfloat_model::EarnedValue::default(),
                obs: None,
                notes: std::collections::BTreeMap::new(),
                udfs: std::collections::BTreeMap::new(),
                source: None,
            },
        );
        for (id, label, data_type) in [
            (1, "Crew size", UdfDataType::Integer),
            (2, "Due", UdfDataType::FinishDate),
            (3, "Trade list", UdfDataType::Code),
        ] {
            s.udf_types.insert(
                UdfTypeId(id),
                UdfType {
                    id: UdfTypeId(id),
                    subject: UdfSubject::Activity,
                    label: label.to_owned(),
                    data_type,
                    source: None,
                },
            );
        }
        let mut a = Activity::new(
            ActivityId(1),
            ProjectId(1),
            WbsId(1),
            CalendarId(1),
            "A1",
            "Dig",
        );
        a.udfs.insert(UdfTypeId(1), UdfValue::Number(4.0));
        a.udfs.insert(
            UdfTypeId(2),
            UdfValue::Date("2026-03-09 17:00".parse().unwrap()),
        );
        a.udfs
            .insert(UdfTypeId(3), UdfValue::Code(totalfloat_model::UdfCodeId(9)));
        s.activities.insert(a.id, a);
        s
    }

    #[test]
    fn values_are_written_and_a_code_list_value_is_noted() {
        let rendered = render(&with_fields());
        let text = format(&rendered.document);
        assert!(
            text.contains("field \"Crew size\" integer\nfield Due finish-date\n"),
            "{text}"
        );
        assert!(!text.contains("Trade list"), "{text}");
        assert!(
            text.contains("  udf \"Crew size\" 4\n  udf Due \"2026-03-09 17:00\"\n"),
            "{text}"
        );
        let kinds: Vec<&str> = rendered.left_out.iter().map(|n| n.kind).collect();
        assert!(
            kinds.contains(&"udf-field") && kinds.contains(&"udfs"),
            "{kinds:?}"
        );
    }

    #[test]
    fn a_changed_or_removed_value_is_an_edit() {
        let schedule = with_fields();
        let text = format(&render(&schedule).document);
        let doc = parse(&text, "p.tfs").unwrap();
        let same = crate::difference(&schedule, &doc, "p.tfs", crate::Leaving::Keep);
        assert!(same.edits.is_empty(), "{:?}", same.edits);
        let changed = text
            .replace("udf \"Crew size\" 4", "udf \"Crew size\" 6")
            .replace("  udf Due \"2026-03-09 17:00\"\n", "");
        let doc = parse(&changed, "p.tfs").unwrap();
        let diff = crate::difference(&schedule, &doc, "p.tfs", crate::Leaving::Keep);
        let edits: Vec<&Edit> = diff.edits.iter().map(|p| &p.edit).collect();
        assert_eq!(edits.len(), 2, "{edits:?}");
        assert!(
            edits
                .iter()
                .any(|e| matches!(e, Edit::SetUdf { value, .. } if value == "6"))
        );
        assert!(
            edits
                .iter()
                .any(|e| matches!(e, Edit::ClearUdf { field, .. } if field == "Due"))
        );
    }
}

/// Schedule text keeps hours and days; weeks, months and years, which edits
/// take, are refused here.
#[test]
fn text_spans_are_hours_or_days_only() {
    for span in ["2w", "1m", "1y"] {
        let source = format!(
            "totalfloat-schedule 1\n\nproject P \"P\"\n  data-date 2026-03-02 08:00\n\nactivity A \"a\" {span}\n"
        );
        assert_eq!(codes(&source), ["bad-span"], "{span}");
    }
}
