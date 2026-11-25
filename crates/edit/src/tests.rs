//! Each edit, and each way it can be refused.

use std::collections::BTreeSet;

use proptest::prelude::*;
use totalfloat_model::{
    Activity, ActivityCode, ActivityCodeId, ActivityCodeType, ActivityCodeTypeId, ActivityId,
    ActivityStatus, ActivityType, AssignmentId, Calendar, CalendarId, CalendarType, CodeScope,
    ConstraintType, HoursPerPeriod, LagCalendar, Minutes, Project, ProjectId, Relationship,
    RelationshipId, RelationshipType, Schedule, TimeOfDay, Wbs, WbsId, Weekday, WorkInterval,
    WorkWeek,
};

use totalfloat_model::{Resource, ResourceId, ResourceType, RoleId};

use super::{Edit, LinkType, NextIds, Span, apply, apply_between, check_loop_across};

fn calendar(id: u64, name: &str, hours: i64) -> Calendar {
    Calendar {
        id: CalendarId(id),
        name: name.to_owned(),
        kind: CalendarType::Global,
        project: None,
        base: None,
        is_default: false,
        hours_per_period: HoursPerPeriod {
            day: Some(Minutes::new(hours * 60)),
            ..HoursPerPeriod::default()
        },
        work_week: WorkWeek::default(),
        exceptions: std::collections::BTreeMap::new(),
        source: None,
    }
}

/// A1 to A2 to A3 to A4, all work not started, on an 8-hour calendar.
fn chain() -> Schedule {
    let mut s = Schedule::new(Project::new(ProjectId(1), "AUTHORED"));
    s.calendars
        .insert(CalendarId(1), calendar(1, "Five day", 8));
    s.calendars
        .insert(CalendarId(2), calendar(2, "Ten hour", 10));
    s.wbs.insert(
        WbsId(1),
        Wbs {
            id: WbsId(1),
            project: ProjectId(1),
            parent: None,
            code: "ROOT".to_owned(),
            name: "Root".to_owned(),
            sequence: 0,
            is_project_node: true,
            earned_value: totalfloat_model::EarnedValue::default(),
            obs: None,
            notes: std::collections::BTreeMap::new(),
            udfs: std::collections::BTreeMap::new(),
            source: None,
        },
    );
    for n in 1..=4 {
        let mut a = Activity::new(
            ActivityId(n),
            ProjectId(1),
            WbsId(1),
            CalendarId(1),
            format!("A{n}"),
            format!("Activity {n}"),
        );
        a.original_duration = Minutes::new(480);
        a.remaining_duration = Minutes::new(480);
        s.activities.insert(a.id, a);
    }
    for n in 1..=3 {
        s.relationships.insert(
            RelationshipId(n),
            Relationship {
                id: RelationshipId(n),
                predecessor: ActivityId(n),
                successor: ActivityId(n + 1),
                predecessor_project: ProjectId(1),
                successor_project: ProjectId(1),
                kind: RelationshipType::FinishToStart,
                lag: Minutes::ZERO,
                source: None,
            },
        );
    }
    s
}

fn activity(s: &Schedule, n: u64) -> &Activity {
    &s.activities[&ActivityId(n)]
}

fn start(s: &mut Schedule, n: u64) {
    let a = s.activities.get_mut(&ActivityId(n)).unwrap();
    a.status = ActivityStatus::InProgress;
}

fn edit(json: serde_json::Value) -> Edit {
    serde_json::from_value(json).unwrap()
}

fn ids() -> NextIds {
    NextIds {
        relationship: 100,
        activity: 100,
        calendar: 100,
        code: 100,
        assignment: 100,
        wbs: 100,
        code_type: 100,
        resource: 100,
        resource_rate: 100,
        role: 100,
        role_rate: 100,
        expense: 100,
        project_code_type: 100,
        project_code: 100,
        obs: 100,
        activity_note: 100,
        wbs_note: 100,
        step: 100,
        udf_type: 100,
    }
}

fn run(s: &mut Schedule, json: serde_json::Value) -> Result<String, &'static str> {
    apply(s, &edit(json), &mut ids()).map_err(|e| e.code)
}

/// The edit is refused with `code`, and the schedule doesn't change.
fn refused(s: &Schedule, json: serde_json::Value, code: &str) {
    let mut copy = s.clone();
    assert_eq!(run(&mut copy, json), Err(code));
    assert_eq!(&copy, s);
}

fn span(text: &str) -> Span {
    text.parse().unwrap()
}

#[test]
fn spans_read_as_written() {
    let day = Some(Minutes::new(480));
    assert_eq!(span("15d").minutes(day), Some(Minutes::new(7200)));
    assert_eq!(span("120h").minutes(None), Some(Minutes::new(7200)));
    assert_eq!(span("-8h").minutes(None), Some(Minutes::new(-480)));
    assert_eq!(span("7.5h").minutes(None), Some(Minutes::new(450)));
    assert_eq!(span("0.5d").minutes(day), Some(Minutes::new(240)));
    assert_eq!(span("1.5D").minutes(day), Some(Minutes::new(720)));
    assert_eq!(span("1d").minutes(None), None);
    assert_eq!(span("15d").to_string(), "15d");
    for bad in [
        "",
        "h",
        "15",
        "15x",
        "1.2.3h",
        "--2h",
        "1e3h",
        "1.2345h",
        "99999999999999h",
    ] {
        assert!(bad.parse::<Span>().is_err(), "{bad}");
    }
}

#[test]
fn remaining_follows_original_on_work_not_started() {
    let mut s = chain();
    let json = serde_json::json!({"op": "set_duration", "activity": "A2", "original": "15d"});
    let line = run(&mut s, json).unwrap();
    assert_eq!(activity(&s, 2).original_duration, Minutes::new(7200));
    assert_eq!(activity(&s, 2).remaining_duration, Minutes::new(7200));
    assert!(line.starts_with("A2 duration"), "{line}");
}

#[test]
fn both_durations_can_be_given() {
    let mut s = chain();
    let json = serde_json::json!({
        "op": "set_duration", "activity": "A2", "original": "16h", "remaining": "4h"
    });
    run(&mut s, json).unwrap();
    assert_eq!(activity(&s, 2).original_duration, Minutes::new(960));
    assert_eq!(activity(&s, 2).remaining_duration, Minutes::new(240));
}

#[test]
fn days_use_the_activity_calendar() {
    let mut s = chain();
    s.activities.get_mut(&ActivityId(2)).unwrap().calendar = CalendarId(2);
    let json = serde_json::json!({"op": "set_duration", "activity": "A2", "original": "2d"});
    run(&mut s, json).unwrap();
    assert_eq!(activity(&s, 2).original_duration, Minutes::new(1200));
}

#[test]
fn an_even_work_week_says_how_long_a_day_is() {
    let mut s = chain();
    let shift = WorkInterval::new(
        TimeOfDay::from_hm(8, 0).unwrap(),
        TimeOfDay::from_hm(15, 0).unwrap(),
    )
    .unwrap();
    let calendar = s.calendars.get_mut(&CalendarId(1)).unwrap();
    calendar.hours_per_period.day = None;
    calendar.work_week.day_mut(Weekday::Monday).push(shift);
    calendar.work_week.day_mut(Weekday::Tuesday).push(shift);
    let json = serde_json::json!({"op": "set_duration", "activity": "A2", "original": "2d"});
    run(&mut s, json.clone()).unwrap();
    assert_eq!(activity(&s, 2).original_duration, Minutes::new(840));
    let half = WorkInterval::new(
        TimeOfDay::from_hm(8, 0).unwrap(),
        TimeOfDay::from_hm(12, 0).unwrap(),
    )
    .unwrap();
    s.calendars
        .get_mut(&CalendarId(1))
        .unwrap()
        .work_week
        .day_mut(Weekday::Saturday)
        .push(half);
    refused(&s, json, "no-hours-per-day");
}

#[test]
fn days_need_a_calendar_that_says_how_long_a_day_is() {
    let mut s = chain();
    s.calendars
        .get_mut(&CalendarId(1))
        .unwrap()
        .hours_per_period
        .day = None;
    let json = serde_json::json!({"op": "set_duration", "activity": "A2", "original": "2d"});
    refused(&s, json, "no-hours-per-day");
}

#[test]
fn original_changes_alone_on_work_in_progress() {
    let mut s = chain();
    start(&mut s, 2);
    let json = serde_json::json!({"op": "set_duration", "activity": "A2", "original": "2d"});
    run(&mut s, json).unwrap();
    assert_eq!(activity(&s, 2).original_duration, Minutes::new(960));
    assert_eq!(activity(&s, 2).remaining_duration, Minutes::new(480));
}

#[test]
fn duration_refusals() {
    let mut s = chain();
    let set = |code: &str, field: &str, value: &str| {
        let mut json = serde_json::json!({"op": "set_duration", "activity": code});
        json[field] = value.into();
        json
    };
    refused(&s, set("NOPE", "original", "1d"), "no-such-activity");
    refused(
        &s,
        serde_json::json!({"op": "set_duration", "activity": "A1"}),
        "nothing-to-change",
    );
    refused(&s, set("A1", "original", "-1d"), "negative-duration");
    start(&mut s, 2);
    refused(&s, set("A2", "remaining", "0h"), "finished-work");
    s.activities.get_mut(&ActivityId(3)).unwrap().status = ActivityStatus::Completed;
    refused(&s, set("A3", "remaining", "1d"), "finished-work");
    set_type(&mut s, 4, ActivityType::FinishMilestone);
    refused(&s, set("A4", "original", "1d"), "no-duration");
    set_type(&mut s, 4, ActivityType::LevelOfEffort);
    refused(&s, set("A4", "original", "1d"), "no-duration");
}

fn set_type(s: &mut Schedule, n: u64, kind: ActivityType) {
    s.activities.get_mut(&ActivityId(n)).unwrap().activity_type = kind;
}

#[test]
fn a_constraint_is_set_and_cleared() {
    let mut s = chain();
    let set = serde_json::json!({
        "op": "set_constraint", "activity": "A2", "constraint": "start-on-or-after",
        "date": "2026-02-02 08:00"
    });
    run(&mut s, set).unwrap();
    let held = activity(&s, 2).primary_constraint.unwrap();
    assert_eq!(held.kind, ConstraintType::StartOnOrAfter);
    assert_eq!(held.date.unwrap().to_string(), "2026-02-02 08:00");
    let clear = serde_json::json!({"op": "clear_constraint", "activity": "A2"});
    run(&mut s, clear).unwrap();
    assert_eq!(activity(&s, 2).primary_constraint, None);
}

fn constrain(kind: &str, secondary: bool) -> serde_json::Value {
    serde_json::json!({
        "op": "set_constraint", "activity": "A2", "constraint": kind,
        "date": "2026-02-02 08:00", "secondary": secondary
    })
}

#[test]
fn a_primary_with_a_secondary_stays_until_the_secondary_goes() {
    let mut s = chain();
    run(&mut s, constrain("start-on-or-after", false)).unwrap();
    run(&mut s, constrain("finish-on-or-before", true)).unwrap();
    let clear = serde_json::json!({"op": "clear_constraint", "activity": "A2"});
    refused(&s, clear.clone(), "secondary-constraint-remains");
    let second = serde_json::json!({
        "op": "clear_constraint", "activity": "A2", "secondary": true
    });
    run(&mut s, second).unwrap();
    run(&mut s, clear).unwrap();
    assert_eq!(activity(&s, 2).primary_constraint, None);
}

#[test]
fn constraint_pairs_the_engine_refuses_are_refused_here() {
    let mut s = chain();
    run(&mut s, constrain("start-on", false)).unwrap();
    refused(
        &s,
        constrain("finish-on-or-before", true),
        "constraints-clash",
    );
    run(&mut s, constrain("start-on-or-after", false)).unwrap();
    refused(&s, constrain("mandatory-finish", true), "constraints-clash");
    run(&mut s, constrain("finish-on-or-before", true)).unwrap();
    refused(&s, constrain("mandatory-start", false), "constraints-clash");
}

#[test]
fn a_constraint_that_would_do_nothing_is_refused() {
    let mut s = chain();
    set_type(&mut s, 2, ActivityType::LevelOfEffort);
    refused(&s, constrain("start-on", false), "no-constraint-here");
    set_type(&mut s, 2, ActivityType::TaskDependent);
    s.activities.get_mut(&ActivityId(2)).unwrap().status = ActivityStatus::Completed;
    refused(&s, constrain("start-on", false), "finished-work");
}

#[test]
fn resource_dependent_work_has_no_day_or_calendar_of_its_own() {
    let mut s = chain();
    set_type(&mut s, 2, ActivityType::ResourceDependent);
    let days = serde_json::json!({"op": "set_duration", "activity": "A2", "original": "2d"});
    refused(&s, days, "no-hours-per-day");
    let hours = serde_json::json!({"op": "set_duration", "activity": "A2", "original": "16h"});
    run(&mut s, hours).unwrap();
    let calendar =
        serde_json::json!({"op": "set_calendar", "activity": "A2", "calendar": "Ten hour"});
    refused(&s, calendar, "resource-calendar");
    let lag = serde_json::json!({
        "op": "add_link", "predecessor": "A2", "successor": "A4", "lag": "1d"
    });
    refused(&s, lag, "no-hours-per-day");
}

#[test]
fn a_link_never_replaces_one_already_there() {
    let mut s = chain();
    let mut low = NextIds {
        relationship: 2,
        ..ids()
    };
    apply(&mut s, &edit(add("A1", "A3")), &mut low).unwrap();
    assert_eq!(s.relationships.len(), 4);
    assert_eq!(low.relationship, 5);
}

#[test]
fn changing_a_link_to_another_project_is_refused_too() {
    let mut s = chain();
    s.relationships
        .get_mut(&RelationshipId(1))
        .unwrap()
        .predecessor_project = ProjectId(2);
    let json = serde_json::json!({
        "op": "change_link", "predecessor": "A1", "successor": "A2", "lag": "1d"
    });
    refused(&s, json, "other-project");
}

#[test]
fn a_milestone_becomes_a_task_only_with_a_duration() {
    let mut s = chain();
    set_type(&mut s, 2, ActivityType::FinishMilestone);
    let json = serde_json::json!({"op": "set_type", "activity": "A2", "activity_type": "task"});
    refused(&s, json.clone(), "needs-duration");
    let mut with = json;
    with["duration"] = "2d".into();
    run(&mut s, with).unwrap();
    assert_eq!(activity(&s, 2).remaining_duration, Minutes::new(960));
}

#[test]
fn spans_keep_their_type() {
    let mut s = chain();
    set_type(&mut s, 2, ActivityType::WbsSummary);
    let json = serde_json::json!({"op": "set_type", "activity": "A2", "activity_type": "task"});
    refused(&s, json, "span-type");
}

#[test]
fn a_name_fits_what_files_hold() {
    let s = chain();
    let json = serde_json::json!({"op": "set_name", "activity": "A2", "name": "x".repeat(121)});
    refused(&s, json, "name-too-long");
}

#[test]
fn a_cleared_constraint_reports_what_it_did() {
    let mut s = chain();
    run(&mut s, constrain("start-on-or-after", false)).unwrap();
    let line = run(
        &mut s,
        serde_json::json!({"op": "clear_constraint", "activity": "A2"}),
    );
    assert_eq!(line.unwrap(), "A2 primary constraint cleared");
}

#[test]
fn constraint_refusals() {
    let s = chain();
    let set = |kind: &str, date: Option<&str>, secondary: bool| {
        serde_json::json!({
            "op": "set_constraint", "activity": "A2", "constraint": kind, "date": date,
            "secondary": secondary
        })
    };
    refused(&s, set("start-on", None, false), "constraint-needs-date");
    refused(
        &s,
        set("as-late-as-possible", Some("2026-02-02 08:00"), false),
        "constraint-takes-no-date",
    );
    refused(&s, set("start-on", Some("soon"), false), "invalid-date");
    refused(
        &s,
        set("start-on", Some("2026-02-02 08:00"), true),
        "no-primary-constraint",
    );
    refused(
        &s,
        serde_json::json!({"op": "clear_constraint", "activity": "A2"}),
        "no-constraint",
    );
    let mut late = s;
    run(&mut late, set("as-late-as-possible", None, false)).unwrap();
    assert_eq!(
        activity(&late, 2).primary_constraint.unwrap().kind,
        ConstraintType::AsLateAsPossible
    );
}

#[test]
fn a_calendar_is_found_by_name_or_id() {
    let mut s = chain();
    let by_name =
        serde_json::json!({"op": "set_calendar", "activity": "A2", "calendar": "ten HOUR"});
    run(&mut s, by_name).unwrap();
    assert_eq!(activity(&s, 2).calendar, CalendarId(2));
    let by_id = serde_json::json!({"op": "set_calendar", "activity": "A2", "calendar": "1"});
    run(&mut s, by_id).unwrap();
    assert_eq!(activity(&s, 2).calendar, CalendarId(1));
}

#[test]
fn calendar_refusals() {
    let mut s = chain();
    let json =
        |name: &str| serde_json::json!({"op": "set_calendar", "activity": "A2", "calendar": name});
    refused(&s, json("Nope"), "no-such-calendar");
    s.calendars
        .insert(CalendarId(3), calendar(3, "Five day", 8));
    refused(&s, json("Five day"), "ambiguous-calendar");
    s.calendars.get_mut(&CalendarId(3)).unwrap().kind = CalendarType::Resource;
    s.calendars.get_mut(&CalendarId(3)).unwrap().name = "Crew".to_owned();
    refused(&s, json("Crew"), "no-such-calendar");
}

#[test]
fn names_stay_on_one_line() {
    let mut s = chain();
    let json = |name: &str| serde_json::json!({"op": "set_name", "activity": "A2", "name": name});
    refused(&s, json("  "), "empty-name");
    refused(&s, json("two\tcells"), "unwritable-name");
    refused(&s, json("two\nlines"), "unwritable-name");
    run(&mut s, json("Pour")).unwrap();
    assert_eq!(activity(&s, 2).name, "Pour");
}

#[test]
fn a_milestone_has_no_duration() {
    let mut s = chain();
    let json = serde_json::json!({
        "op": "set_type", "activity": "A2", "activity_type": "finish-milestone"
    });
    run(&mut s, json.clone()).unwrap();
    assert_eq!(activity(&s, 2).activity_type, ActivityType::FinishMilestone);
    assert_eq!(activity(&s, 2).remaining_duration, Minutes::ZERO);
    start(&mut s, 3);
    let mut again = json;
    again["activity"] = "A3".into();
    refused(&s, again, "started-work");
}

#[test]
fn an_expected_finish_is_set_and_cleared() {
    let mut s = chain();
    let set = serde_json::json!({
        "op": "set_expected_finish", "activity": "A2", "date": "2026-03-02 17:00"
    });
    run(&mut s, set).unwrap();
    assert!(activity(&s, 2).expected_finish.is_some());
    let clear = serde_json::json!({"op": "set_expected_finish", "activity": "A2"});
    run(&mut s, clear).unwrap();
    assert_eq!(activity(&s, 2).expected_finish, None);
}

fn new_activity(extra: &serde_json::Value) -> serde_json::Value {
    let mut json = serde_json::json!({
        "op": "add_activity", "code": "N1", "name": "New work", "wbs": "root",
        "calendar": "Five day", "duration": "2d"
    });
    for (key, value) in extra.as_object().unwrap() {
        json[key] = value.clone();
    }
    json
}

#[test]
fn an_activity_is_added_above_every_id_and_can_be_linked_at_once() {
    let mut s = chain();
    let mut next = ids();
    let line = apply(
        &mut s,
        &edit(new_activity(&serde_json::json!({}))),
        &mut next,
    )
    .unwrap();
    assert_eq!(line, "N1 added, 16h long, with no links yet");
    let added = activity(&s, 100);
    assert_eq!(added.code, "N1");
    assert_eq!(added.remaining_duration, Minutes::new(960));
    assert_eq!(next.activity, 101);
    apply(&mut s, &edit(add("A1", "N1")), &mut next).unwrap();
    assert!(s.validate().is_empty(), "{:?}", s.validate());
}

#[test]
fn an_added_activity_falls_back_on_the_project_calendar() {
    let mut s = chain();
    s.project.default_calendar = Some(CalendarId(2));
    let json = serde_json::json!({
        "op": "add_activity", "code": "N1", "name": "New", "wbs": "ROOT", "duration": "1d"
    });
    run(&mut s, json).unwrap();
    assert_eq!(activity(&s, 100).original_duration, Minutes::new(600));
}

#[test]
fn add_refusals() {
    let s = chain();
    let extra = |json: serde_json::Value| new_activity(&json);
    refused(
        &s,
        extra(serde_json::json!({"code": "A1"})),
        "duplicate-activity",
    );
    refused(
        &s,
        extra(serde_json::json!({"code": " N"})),
        "invalid-activity-id",
    );
    refused(
        &s,
        extra(serde_json::json!({"code": "x".repeat(41)})),
        "invalid-activity-id",
    );
    refused(&s, extra(serde_json::json!({"wbs": "NOPE"})), "no-such-wbs");
    refused(&s, extra(serde_json::json!({"name": ""})), "empty-name");
    refused(
        &s,
        extra(serde_json::json!({"calendar": "NOPE"})),
        "no-such-calendar",
    );
    refused(
        &s,
        extra(serde_json::json!({"duration": "-1d"})),
        "negative-duration",
    );
    refused(
        &s,
        extra(serde_json::json!({"activity_type": "finish-milestone"})),
        "no-duration",
    );
    let no_duration = serde_json::json!({
        "op": "add_activity", "code": "N1", "name": "New", "wbs": "ROOT", "calendar": "Five day"
    });
    refused(&s, no_duration, "needs-duration");
    let no_calendar = serde_json::json!({
        "op": "add_activity", "code": "N1", "name": "New", "wbs": "ROOT", "duration": "1d"
    });
    refused(&s, no_calendar, "needs-calendar");
}

#[test]
fn deleting_says_what_it_opens() {
    let mut s = chain();
    let json = serde_json::json!({"op": "delete_activity", "activity": "A2"});
    let line = run(&mut s, json.clone()).unwrap();
    assert_eq!(
        line,
        "A2 deleted, with 2 links and 0 assignments; no predecessor now: A3; no successor now: A1"
    );
    assert!(!s.activities.contains_key(&ActivityId(2)));
    assert_eq!(s.relationships.len(), 1);
    assert_eq!(s.validate(), []);
    refused(&s, json, "no-such-activity");
}

#[test]
fn deleting_finished_work_says_its_actuals_are_gone() {
    let mut s = chain();
    s.activities.get_mut(&ActivityId(1)).unwrap().actual_start =
        Some("2026-01-05 08:00".parse().unwrap());
    let json = serde_json::json!({"op": "delete_activity", "activity": "A1"});
    let line = run(&mut s, json).unwrap();
    assert!(
        line.starts_with("A1 deleted, with 1 link and 0 assignments; its actual dates are gone"),
        "{line}"
    );
}

#[test]
fn a_wbs_is_found_by_id_or_exact_code() {
    let mut s = chain();
    let mut twin = s.wbs[&WbsId(1)].clone();
    twin.id = WbsId(2);
    twin.code = "root".to_owned();
    s.wbs.insert(WbsId(2), twin.clone());
    let mut third = twin;
    third.id = WbsId(3);
    third.code = "Root".to_owned();
    s.wbs.insert(WbsId(3), third);
    refused(
        &s,
        new_activity(&serde_json::json!({"wbs": "rOOT"})),
        "ambiguous-wbs",
    );
    run(&mut s, new_activity(&serde_json::json!({"wbs": "root"}))).unwrap();
    assert_eq!(activity(&s, 100).wbs, WbsId(2));
    run(
        &mut s,
        new_activity(&serde_json::json!({"wbs": "3", "code": "N2"})),
    )
    .unwrap();
    assert_eq!(activity(&s, 101).wbs, WbsId(3));
}

#[test]
fn a_new_id_avoids_what_flags_use_as_separators() {
    let s = chain();
    refused(
        &s,
        new_activity(&serde_json::json!({"code": "A=B"})),
        "invalid-activity-id",
    );
}

fn at_data_date(mut s: Schedule) -> Schedule {
    s.project.data_date = Some("2026-02-02 08:00".parse().unwrap());
    s
}

fn progress(extra: &serde_json::Value) -> serde_json::Value {
    let mut json = serde_json::json!({"op": "set_progress", "activity": "A2"});
    for (key, value) in extra.as_object().unwrap() {
        json[key] = value.clone();
    }
    json
}

#[test]
fn a_start_starts_work_and_a_finish_finishes_it() {
    let mut s = at_data_date(chain());
    let line = run(
        &mut s,
        progress(&serde_json::json!({
            "actual_start": "2026-01-12 08:00", "remaining": "4h", "percent": 50.0
        })),
    )
    .unwrap();
    assert_eq!(line, "A2 is in progress, with 4h remaining");
    assert_eq!(activity(&s, 2).status, ActivityStatus::InProgress);
    assert_eq!(activity(&s, 2).physical_percent_complete, Some(50.0));
    s.activities
        .get_mut(&ActivityId(2))
        .unwrap()
        .stored
        .total_float = Some(Minutes::new(60));
    run(
        &mut s,
        progress(&serde_json::json!({"actual_finish": "2026-01-13 17:00"})),
    )
    .unwrap();
    let done = activity(&s, 2);
    assert_eq!(done.status, ActivityStatus::Completed);
    assert_eq!(done.remaining_duration, Minutes::ZERO);
    assert_eq!(done.physical_percent_complete, Some(100.0));
    assert_eq!(done.stored.total_float, None);
    assert_eq!(s.validate(), []);
}

#[test]
fn a_milestone_takes_both_actuals_from_one() {
    let mut s = at_data_date(chain());
    set_type(&mut s, 2, ActivityType::FinishMilestone);
    run(
        &mut s,
        progress(&serde_json::json!({"actual_finish": "2026-01-13 17:00"})),
    )
    .unwrap();
    let done = activity(&s, 2);
    assert_eq!(done.status, ActivityStatus::Completed);
    assert_eq!(done.actual_start, done.actual_finish);
}

#[test]
fn a_milestone_finishes_from_either_actual_and_they_must_match() {
    let mut s = at_data_date(chain());
    set_type(&mut s, 2, ActivityType::StartMilestone);
    let both = serde_json::json!({
        "actual_start": "2026-01-13 08:00", "actual_finish": "2026-01-14 08:00"
    });
    refused(&s, progress(&both), "milestone-dates");
    refused(
        &s,
        progress(&serde_json::json!({"remaining": "4h"})),
        "no-duration",
    );
    let line = run(
        &mut s,
        progress(&serde_json::json!({"actual_start": "2026-01-13 08:00"})),
    );
    assert_eq!(line.unwrap(), "A2 is completed, with 0h remaining");
    assert_eq!(activity(&s, 2).actual_finish, activity(&s, 2).actual_start);
}

#[test]
fn finishing_clears_the_forecast_on_finished_work() {
    let mut s = at_data_date(chain());
    let stored = &mut s.activities.get_mut(&ActivityId(2)).unwrap().stored;
    stored.remaining_early_start = Some("2026-02-02 08:00".parse().unwrap());
    stored.remaining_early_finish = Some("2026-02-03 17:00".parse().unwrap());
    run(
        &mut s,
        progress(&serde_json::json!({
            "actual_start": "2026-01-12 08:00", "actual_finish": "2026-01-13 17:00"
        })),
    )
    .unwrap();
    assert_eq!(activity(&s, 2).stored.remaining_early_start, None);
    assert_eq!(activity(&s, 2).stored.remaining_early_finish, None);
}

#[test]
fn clearing_progress_takes_suspension_with_it_and_leaves_spans_alone() {
    let mut s = at_data_date(chain());
    let a = s.activities.get_mut(&ActivityId(2)).unwrap();
    a.status = ActivityStatus::InProgress;
    a.actual_start = Some("2026-01-12 08:00".parse().unwrap());
    a.suspend = Some("2026-01-13 08:00".parse().unwrap());
    let clear = serde_json::json!({"op": "clear_progress", "activity": "A2"});
    let mut spanning = s.clone();
    set_type(&mut spanning, 2, ActivityType::LevelOfEffort);
    refused(&spanning, clear.clone(), "span-progress");
    run(&mut s, clear).unwrap();
    assert_eq!(activity(&s, 2).suspend, None);
}

#[test]
fn a_percent_on_finished_work_is_refused() {
    let mut s = at_data_date(chain());
    let done = serde_json::json!({
        "actual_start": "2026-01-12 08:00", "actual_finish": "2026-01-13 17:00", "percent": 60.0
    });
    refused(&s, progress(&done), "finished-work");
    let mut full = done;
    full["percent"] = 100.0.into();
    run(&mut s, progress(&full)).unwrap();
    refused(
        &s,
        progress(&serde_json::json!({"percent": 60.0})),
        "finished-work",
    );
}

#[test]
fn percent_or_remaining_alone_checks_no_dates() {
    let mut s = chain();
    run(&mut s, progress(&serde_json::json!({"percent": 50.0}))).unwrap();
    run(&mut s, progress(&serde_json::json!({"remaining": "4h"}))).unwrap();
    assert_eq!(activity(&s, 2).physical_percent_complete, Some(50.0));
    let a = s.activities.get_mut(&ActivityId(2)).unwrap();
    a.status = ActivityStatus::Completed;
    a.actual_finish = Some("2026-01-13 17:00".parse().unwrap());
    a.remaining_duration = Minutes::ZERO;
    run(&mut s, progress(&serde_json::json!({"percent": 100.0}))).unwrap();
}

#[test]
fn work_in_progress_needs_something_left_to_do() {
    let mut s = at_data_date(chain());
    s.activities
        .get_mut(&ActivityId(2))
        .unwrap()
        .remaining_duration = Minutes::ZERO;
    refused(
        &s,
        progress(&serde_json::json!({"actual_start": "2026-01-12 08:00"})),
        "finished-work",
    );
}

#[test]
fn progress_refusals() {
    let s = at_data_date(chain());
    let check = |extra: serde_json::Value, code: &str| refused(&s, progress(&extra), code);
    check(serde_json::json!({}), "nothing-to-change");
    check(
        serde_json::json!({"actual_start": "2026-02-03 08:00"}),
        "actual-after-data-date",
    );
    check(
        serde_json::json!({
            "actual_start": "2026-01-05 08:00", "actual_finish": "2026-02-03 17:00"
        }),
        "actual-after-data-date",
    );
    check(
        serde_json::json!({"actual_finish": "2026-01-05 17:00"}),
        "finish-needs-start",
    );
    check(
        serde_json::json!({
            "actual_start": "2026-01-07 08:00", "actual_finish": "2026-01-05 17:00"
        }),
        "finish-before-start",
    );
    check(serde_json::json!({"percent": 101.0}), "invalid-percent");
    check(serde_json::json!({"percent": -1.0}), "invalid-percent");
    check(serde_json::json!({"remaining": "-1h"}), "negative-duration");
    check(
        serde_json::json!({"actual_start": "2026-01-05 08:00", "remaining": "0h"}),
        "finished-work",
    );
    check(
        serde_json::json!({
            "actual_start": "2026-01-05 08:00", "actual_finish": "2026-01-07 17:00",
            "remaining": "1d"
        }),
        "finished-work",
    );
    let mut spans = s.clone();
    set_type(&mut spans, 2, ActivityType::LevelOfEffort);
    refused(
        &spans,
        progress(&serde_json::json!({"percent": 50.0})),
        "span-progress",
    );
    let bare = chain();
    refused(
        &bare,
        progress(&serde_json::json!({"actual_start": "2026-01-05 08:00"})),
        "no-data-date",
    );
}

#[test]
fn progress_can_be_taken_away() {
    let mut s = at_data_date(chain());
    let clear = serde_json::json!({"op": "clear_progress", "activity": "A2"});
    refused(&s, clear.clone(), "nothing-to-change");
    run(
        &mut s,
        progress(&serde_json::json!({"actual_start": "2026-01-12 08:00", "remaining": "4h"})),
    )
    .unwrap();
    let line = run(&mut s, clear).unwrap();
    assert_eq!(line, "A2 is not started, with 8h remaining");
    assert_eq!(activity(&s, 2).actual_start, None);
    assert_eq!(activity(&s, 2).status, ActivityStatus::NotStarted);
}

fn holiday() -> serde_json::Value {
    serde_json::json!({
        "op": "add_calendar_exception", "calendar": "five DAY", "date": "2026-02-02"
    })
}

fn date(text: &str) -> totalfloat_model::Date {
    text.parse().unwrap()
}

#[test]
fn a_date_becomes_a_holiday_and_goes_back() {
    let mut s = chain();
    assert_eq!(
        run(&mut s, holiday()).unwrap(),
        "Five day is non-work on 2026-02-02"
    );
    let calendar = &s.calendars[&CalendarId(1)];
    assert_eq!(calendar.exceptions[&date("2026-02-02")], []);
    let back = serde_json::json!({
        "op": "remove_calendar_exception", "calendar": "1", "date": "2026-02-02"
    });
    run(&mut s, back.clone()).unwrap();
    assert!(s.calendars[&CalendarId(1)].exceptions.is_empty());
    refused(&s, back, "no-such-exception");
}

#[test]
fn a_date_or_day_takes_shifts() {
    let mut s = chain();
    let worked = serde_json::json!({
        "op": "add_calendar_exception", "calendar": "Five day", "date": "2026-02-07",
        "shifts": ["13:00-17:00", "08:00-12:00"]
    });
    run(&mut s, worked).unwrap();
    assert_eq!(
        s.calendars[&CalendarId(1)].exceptions[&date("2026-02-07")].len(),
        2
    );
    let saturday = serde_json::json!({
        "op": "set_work_week", "calendar": "Five day", "day": "saturday",
        "shifts": ["08:00-24:00"]
    });
    run(&mut s, saturday).unwrap();
    assert_eq!(
        s.calendars[&CalendarId(1)].work_week.day(Weekday::Saturday)[0].duration(),
        Minutes::new(960)
    );
}

#[test]
fn calendar_change_refusals() {
    let mut s = chain();
    let bad = |shifts: serde_json::Value| {
        serde_json::json!({
            "op": "set_work_week", "calendar": "Five day", "day": "monday", "shifts": shifts
        })
    };
    refused(&s, bad(serde_json::json!(["soon"])), "invalid-shift");
    refused(&s, bad(serde_json::json!(["12:00-08:00"])), "invalid-shift");
    refused(
        &s,
        bad(serde_json::json!(["08:00-12:00", "11:00-13:00"])),
        "invalid-shift",
    );
    let mut nameless = holiday();
    nameless["calendar"] = "NOPE".into();
    refused(&s, nameless, "no-such-calendar");
    let mut undated = holiday();
    undated["date"] = "soon".into();
    refused(&s, undated, "invalid-date");
    let off = serde_json::json!({"op": "set_work_week", "calendar": "Five day", "day": "monday"});
    refused(&s, off, "no-working-time");
    run(&mut s, holiday()).unwrap();
    let built = s.calendars.get_mut(&CalendarId(2)).unwrap();
    built.base = Some(CalendarId(1));
    // Reading flattens what a calendar inherits into its own exceptions.
    built.exceptions.insert(date("2026-02-02"), Vec::new());
    let inherited = serde_json::json!({
        "op": "remove_calendar_exception", "calendar": "Ten hour", "date": "2026-02-02"
    });
    refused(&s, inherited.clone(), "inherited-exception");
    // A date the derived calendar sets for itself is its own to remove.
    s.calendars
        .get_mut(&CalendarId(2))
        .unwrap()
        .exceptions
        .insert(date("2026-02-02"), parse_shift("08:00-12:00"));
    run(&mut s, inherited).unwrap();
}

/// Copy a five-day calendar, give it a ten-hour Monday to Thursday,
/// and a `5d` activity on it is 40 hours, because hours per day are kept
/// apart from the week (TF-033). The report says so, and once hours per
/// day are set to 10 it's 50 hours.
#[test]
fn hours_per_day_stay_when_the_week_changes_until_set() {
    let mut s = chain();
    let five_day = s.calendars.get_mut(&CalendarId(1)).unwrap();
    for day in &Weekday::ALL[1..6] {
        *five_day.work_week.day_mut(*day) =
            [parse_shift("08:00-12:00"), parse_shift("13:00-17:00")].concat();
    }
    run(
        &mut s,
        serde_json::json!({"op": "add_calendar", "name": "Long days", "copy_of": "Five day"}),
    )
    .unwrap();
    let mut reports = Vec::new();
    for day in ["monday", "tuesday", "wednesday", "thursday"] {
        reports.push(
            run(
                &mut s,
                serde_json::json!({
                    "op": "set_work_week", "calendar": "Long days", "day": day,
                    "shifts": ["07:00-12:00", "13:00-18:00"]
                }),
            )
            .unwrap(),
        );
    }
    reports.push(
        run(
            &mut s,
            serde_json::json!({"op": "set_work_week", "calendar": "Long days", "day": "friday"}),
        )
        .unwrap(),
    );
    // Only the last change leaves every working day the same length.
    assert!(reports[..4].iter().all(|r| !r.contains("still counts")));
    assert_eq!(
        reports[4],
        "Long days work week changed on Friday; its working days are 10 hours, but a day still counts as 8 (set its hours per day to change that)"
    );
    let add = |code: &str| {
        serde_json::json!({
            "op": "add_activity", "code": code, "name": "Pour", "wbs": "ROOT",
            "calendar": "Long days", "duration": "5d"
        })
    };
    run(&mut s, add("N1")).unwrap();
    let set = serde_json::json!({"op": "set_hours_per_day", "calendar": "Long days", "hours": 10});
    assert_eq!(
        run(&mut s, set).unwrap(),
        "Long days counts a day as 10 hours"
    );
    run(&mut s, add("N2")).unwrap();
    let duration = |code: &str| {
        s.activities
            .values()
            .find(|a| a.code == code)
            .unwrap()
            .original_duration
    };
    assert_eq!(duration("N1"), Minutes::new(40 * 60));
    assert_eq!(duration("N2"), Minutes::new(50 * 60));
    for bad in [0.0, -1.0, 24.5, 7.51] {
        refused(
            &s,
            serde_json::json!({"op": "set_hours_per_day", "calendar": "Long days", "hours": bad}),
            "bad-hours-per-day",
        );
    }
    refused(
        &s,
        serde_json::json!({"op": "set_hours_per_day", "calendar": "NOPE", "hours": 8}),
        "no-such-calendar",
    );
    let set = serde_json::json!({"op": "set_hours_per_day", "calendar": "Long days", "hours": 7.5});
    run(&mut s, set).unwrap();
    assert_eq!(
        s.calendars
            .values()
            .find(|c| c.name == "Long days")
            .unwrap()
            .hours_per_period
            .day,
        Some(Minutes::new(450))
    );
}

fn parse_shift(text: &str) -> Vec<WorkInterval> {
    super::parse_shifts(&[text.to_owned()]).unwrap()
}

#[test]
fn a_calendar_is_added_as_a_copy_for_this_project() {
    let mut s = chain();
    let add = serde_json::json!({"op": "add_calendar", "name": "Winter", "copy_of": "Five day"});
    assert_eq!(
        run(&mut s, add.clone()).unwrap(),
        "Winter added, copied from Five day"
    );
    let added = &s.calendars[&CalendarId(100)];
    assert_eq!(added.kind, CalendarType::Project);
    assert_eq!(added.project, Some(ProjectId(1)));
    assert_eq!(added.source, None);
    assert_eq!(s.validate(), []);
    refused(&s, add, "duplicate-calendar");
    let nowhere = serde_json::json!({"op": "add_calendar", "name": "Spring"});
    refused(&s, nowhere.clone(), "needs-calendar");
    s.project.default_calendar = Some(CalendarId(2));
    run(&mut s, nowhere).unwrap();
}

fn with_codes() -> Schedule {
    let mut s = chain();
    s.activity_code_types.insert(
        ActivityCodeTypeId(1),
        ActivityCodeType {
            id: ActivityCodeTypeId(1),
            name: "Trade".to_owned(),
            scope: CodeScope::Project,
            project: Some(ProjectId(1)),
            max_length: Some(4),
            sequence: 1,
            source: None,
        },
    );
    for (id, value, description) in [(1, "ELEC", "Electrical"), (2, "CIV", "Civil")] {
        s.activity_codes.insert(
            ActivityCodeId(id),
            ActivityCode {
                id: ActivityCodeId(id),
                code_type: ActivityCodeTypeId(1),
                parent: None,
                value: value.to_owned(),
                description: description.to_owned(),
                sequence: i64::try_from(id).unwrap(),
                source: None,
            },
        );
    }
    s
}

fn code_edit(value: &str) -> serde_json::Value {
    serde_json::json!({"op": "set_code", "activity": "A2", "code_type": "trade", "value": value})
}

#[test]
fn a_code_is_set_replaced_and_cleared() {
    let mut s = with_codes();
    run(&mut s, code_edit("elec")).unwrap();
    assert_eq!(
        activity(&s, 2).codes[&ActivityCodeTypeId(1)],
        ActivityCodeId(1)
    );
    run(&mut s, code_edit("Civil")).unwrap();
    assert_eq!(
        activity(&s, 2).codes[&ActivityCodeTypeId(1)],
        ActivityCodeId(2)
    );
    assert_eq!(s.validate(), []);
    let clear = serde_json::json!({"op": "clear_code", "activity": "A2", "code_type": "Trade"});
    run(&mut s, clear.clone()).unwrap();
    assert!(activity(&s, 2).codes.is_empty());
    refused(&s, clear, "no-code");
}

#[test]
fn a_code_value_is_added_and_usable_at_once() {
    let mut s = with_codes();
    let mut next = ids();
    let add = edit(serde_json::json!({
        "op": "add_code_value", "code_type": "Trade", "value": "MECH", "description": "Mechanical"
    }));
    apply(&mut s, &add, &mut next).unwrap();
    assert_eq!(s.activity_codes[&ActivityCodeId(100)].sequence, 3);
    assert_eq!(next.code, 101);
    run(&mut s, code_edit("MECH")).unwrap();
    assert_eq!(s.validate(), []);
}

#[test]
fn code_refusals() {
    let s = with_codes();
    refused(&s, code_edit("NOPE"), "no-such-code");
    let mut unknown = code_edit("ELEC");
    unknown["code_type"] = "Phase".into();
    refused(&s, unknown, "no-such-code");
    let add = |value: &str| serde_json::json!({"op": "add_code_value", "code_type": "Trade", "value": value});
    refused(&s, add("elec"), "duplicate-code");
    refused(&s, add("TOOLONG"), "code-too-long");
    refused(&s, add(" "), "invalid-code-value");
    refused(&s, add(" MEC"), "invalid-code-value");
    let mut twice = s;
    twice
        .activity_codes
        .get_mut(&ActivityCodeId(2))
        .unwrap()
        .description = "Electrical".to_owned();
    refused(&twice, code_edit("Electrical"), "ambiguous-code");
}

fn with_crew() -> Schedule {
    let mut s = chain();
    s.resources.insert(
        ResourceId(1),
        Resource {
            id: ResourceId(1),
            short_name: "CREW".to_owned(),
            name: "Electrical Crew".to_owned(),
            kind: ResourceType::Labor,
            calendar: None,
            parent: None,
            default_units_per_hour: None,
            default_cost_follows_units: false,
            rates: Vec::new(),
            roles: Vec::new(),
            udfs: std::collections::BTreeMap::new(),
            source: None,
        },
    );
    s
}

fn staff(op: &str, resource: &str) -> serde_json::Value {
    serde_json::json!({"op": op, "activity": "A2", "resource": resource})
}

#[test]
fn work_with_resources_can_t_become_a_milestone_or_be_staffed_once_finished() {
    let mut s = with_crew();
    run(&mut s, staff("assign_resource", "CREW")).unwrap();
    let json = serde_json::json!({"op": "set_type", "activity": "A2", "activity_type": "finish-milestone"});
    refused(&s, json, "no-resources-here");
    let mut done = with_crew();
    done.activities.get_mut(&ActivityId(2)).unwrap().status = ActivityStatus::Completed;
    refused(&done, staff("assign_resource", "CREW"), "no-resources-here");
    let negative = serde_json::json!({
        "op": "assign_resource", "activity": "A2", "resource": "CREW", "units": -5
    });
    refused(&with_crew(), negative, "invalid-units");
}

/// Two rates: 50 from 2026-01-01, 80 from 2026-06-01. Work starting in March
/// takes 50, and work starting in July takes 80.
#[test]
fn a_new_assignment_takes_the_rate_in_force_when_its_work_starts() {
    use totalfloat_model::{ResourceRate, ResourceRateId};
    let rate = |id: u64, from: &str, price: f64| ResourceRate {
        id: ResourceRateId(id),
        effective: from.parse().ok(),
        prices: [Some(price), None, None, None, None],
        max_units_per_hour: None,
        source: None,
    };
    for (starts, price, cost) in [
        ("2026-03-02 08:00", 50.0, 800.0),
        ("2026-07-01 08:00", 80.0, 1280.0),
    ] {
        let mut s = with_crew();
        let crew = s.resources.get_mut(&ResourceId(1)).unwrap();
        crew.rates = vec![
            rate(1, "2026-01-01 00:00", 50.0),
            rate(2, "2026-06-01 00:00", 80.0),
        ];
        crew.default_cost_follows_units = true;
        s.activities.get_mut(&ActivityId(2)).unwrap().planned_start = starts.parse().ok();
        let json = serde_json::json!({
            "op": "assign_resource", "activity": "A2", "resource": "CREW", "units": 16
        });
        let line = run(&mut s, json).unwrap();
        let held = &s.assignments[&AssignmentId(100)].amounts;
        assert_eq!(held.price_per_unit, Some(price), "{line}");
        // 16 units at the price.
        assert_eq!(held.budgeted_cost, Some(cost));
        assert!(line.ends_with(&format!("at {price} a unit")), "{line}");
    }
}

#[test]
fn a_new_assignment_takes_the_resource_s_cost_link() {
    let mut s = with_crew();
    s.resources
        .get_mut(&ResourceId(1))
        .unwrap()
        .default_cost_follows_units = true;
    run(&mut s, staff("assign_resource", "CREW")).unwrap();
    assert!(s.assignments[&AssignmentId(100)].amounts.cost_follows_units);
}

#[test]
fn a_resource_goes_on_and_comes_off() {
    let mut s = with_crew();
    let line = run(&mut s, staff("assign_resource", "electrical crew")).unwrap();
    assert_eq!(line, "A2 assigned electrical crew, with no units");
    assert_eq!(
        s.assignments[&AssignmentId(100)].resource,
        Some(ResourceId(1))
    );
    assert!(!s.assignments[&AssignmentId(100)].amounts.cost_follows_units);
    assert_eq!(s.validate(), []);
    refused(&s, staff("assign_resource", "crew"), "already-assigned");
    run(&mut s, staff("unassign_resource", "CREW")).unwrap();
    assert!(s.assignments.is_empty());
    refused(&s, staff("unassign_resource", "CREW"), "not-assigned");
}

#[test]
fn unassigning_takes_every_row_for_that_resource() {
    let mut s = with_crew();
    run(&mut s, staff("assign_resource", "CREW")).unwrap();
    let mut twin = s.assignments[&AssignmentId(100)].clone();
    twin.id = AssignmentId(101);
    s.assignments.insert(twin.id, twin);
    run(&mut s, staff("unassign_resource", "CREW")).unwrap();
    assert!(s.assignments.is_empty());
}

#[test]
fn resource_dependent_work_keeps_one_calendar_and_some_resource() {
    let mut s = with_crew();
    let mut other = s.resources[&ResourceId(1)].clone();
    other.id = ResourceId(2);
    other.short_name = "NIGHT".to_owned();
    other.calendar = Some(CalendarId(2));
    s.resources.insert(ResourceId(2), other);
    set_type(&mut s, 2, ActivityType::ResourceDependent);
    run(&mut s, staff("assign_resource", "CREW")).unwrap();
    refused(&s, staff("assign_resource", "NIGHT"), "resource-calendar");
    refused(&s, staff("unassign_resource", "CREW"), "resource-calendar");
}

#[test]
fn resource_refusals() {
    let mut s = with_crew();
    refused(&s, staff("assign_resource", "NOBODY"), "no-such-resource");
    set_type(&mut s, 2, ActivityType::FinishMilestone);
    refused(&s, staff("assign_resource", "CREW"), "no-resources-here");
}

fn add_node(code: &str, parent: &str) -> serde_json::Value {
    serde_json::json!({"op": "add_wbs", "code": code, "name": "Authored", "parent": parent})
}

#[test]
fn a_wbs_node_is_added_renamed_and_moved() {
    let mut s = chain();
    run(&mut s, add_node("CIV", "ROOT")).unwrap();
    run(&mut s, add_node("ELE", "ROOT")).unwrap();
    assert_eq!(s.wbs[&WbsId(100)].parent, Some(WbsId(1)));
    assert_eq!(s.wbs[&WbsId(101)].sequence, 1);
    let rename = serde_json::json!({
        "op": "rename_wbs", "wbs": "CIV", "code": "CIVIL", "name": "Civil Works"
    });
    run(&mut s, rename).unwrap();
    assert_eq!(s.wbs[&WbsId(100)].code, "CIVIL");
    let moved = serde_json::json!({"op": "move_wbs", "wbs": "ELE", "parent": "CIVIL"});
    run(&mut s, moved).unwrap();
    assert_eq!(s.wbs[&WbsId(101)].parent, Some(WbsId(100)));
    let work = serde_json::json!({"op": "set_wbs", "activity": "A2", "wbs": "ELE"});
    run(&mut s, work).unwrap();
    assert_eq!(activity(&s, 2).wbs, WbsId(101));
    assert_eq!(s.validate(), []);
}

#[test]
fn wbs_refusals() {
    let mut s = chain();
    run(&mut s, add_node("CIV", "ROOT")).unwrap();
    run(&mut s, add_node("SITE", "CIV")).unwrap();
    refused(&s, add_node("civ", "ROOT"), "duplicate-wbs");
    refused(&s, add_node(" X", "ROOT"), "invalid-wbs-code");
    refused(&s, add_node("X", "NOPE"), "no-such-wbs");
    let looped = serde_json::json!({"op": "move_wbs", "wbs": "CIV", "parent": "SITE"});
    refused(&s, looped, "circular-wbs");
    let itself = serde_json::json!({"op": "move_wbs", "wbs": "CIV", "parent": "CIV"});
    refused(&s, itself, "circular-wbs");
    let root = serde_json::json!({"op": "rename_wbs", "wbs": "ROOT", "name": "New"});
    refused(&s, root, "project-node");
    let nothing = serde_json::json!({"op": "rename_wbs", "wbs": "CIV"});
    refused(&s, nothing, "nothing-to-change");
    let stays = serde_json::json!({"op": "move_wbs", "wbs": "SITE", "parent": "CIV"});
    refused(&s, stays, "nothing-to-change");
    refused(&s, add_node("A=B", "ROOT"), "invalid-wbs-code");
}

#[test]
fn a_repeated_code_is_refused_with_the_ids_to_use() {
    let mut s = chain();
    run(&mut s, add_node("CIV", "ROOT")).unwrap();
    run(&mut s, add_node("SITE", "CIV")).unwrap();
    run(&mut s, add_node("ELE", "ROOT")).unwrap();
    run(&mut s, add_node("SITE", "ELE")).unwrap();
    let error = apply(
        &mut s,
        &edit(serde_json::json!({"op": "rename_wbs", "wbs": "SITE", "name": "New"})),
        &mut ids(),
    )
    .unwrap_err();
    assert_eq!(error.code, "ambiguous-wbs");
    assert!(error.hint.contains("101"), "{}", error.hint);
}

#[test]
fn a_loop_already_in_the_file_doesn_t_hang_a_move() {
    let mut s = chain();
    run(&mut s, add_node("A", "ROOT")).unwrap();
    run(&mut s, add_node("B", "A")).unwrap();
    run(&mut s, add_node("X", "ROOT")).unwrap();
    s.wbs.get_mut(&WbsId(100)).unwrap().parent = Some(WbsId(101));
    let moved = serde_json::json!({"op": "move_wbs", "wbs": "X", "parent": "A"});
    run(&mut s, moved).unwrap();
}

#[test]
fn a_numeric_code_means_that_code_before_any_id() {
    let mut s = chain();
    run(&mut s, add_node("1", "ROOT")).unwrap();
    let work = serde_json::json!({"op": "set_wbs", "activity": "A2", "wbs": "1"});
    run(&mut s, work).unwrap();
    assert_eq!(activity(&s, 2).wbs, WbsId(100));
}

#[test]
fn delete_refusals() {
    let mut s = chain();
    let json = serde_json::json!({"op": "delete_activity", "activity": "A2"});
    s.project.options.float_paths.end_activity = Some(ActivityId(2));
    refused(&s, json.clone(), "float-path-end");
    s.project.options.float_paths.end_activity = None;
    s.relationships
        .get_mut(&RelationshipId(1))
        .unwrap()
        .predecessor_project = ProjectId(2);
    refused(&s, json, "other-project");
}

fn add(from: &str, to: &str) -> serde_json::Value {
    serde_json::json!({"op": "add_link", "predecessor": from, "successor": to})
}

#[test]
fn a_link_is_added_above_every_id_in_the_file() {
    let mut s = chain();
    let mut next = ids();
    let edit = edit(serde_json::json!({
        "op": "add_link", "predecessor": "A1", "successor": "A3", "link_type": "SS", "lag": "2d"
    }));
    apply(&mut s, &edit, &mut next).unwrap();
    let link = &s.relationships[&RelationshipId(100)];
    assert_eq!(link.kind, RelationshipType::StartToStart);
    assert_eq!(link.lag, Minutes::new(960));
    assert_eq!(next.relationship, 101);
}

#[test]
fn lag_days_use_the_calendar_lag_runs_on() {
    let mut s = chain();
    s.project.options.lag_calendar = totalfloat_model::LagCalendar::Successor;
    s.activities.get_mut(&ActivityId(3)).unwrap().calendar = CalendarId(2);
    let json = serde_json::json!({
        "op": "add_link", "predecessor": "A1", "successor": "A3", "lag": "1d"
    });
    run(&mut s, json).unwrap();
    assert_eq!(s.relationships[&RelationshipId(100)].lag, Minutes::new(600));
}

#[test]
fn link_refusals_name_the_problem() {
    let mut s = chain();
    refused(&s, add("A1", "A1"), "self-link");
    refused(&s, add("A1", "A2"), "duplicate-link");
    refused(&s, add("A1", "NOPE"), "no-such-activity");
    let error = apply(&mut s, &edit(add("A4", "A1")), &mut ids()).unwrap_err();
    assert_eq!(error.code, "circular-logic");
    assert!(
        error.message.contains("A1 to A2 to A3 to A4 back to A1"),
        "{}",
        error.message
    );
}

#[test]
fn a_second_type_between_the_same_activities_is_allowed_and_then_named() {
    let mut s = chain();
    let json = serde_json::json!({
        "op": "add_link", "predecessor": "A1", "successor": "A2", "link_type": "SS"
    });
    run(&mut s, json).unwrap();
    let remove = serde_json::json!({"op": "remove_link", "predecessor": "A1", "successor": "A2"});
    refused(&s, remove.clone(), "ambiguous-link");
    let mut named = remove;
    named["link_type"] = "SS".into();
    run(&mut s, named).unwrap();
    assert_eq!(s.relationships.len(), 3);
}

#[test]
fn a_link_is_removed() {
    let mut s = chain();
    let json = serde_json::json!({"op": "remove_link", "predecessor": "A2", "successor": "A3"});
    run(&mut s, json.clone()).unwrap();
    assert!(!s.relationships.contains_key(&RelationshipId(2)));
    refused(&s, json, "no-such-link");
}

#[test]
fn a_link_to_another_project_stays() {
    let mut s = chain();
    s.relationships
        .get_mut(&RelationshipId(1))
        .unwrap()
        .predecessor_project = ProjectId(2);
    let json = serde_json::json!({"op": "remove_link", "predecessor": "A1", "successor": "A2"});
    refused(&s, json, "other-project");
}

/// `chain` as project 1, and B10 to B11 as project 2 on a ten-hour calendar.
fn two_projects() -> Vec<Schedule> {
    let mut other = Schedule::new(Project::new(ProjectId(2), "OTHER"));
    other
        .calendars
        .insert(CalendarId(2), calendar(2, "Ten hour", 10));
    for n in 10..=11 {
        let code = format!("B{n}");
        let name = format!("Other {n}");
        let mut a = Activity::new(
            ActivityId(n),
            ProjectId(2),
            WbsId(2),
            CalendarId(2),
            code,
            name,
        );
        a.original_duration = Minutes::new(600);
        other.activities.insert(a.id, a);
    }
    other.relationships.insert(
        RelationshipId(4),
        Relationship {
            id: RelationshipId(4),
            predecessor: ActivityId(10),
            successor: ActivityId(11),
            predecessor_project: ProjectId(2),
            successor_project: ProjectId(2),
            kind: RelationshipType::FinishToStart,
            lag: Minutes::ZERO,
            source: None,
        },
    );
    vec![chain(), other]
}

fn between(schedules: &mut [Schedule], json: serde_json::Value) -> Result<String, &'static str> {
    let edit: Edit = serde_json::from_value(json).unwrap();
    let mut low = NextIds {
        relationship: 5,
        ..ids()
    };
    apply_between(schedules, &edit, (0, 1), &mut low).map_err(|e| e.code)
}

#[test]
fn a_link_between_projects_is_in_both() {
    let mut schedules = two_projects();
    let json = serde_json::json!({
        "op": "add_link", "predecessor": "A1", "successor": "B10", "lag": "1d"
    });
    let said = between(&mut schedules, json).unwrap();
    assert_eq!(said, "AUTHORED:A1 to OTHER:B10 FS added, lag 8h");
    let id = RelationshipId(5);
    assert_eq!(
        schedules[0].relationships[&id],
        schedules[1].relationships[&id]
    );
    let link = &schedules[0].relationships[&id];
    assert!(schedules[0].is_external(link) && schedules[1].is_external(link));
    assert_eq!(link.predecessor_project, ProjectId(1));
    assert_eq!(link.successor_project, ProjectId(2));
}

#[test]
fn a_lag_between_projects_counts_days_by_the_successor_s_option() {
    for (theirs, ours, lag) in [
        (LagCalendar::Predecessor, LagCalendar::Successor, "lag 8h"),
        (LagCalendar::Successor, LagCalendar::Predecessor, "lag 10h"),
    ] {
        let mut schedules = two_projects();
        schedules[0].project.options.lag_calendar = ours;
        schedules[1].project.options.lag_calendar = theirs;
        let json = serde_json::json!({
            "op": "add_link", "predecessor": "A1", "successor": "B10", "lag": "1d"
        });
        let said = between(&mut schedules, json).unwrap();
        assert!(said.ends_with(lag), "{said}");
    }
}

#[test]
fn a_link_between_projects_changes_and_goes_from_both() {
    let mut schedules = two_projects();
    let add = serde_json::json!({"op": "add_link", "predecessor": "A1", "successor": "B10"});
    between(&mut schedules, add).unwrap();
    let id = RelationshipId(5);
    let change = serde_json::json!({
        "op": "change_link", "predecessor": "A1", "successor": "B10",
        "new_type": "SS", "lag": "5h"
    });
    between(&mut schedules, change).unwrap();
    for schedule in &schedules {
        let link = &schedule.relationships[&id];
        assert_eq!(link.kind, RelationshipType::StartToStart);
        assert_eq!(link.lag, Minutes::new(300));
    }
    let remove = serde_json::json!({"op": "remove_link", "predecessor": "A1", "successor": "B10"});
    between(&mut schedules, remove.clone()).unwrap();
    assert!(schedules.iter().all(|s| !s.relationships.contains_key(&id)));
    assert_eq!(between(&mut schedules, remove), Err("no-such-link"));
}

#[test]
fn links_between_projects_are_refused_as_any_link_is() {
    let mut schedules = two_projects();
    let held = schedules.clone();
    let add = serde_json::json!({"op": "add_link", "predecessor": "A1", "successor": "B10"});
    between(&mut schedules, add.clone()).unwrap();
    assert_eq!(between(&mut schedules, add), Err("duplicate-link"));
    let mut schedules = held;
    let nothing = serde_json::json!({"op": "add_link", "predecessor": "A1", "successor": "NONE"});
    assert_eq!(between(&mut schedules, nothing), Err("no-such-activity"));
    let edit = serde_json::json!({"op": "delete_activity", "activity": "A1"});
    assert_eq!(between(&mut schedules, edit), Err("not-a-link"));
    assert!(schedules.iter().all(|s| s.relationships.len() < 4));
}

/// A3 leads to B10, so B11 can't lead back to A1: the loop runs through both.
#[test]
fn a_loop_through_two_projects_is_refused() {
    let mut schedules = two_projects();
    let add = serde_json::json!({"op": "add_link", "predecessor": "A3", "successor": "B10"});
    between(&mut schedules, add).unwrap();
    schedules.swap(0, 1);
    let back = serde_json::json!({"op": "add_link", "predecessor": "B11", "successor": "A1"});
    assert_eq!(between(&mut schedules, back), Err("circular-logic"));
}

/// A1 to B10 and B11 to A3 join the projects, so A3 can't lead back to A1.
#[test]
fn a_link_inside_one_project_can_t_close_a_loop_through_another() {
    let mut schedules = two_projects();
    let out = serde_json::json!({"op": "add_link", "predecessor": "A1", "successor": "B10"});
    between(&mut schedules, out).unwrap();
    schedules.swap(0, 1);
    let back = serde_json::json!({"op": "add_link", "predecessor": "B11", "successor": "A3"});
    between(&mut schedules, back).unwrap();
    schedules.swap(0, 1);
    let edit: Edit = serde_json::from_value(add("A3", "A1")).unwrap();
    let refused = check_loop_across(&schedules, 0, &edit).map_err(|e| e.code);
    assert_eq!(refused, Err("circular-logic"));
    let fine: Edit = serde_json::from_value(add("A1", "A3")).unwrap();
    assert!(check_loop_across(&schedules, 0, &fine).is_ok());
}

#[test]
fn a_new_link_clears_both_projects_ids() {
    let mut schedules = two_projects();
    let link = schedules[1].relationships[&RelationshipId(4)].clone();
    schedules[1].relationships.remove(&RelationshipId(4));
    schedules[1].relationships.insert(
        RelationshipId(50),
        Relationship {
            id: RelationshipId(50),
            ..link
        },
    );
    let add = serde_json::json!({"op": "add_link", "predecessor": "A1", "successor": "B10"});
    between(&mut schedules, add).unwrap();
    assert!(schedules[0].relationships.contains_key(&RelationshipId(51)));
}

#[test]
fn a_link_changes_type_and_lag() {
    let mut s = chain();
    let json = serde_json::json!({
        "op": "change_link", "predecessor": "A1", "successor": "A2", "new_type": "FF", "lag": "-4h"
    });
    run(&mut s, json).unwrap();
    let link = &s.relationships[&RelationshipId(1)];
    assert_eq!(link.kind, RelationshipType::FinishToFinish);
    assert_eq!(link.lag, Minutes::new(-240));
    let nothing = serde_json::json!({"op": "change_link", "predecessor": "A1", "successor": "A2"});
    refused(&s, nothing, "nothing-to-change");
}

#[test]
fn changing_a_link_to_a_type_it_already_has_elsewhere_is_refused() {
    let mut s = chain();
    run(
        &mut s,
        serde_json::json!({
            "op": "add_link", "predecessor": "A1", "successor": "A2", "link_type": "SS"
        }),
    )
    .unwrap();
    let json = serde_json::json!({
        "op": "change_link", "predecessor": "A1", "successor": "A2", "link_type": "SS",
        "new_type": "FS"
    });
    refused(&s, json, "duplicate-link");
}

#[test]
fn json_is_strict() {
    let wrong = serde_json::json!({"op": "set_duration", "activity": "A1", "orignal": "1d"});
    assert!(serde_json::from_value::<Edit>(wrong).is_err());
    let unknown = serde_json::json!({"op": "level_resources"});
    assert!(serde_json::from_value::<Edit>(unknown).is_err());
    let lower = serde_json::json!({
        "op": "add_link", "predecessor": "A", "successor": "B", "link_type": "ss"
    });
    let Ok(Edit::AddLink { link_type, .. }) = serde_json::from_value(lower) else {
        panic!("lower case link type");
    };
    assert_eq!(link_type, Some(LinkType::SS));
}

/// Some edit to a chain of `count` activities.
fn any_edit(count: u64) -> impl Strategy<Value = Edit> {
    let code = move || (1..=count).prop_map(|n| format!("A{n}"));
    let hours = (-40i64..400).prop_map(|h| format!("{h}h").parse::<Span>().unwrap());
    let kinds = prop_oneof![
        Just(LinkType::FS),
        Just(LinkType::SS),
        Just(LinkType::FF),
        Just(LinkType::SF)
    ];
    let kinds_of_constraint = prop_oneof![
        Just(super::ConstraintKind::StartOn),
        Just(super::ConstraintKind::StartOnOrAfter),
        Just(super::ConstraintKind::FinishOnOrBefore),
        Just(super::ConstraintKind::MandatoryStart),
    ];
    prop_oneof![
        (1..=count).prop_map(|n| Edit::DeleteActivity {
            activity: format!("A{n}")
        }),
        (5..9u64).prop_map(|n| Edit::AddActivity {
            code: format!("A{n}"),
            name: "Added".to_owned(),
            wbs: "ROOT".to_owned(),
            activity_type: None,
            calendar: Some("Five day".to_owned()),
            duration: Some("8h".parse().unwrap()),
        }),
        (code(), kinds_of_constraint, any::<bool>()).prop_map(|(activity, kind, secondary)| {
            Edit::SetConstraint {
                activity,
                constraint: kind,
                date: Some("2026-02-02 08:00".to_owned()),
                secondary,
            }
        }),
        (code(), any::<bool>()).prop_map(|(activity, secondary)| Edit::ClearConstraint {
            activity,
            secondary
        }),
        (code(), hours.clone()).prop_map(|(activity, original)| Edit::SetDuration {
            activity,
            original: Some(original),
            remaining: None
        }),
        (code(), code(), kinds.clone(), hours.clone()).prop_map(
            |(predecessor, successor, link_type, lag)| Edit::AddLink {
                predecessor,
                successor,
                link_type: Some(link_type),
                lag: Some(lag)
            }
        ),
        (code(), code()).prop_map(|(predecessor, successor)| Edit::RemoveLink {
            predecessor,
            successor,
            link_type: None
        }),
        (code(), code(), kinds, hours).prop_map(|(predecessor, successor, new_type, lag)| {
            Edit::ChangeLink {
                predecessor,
                successor,
                link_type: None,
                new_type: Some(new_type),
                lag: Some(lag),
            }
        }),
    ]
}

fn has_cycle(s: &Schedule) -> bool {
    let mut left: BTreeSet<ActivityId> = s.activities.keys().copied().collect();
    loop {
        let free: Vec<ActivityId> = left
            .iter()
            .copied()
            .filter(|id| {
                !s.relationships
                    .values()
                    .any(|r| r.successor == *id && left.contains(&r.predecessor))
            })
            .collect();
        if free.is_empty() {
            return !left.is_empty();
        }
        for id in free {
            left.remove(&id);
        }
    }
}

proptest! {
    #[test]
    fn minutes_written_as_a_span_come_back_the_same(minutes in -10_000_000i64..10_000_000, day in 1i64..1440) {
        let m = totalfloat_model::Minutes::new(minutes);
        let hours = Span::hours_of(m).unwrap();
        prop_assert_eq!(hours.minutes(None), Some(m));
        let reread: Span = hours.to_string().parse().unwrap();
        prop_assert_eq!(reread, hours);
        if let Some(days) = Span::days_of(m, totalfloat_model::Minutes::new(day)) {
            prop_assert_eq!(days.minutes(Some(totalfloat_model::Minutes::new(day))), Some(m));
            let reread: Span = days.to_string().parse().unwrap();
            prop_assert_eq!(reread, days);
        }
    }

    #[test]
    fn accepted_edits_keep_a_schedule_sound(edits in prop::collection::vec(any_edit(4), 0..40)) {
        let mut s = chain();
        let mut next = ids();
        for edit in &edits {
            let before = s.clone();
            if apply(&mut s, edit, &mut next).is_err() {
                prop_assert_eq!(&s, &before);
            }
            prop_assert!(s.validate().is_empty(), "{:?}", s.validate());
            prop_assert!(!has_cycle(&s));
            for a in s.activities.values() {
                if let Some(second) = a.secondary_constraint {
                    let primary = a.primary_constraint.expect("a secondary needs a primary");
                    prop_assert!(primary.kind.allows_secondary());
                    prop_assert!(second.kind.can_be_secondary());
                }
            }
        }
    }
}

#[test]
fn a_new_code_type_takes_values_and_codes() {
    let mut s = chain();
    let json = serde_json::json!({"op": "add_code_type", "name": "Phase"});
    let line = run(&mut s, json.clone()).unwrap();
    assert_eq!(line, "Added code type Phase, for this project");
    let kind = &s.activity_code_types[&ActivityCodeTypeId(100)];
    assert_eq!(
        (kind.scope, kind.project, kind.max_length),
        (CodeScope::Project, Some(ProjectId(1)), Some(12))
    );
    refused(&s, json, "duplicate-code-type");
    refused(
        &s,
        serde_json::json!({"op": "add_code_type", "name": "PHASE", "scope": "global"}),
        "duplicate-code-type",
    );
    for bad in [
        serde_json::json!({"op": "add_code_type", "name": " Area"}),
        serde_json::json!({"op": "add_code_type", "name": "Area", "max_length": 0}),
    ] {
        refused(&s, bad, "invalid-code-type");
    }
    run(
        &mut s,
        serde_json::json!({"op": "add_code_value", "code_type": "phase", "value": "CON"}),
    )
    .unwrap();
    run(
        &mut s,
        serde_json::json!({"op": "set_code", "activity": "A1", "code_type": "Phase", "value": "CON"}),
    )
    .unwrap();
    assert_eq!(activity(&s, 1).codes.len(), 1);
    assert_eq!(s.validate(), []);
    let global = serde_json::json!({
        "op": "add_code_type", "name": "Area", "scope": "global", "max_length": 4
    });
    run(&mut s, global).unwrap();
    let area = &s.activity_code_types[&ActivityCodeTypeId(101)];
    assert_eq!(
        (area.scope, area.project, area.max_length, area.sequence),
        (CodeScope::Global, None, Some(4), 1)
    );
}

#[test]
fn a_new_resource_can_be_assigned() {
    let mut s = chain();
    s.calendars.get_mut(&CalendarId(2)).unwrap().is_default = true;
    let json = serde_json::json!({
        "op": "add_resource", "resource": "ELEC", "name": "Electricians"
    });
    let line = run(&mut s, json.clone()).unwrap();
    assert_eq!(line, "Added resource ELEC (Electricians, labor)");
    let elec = &s.resources[&ResourceId(100)];
    assert_eq!(
        (
            elec.kind,
            elec.calendar,
            elec.default_units_per_hour,
            elec.default_cost_follows_units
        ),
        (ResourceType::Labor, Some(CalendarId(2)), Some(1.0), true)
    );
    refused(&s, json, "duplicate-resource");
    run(&mut s, staff("assign_resource", "ELEC")).unwrap();
    assert_eq!(s.validate(), []);
    let crane = serde_json::json!({
        "op": "add_resource", "resource": "CRANE", "name": "Crane",
        "resource_type": "nonlabor", "units_per_hour": 0.5, "calendar": "Five day"
    });
    run(&mut s, crane).unwrap();
    let crane = &s.resources[&ResourceId(101)];
    assert_eq!(
        (crane.kind, crane.calendar, crane.default_units_per_hour),
        (ResourceType::Nonlabor, Some(CalendarId(1)), Some(0.5))
    );
}

#[test]
fn a_new_resource_is_refused_what_it_can_t_hold() {
    let mut s = chain();
    s.calendars.get_mut(&CalendarId(2)).unwrap().kind = CalendarType::Project;
    let add = |extra: serde_json::Value| {
        let mut json =
            serde_json::json!({"op": "add_resource", "resource": "ELEC", "name": "Electricians"});
        json.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        json
    };
    refused(
        &s,
        add(serde_json::json!({"calendar": "Ten hour"})),
        "resource-calendar",
    );
    refused(
        &s,
        add(serde_json::json!({"calendar": "Night"})),
        "no-such-calendar",
    );
    refused(
        &s,
        add(serde_json::json!({"units_per_hour": -1})),
        "invalid-units",
    );
    refused(&s, add(serde_json::json!({"name": ""})), "invalid-resource");
    // No default global calendar: the resource takes none.
    run(&mut s, add(serde_json::json!({}))).unwrap();
    assert_eq!(s.resources[&ResourceId(100)].calendar, None);
}

fn limit(json: serde_json::Value) -> serde_json::Value {
    let mut edit = serde_json::json!({"op": "set_resource_limit", "resource": "CREW"});
    let serde_json::Value::Object(fields) = json else {
        panic!("give the fields as an object");
    };
    edit.as_object_mut().unwrap().extend(fields);
    edit
}

/// A rate as the tests compare it: its ID, start, prices and limit.
type Held = (u64, Option<String>, [Option<f64>; 5], Option<f64>);

fn limits(s: &Schedule) -> Vec<Held> {
    s.resources[&ResourceId(1)]
        .rates
        .iter()
        .map(|r| {
            let from = r.effective.map(|d| d.to_string());
            (r.id.0, from, r.prices, r.max_units_per_hour)
        })
        .collect()
}

#[test]
fn a_new_resource_can_have_a_limit() {
    let mut s = chain();
    // With no planned start, the rate starts on the data date.
    s.project.data_date = "2026-03-02 08:00".parse().ok();
    let json = serde_json::json!({
        "op": "add_resource", "resource": "ELEC", "name": "Electricians", "max_units_per_hour": 2
    });
    let line = run(&mut s, json).unwrap();
    assert_eq!(
        line,
        "Added resource ELEC (Electricians, labor, at most 2 units per hour)"
    );
    let rates = &s.resources[&ResourceId(100)].rates;
    assert_eq!(rates.len(), 1);
    assert_eq!(
        (
            rates[0].id.0,
            rates[0].effective,
            rates[0].prices,
            rates[0].max_units_per_hour
        ),
        (100, "2026-03-02 00:00".parse().ok(), [None; 5], Some(2.0))
    );
    let negative = serde_json::json!({
        "op": "add_resource", "resource": "PIPE", "name": "Pipefitters", "max_units_per_hour": -1
    });
    refused(&s, negative, "invalid-units");
}

#[test]
fn a_limit_is_set_and_cleared_on_every_rate() {
    let mut s = with_crew();
    s.project.planned_start = "2026-03-02 08:00".parse().ok();
    let opening = Some("2026-03-02 00:00".to_owned());
    let line = run(&mut s, limit(serde_json::json!({"max_units_per_hour": 2}))).unwrap();
    assert_eq!(line, "CREW now has at most 2 units per hour");
    // A resource with no rate gets one, with no prices, from the project's start.
    assert_eq!(limits(&s), [(100, opening.clone(), [None; 5], Some(2.0))]);
    run(&mut s, limit(serde_json::json!({"max_units_per_hour": 3}))).unwrap();
    assert_eq!(limits(&s), [(100, opening.clone(), [None; 5], Some(3.0))]);
    let line = run(
        &mut s,
        limit(serde_json::json!({"max_units_per_hour": null})),
    )
    .unwrap();
    assert_eq!(line, "CREW now has no limit");
    assert_eq!(limits(&s), [(100, opening, [None; 5], None)]);
    refused(
        &s,
        limit(serde_json::json!({"max_units_per_hour": null})),
        "nothing-to-change",
    );
}

#[test]
fn a_limit_without_a_day_goes_on_every_rate_the_resource_has() {
    use totalfloat_model::{ResourceRate, ResourceRateId};
    let mut s = with_crew();
    let rate = |id: u64, from: &str, price: f64| ResourceRate {
        id: ResourceRateId(id),
        effective: from.parse().ok(),
        prices: [Some(price), None, None, None, None],
        max_units_per_hour: Some(1.0),
        source: None,
    };
    s.resources.get_mut(&ResourceId(1)).unwrap().rates = vec![
        rate(1, "2026-01-01 00:00", 50.0),
        rate(2, "2026-06-01 00:00", 80.0),
    ];
    run(&mut s, limit(serde_json::json!({"max_units_per_hour": 4}))).unwrap();
    let held: Vec<_> = limits(&s).into_iter().map(|r| (r.0, r.3)).collect();
    assert_eq!(held, [(1, Some(4.0)), (2, Some(4.0))]);
    // From a day: a rate starts then, with the prices in force then.
    let line = run(
        &mut s,
        limit(serde_json::json!({"max_units_per_hour": 6, "from": "2026-03-02"})),
    )
    .unwrap();
    assert_eq!(line, "CREW has at most 6 units per hour from 2026-03-02");
    assert_eq!(
        limits(&s)[2],
        (
            100,
            Some("2026-03-02 00:00".to_owned()),
            [Some(50.0), None, None, None, None],
            Some(6.0)
        )
    );
    // A rate that starts that day already takes the limit instead.
    run(
        &mut s,
        limit(serde_json::json!({"max_units_per_hour": 5, "from": "2026-06-01"})),
    )
    .unwrap();
    let held: Vec<_> = limits(&s).into_iter().map(|r| (r.0, r.3)).collect();
    assert_eq!(held, [(1, Some(4.0)), (2, Some(5.0)), (100, Some(6.0))]);
    // Cleared from a day on.
    run(
        &mut s,
        limit(serde_json::json!({"max_units_per_hour": null, "from": "2026-09-01"})),
    )
    .unwrap();
    assert_eq!(limits(&s)[3].3, None);
    assert_eq!(limits(&s)[3].2[0], Some(80.0));
}

#[test]
fn a_dated_limit_takes_over_from_an_undated_one_on_its_day() {
    let mut s = with_crew();
    run(&mut s, limit(serde_json::json!({"max_units_per_hour": 2}))).unwrap();
    run(
        &mut s,
        limit(serde_json::json!({"max_units_per_hour": 3, "from": "2027-01-04"})),
    )
    .unwrap();
    let crew = &s.resources[&ResourceId(1)];
    let at = |when: &str| crew.rate_at(when.parse().ok())?.max_units_per_hour;
    assert_eq!(at("2026-12-28 08:00"), Some(2.0));
    assert_eq!(at("2027-01-04 08:00"), Some(3.0));
}

/// A first dated limit starts on its day: what held before stays in force
/// until then, kept on an undated rate.
#[test]
fn a_first_dated_limit_doesn_t_reach_back() {
    let mut s = with_crew();
    s.project.planned_start = "2026-12-28 08:00".parse().ok();
    run(
        &mut s,
        limit(serde_json::json!({"max_units_per_hour": 2, "from": "2027-01-04"})),
    )
    .unwrap();
    assert_eq!(
        limits(&s),
        [
            (100, Some("2026-12-28 00:00".to_owned()), [None; 5], None),
            (
                101,
                Some("2027-01-04 00:00".to_owned()),
                [None; 5],
                Some(2.0)
            )
        ]
    );
    let crew = &s.resources[&ResourceId(1)];
    let at = |when: &str| crew.rate_at(when.parse().ok())?.max_units_per_hour;
    assert_eq!(at("2026-12-28 08:00"), None);
    assert_eq!(at("2027-01-04 08:00"), Some(2.0));

    // Every rate starting later: the earliest held before them, and still does.
    let mut s = with_crew();
    s.resources.get_mut(&ResourceId(1)).unwrap().rates = vec![totalfloat_model::ResourceRate {
        id: totalfloat_model::ResourceRateId(1),
        effective: "2027-06-01 00:00".parse().ok(),
        prices: [Some(50.0), None, None, None, None],
        max_units_per_hour: Some(1.0),
        source: None,
    }];
    run(
        &mut s,
        limit(serde_json::json!({"max_units_per_hour": 3, "from": "2027-01-04"})),
    )
    .unwrap();
    let crew = &s.resources[&ResourceId(1)];
    let at = |when: &str| {
        let rate = crew.rate_at(when.parse().ok())?;
        Some((rate.prices[0], rate.max_units_per_hour))
    };
    assert_eq!(at("2026-12-28 08:00"), Some((Some(50.0), Some(1.0))));
    assert_eq!(at("2027-01-04 08:00"), Some((Some(50.0), Some(3.0))));
    assert_eq!(at("2027-06-01 08:00"), Some((Some(50.0), Some(1.0))));
}

/// A limit set on the day the earliest rate starts changes that rate, and
/// what held before stays in force before it.
#[test]
fn a_limit_on_the_earliest_rate_s_day_doesn_t_reach_back() {
    let mut s = with_crew();
    s.resources.get_mut(&ResourceId(1)).unwrap().rates = vec![totalfloat_model::ResourceRate {
        id: totalfloat_model::ResourceRateId(1),
        effective: "2027-01-04 00:00".parse().ok(),
        prices: [Some(50.0), None, None, None, None],
        max_units_per_hour: Some(1.0),
        source: None,
    }];
    run(
        &mut s,
        limit(serde_json::json!({"max_units_per_hour": 2, "from": "2027-01-04"})),
    )
    .unwrap();
    let crew = &s.resources[&ResourceId(1)];
    assert_eq!(crew.rates.len(), 2);
    let at = |when: &str| crew.rate_at(when.parse().ok())?.max_units_per_hour;
    assert_eq!(at("2026-12-28 08:00"), Some(1.0));
    assert_eq!(at("2027-01-04 08:00"), Some(2.0));
    // Once a rate holds before the day, none is added.
    run(
        &mut s,
        limit(serde_json::json!({"max_units_per_hour": 3, "from": "2027-01-04"})),
    )
    .unwrap();
    assert_eq!(s.resources[&ResourceId(1)].rates.len(), 2);
}

/// From the project's start or earlier, what held before goes on a rate
/// from the day before, since work can come before the project's start.
#[test]
fn a_limit_from_the_project_s_start_keeps_what_held_the_day_before() {
    let mut s = with_crew();
    s.project.planned_start = "2027-01-04 08:00".parse().ok();
    run(
        &mut s,
        limit(serde_json::json!({"max_units_per_hour": 2, "from": "2027-01-04"})),
    )
    .unwrap();
    assert_eq!(
        limits(&s),
        [
            (100, Some("2027-01-03 00:00".to_owned()), [None; 5], None),
            (
                101,
                Some("2027-01-04 00:00".to_owned()),
                [None; 5],
                Some(2.0)
            )
        ]
    );
    let crew = &s.resources[&ResourceId(1)];
    let at = |when: &str| crew.rate_at(when.parse().ok())?.max_units_per_hour;
    assert_eq!(at("2026-11-02 08:00"), None);
}

/// Of two rates starting the same day, the limit goes on the one in force.
#[test]
fn a_limit_on_a_day_two_rates_start_goes_on_the_one_in_force() {
    let mut s = with_crew();
    let rate = |id: u64| totalfloat_model::ResourceRate {
        id: totalfloat_model::ResourceRateId(id),
        effective: "2027-01-04 00:00".parse().ok(),
        prices: [None; 5],
        max_units_per_hour: Some(1.0),
        source: None,
    };
    s.resources.get_mut(&ResourceId(1)).unwrap().rates = vec![rate(9), rate(5)];
    run(
        &mut s,
        limit(serde_json::json!({"max_units_per_hour": 3, "from": "2027-01-04"})),
    )
    .unwrap();
    let crew = &s.resources[&ResourceId(1)];
    let in_force = crew.rate_at("2027-01-05 08:00".parse().ok()).unwrap();
    assert_eq!((in_force.id.0, in_force.max_units_per_hour), (9, Some(3.0)));
}

#[test]
fn a_limit_is_refused_what_isn_t_one() {
    let s = with_crew();
    refused(
        &s,
        limit(serde_json::json!({"max_units_per_hour": -1})),
        "invalid-units",
    );
    refused(
        &s,
        limit(serde_json::json!({"max_units_per_hour": 2, "from": "2027-13-01"})),
        "invalid-date",
    );
    refused(
        &s,
        limit(serde_json::json!({"max_units_per_hour": null, "from": "2027-01-04"})),
        "nothing-to-change",
    );
    let other = serde_json::json!({
        "op": "set_resource_limit", "resource": "NOBODY", "max_units_per_hour": 2
    });
    refused(&s, other, "no-such-resource");
    // The limit has to be given, as a number or null.
    let missing = serde_json::json!({"op": "set_resource_limit", "resource": "CREW"});
    assert!(serde_json::from_value::<Edit>(missing).is_err());
    let text = serde_json::json!({
        "op": "set_resource_limit", "resource": "CREW", "max_units_per_hour": "two"
    });
    assert!(serde_json::from_value::<Edit>(text).is_err());
}

#[test]
fn a_limit_waits_for_its_resource() {
    let crew =
        edit(serde_json::json!({"op": "add_resource", "resource": "ELEC", "name": "Electricians"}));
    let set = edit(serde_json::json!({
        "op": "set_resource_limit", "resource": "elec", "max_units_per_hour": 2
    }));
    assert!(set.depends_on(&crew));
}

#[test]
fn values_and_assignments_wait_for_what_they_name() {
    let kind = edit(serde_json::json!({"op": "add_code_type", "name": "Phase"}));
    let value =
        edit(serde_json::json!({"op": "add_code_value", "code_type": "PHASE", "value": "CON"}));
    assert!(value.depends_on(&kind));
    let crew =
        edit(serde_json::json!({"op": "add_resource", "resource": "ELEC", "name": "Electricians"}));
    assert!(edit(staff("assign_resource", "elec")).depends_on(&crew));
    assert!(edit(staff("assign_resource", "Electricians")).depends_on(&crew));
}

#[test]
fn resource_dependent_durations_have_no_day_to_count_in() {
    let mut s = chain();
    let a2 = s.activities.get(&ActivityId(2)).unwrap();
    let day = crate::day_length(&s, a2.calendar);
    assert!(day.is_some());
    assert_eq!(crate::duration_day_length(&s, a2), day);
    s.activities.get_mut(&ActivityId(2)).unwrap().activity_type = ActivityType::ResourceDependent;
    let a2 = s.activities.get(&ActivityId(2)).unwrap();
    // The calendar still has its day, but a duration in days can't use it.
    assert_eq!(crate::duration_day_length(&s, a2), None);
    let json = serde_json::json!({"op": "set_duration", "activity": "A2", "original": "2d"});
    refused(&s, json, "no-hours-per-day");
}

fn role(code: &str, name: &str, parent: Option<&str>) -> serde_json::Value {
    serde_json::json!({"op": "add_role", "role": code, "name": name, "parent": parent})
}

fn assign_role(code: &str, units: Option<f64>) -> serde_json::Value {
    serde_json::json!({"op": "assign_role", "activity": "A2", "role": code, "units": units})
}

#[test]
fn a_role_is_added_under_another_and_assigned_with_no_resource() {
    let mut s = with_crew();
    run(&mut s, role("SUPER", "Superintendent", None)).unwrap();
    let line = run(
        &mut s,
        role("FOREMAN", "Site foreman", Some("superintendent")),
    )
    .unwrap();
    assert_eq!(line, "added role FOREMAN");
    let foreman = s
        .roles
        .values()
        .find(|r| r.short_name == "FOREMAN")
        .unwrap();
    let super_id = s
        .roles
        .values()
        .find(|r| r.short_name == "SUPER")
        .unwrap()
        .id;
    assert_eq!(foreman.parent, Some(super_id));
    refused(&s, role("foreman", "Again", None), "duplicate-role");
    refused(&s, role("RIGGER", "Rigger", Some("NOPE")), "no-such-role");
    refused(&s, role("TWO\nLINES", "Rigger", None), "invalid-role");

    let line = run(&mut s, assign_role("FOREMAN", Some(16.0))).unwrap();
    assert!(
        line.starts_with("A2 assigned role FOREMAN, with 16 units"),
        "{line}"
    );
    let held = s
        .assignments
        .values()
        .find(|a| a.activity == ActivityId(2))
        .unwrap();
    assert_eq!((held.resource, held.role), (None, Some(foreman_id(&s))));
    assert_eq!(held.amounts.budgeted_units, Some(16.0));
    assert_eq!(s.validate(), []);
    refused(&s, assign_role("FOREMAN", None), "already-assigned");
    refused(&s, assign_role("NOPE", None), "no-such-role");
    refused(&s, assign_role("SUPER", Some(-1.0)), "invalid-units");
    // A resource and a role alone sit on the same work, and taking the
    // resource off leaves the role.
    run(&mut s, staff("assign_resource", "CREW")).unwrap();
    run(&mut s, staff("unassign_resource", "CREW")).unwrap();
    assert_eq!(s.assignments.len(), 1);
    let off = serde_json::json!({"op": "unassign_role", "activity": "A2", "role": "foreman"});
    assert_eq!(
        run(&mut s, off.clone()).unwrap(),
        "A2 unassigned role foreman"
    );
    assert!(s.assignments.is_empty());
    refused(&s, off, "not-assigned");
    let mut done = with_crew();
    run(&mut done, role("SUPER", "Superintendent", None)).unwrap();
    done.activities.get_mut(&ActivityId(2)).unwrap().status = ActivityStatus::Completed;
    refused(&done, assign_role("SUPER", None), "no-resources-here");
}

fn foreman_id(s: &Schedule) -> RoleId {
    s.roles
        .values()
        .find(|r| r.short_name == "FOREMAN")
        .unwrap()
        .id
}

/// A role's rate in force when the work starts prices the assignment, and the
/// price is marked as the role's.
#[test]
fn an_assignment_by_role_takes_the_role_s_rate() {
    use totalfloat_model::{RoleRate, RoleRateId};
    let mut s = with_crew();
    run(&mut s, role("FOREMAN", "Site foreman", None)).unwrap();
    let id = foreman_id(&s);
    s.roles.get_mut(&id).unwrap().rates = vec![
        RoleRate {
            id: RoleRateId(1),
            effective: "2026-01-01 00:00".parse().ok(),
            prices: [Some(60.0), None, None, None, None],
            max_units_per_hour: None,
            source: None,
        },
        RoleRate {
            id: RoleRateId(2),
            effective: "2026-06-01 00:00".parse().ok(),
            prices: [Some(75.0), None, None, None, None],
            max_units_per_hour: None,
            source: None,
        },
    ];
    s.activities.get_mut(&ActivityId(2)).unwrap().planned_start = "2026-07-01 08:00".parse().ok();
    run(&mut s, assign_role("FOREMAN", Some(8.0))).unwrap();
    let held = &s.assignments.values().next().unwrap().amounts;
    assert_eq!(held.price_per_unit, Some(75.0));
    assert_eq!(held.price_source, Some(totalfloat_model::PriceSource::Role));
    assert_eq!(held.budgeted_cost, Some(600.0));
}

/// A role with no price yet still prices the assignment as the role's, and
/// its own cost link carries over.
#[test]
fn an_assignment_by_role_is_the_role_s_even_with_no_price() {
    let mut s = with_crew();
    run(&mut s, role("FOREMAN", "Site foreman", None)).unwrap();
    let id = foreman_id(&s);
    s.roles.get_mut(&id).unwrap().default_cost_follows_units = false;
    run(&mut s, assign_role("FOREMAN", Some(8.0))).unwrap();
    let held = &s.assignments.values().next().unwrap().amounts;
    assert_eq!(held.price_source, Some(totalfloat_model::PriceSource::Role));
    assert_eq!(held.price_per_unit, None);
    assert!(!held.cost_follows_units);
}

/// `chain()` with a cost account under another, and an expense category.
fn with_accounts() -> Schedule {
    use totalfloat_model::{CostAccount, CostAccountId, ExpenseCategory, ExpenseCategoryId};
    let mut s = chain();
    for (id, code, name, parent) in [
        (1, "SITE", "Site costs", None),
        (2, "PERMIT", "Permits", Some(CostAccountId(1))),
    ] {
        s.cost_accounts.insert(
            CostAccountId(id),
            CostAccount {
                id: CostAccountId(id),
                short_name: code.to_owned(),
                name: name.to_owned(),
                parent,
                sequence: None,
                description: String::new(),
                source: None,
            },
        );
    }
    s.expense_categories.insert(
        ExpenseCategoryId(4),
        ExpenseCategory {
            id: ExpenseCategoryId(4),
            name: "Fees".to_owned(),
            sequence: None,
            source: None,
        },
    );
    s
}

fn add_expense(name: &str, cost: f64) -> serde_json::Value {
    serde_json::json!({"op": "add_expense", "activity": "A2", "expense": name, "cost": cost})
}

/// An expense is one unit at its budget, all of it to spend, with the
/// account, category and accrual it's given; a second of the same name on
/// the activity, a missing account or category, and a bad cost are refused.
#[test]
fn an_expense_is_added_with_its_account_category_and_accrual() {
    use totalfloat_model::{Accrual, CostAccountId, ExpenseCategoryId, ExpenseId};
    let mut s = with_accounts();
    let json = serde_json::json!({
        "op": "add_expense", "activity": "A2", "expense": "Permit", "cost": 1500.0,
        "account": "permits", "category": "fees", "accrual": "start"
    });
    assert_eq!(run(&mut s, json).unwrap(), "A2 has expense Permit, 1500");
    let x = &s.expenses[&ExpenseId(100)];
    assert_eq!(x.activity, ActivityId(2));
    assert_eq!(x.cost_account, Some(CostAccountId(2)));
    assert_eq!(x.category, Some(ExpenseCategoryId(4)));
    assert_eq!(x.accrual, Some(totalfloat_model::Accrual::Start));
    assert_eq!(
        (x.budgeted_cost, x.actual_cost, x.remaining_cost),
        (Some(1500.0), Some(0.0), Some(1500.0))
    );
    assert_eq!(
        (x.budgeted_units, x.price_per_unit),
        (Some(1.0), Some(1500.0))
    );
    run(&mut s, add_expense("Survey", 650.0)).unwrap();
    assert_eq!(s.expenses[&ExpenseId(101)].accrual, Some(Accrual::Uniform));
    refused(&s, add_expense("permit", 10.0), "duplicate-expense");
    refused(&s, add_expense("Permit ", 10.0), "duplicate-expense");
    refused(&s, add_expense(" ", 1.0), "invalid-expense");
    refused(&s, add_expense("Skip\nhire", 1.0), "invalid-expense");
    let nowhere = serde_json::json!({
        "op": "add_expense", "activity": "A2", "expense": "Skip", "cost": 1.0, "account": "NOPE"
    });
    refused(&s, nowhere, "no-such-cost-account");
    let nothing = serde_json::json!({
        "op": "add_expense", "activity": "A2", "expense": "Skip", "cost": 1.0, "category": "NOPE"
    });
    refused(&s, nothing, "no-such-category");
    // A late invoice can still come in on finished work.
    let mut done = s.clone();
    done.activities.get_mut(&ActivityId(2)).unwrap().status = ActivityStatus::Completed;
    run(&mut done, add_expense("Invoice", 300.0)).unwrap();
}

/// Without a remaining cost, the cost at completion holds: an actual cost
/// comes out of what's left, and while nothing was spent, a new budget is the
/// new cost at completion, even with an actual cost in the same edit.
#[test]
fn an_expense_s_cost_at_completion_holds_without_a_remaining_cost() {
    use totalfloat_model::ExpenseId;
    let costs = |s: &Schedule| {
        let x = &s.expenses[&ExpenseId(100)];
        (x.budgeted_cost, x.actual_cost, x.remaining_cost)
    };
    let set = |costs: serde_json::Value| {
        let mut json =
            serde_json::json!({"op": "set_expense", "activity": "A2", "expense": "Permit"});
        json.as_object_mut()
            .unwrap()
            .extend(costs.as_object().unwrap().clone());
        json
    };
    let mut s = with_accounts();
    run(&mut s, add_expense("Permit", 1500.0)).unwrap();
    run(&mut s, set(serde_json::json!({"actual_cost": 500.0}))).unwrap();
    assert_eq!(costs(&s), (Some(1500.0), Some(500.0), Some(1000.0)));
    let mut s = with_accounts();
    run(&mut s, add_expense("Permit", 1500.0)).unwrap();
    run(
        &mut s,
        set(serde_json::json!({"budgeted_cost": 1000.0, "actual_cost": 400.0})),
    )
    .unwrap();
    assert_eq!(costs(&s), (Some(1000.0), Some(400.0), Some(600.0)));
    // Once something is spent, a new budget leaves the cost at completion.
    run(&mut s, set(serde_json::json!({"budgeted_cost": 2000.0}))).unwrap();
    assert_eq!(costs(&s), (Some(2000.0), Some(400.0), Some(600.0)));
    // An actual cost past the cost at completion leaves nothing remaining.
    run(&mut s, set(serde_json::json!({"actual_cost": 1200.0}))).unwrap();
    assert_eq!(costs(&s), (Some(2000.0), Some(1200.0), Some(0.0)));
}

/// A new budget moves the price, and the remaining cost while nothing is
/// spent; once something is, costs move only as they're given.
#[test]
fn an_expense_s_costs_are_set_and_it_is_deleted() {
    use totalfloat_model::ExpenseId;
    let mut s = with_accounts();
    run(&mut s, add_expense("Permit", 1500.0)).unwrap();
    let set = |costs: serde_json::Value| {
        let mut json =
            serde_json::json!({"op": "set_expense", "activity": "A2", "expense": "permit"});
        json.as_object_mut()
            .unwrap()
            .extend(costs.as_object().unwrap().clone());
        json
    };
    let line = run(&mut s, set(serde_json::json!({"budgeted_cost": 1800.0}))).unwrap();
    assert_eq!(
        line,
        "A2 expense permit: budget 1800, actual 0, remaining 1800"
    );
    assert_eq!(s.expenses[&ExpenseId(100)].price_per_unit, Some(1800.0));
    run(
        &mut s,
        set(serde_json::json!({"actual_cost": 500.0, "remaining_cost": 1300.0})),
    )
    .unwrap();
    run(&mut s, set(serde_json::json!({"budgeted_cost": 2000.0}))).unwrap();
    let x = &s.expenses[&ExpenseId(100)];
    assert_eq!(
        (x.budgeted_cost, x.actual_cost, x.remaining_cost),
        (Some(2000.0), Some(500.0), Some(1300.0))
    );
    refused(&s, set(serde_json::json!({})), "invalid-edit");
    let other = serde_json::json!({"op": "set_expense", "activity": "A2", "expense": "Survey", "actual_cost": 1.0});
    refused(&s, other, "no-such-expense");
    let gone = serde_json::json!({"op": "delete_expense", "activity": "A2", "expense": "Permit"});
    assert_eq!(
        run(&mut s, gone.clone()).unwrap(),
        "A2 expense Permit deleted; its actual cost of 500 is gone"
    );
    assert!(s.expenses.is_empty());
    refused(&s, gone, "no-such-expense");
}

#[test]
fn deleting_an_activity_takes_its_expenses() {
    let mut s = with_accounts();
    run(&mut s, add_expense("Permit", 1500.0)).unwrap();
    let line = run(
        &mut s,
        serde_json::json!({"op": "delete_activity", "activity": "A2"}),
    )
    .unwrap();
    assert!(line.contains("; 1 expense gone with it"), "{line}");
    assert!(s.expenses.is_empty());
}

#[test]
fn two_expenses_of_one_name_can_t_be_told_apart() {
    use totalfloat_model::ExpenseId;
    let mut s = with_accounts();
    run(&mut s, add_expense("Permit", 1500.0)).unwrap();
    let mut twin = s.expenses[&ExpenseId(100)].clone();
    twin.id = ExpenseId(7);
    s.expenses.insert(twin.id, twin);
    let gone = serde_json::json!({"op": "delete_expense", "activity": "A2", "expense": "Permit"});
    refused(&s, gone, "ambiguous-expense");
    refused(&s, add_expense("permit", 10.0), "duplicate-expense");
}

/// A file can hold a negative cost, a credit, so an edit can make one, and
/// what's left of it stays a credit.
#[test]
fn an_expense_can_be_a_credit() {
    use totalfloat_model::ExpenseId;
    let mut s = with_accounts();
    run(&mut s, add_expense("Refund", -500.0)).unwrap();
    let x = &s.expenses[&ExpenseId(100)];
    assert_eq!(
        (x.budgeted_cost, x.remaining_cost),
        (Some(-500.0), Some(-500.0))
    );
    let spent = serde_json::json!({
        "op": "set_expense", "activity": "A2", "expense": "Refund", "actual_cost": -200.0
    });
    run(&mut s, spent).unwrap();
    assert_eq!(s.expenses[&ExpenseId(100)].remaining_cost, Some(-300.0));
    let over = serde_json::json!({
        "op": "set_expense", "activity": "A2", "expense": "Refund", "actual_cost": -600.0
    });
    run(&mut s, over).unwrap();
    assert_eq!(s.expenses[&ExpenseId(100)].remaining_cost, Some(0.0));
}

/// A new accrual alone leaves the costs as they were, even a remaining cost
/// that isn't the rest of the budget.
#[test]
fn an_accrual_alone_leaves_an_expense_s_costs() {
    use totalfloat_model::ExpenseId;
    let mut s = with_accounts();
    run(&mut s, add_expense("Skip hire", 400.0)).unwrap();
    let spent = serde_json::json!({
        "op": "set_expense", "activity": "A2", "expense": "Skip hire",
        "actual_cost": 400.0, "remaining_cost": -100.0
    });
    run(&mut s, spent).unwrap();
    let accrual = serde_json::json!({
        "op": "set_expense", "activity": "A2", "expense": "Skip hire", "accrual": "start"
    });
    run(&mut s, accrual).unwrap();
    let x = &s.expenses[&ExpenseId(100)];
    assert_eq!(x.remaining_cost, Some(-100.0));
    assert_eq!(x.accrual, Some(totalfloat_model::Accrual::Start));
}

/// An item read from a file can have spaces at the ends: it's kept as it is,
/// and found without them.
#[test]
fn an_expense_item_keeps_spaces_at_its_ends_and_is_found_without_them() {
    use totalfloat_model::ExpenseId;
    let mut s = with_accounts();
    run(&mut s, add_expense(" Skip hire ", 1200.0)).unwrap();
    assert_eq!(s.expenses[&ExpenseId(100)].name, " Skip hire ");
    let set = serde_json::json!({
        "op": "set_expense", "activity": "A2", "expense": "skip hire", "budgeted_cost": 1300.0
    });
    run(&mut s, set).unwrap();
    assert_eq!(s.expenses[&ExpenseId(100)].budgeted_cost, Some(1300.0));
}

fn project_code(edit_json: serde_json::Value, s: &mut Schedule) -> Result<String, &'static str> {
    run(s, edit_json)
}

#[test]
fn project_codes_are_added_set_replaced_and_cleared() {
    use totalfloat_model::{ProjectCodeId, ProjectCodeTypeId};
    let mut s = chain();
    let mut next = ids();
    let add_type = edit(serde_json::json!({"op": "add_project_code_type", "name": "Region"}));
    let said = apply(&mut s, &add_type, &mut next).unwrap();
    assert_eq!(said, "Added project code type Region");
    assert_eq!(next.project_code_type, 101);
    let kind = ProjectCodeTypeId(100);
    assert_eq!(s.project_code_types[&kind].max_length, Some(20));
    for (value, description) in [("WEST", "Western"), ("EAST", "Eastern")] {
        let add = edit(serde_json::json!({
            "op": "add_project_code_value", "code_type": "region",
            "value": value, "description": description
        }));
        apply(&mut s, &add, &mut next).unwrap();
    }
    assert_eq!(next.project_code, 102);
    assert_eq!(s.project_codes[&ProjectCodeId(101)].sequence, 1);
    let set = |value: &str| serde_json::json!({"op": "set_project_code", "code_type": "Region", "value": value});
    let said = project_code(set("west"), &mut s).unwrap();
    assert_eq!(said, "project AUTHORED set to Region=WEST");
    assert_eq!(s.project.codes[&kind], ProjectCodeId(100));
    // A description is a fallback, as for activity codes.
    project_code(set("Eastern"), &mut s).unwrap();
    assert_eq!(s.project.codes[&kind], ProjectCodeId(101));
    assert_eq!(s.validate(), []);
    let clear = serde_json::json!({"op": "clear_project_code", "code_type": "Region"});
    project_code(clear.clone(), &mut s).unwrap();
    assert!(s.project.codes.is_empty());
    refused(&s, clear, "no-project-code");
}

#[test]
fn project_code_refusals_name_what_is_there() {
    let mut s = chain();
    let none =
        serde_json::json!({"op": "set_project_code", "code_type": "Region", "value": "WEST"});
    let mut copy = s.clone();
    let error = apply(&mut copy, &edit(none.clone()), &mut ids()).unwrap_err();
    assert_eq!(error.code, "no-such-project-code");
    assert!(error.hint.contains("add_project_code_type"), "{error:?}");
    run(
        &mut s,
        serde_json::json!({"op": "add_project_code_type", "name": "Region", "max_length": 4}),
    )
    .unwrap();
    run(
        &mut s,
        serde_json::json!({"op": "add_project_code_value", "code_type": "Region", "value": "WEST"}),
    )
    .unwrap();
    let mut wrong = none;
    wrong["value"] = "NORTH".into();
    let mut copy = s.clone();
    let error = apply(&mut copy, &edit(wrong), &mut ids()).unwrap_err();
    assert_eq!(error.code, "no-such-project-code");
    assert_eq!(error.message, "Region has no value NORTH");
    assert_eq!(error.hint, "Region holds: WEST");
    let add = |value: &str| serde_json::json!({"op": "add_project_code_value", "code_type": "Region", "value": value});
    refused(&s, add("west"), "duplicate-project-code");
    refused(&s, add("NORTH"), "project-code-too-long");
    refused(&s, add(" N"), "invalid-project-code-value");
    let kind = |name: &str| serde_json::json!({"op": "add_project_code_type", "name": name});
    refused(&s, kind("region"), "duplicate-project-code-type");
    refused(&s, kind(""), "invalid-project-code-type");
    refused(
        &s,
        serde_json::json!({"op": "add_project_code_type", "name": "Phase", "max_length": 0}),
        "invalid-project-code-type",
    );
    let mut copy = s.clone();
    let error = apply(
        &mut copy,
        &edit(serde_json::json!({"op": "clear_project_code", "code_type": "Regoin"})),
        &mut ids(),
    )
    .unwrap_err();
    assert_eq!(
        error.hint,
        "project code types: Region. add_project_code_type adds one"
    );
}

#[test]
fn a_project_code_value_depends_on_adding_its_type() {
    let kind = edit(serde_json::json!({"op": "add_project_code_type", "name": "Region"}));
    let value = edit(
        serde_json::json!({"op": "add_project_code_value", "code_type": "REGION", "value": "W"}),
    );
    let set =
        edit(serde_json::json!({"op": "set_project_code", "code_type": "region", "value": "w"}));
    assert!(value.depends_on(&kind));
    assert!(set.depends_on(&value));
    assert!(!kind.depends_on(&value));
}

fn with_obs() -> Schedule {
    use totalfloat_model::{Obs, ObsId};
    let mut s = chain();
    s.obs.insert(
        ObsId(1),
        Obs {
            id: ObsId(1),
            parent: None,
            name: "Owner".to_owned(),
            description: String::new(),
            sequence: 0,
            source: None,
        },
    );
    s.wbs.get_mut(&WbsId(1)).unwrap().obs = Some(ObsId(1));
    s
}

#[test]
fn an_obs_node_is_added_and_made_responsible_for_a_wbs_node() {
    use totalfloat_model::ObsId;
    let mut s = with_obs();
    let mut next = ids();
    let add = edit(serde_json::json!({
        "op": "add_obs", "obs": "Contractor", "parent": "owner", "description": "Main contractor"
    }));
    let said = apply(&mut s, &add, &mut next).unwrap();
    assert_eq!(said, "Added OBS node Contractor, under Owner");
    assert_eq!(next.obs, 101);
    let node = &s.obs[&ObsId(100)];
    assert_eq!((node.parent, node.sequence), (Some(ObsId(1)), 0));
    // A new WBS node is owned as the project's root is.
    run(
        &mut s,
        serde_json::json!({"op": "add_wbs", "code": "CIV", "name": "Civil", "parent": "ROOT"}),
    )
    .unwrap();
    let civil = s.wbs.values().find(|w| w.code == "CIV").unwrap().id;
    assert_eq!(s.wbs[&civil].obs, Some(ObsId(1)));
    let said = run(
        &mut s,
        serde_json::json!({"op": "set_wbs_obs", "wbs": "civ", "obs": "contractor"}),
    )
    .unwrap();
    assert_eq!(said, "WBS civ is now Contractor's");
    assert_eq!(s.wbs[&civil].obs, Some(ObsId(100)));
    // By `obs_id` too.
    run(
        &mut s,
        serde_json::json!({"op": "set_wbs_obs", "wbs": "CIV", "obs": "1"}),
    )
    .unwrap();
    assert_eq!(s.wbs[&civil].obs, Some(ObsId(1)));
    assert_eq!(s.validate(), []);
}

#[test]
fn obs_refusals_name_what_is_there() {
    let s = with_obs();
    let add = |name: &str| serde_json::json!({"op": "add_obs", "obs": name});
    refused(&s, add("owner"), "duplicate-obs");
    refused(&s, add(" Owner"), "invalid-obs");
    refused(
        &s,
        serde_json::json!({"op": "add_obs", "obs": "Sub", "parent": "Nobody"}),
        "no-such-obs",
    );
    refused(
        &s,
        serde_json::json!({"op": "set_wbs_obs", "wbs": "NOPE", "obs": "Owner"}),
        "no-such-wbs",
    );
    let mut copy = s.clone();
    let error = apply(
        &mut copy,
        &edit(serde_json::json!({"op": "set_wbs_obs", "wbs": "ROOT", "obs": "Ownr"})),
        &mut ids(),
    )
    .unwrap_err();
    assert_eq!(error.code, "no-such-obs");
    assert_eq!(error.hint, "OBS nodes: Owner. add_obs adds one");
    let set = edit(serde_json::json!({"op": "set_wbs_obs", "wbs": "ROOT", "obs": "New"}));
    let made = edit(serde_json::json!({"op": "add_obs", "obs": "new"}));
    assert!(set.depends_on(&made));
}

fn with_topics() -> Schedule {
    use totalfloat_model::{NotebookTopic, NotebookTopicId};
    let mut s = chain();
    for (id, name, wbs, activities) in [(1, "Basis", true, true), (2, "Scope", true, false)] {
        s.notebook_topics.insert(
            NotebookTopicId(id),
            NotebookTopic {
                id: NotebookTopicId(id),
                name: name.to_owned(),
                sequence: 0,
                for_eps: false,
                for_projects: false,
                for_wbs: wbs,
                for_activities: activities,
                source: None,
            },
        );
    }
    s
}

#[test]
fn notes_are_added_replaced_and_deleted() {
    use totalfloat_model::NotebookTopicId;
    let mut s = with_topics();
    let mut next = ids();
    let add = edit(serde_json::json!({
        "op": "add_note", "activity": "A1", "topic": "basis", "text": "Two crews\nOne crane"
    }));
    assert_eq!(
        apply(&mut s, &add, &mut next).unwrap(),
        "A1 gained a Basis note"
    );
    assert_eq!(next.activity_note, 101);
    let note = &activity(&s, 1).notes[&NotebookTopicId(1)];
    assert_eq!(
        (note.id, note.text().as_str()),
        (100, "Two crews\nOne crane")
    );
    let set = serde_json::json!({
        "op": "set_note", "activity": "A1", "topic": "Basis", "text": "Three crews"
    });
    run(&mut s, set).unwrap();
    assert_eq!(
        activity(&s, 1).notes[&NotebookTopicId(1)].text(),
        "Three crews"
    );
    let wbs = edit(serde_json::json!({
        "op": "add_note", "wbs": "ROOT", "topic": "Scope", "text": "Everything"
    }));
    apply(&mut s, &wbs, &mut next).unwrap();
    assert_eq!(next.wbs_note, 101);
    assert_eq!(
        s.wbs[&WbsId(1)].notes[&NotebookTopicId(2)].text(),
        "Everything"
    );
    assert_eq!(s.validate(), []);
    let delete = serde_json::json!({"op": "delete_note", "activity": "A1", "topic": "Basis"});
    run(&mut s, delete.clone()).unwrap();
    assert!(activity(&s, 1).notes.is_empty());
    refused(&s, delete, "no-note");
}

#[test]
fn note_refusals_say_which_topics_fit() {
    let s = with_topics();
    let note = |extra: serde_json::Value| {
        let mut json = serde_json::json!({"op": "add_note", "topic": "Basis", "text": "x"});
        json.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        json
    };
    refused(&s, note(serde_json::json!({})), "note-target");
    refused(
        &s,
        note(serde_json::json!({"activity": "A1", "wbs": "ROOT"})),
        "note-target",
    );
    refused(
        &s,
        note(serde_json::json!({"activity": "NOPE"})),
        "no-such-activity",
    );
    let mut copy = s.clone();
    let error = apply(
        &mut copy,
        &edit(
            serde_json::json!({"op": "add_note", "activity": "A1", "topic": "Scope", "text": "x"}),
        ),
        &mut ids(),
    )
    .unwrap_err();
    assert_eq!(error.code, "wrong-topic");
    assert_eq!(error.hint, "topics: Basis");
    refused(
        &s,
        serde_json::json!({"op": "add_note", "activity": "A1", "topic": "Nope", "text": "x"}),
        "no-such-topic",
    );
    refused(
        &s,
        serde_json::json!({"op": "add_note", "activity": "A1", "topic": "Basis", "text": "  "}),
        "empty-note",
    );
    refused(
        &s,
        serde_json::json!({"op": "set_note", "activity": "A1", "topic": "Basis", "text": "x"}),
        "no-note",
    );
    let mut twice = s;
    run(&mut twice, note(serde_json::json!({"activity": "A1"}))).unwrap();
    refused(
        &twice,
        note(serde_json::json!({"activity": "A1"})),
        "duplicate-note",
    );
}

#[test]
fn a_topic_name_two_topics_share_means_the_one_for_that_kind() {
    use totalfloat_model::{NotebookTopic, NotebookTopicId};
    let mut s = with_topics();
    s.notebook_topics.insert(
        NotebookTopicId(3),
        NotebookTopic {
            id: NotebookTopicId(3),
            name: "Scope".to_owned(),
            sequence: 1,
            for_eps: false,
            for_projects: false,
            for_wbs: false,
            for_activities: true,
            source: None,
        },
    );
    run(
        &mut s,
        serde_json::json!({"op": "add_note", "activity": "A1", "topic": "Scope", "text": "x"}),
    )
    .unwrap();
    assert!(activity(&s, 1).notes.contains_key(&NotebookTopicId(3)));
}

fn steps_on_a1(from_steps: bool) -> Schedule {
    let mut s = chain();
    s.project.percent_from_steps = from_steps;
    let a = s.activities.get_mut(&ActivityId(1)).unwrap();
    a.percent_complete_type = totalfloat_model::PercentCompleteType::Physical;
    s
}

#[test]
fn steps_are_added_progressed_and_drive_physical_percent() {
    use totalfloat_model::StepId;
    let mut s = steps_on_a1(true);
    let mut next = ids();
    for (name, weight) in [("Excavate", 1.0), ("Pour", 3.0)] {
        let add = edit(serde_json::json!({
            "op": "add_step", "activity": "A1", "step": name, "weight": weight
        }));
        apply(&mut s, &add, &mut next).unwrap();
    }
    assert_eq!(next.step, 102);
    assert_eq!(activity(&s, 1).steps[&StepId(101)].sequence, 1);
    let said = run(
        &mut s,
        serde_json::json!({"op": "complete_step", "activity": "A1", "step": "excavate"}),
    )
    .unwrap();
    assert_eq!(said, "A1 step excavate complete; physical percent now 25");
    assert_eq!(activity(&s, 1).physical_percent_complete, Some(25.0));
    run(
        &mut s,
        serde_json::json!({"op": "set_step", "activity": "A1", "step": "Pour", "percent": 50}),
    )
    .unwrap();
    // (1 x 100 + 3 x 50) / 4
    assert_eq!(activity(&s, 1).physical_percent_complete, Some(62.5));
    run(
        &mut s,
        serde_json::json!({"op": "set_step", "activity": "A1", "step": "101", "name": "Pour slab", "weight": 1}),
    )
    .unwrap();
    assert_eq!(activity(&s, 1).steps[&StepId(101)].name, "Pour slab");
    assert_eq!(activity(&s, 1).physical_percent_complete, Some(75.0));
    run(
        &mut s,
        serde_json::json!({"op": "delete_step", "activity": "A1", "step": "Excavate"}),
    )
    .unwrap();
    assert_eq!(activity(&s, 1).physical_percent_complete, Some(50.0));
}

#[test]
fn steps_leave_physical_percent_alone_unless_the_project_measures_by_them() {
    let mut s = steps_on_a1(false);
    run(
        &mut s,
        serde_json::json!({"op": "add_step", "activity": "A1", "step": "All", "percent": 100}),
    )
    .unwrap();
    assert_eq!(activity(&s, 1).physical_percent_complete, None);
    let mut s = steps_on_a1(true);
    s.activities
        .get_mut(&ActivityId(1))
        .unwrap()
        .percent_complete_type = totalfloat_model::PercentCompleteType::Duration;
    run(
        &mut s,
        serde_json::json!({"op": "add_step", "activity": "A1", "step": "All", "percent": 100}),
    )
    .unwrap();
    assert_eq!(activity(&s, 1).physical_percent_complete, None);
}

#[test]
fn step_refusals() {
    let mut s = steps_on_a1(true);
    run(
        &mut s,
        serde_json::json!({"op": "add_step", "activity": "A1", "step": "Pour"}),
    )
    .unwrap();
    let add = |name: &str| serde_json::json!({"op": "add_step", "activity": "A1", "step": name});
    refused(&s, add("pour"), "duplicate-step");
    refused(&s, add(" Pour"), "invalid-step");
    refused(
        &s,
        serde_json::json!({"op": "add_step", "activity": "A1", "step": "X", "weight": -1}),
        "invalid-step",
    );
    refused(
        &s,
        serde_json::json!({"op": "set_step", "activity": "A1", "step": "Pour", "percent": 101}),
        "invalid-step",
    );
    refused(
        &s,
        serde_json::json!({"op": "set_step", "activity": "A1", "step": "Pour"}),
        "nothing-to-change",
    );
    let mut copy = s.clone();
    let error = apply(
        &mut copy,
        &edit(serde_json::json!({"op": "complete_step", "activity": "A1", "step": "Cure"})),
        &mut ids(),
    )
    .unwrap_err();
    assert_eq!(error.code, "no-such-step");
    assert_eq!(error.hint, "its steps: Pour");
    refused(
        &s,
        serde_json::json!({"op": "delete_step", "activity": "NOPE", "step": "Pour"}),
        "no-such-activity",
    );
}

#[test]
fn a_duration_percent_sets_remaining_duration() {
    let mut s = at_data_date(chain());
    // A2 is 8h long and measured by duration percent, the default.
    let line = run(
        &mut s,
        progress(&serde_json::json!({"actual_start": "2026-01-12 08:00", "percent": 25.0})),
    )
    .unwrap();
    assert_eq!(line, "A2 is in progress, with 6h remaining");
    assert_eq!(activity(&s, 2).remaining_duration, Minutes::new(360));
    // On work already started, a percent alone moves remaining again.
    run(&mut s, progress(&serde_json::json!({"percent": 62.5}))).unwrap();
    assert_eq!(activity(&s, 2).remaining_duration, Minutes::new(180));
    // Remaining given with it wins, as written.
    run(
        &mut s,
        progress(&serde_json::json!({"percent": 10.0, "remaining": "2h"})),
    )
    .unwrap();
    assert_eq!(activity(&s, 2).remaining_duration, Minutes::new(120));
    // 100% by duration leaves nothing to do, which takes an actual finish.
    refused(
        &s,
        progress(&serde_json::json!({"percent": 100.0})),
        "finished-work",
    );
}

#[test]
fn remaining_duration_sets_the_duration_percent() {
    let mut s = at_data_date(chain());
    run(
        &mut s,
        progress(&serde_json::json!({"actual_start": "2026-01-12 08:00", "remaining": "2h"})),
    )
    .unwrap();
    let a = activity(&s, 2);
    let done =
        (a.original_duration.get() - a.remaining_duration.get()) * 100 / a.original_duration.get();
    assert_eq!(done, 75);
}

#[test]
fn physical_and_units_percent_leave_remaining_alone() {
    for kind in [
        totalfloat_model::PercentCompleteType::Physical,
        totalfloat_model::PercentCompleteType::Units,
    ] {
        let mut s = at_data_date(chain());
        s.activities
            .get_mut(&ActivityId(2))
            .unwrap()
            .percent_complete_type = kind;
        run(
            &mut s,
            progress(&serde_json::json!({"actual_start": "2026-01-12 08:00", "percent": 25.0})),
        )
        .unwrap();
        assert_eq!(
            activity(&s, 2).remaining_duration,
            Minutes::new(480),
            "{kind:?}"
        );
        assert_eq!(activity(&s, 2).physical_percent_complete, Some(25.0));
    }
    // Work not started keeps its remaining whatever its type.
    let mut s = at_data_date(chain());
    run(&mut s, progress(&serde_json::json!({"percent": 25.0}))).unwrap();
    assert_eq!(activity(&s, 2).remaining_duration, Minutes::new(480));
}

fn with_fields() -> Schedule {
    use totalfloat_model::{UdfDataType, UdfSubject, UdfType, UdfTypeId};
    let mut s = with_crew();
    let fields = [
        (1, UdfSubject::Activity, "Note", UdfDataType::Text),
        (2, UdfSubject::Activity, "Score", UdfDataType::Number),
        (3, UdfSubject::Activity, "Count", UdfDataType::Integer),
        (4, UdfSubject::Activity, "Allowance", UdfDataType::Cost),
        (5, UdfSubject::Activity, "Due", UdfDataType::FinishDate),
        (6, UdfSubject::Activity, "Risk", UdfDataType::Indicator),
        (7, UdfSubject::Activity, "Trade list", UdfDataType::Code),
        (8, UdfSubject::Wbs, "Owner", UdfDataType::Text),
        (9, UdfSubject::Resource, "Rate band", UdfDataType::Text),
        (10, UdfSubject::Assignment, "Shift", UdfDataType::Text),
    ];
    for (id, subject, label, data_type) in fields {
        s.udf_types.insert(
            UdfTypeId(id),
            UdfType {
                id: UdfTypeId(id),
                subject,
                label: label.to_owned(),
                data_type,
                source: None,
            },
        );
    }
    s
}

fn udf(field: &str, value: &str) -> serde_json::Value {
    serde_json::json!({"op": "set_udf", "activity": "A1", "field": field, "value": value})
}

#[test]
fn each_data_type_takes_its_values() {
    use totalfloat_model::{UdfTypeId, UdfValue};
    let mut s = with_fields();
    for (field, value) in [
        ("note", "Night pour"),
        ("Score", "4.256"),
        ("Count", "3"),
        ("Allowance", "1500.5"),
        ("Due", "2026-03-02 17:00"),
        ("Risk", "2"),
    ] {
        run(&mut s, udf(field, value)).unwrap();
    }
    let held = &activity(&s, 1).udfs;
    assert_eq!(held[&UdfTypeId(1)], UdfValue::Text("Night pour".to_owned()));
    assert_eq!(held[&UdfTypeId(2)], UdfValue::Number(4.26));
    assert_eq!(held[&UdfTypeId(3)], UdfValue::Number(3.0));
    assert_eq!(held[&UdfTypeId(4)], UdfValue::Number(1500.5));
    assert_eq!(
        held[&UdfTypeId(5)],
        UdfValue::Date("2026-03-02 17:00".parse().unwrap())
    );
    assert_eq!(held[&UdfTypeId(6)], UdfValue::Text("UDF_G2".to_owned()));
    let clear = serde_json::json!({"op": "clear_udf", "activity": "A1", "field": "Note"});
    run(&mut s, clear.clone()).unwrap();
    assert!(!activity(&s, 1).udfs.contains_key(&UdfTypeId(1)));
    refused(&s, clear, "no-udf-value");
}

#[test]
fn a_wrong_value_for_the_type_is_refused_with_a_hint() {
    let s = with_fields();
    for (field, value) in [
        ("Score", "lots"),
        ("Count", "2.5"),
        ("Allowance", "inf"),
        ("Due", "next week"),
        ("Risk", "5"),
        ("Note", "two\nlines"),
    ] {
        let mut copy = s.clone();
        let error = apply(&mut copy, &edit(udf(field, value)), &mut ids()).unwrap_err();
        assert_eq!(error.code, "wrong-udf-type", "{field} {value}");
        assert!(!error.hint.is_empty(), "{field}");
        assert_eq!(copy, s);
    }
    refused(&s, udf("Trade list", "ELEC"), "unsupported-udf");
    let mut copy = s.clone();
    let error = apply(&mut copy, &edit(udf("Nope", "1")), &mut ids()).unwrap_err();
    assert_eq!(error.code, "no-such-udf");
    assert!(
        error.hint.starts_with("fields for activities: Note, Score"),
        "{}",
        error.hint
    );
    // A field for another kind of record isn't an activity's.
    refused(&s, udf("Owner", "x"), "no-such-udf");
}

#[test]
fn values_go_on_wbs_nodes_resources_and_assignments() {
    use totalfloat_model::{AssignmentId, UdfTypeId, UdfValue};
    let mut s = with_fields();
    run(
        &mut s,
        serde_json::json!({"op": "set_udf", "wbs": "ROOT", "field": "Owner", "value": "Civil"}),
    )
    .unwrap();
    assert_eq!(
        s.wbs[&WbsId(1)].udfs[&UdfTypeId(8)],
        UdfValue::Text("Civil".to_owned())
    );
    run(
        &mut s,
        serde_json::json!({"op": "set_udf", "resource": "CREW", "field": "Rate band", "value": "B"}),
    )
    .unwrap();
    assert_eq!(
        s.resources[&ResourceId(1)].udfs[&UdfTypeId(9)],
        UdfValue::Text("B".to_owned())
    );
    refused(
        &s,
        serde_json::json!({"op": "set_udf", "activity": "A1", "resource": "CREW", "field": "Shift", "value": "N"}),
        "no-such-assignment",
    );
    run(
        &mut s,
        serde_json::json!({"op": "assign_resource", "activity": "A1", "resource": "CREW"}),
    )
    .unwrap();
    run(
        &mut s,
        serde_json::json!({"op": "set_udf", "activity": "A1", "resource": "CREW", "field": "Shift", "value": "Night"}),
    )
    .unwrap();
    assert_eq!(
        s.assignments[&AssignmentId(100)].udfs[&UdfTypeId(10)],
        UdfValue::Text("Night".to_owned())
    );
    refused(
        &s,
        serde_json::json!({"op": "set_udf", "wbs": "ROOT", "resource": "CREW", "field": "Shift", "value": "N"}),
        "udf-target",
    );
}

#[test]
fn a_new_field_takes_values_at_once() {
    use totalfloat_model::{UdfDataType, UdfTypeId};
    let mut s = with_fields();
    let mut next = ids();
    let add = edit(serde_json::json!({
        "op": "add_udf_type", "field": "Crew size", "for": "activity", "data_type": "integer"
    }));
    let said = apply(&mut s, &add, &mut next).unwrap();
    assert_eq!(said, "Added the field Crew size, for activities");
    assert_eq!(next.udf_type, 101);
    assert_eq!(s.udf_types[&UdfTypeId(100)].data_type, UdfDataType::Integer);
    run(&mut s, udf("crew size", "4")).unwrap();
    refused(
        &s,
        serde_json::json!({"op": "add_udf_type", "field": "crew SIZE", "for": "activity", "data_type": "text"}),
        "duplicate-udf",
    );
    // The same label for another kind of record is another field.
    run(
        &mut s,
        serde_json::json!({"op": "add_udf_type", "field": "Crew size", "for": "wbs", "data_type": "text"}),
    )
    .unwrap();
}

#[test]
fn a_label_wins_over_an_id_and_integers_and_costs_are_checked() {
    use totalfloat_model::{UdfDataType, UdfSubject, UdfType, UdfTypeId, UdfValue};
    let mut s = with_fields();
    // A field labelled "2" is that field, not field 2.
    s.udf_types.insert(
        UdfTypeId(20),
        UdfType {
            id: UdfTypeId(20),
            subject: UdfSubject::Activity,
            label: "2".to_owned(),
            data_type: UdfDataType::Text,
            source: None,
        },
    );
    run(&mut s, udf("2", "by label")).unwrap();
    assert_eq!(
        activity(&s, 1).udfs[&UdfTypeId(20)],
        UdfValue::Text("by label".to_owned())
    );
    run(&mut s, udf("Allowance", "99.999")).unwrap();
    assert_eq!(activity(&s, 1).udfs[&UdfTypeId(4)], UdfValue::Number(100.0));
    refused(&s, udf("Count", "1e3"), "wrong-udf-type");
    refused(&s, udf("Count", "9999999999"), "wrong-udf-type");
}

/// A chain with A2 started on 2026-01-05 08:00 and still in progress.
fn started() -> Schedule {
    let mut s = chain();
    s.project.data_date = Some("2026-01-15 08:00".parse().unwrap());
    let a = s.activities.get_mut(&ActivityId(2)).unwrap();
    a.status = ActivityStatus::InProgress;
    a.actual_start = Some("2026-01-05 08:00".parse().unwrap());
    s
}

fn suspend(activity: &str, suspend: &str, resume: &str) -> Edit {
    edit(
        serde_json::json!({"op": "set_suspend", "activity": activity, "suspend": suspend, "resume": resume}),
    )
}

#[test]
fn suspend_and_resume_set_only_their_dates_and_clear_again() {
    let mut s = started();
    let before = s.clone();
    let line = apply(
        &mut s,
        &suspend("A2", "2026-01-06 17:00", "2026-01-12 08:00"),
        &mut ids(),
    )
    .unwrap();
    assert_eq!(
        line,
        "A2 suspended 2026-01-06 17:00, resuming 2026-01-12 08:00"
    );
    let mut expected = before.clone();
    let a = expected.activities.get_mut(&ActivityId(2)).unwrap();
    a.suspend = Some("2026-01-06 17:00".parse().unwrap());
    a.resume = Some("2026-01-12 08:00".parse().unwrap());
    assert_eq!(s, expected);
    let line = apply(
        &mut s,
        &edit(serde_json::json!({"op": "clear_suspend", "activity": "A2"})),
        &mut ids(),
    )
    .unwrap();
    assert_eq!(line, "A2 no longer suspended");
    assert_eq!(s, before);
}

#[test]
fn only_work_in_progress_is_suspended_and_resumed_later() {
    let mut s = started();
    s.activities.get_mut(&ActivityId(3)).unwrap().status = ActivityStatus::Completed;
    for (change, code) in [
        (
            suspend("A1", "2026-01-06 17:00", "2026-01-12 08:00"),
            "not-in-progress",
        ),
        (
            suspend("A3", "2026-01-06 17:00", "2026-01-12 08:00"),
            "not-in-progress",
        ),
        (
            suspend("A2", "2026-01-02 17:00", "2026-01-12 08:00"),
            "suspend-before-start",
        ),
        (
            suspend("A2", "2026-01-06 17:00", "2026-01-06 17:00"),
            "resume-not-after-suspend",
        ),
        (
            suspend("A2", "2026-01-06", "2026-01-12 08:00"),
            "invalid-date",
        ),
        (
            suspend("NOPE", "2026-01-06 17:00", "2026-01-12 08:00"),
            "no-such-activity",
        ),
        (
            edit(serde_json::json!({"op": "clear_suspend", "activity": "A2"})),
            "nothing-to-change",
        ),
    ] {
        let mut copy = s.clone();
        let error = apply(&mut copy, &change, &mut ids()).unwrap_err();
        assert_eq!(error.code, code, "{change:?}: {}", error.message);
        assert_eq!(copy, s, "{change:?} changed something");
    }
}

#[test]
fn a_later_actual_start_can_t_pass_a_suspension() {
    let mut s = started();
    apply(
        &mut s,
        &suspend("A2", "2026-01-06 17:00", "2026-01-12 08:00"),
        &mut ids(),
    )
    .unwrap();
    let kept = s.clone();
    let later = edit(
        serde_json::json!({"op": "set_progress", "activity": "A2", "actual_start": "2026-01-08 08:00"}),
    );
    let error = apply(&mut s, &later, &mut ids()).unwrap_err();
    assert_eq!(error.code, "suspend-before-start", "{}", error.message);
    assert_eq!(s, kept);
    // An earlier start still fits.
    let earlier = edit(
        serde_json::json!({"op": "set_progress", "activity": "A2", "actual_start": "2026-01-05 13:00"}),
    );
    apply(&mut s, &earlier, &mut ids()).unwrap();
}

/// Spans in weeks, months and years use the calendar's hours in each
/// (TF-033), and `set_hours_per_period` sets them.
#[test]
fn weeks_months_and_years_use_the_calendar_s_hours() {
    let mut s = chain();
    let calendar = s.calendars.get_mut(&CalendarId(1)).unwrap();
    calendar.hours_per_period.week = Some(Minutes::new(40 * 60));
    calendar.hours_per_period.month = None;
    let length = |s: &Schedule| {
        s.activities
            .values()
            .find(|a| a.code == "A1")
            .unwrap()
            .original_duration
    };
    run(
        &mut s,
        serde_json::json!({"op": "set_duration", "activity": "A1", "original": "2w"}),
    )
    .unwrap();
    assert_eq!(length(&s), Minutes::new(80 * 60));
    // Without hours in a month, a month can't be used, and it says so.
    refused(
        &s,
        serde_json::json!({"op": "set_duration", "activity": "A1", "original": "1m"}),
        "no-hours-per-period",
    );
    let message = apply(
        &mut s.clone(),
        &edit(serde_json::json!({"op": "set_duration", "activity": "A1", "original": "1m"})),
        &mut ids(),
    )
    .unwrap_err()
    .message;
    assert!(message.contains("hours in a month"), "{message}");
    let set = serde_json::json!({"op": "set_hours_per_period", "calendar": "Five day", "period": "month", "hours": 172});
    assert_eq!(
        run(&mut s, set).unwrap(),
        "Five day counts a month as 172 hours"
    );
    run(
        &mut s,
        serde_json::json!({"op": "set_duration", "activity": "A1", "original": "1m"}),
    )
    .unwrap();
    assert_eq!(length(&s), Minutes::new(172 * 60));
    // A longer week makes the same span longer.
    let set = serde_json::json!({"op": "set_hours_per_period", "calendar": "Five day", "period": "week", "hours": 37.5});
    run(&mut s, set).unwrap();
    run(
        &mut s,
        serde_json::json!({"op": "set_duration", "activity": "A1", "original": "2w"}),
    )
    .unwrap();
    assert_eq!(length(&s), Minutes::new(75 * 60));
    assert_eq!(
        s.calendars[&CalendarId(1)].hours_per_period.week,
        Some(Minutes::new(2250))
    );
    // Hours that can't fit the period, or aren't whole minutes, are refused.
    for (period, bad) in [
        ("week", 0.0),
        ("week", 168.5),
        ("month", 745.0),
        ("year", 8785.0),
        ("year", 40.001),
    ] {
        refused(
            &s,
            serde_json::json!({"op": "set_hours_per_period", "calendar": "Five day", "period": period, "hours": bad}),
            "bad-hours-per-period",
        );
    }
    refused(
        &s,
        serde_json::json!({"op": "set_hours_per_period", "calendar": "NOPE", "period": "week", "hours": 40}),
        "no-such-calendar",
    );
}

#[test]
fn spans_read_and_write_weeks_months_and_years() {
    for (text, shown, period) in [
        ("2w", "2w", Some("week")),
        ("1.5M", "1.5m", Some("month")),
        ("1y", "1y", Some("year")),
        ("-3d", "-3d", Some("day")),
        ("8h", "8h", None),
    ] {
        let span: Span = text.parse().unwrap();
        assert_eq!(span.to_string(), shown);
        assert_eq!(span.period(), period);
    }
    let lengths = HoursPerPeriod {
        day: Some(Minutes::new(480)),
        week: Some(Minutes::new(2400)),
        month: Some(Minutes::new(10_320)),
        year: None,
    };
    let minutes = |text: &str| text.parse::<Span>().unwrap().minutes_in(Some(&lengths));
    assert_eq!(minutes("2w"), Some(Minutes::new(4800)));
    assert_eq!(minutes("0.5m"), Some(Minutes::new(5160)));
    assert_eq!(minutes("1y"), None);
    // The day-only conversion leaves weeks to `minutes_in`.
    assert_eq!(
        "1w".parse::<Span>()
            .unwrap()
            .minutes(Some(Minutes::new(480))),
        None
    );
}

/// A week needs the calendar's hours per week, and a lag in weeks on
/// 24 hours a day is 168 hours, where months and years have no length.
#[test]
fn weeks_need_hours_per_week_and_a_24_hour_lag_week_is_168_hours() {
    let mut s = chain();
    s.calendars
        .get_mut(&CalendarId(1))
        .unwrap()
        .hours_per_period
        .week = None;
    refused(
        &s,
        serde_json::json!({"op": "set_duration", "activity": "A1", "original": "1w"}),
        "no-hours-per-period",
    );
    s.project.options.lag_calendar = LagCalendar::TwentyFourHour;
    let lag = |s: &Schedule| s.relationships[&RelationshipId(1)].lag;
    run(
        &mut s,
        serde_json::json!({"op": "change_link", "predecessor": "A1", "successor": "A2", "lag": "1w"}),
    )
    .unwrap();
    assert_eq!(lag(&s), Minutes::new(168 * 60));
    for span in ["1m", "1y"] {
        refused(
            &s,
            serde_json::json!({"op": "change_link", "predecessor": "A1", "successor": "A2", "lag": span}),
            "no-hours-per-period",
        );
    }
}

/// A resource's assignment, a role's with no resource and an expense
/// are each booked to a cost account by name, and to none without one.
#[test]
fn costs_are_booked_to_a_cost_account_and_to_none() {
    use totalfloat_model::{AssignmentId, CostAccountId, ExpenseId};
    let mut s = with_crew();
    s.cost_accounts = with_accounts().cost_accounts;
    let steps = [
        serde_json::json!({"op": "assign_resource", "activity": "A2", "resource": "CREW", "units": 8}),
        role("RIGGER", "Rigger", None),
        serde_json::json!({"op": "assign_role", "activity": "A2", "role": "RIGGER"}),
        add_expense("Permit", 1500.0),
    ];
    for step in steps {
        run(&mut s, step).unwrap();
    }
    let book = |held: (&str, &str), account: Option<&str>| {
        let mut json = serde_json::json!({"op": "set_cost_account", "activity": "A2"});
        let object = json.as_object_mut().unwrap();
        object.insert(held.0.to_owned(), held.1.into());
        if let Some(account) = account {
            object.insert("account".to_owned(), account.into());
        }
        json
    };
    let line = run(&mut s, book(("resource", "crew"), Some("Site costs"))).unwrap();
    assert_eq!(line, "A2's crew is booked to cost account Site costs");
    run(&mut s, book(("role", "RIGGER"), Some("PERMIT"))).unwrap();
    run(&mut s, book(("expense", "permit"), Some("SITE"))).unwrap();
    let account = |id| s.assignments[&AssignmentId(id)].cost_account;
    assert_eq!(account(100), Some(CostAccountId(1)));
    assert_eq!(account(101), Some(CostAccountId(2)));
    assert_eq!(
        s.expenses[&ExpenseId(100)].cost_account,
        Some(CostAccountId(1))
    );
    let line = run(&mut s, book(("resource", "CREW"), None)).unwrap();
    assert_eq!(line, "A2's CREW has no cost account");
    assert_eq!(s.assignments[&AssignmentId(100)].cost_account, None);

    refused(
        &s,
        book(("resource", "CREW"), Some("NOPE")),
        "no-such-cost-account",
    );
    refused(
        &s,
        book(("expense", "Survey"), Some("SITE")),
        "no-such-expense",
    );
    refused(&s, book(("role", "NOBODY"), Some("SITE")), "no-such-role");
    let crewless = book(("resource", "CREW"), Some("SITE"));
    let mut elsewhere = crewless.clone();
    elsewhere["activity"] = "A1".into();
    refused(&s, elsewhere, "not-assigned");
    let neither =
        serde_json::json!({"op": "set_cost_account", "activity": "A2", "account": "SITE"});
    refused(&s, neither, "invalid-edit");
    let mut both = crewless;
    both["expense"] = "Permit".into();
    refused(&s, both, "invalid-edit");
}

/// A2 measured by units percent and started, with a labor crew on 40 hours
/// whose cost follows its units, a nonlabor pump on 10 hours, and 5 tonnes of
/// concrete, which aren't hours and so don't count.
fn by_units() -> Schedule {
    let mut s = at_data_date(with_crew());
    for (id, short, kind) in [
        (2, "PUMP", ResourceType::Nonlabor),
        (3, "CONC", ResourceType::Material),
    ] {
        let mut r = s.resources[&ResourceId(1)].clone();
        r.id = ResourceId(id);
        r.short_name = short.to_owned();
        r.name = short.to_owned();
        r.kind = kind;
        s.resources.insert(r.id, r);
    }
    for (resource, units) in [("CREW", 40), ("PUMP", 10), ("CONC", 5)] {
        run(
            &mut s,
            serde_json::json!({"op": "assign_resource", "activity": "A2", "resource": resource, "units": units}),
        )
        .unwrap();
    }
    let crew = s.assignments.get_mut(&AssignmentId(100)).unwrap();
    crew.amounts.cost_follows_units = true;
    crew.amounts.budgeted_cost = Some(4000.0);
    crew.amounts.remaining_cost = Some(4000.0);
    crew.amounts.actual_cost = Some(0.0);
    let a2 = s.activities.get_mut(&ActivityId(2)).unwrap();
    a2.percent_complete_type = totalfloat_model::PercentCompleteType::Units;
    run(
        &mut s,
        progress(&serde_json::json!({"actual_start": "2026-01-12 08:00"})),
    )
    .unwrap();
    s
}

fn units_of(s: &Schedule, id: u64) -> (Option<f64>, Option<f64>) {
    let a = &s.assignments[&AssignmentId(id)].amounts;
    (a.actual_units, a.remaining_units)
}

#[test]
fn a_units_percent_splits_each_assignment_s_units() {
    let mut s = by_units();
    let line = run(&mut s, progress(&serde_json::json!({"percent": 40.0}))).unwrap();
    assert_eq!(
        line,
        "A2 is in progress, with 8h remaining; CREW: actual units 0 to 16, remaining units 40 to 24, \
         remaining units per hour 5 to 3, actual cost 0 to 1600, remaining cost 4000 to 2400; \
         PUMP: actual units 0 to 4, remaining units 10 to 6, remaining units per hour 1.25 to 0.75"
    );
    assert_eq!(units_of(&s, 100), (Some(16.0), Some(24.0)));
    assert_eq!(units_of(&s, 101), (Some(4.0), Some(6.0)));
    // Concrete is in tonnes, so the percent leaves it alone.
    assert_eq!(units_of(&s, 102), (Some(0.0), Some(5.0)));
    assert_eq!(crate::units::units_percent(&s, ActivityId(2)), Some(40.0));
    // Durations are the planner's: a units percent doesn't move them.
    assert_eq!(activity(&s, 2).remaining_duration, Minutes::new(480));
    // A lower percent moves units back, keeping each assignment's total.
    run(&mut s, progress(&serde_json::json!({"percent": 25.0}))).unwrap();
    assert_eq!(units_of(&s, 100), (Some(10.0), Some(30.0)));
    assert_eq!(units_of(&s, 101), (Some(2.5), Some(7.5)));
}

#[test]
fn a_units_percent_on_work_not_started_moves_no_units() {
    let mut s = by_units();
    run(
        &mut s,
        serde_json::json!({"op": "clear_progress", "activity": "A2"}),
    )
    .unwrap();
    run(&mut s, progress(&serde_json::json!({"percent": 40.0}))).unwrap();
    assert_eq!(units_of(&s, 100), (Some(0.0), Some(40.0)));
}

#[test]
fn units_set_on_an_assignment_move_the_units_percent() {
    let mut s = by_units();
    let line = run(
        &mut s,
        serde_json::json!({"op": "set_assignment_units", "activity": "A2", "resource": "CREW", "actual_units": 20, "remaining_units": 20}),
    )
    .unwrap();
    assert_eq!(
        line,
        "A2's CREW: actual units 0 to 20, remaining units 40 to 20, remaining units per hour 5 to 2.5, \
         actual cost 0 to 2000, remaining cost 4000 to 2000; A2 is 40.00% complete by units"
    );
    // 20 of the crew's 40 and none of the pump's 10: 20 of 50.
    assert_eq!(crate::units::units_percent(&s, ActivityId(2)), Some(40.0));
    run(
        &mut s,
        serde_json::json!({"op": "set_assignment_units", "activity": "A2", "resource": "PUMP", "actual_units": 10, "remaining_units": 0}),
    )
    .unwrap();
    assert_eq!(crate::units::units_percent(&s, ActivityId(2)), Some(60.0));
}

#[test]
fn assignment_units_refusals() {
    let s = by_units();
    let set = |extra: serde_json::Value| {
        let mut json =
            serde_json::json!({"op": "set_assignment_units", "activity": "A2", "resource": "CREW"});
        for (key, value) in extra.as_object().unwrap() {
            json[key] = value.clone();
        }
        json
    };
    refused(&s, set(serde_json::json!({})), "nothing-to-change");
    refused(
        &s,
        set(serde_json::json!({"actual_units": -1})),
        "invalid-units",
    );
    let other = serde_json::json!({"op": "set_assignment_units", "activity": "A3", "resource": "CREW", "actual_units": 1});
    refused(&s, other, "not-assigned");
    let mut fresh = by_units();
    run(
        &mut fresh,
        serde_json::json!({"op": "clear_progress", "activity": "A2"}),
    )
    .unwrap();
    refused(
        &fresh,
        set(serde_json::json!({"actual_units": 5})),
        "not-started",
    );
    let mut done = by_units();
    run(
        &mut done,
        progress(&serde_json::json!({"actual_finish": "2026-01-12 17:00"})),
    )
    .unwrap();
    refused(
        &done,
        set(serde_json::json!({"remaining_units": 5})),
        "finished-work",
    );
}

#[test]
fn a_units_percent_with_a_remaining_duration_keeps_its_split() {
    let mut s = by_units();
    s.activities.get_mut(&ActivityId(2)).unwrap().duration_type =
        totalfloat_model::DurationType::FixedUnitsPerTime;
    run(
        &mut s,
        progress(&serde_json::json!({"percent": 50.0, "remaining": "4h"})),
    )
    .unwrap();
    // Duration types don't rescale units a units percent just split.
    assert_eq!(units_of(&s, 100), (Some(20.0), Some(20.0)));
    // Concrete isn't split, so it follows the halved remaining duration.
    assert_eq!(units_of(&s, 102), (Some(0.0), Some(2.5)));
    assert_eq!(
        s.assignments[&AssignmentId(100)]
            .amounts
            .remaining_units_per_hour,
        Some(5.0)
    );
    assert_eq!(activity(&s, 2).remaining_duration, Minutes::new(240));
}

#[test]
fn the_stored_percent_carried_back_splits_nothing() {
    let mut s = by_units();
    for (resource, actual, left) in [("CREW", 20, 20), ("PUMP", 10, 0)] {
        run(
            &mut s,
            serde_json::json!({"op": "set_assignment_units", "activity": "A2", "resource": resource, "actual_units": actual, "remaining_units": left}),
        )
        .unwrap();
    }
    // The stored percent follows the units: 30 of 50.
    assert_eq!(activity(&s, 2).physical_percent_complete, Some(60.0));
    // Text and progress tables send it back with other changes.
    run(
        &mut s,
        progress(&serde_json::json!({"percent": 60.0, "remaining": "6h"})),
    )
    .unwrap();
    assert_eq!(units_of(&s, 100), (Some(20.0), Some(20.0)));
    assert_eq!(units_of(&s, 101), (Some(10.0), Some(0.0)));
}

#[test]
fn a_units_percent_leaves_blank_costs_blank() {
    let mut s = by_units();
    s.assignments
        .get_mut(&AssignmentId(101))
        .unwrap()
        .amounts
        .cost_follows_units = true;
    run(&mut s, progress(&serde_json::json!({"percent": 40.0}))).unwrap();
    let pump = &s.assignments[&AssignmentId(101)].amounts;
    assert_eq!((pump.actual_cost, pump.remaining_cost), (None, None));
}

#[test]
fn remaining_units_on_work_not_started_move_its_budget() {
    let mut s = by_units();
    run(
        &mut s,
        serde_json::json!({"op": "clear_progress", "activity": "A2"}),
    )
    .unwrap();
    run(
        &mut s,
        serde_json::json!({"op": "set_assignment_units", "activity": "A2", "resource": "CREW", "remaining_units": 30}),
    )
    .unwrap();
    let crew = &s.assignments[&AssignmentId(100)].amounts;
    assert_eq!(
        (crew.budgeted_units, crew.remaining_units),
        (Some(30.0), Some(30.0))
    );
    assert_eq!(crew.budgeted_cost, Some(3000.0));
    // So a later duration edit keeps them.
    run(
        &mut s,
        serde_json::json!({"op": "set_duration", "activity": "A2", "original": "8h"}),
    )
    .unwrap();
    assert_eq!(units_of(&s, 100), (Some(0.0), Some(30.0)));
}

#[test]
fn a_resource_on_an_activity_twice_is_refused() {
    let mut s = by_units();
    let mut twice = s.assignments[&AssignmentId(100)].clone();
    twice.id = AssignmentId(200);
    s.assignments.insert(twice.id, twice);
    refused(
        &s,
        serde_json::json!({"op": "set_assignment_units", "activity": "A2", "resource": "CREW", "actual_units": 5}),
        "ambiguous-assignment",
    );
}

/// Short names repeat under different parents, as in one corpus file,
/// so an account is named for sure by its path of short names.
#[test]
fn a_cost_account_is_named_by_its_path_where_short_names_repeat() {
    use totalfloat_model::{CostAccount, CostAccountId};
    let mut s = with_accounts();
    let mut fees = s.cost_accounts[&CostAccountId(2)].clone();
    fees.id = CostAccountId(3);
    fees.short_name = "FEES".to_owned();
    fees.name = "Fees".to_owned();
    fees.parent = None;
    s.cost_accounts.insert(fees.id, fees);
    // A second PERMIT, under FEES.
    s.cost_accounts.insert(
        CostAccountId(4),
        CostAccount {
            id: CostAccountId(4),
            parent: Some(CostAccountId(3)),
            name: "Permit fees".to_owned(),
            ..s.cost_accounts[&CostAccountId(2)].clone()
        },
    );
    assert_eq!(
        crate::cost_account_path(&s, CostAccountId(4)).as_deref(),
        Some("FEES.PERMIT")
    );
    assert_eq!(
        crate::cost_account_named(&s, "fees.permit"),
        Some(CostAccountId(4))
    );
    assert_eq!(
        crate::cost_account_named(&s, "SITE.PERMIT"),
        Some(CostAccountId(2))
    );
    // The short name alone fits two accounts; a unique name still works.
    assert_eq!(crate::cost_account_named(&s, "PERMIT"), None);
    assert_eq!(
        crate::cost_account_named(&s, "Permit fees"),
        Some(CostAccountId(4))
    );
    run(&mut s, add_expense("Licence", 300.0)).unwrap();
    let book = |account: &str| serde_json::json!({"op": "set_cost_account", "activity": "A2", "expense": "Licence", "account": account});
    refused(&s, book("PERMIT"), "no-such-cost-account");
    run(&mut s, book("FEES.PERMIT")).unwrap();
    let licence = s.expenses.values().find(|x| x.name == "Licence").unwrap();
    assert_eq!(licence.cost_account, Some(CostAccountId(4)));
}

/// A root account's path that is also another account's short name
/// fits two accounts, so it names neither; a loop of parents still has a path.
#[test]
fn a_cost_account_name_that_fits_two_is_refused_and_loops_have_paths() {
    use totalfloat_model::CostAccountId;
    let mut s = with_accounts();
    // A root account PERMIT, beside SITE's child PERMIT.
    let mut root = s.cost_accounts[&CostAccountId(2)].clone();
    root.id = CostAccountId(5);
    root.parent = None;
    root.name = "Root permits".to_owned();
    s.cost_accounts.insert(root.id, root);
    assert_eq!(crate::cost_account_named(&s, "PERMIT"), None);
    assert_eq!(
        crate::cost_account_named(&s, "SITE.PERMIT"),
        Some(CostAccountId(2))
    );
    assert_eq!(
        crate::cost_account_named(&s, "Root permits"),
        Some(CostAccountId(5))
    );
    // SITE and the root PERMIT under each other.
    s.cost_accounts.get_mut(&CostAccountId(1)).unwrap().parent = Some(CostAccountId(5));
    s.cost_accounts.get_mut(&CostAccountId(5)).unwrap().parent = Some(CostAccountId(1));
    assert!(crate::cost_account_path(&s, CostAccountId(1)).is_some());
}

/// CREW on A2 with 40 hours to do over a five-day week of 8-hour days,
/// 2026-03-02 to 2026-03-06, with a bell curve in the file.
fn spreadable() -> Schedule {
    use totalfloat_model::{ResourceCurve, ResourceCurveId};
    let mut s = with_crew();
    let shift = WorkInterval::new(
        TimeOfDay::from_hm(8, 0).unwrap(),
        TimeOfDay::from_hm(16, 0).unwrap(),
    )
    .unwrap();
    let week = &mut s.calendars.get_mut(&CalendarId(1)).unwrap().work_week;
    for day in [
        Weekday::Monday,
        Weekday::Tuesday,
        Weekday::Wednesday,
        Weekday::Thursday,
        Weekday::Friday,
    ] {
        *week.day_mut(day) = vec![shift];
    }
    let units = serde_json::json!({
        "op": "assign_resource", "activity": "A2", "resource": "CREW", "units": 40
    });
    run(&mut s, units).unwrap();
    let a2 = s.activities.get_mut(&ActivityId(2)).unwrap();
    a2.stored.remaining_early_start = "2026-03-02 08:00".parse().ok();
    a2.stored.remaining_early_finish = "2026-03-06 16:00".parse().ok();
    s.resource_curves.insert(
        ResourceCurveId(3),
        ResourceCurve {
            id: ResourceCurveId(3),
            name: "Bell Shaped".to_owned(),
            default: true,
            points: [5.0; totalfloat_model::CURVE_POINTS],
            source: None,
        },
    );
    s
}

fn crew_on_a2(s: &Schedule) -> &totalfloat_model::Assignment {
    s.assignments
        .values()
        .find(|a| a.activity == ActivityId(2))
        .unwrap()
}

fn curve(name: Option<&str>) -> serde_json::Value {
    let mut json = staff("set_curve", "CREW");
    if let Some(name) = name {
        json["curve"] = name.into();
    }
    json
}

fn typed(period: &str, start: &str, units: &[f64]) -> serde_json::Value {
    let mut json = staff("set_spread", "CREW");
    json["period"] = period.into();
    json["start"] = start.into();
    json["units"] = units.into();
    json
}

fn hours(list: &[(f64, i64)]) -> Vec<totalfloat_model::SpreadSegment> {
    list.iter()
        .map(|&(units, h)| totalfloat_model::SpreadSegment {
            units,
            work: Minutes::new(h * 60),
        })
        .collect()
}

/// A curve by name, ignoring case, or even with none; either clears a
/// typed spread.
#[test]
fn an_assignment_s_curve_is_chosen_or_cleared() {
    use totalfloat_model::ResourceCurveId;
    let mut s = spreadable();
    assert_eq!(
        run(&mut s, curve(Some("bell shaped"))).unwrap(),
        "A2's CREW now spreads by Bell Shaped"
    );
    assert_eq!(crew_on_a2(&s).curve, Some(ResourceCurveId(3)));
    run(&mut s, typed("day", "2026-03-02", &[8.0; 5])).unwrap();
    assert_eq!(crew_on_a2(&s).curve, None);
    run(&mut s, curve(Some("Bell Shaped"))).unwrap();
    assert_eq!(crew_on_a2(&s).typed_spread, None);
    assert_eq!(
        run(&mut s, curve(None)).unwrap(),
        "A2's CREW now spreads evenly"
    );
    assert_eq!(
        (crew_on_a2(&s).curve, &crew_on_a2(&s).typed_spread),
        (None, &None)
    );
    assert_eq!(s.validate(), []);
}

#[test]
fn an_unknown_curve_or_assignment_is_refused() {
    let s = spreadable();
    refused(&s, curve(Some("Bel Shaped")), "no-such-curve");
    let mut other = curve(None);
    other["activity"] = "A3".into();
    refused(&s, other, "not-assigned");
    refused(&s, staff("set_curve", "NOBODY"), "no-such-resource");
    let mut twice = s.clone();
    let again = serde_json::json!({
        "op": "assign_resource", "activity": "A2", "resource": "CREW", "units": 8
    });
    // The edit refuses a second assignment, so make one directly.
    assert!(run(&mut twice, again).is_err());
    let mut copy = crew_on_a2(&twice).clone();
    copy.id = AssignmentId(99);
    twice.assignments.insert(copy.id, copy);
    refused(&twice, curve(None), "ambiguous-assignment");
}

/// Units typed per day become stretches of the remaining work; a
/// holiday or weekend takes none, and work not started plans the same.
#[test]
fn units_typed_per_period_become_the_typed_spread() {
    let mut s = spreadable();
    run(&mut s, curve(Some("Bell Shaped"))).unwrap();
    assert_eq!(
        run(
            &mut s,
            typed("day", "2026-03-02", &[12.0, 4.0, 0.0, 12.0, 12.0])
        )
        .unwrap(),
        "A2's CREW now spreads by 5 typed days from 2026-03-02"
    );
    let spread = crew_on_a2(&s).typed_spread.clone().unwrap();
    let expected = hours(&[(12.0, 8), (4.0, 8), (0.0, 8), (12.0, 8), (12.0, 8)]);
    assert_eq!(spread.remaining, expected);
    assert_eq!(spread.planned, expected);
    assert_eq!(spread.actual, []);
    assert_eq!(crew_on_a2(&s).curve, None);
    // A week from Sunday holds the whole week.
    run(&mut s, typed("week", "2026-03-01", &[40.0])).unwrap();
    assert_eq!(
        crew_on_a2(&s).typed_spread.as_ref().unwrap().remaining,
        hours(&[(40.0, 40)])
    );
    // A month holds the whole week.
    run(&mut s, typed("month", "2026-03-01", &[40.0])).unwrap();
    // Within rounding of the remaining units.
    run(
        &mut s,
        typed("day", "2026-03-02", &[8.0, 8.0, 8.0, 8.0, 8.0004]),
    )
    .unwrap();
    assert_eq!(s.validate(), []);
}

/// On work in progress, the plan stays as it was and so do actuals.
#[test]
fn typing_started_work_keeps_its_plan_and_actuals() {
    let mut s = spreadable();
    s.activities.get_mut(&ActivityId(2)).unwrap().status = ActivityStatus::InProgress;
    let id = crew_on_a2(&s).id;
    s.assignments.get_mut(&id).unwrap().typed_spread = Some(totalfloat_model::TypedSpread {
        planned: hours(&[(48.0, 48)]),
        remaining: hours(&[(40.0, 40)]),
        actual: hours(&[(8.0, 8)]),
    });
    run(
        &mut s,
        typed("day", "2026-03-02", &[0.0, 0.0, 20.0, 20.0, 0.0]),
    )
    .unwrap();
    let spread = crew_on_a2(&s).typed_spread.clone().unwrap();
    assert_eq!(spread.planned, hours(&[(48.0, 48)]));
    assert_eq!(spread.actual, hours(&[(8.0, 8)]));
    assert_eq!(
        spread.remaining,
        hours(&[(0.0, 8), (0.0, 8), (20.0, 8), (20.0, 8), (0.0, 8)])
    );

    // A curve would drop the typed actuals, so it's refused.
    refused(&s, curve(None), "typed-actuals");
    // Started work with no typed plan stays without one: its remaining
    // units aren't its budget.
    let mut even = spreadable();
    even.activities.get_mut(&ActivityId(2)).unwrap().status = ActivityStatus::InProgress;
    run(
        &mut even,
        typed("day", "2026-03-02", &[8.0, 8.0, 8.0, 8.0, 8.0]),
    )
    .unwrap();
    let spread = crew_on_a2(&even).typed_spread.clone().unwrap();
    assert!(spread.planned.is_empty() && spread.actual.is_empty());
    assert_eq!(spread.remaining.len(), 5);
}

/// Without remaining dates, a spread takes the early dates, as
/// `query --usage` does.
#[test]
fn a_spread_without_remaining_dates_takes_the_early_dates() {
    let mut s = spreadable();
    let a2 = s.activities.get_mut(&ActivityId(2)).unwrap();
    a2.stored.early_start = a2.stored.remaining_early_start.take();
    a2.stored.early_finish = a2.stored.remaining_early_finish.take();
    run(&mut s, typed("week", "2026-03-02", &[40.0])).unwrap();
    let spread = crew_on_a2(&s).typed_spread.clone().unwrap();
    assert_eq!(spread.remaining, hours(&[(40.0, 40)]));
    let a2 = s.activities.get_mut(&ActivityId(2)).unwrap();
    a2.stored.early_start = None;
    refused(
        &s,
        typed("week", "2026-03-02", &[40.0]),
        "no-remaining-dates",
    );
}

#[test]
fn spreads_that_don_t_fit_are_refused() {
    let s = spreadable();
    // Units must add up to the remaining 40.
    refused(
        &s,
        typed("day", "2026-03-02", &[8.0, 8.0, 8.0, 8.0, 7.0]),
        "spread-units",
    );
    refused(&s, typed("day", "2026-03-02", &[9.0; 5]), "spread-units");
    // Periods must lie within the remaining dates and cover them: not a
    // sixth day, a week past the finish, or four days of five.
    refused(
        &s,
        typed("week", "2026-03-01", &[40.0, 0.0]),
        "spread-outside-dates",
    );
    refused(
        &s,
        typed("day", "2026-03-02", &[10.0; 4]),
        "spread-outside-dates",
    );
    refused(
        &s,
        typed("day", "2026-03-02", &[8.0; 6]),
        "spread-outside-dates",
    );
    refused(
        &s,
        typed("day", "2026-02-27", &[10.0; 4]),
        "spread-outside-dates",
    );
    refused(
        &s,
        typed("day", "2026-03-03", &[10.0; 4]),
        "spread-outside-dates",
    );
    // A weekend has no work for units.
    let weekend = typed("day", "2026-03-06", &[0.0, 0.0, 0.0]);
    refused(&s, weekend, "spread-outside-dates");
    refused(
        &s,
        typed("week", "2026-02-28", &[39.0, 1.0]),
        "spread-outside-dates",
    );
    // Bad input.
    refused(&s, typed("day", "2026-03-02", &[]), "invalid-spread");
    refused(
        &s,
        typed("day", "2026-03-02", &[48.0, -8.0]),
        "invalid-units",
    );
    refused(&s, typed("month", "2026-03-02", &[40.0]), "invalid-spread");
    refused(&s, typed("day", "March", &[40.0]), "invalid-date");
    let mut nodates = s.clone();
    nodates
        .activities
        .get_mut(&ActivityId(2))
        .unwrap()
        .stored
        .remaining_early_start = None;
    refused(
        &nodates,
        typed("day", "2026-03-02", &[8.0; 5]),
        "no-remaining-dates",
    );
    let mut done = s.clone();
    done.activities.get_mut(&ActivityId(2)).unwrap().status = ActivityStatus::Completed;
    refused(
        &done,
        typed("day", "2026-03-02", &[8.0; 5]),
        "finished-work",
    );
    let mut lost = typed("day", "2026-03-02", &[8.0; 5]);
    lost["resource"] = "NOBODY".into();
    refused(&s, lost, "no-such-resource");
}

/// A curve name the schedule lacks is suggested from its curves.
#[test]
fn a_mistyped_curve_is_suggested() {
    use super::{Name, holds, suggest};
    let s = spreadable();
    let wanted = super::Edit::SetCurve {
        activity: "A2".into(),
        resource: "CREW".into(),
        curve: Some("Bel Shaped".into()),
    };
    let missing: Vec<Name> = wanted
        .needs()
        .into_iter()
        .filter(|n| !holds(&s, n))
        .collect();
    assert_eq!(missing, [Name::Curve("Bel Shaped".into())]);
    assert_eq!(suggest(&[s], &missing[0]), ["Bell Shaped"]);
}

fn role_limit(json: serde_json::Value) -> serde_json::Value {
    let mut edit = serde_json::json!({"op": "set_role_limit", "role": "ENG"});
    let serde_json::Value::Object(fields) = json else {
        panic!("give the fields as an object");
    };
    edit.as_object_mut().unwrap().extend(fields);
    edit
}

fn role_limits(s: &Schedule) -> Vec<Held> {
    let role = s.roles.values().find(|r| r.short_name == "ENG").unwrap();
    role.rates
        .iter()
        .map(|r| {
            let from = r.effective.map(|d| d.to_string());
            (r.id.0, from, r.prices, r.max_units_per_hour)
        })
        .collect()
}

/// A role's limit is set, changed and cleared on every rate, as a
/// resource's is, and a role with no rate gets one from the project's start.
#[test]
fn a_role_s_limit_is_set_and_cleared_on_every_rate() {
    let mut s = chain();
    s.project.planned_start = "2026-03-02 08:00".parse().ok();
    let opening = Some("2026-03-02 00:00".to_owned());
    run(&mut s, role("ENG", "Engineer", None)).unwrap();
    let line = run(
        &mut s,
        role_limit(serde_json::json!({"max_units_per_hour": 2})),
    )
    .unwrap();
    assert_eq!(line, "ENG now has at most 2 units per hour");
    assert_eq!(
        role_limits(&s),
        [(100, opening.clone(), [None; 5], Some(2.0))]
    );
    run(
        &mut s,
        role_limit(serde_json::json!({"max_units_per_hour": 3})),
    )
    .unwrap();
    assert_eq!(
        role_limits(&s),
        [(100, opening.clone(), [None; 5], Some(3.0))]
    );
    let line = run(
        &mut s,
        role_limit(serde_json::json!({"max_units_per_hour": null})),
    )
    .unwrap();
    assert_eq!(line, "ENG now has no limit");
    assert_eq!(role_limits(&s), [(100, opening, [None; 5], None)]);
    refused(
        &s,
        role_limit(serde_json::json!({"max_units_per_hour": null})),
        "nothing-to-change",
    );
    refused(
        &s,
        role_limit(serde_json::json!({"max_units_per_hour": -1})),
        "invalid-units",
    );
    let nobody =
        serde_json::json!({"op": "set_role_limit", "role": "NOPE", "max_units_per_hour": 1});
    refused(&s, nobody, "no-such-role");
}

/// A dated role limit starts that day with the prices in force then, and a
/// rate from the project's start keeps what held before (TF-001 item 6).
#[test]
fn a_dated_role_limit_doesn_t_reach_back() {
    use totalfloat_model::{RoleRate, RoleRateId};
    let mut s = chain();
    s.project.planned_start = "2026-03-02 08:00".parse().ok();
    run(&mut s, role("ENG", "Engineer", None)).unwrap();
    let eng = s
        .roles
        .values_mut()
        .find(|r| r.short_name == "ENG")
        .unwrap();
    eng.rates = vec![RoleRate {
        id: RoleRateId(7),
        effective: "2026-04-01 00:00".parse().ok(),
        prices: [Some(90.0), None, None, None, None],
        max_units_per_hour: Some(1.0),
        source: None,
    }];
    let line = run(
        &mut s,
        role_limit(serde_json::json!({"max_units_per_hour": 4, "from": "2026-05-04"})),
    )
    .unwrap();
    assert_eq!(line, "ENG has at most 4 units per hour from 2026-05-04");
    let price = [Some(90.0), None, None, None, None];
    assert_eq!(
        role_limits(&s),
        [
            (7, Some("2026-04-01 00:00".to_owned()), price, Some(1.0)),
            (100, Some("2026-05-04 00:00".to_owned()), price, Some(4.0)),
        ]
    );
    // Before the only rate starts: a rate from the project's start keeps
    // what held before, so the new limit doesn't reach back.
    let line = run(
        &mut s,
        role_limit(serde_json::json!({"max_units_per_hour": 2, "from": "2026-03-16"})),
    )
    .unwrap();
    assert_eq!(line, "ENG has at most 2 units per hour from 2026-03-16");
    let held = role_limits(&s);
    assert_eq!(
        held[2..],
        [
            (101, Some("2026-03-02 00:00".to_owned()), price, Some(1.0)),
            (102, Some("2026-03-16 00:00".to_owned()), price, Some(2.0)),
        ]
    );
    // Cleared from a day on.
    run(
        &mut s,
        role_limit(serde_json::json!({"max_units_per_hour": null, "from": "2026-05-04"})),
    )
    .unwrap();
    assert_eq!(role_limits(&s)[1].3, None);
}

/// `add_role` takes a limit, on a rate from the project's start.
#[test]
fn a_new_role_can_have_a_limit() {
    let mut s = chain();
    s.project.planned_start = "2026-03-02 08:00".parse().ok();
    let json = serde_json::json!({"op": "add_role", "role": "ENG", "name": "Engineer", "max_units_per_hour": 2});
    let line = run(&mut s, json).unwrap();
    assert_eq!(line, "added role ENG, at most 2 units per hour");
    assert_eq!(
        role_limits(&s),
        [(
            100,
            Some("2026-03-02 00:00".to_owned()),
            [None; 5],
            Some(2.0)
        )]
    );
    let bad =
        serde_json::json!({"op": "add_role", "role": "X", "name": "X", "max_units_per_hour": -2});
    refused(&s, bad, "invalid-units");
}
