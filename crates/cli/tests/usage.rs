//! A resource's usage over time, from `fixtures/xer/usage.xer`, written
//! fresh from `model-mapping.xer`'s first project with a resource limit and
//! two assignments: one part done, one over the limit.

#![allow(clippy::unwrap_used, reason = "test code")]

use std::path::Path;
use std::process::Command;

use totalfloat_model as m;
use totalfloat_xer::fresh::{FreshOptions, write};
use totalfloat_xer::{raw, to_model};

fn root() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

const FIXTURE: &str = "../../fixtures/xer/usage.xer";

fn date(text: &str) -> m::DateTime {
    text.parse().unwrap()
}

/// The model the fixture is written from. The crew can work 1 unit an hour
/// on its calendar. A1 has 16h done and 24h left; A2, after it, 120h over
/// 80h of work, is half as much again as the limit.
fn authored() -> m::Schedule {
    let input =
        raw::read(std::fs::read(root().join("fixtures/xer/model-mapping.xer")).unwrap()).unwrap();
    let mut s = to_model(&input).0.swap_remove(0);
    let project = s.project.id;
    s.relationships.clear();
    let crew = s.resources.get_mut(&m::ResourceId(501)).unwrap();
    "CREW".clone_into(&mut crew.short_name);
    crew.rates = vec![m::ResourceRate {
        id: m::ResourceRateId(1),
        effective: Some(date("2026-01-01 00:00")),
        prices: [Some(50.0), None, None, None, None],
        max_units_per_hour: Some(1.0),
        source: None,
    }];
    let a1 = s.activities.get_mut(&m::ActivityId(101)).unwrap();
    a1.remaining_duration = m::Minutes::new(24 * 60);
    a1.stored.remaining_early_start = Some(date("2026-01-05 08:00"));
    a1.stored.remaining_early_finish = Some(date("2026-01-07 17:00"));
    a1.actual_start = Some(date("2026-01-01 08:00"));
    let first = s.assignments.get_mut(&m::AssignmentId(801)).unwrap();
    first.amounts.budgeted_units = Some(40.0);
    first.amounts.actual_units = Some(16.0);
    first.amounts.remaining_units = Some(24.0);
    let mut a2 = m::Activity::new(
        m::ActivityId(103),
        project,
        m::WbsId(12),
        m::CalendarId(10),
        "A2",
        "Synthetic busy task",
    );
    a2.original_duration = m::Minutes::new(80 * 60);
    a2.remaining_duration = m::Minutes::new(80 * 60);
    a2.stored.early_start = Some(date("2026-01-08 08:00"));
    a2.stored.early_finish = Some(date("2026-01-21 17:00"));
    a2.stored.remaining_early_start = a2.stored.early_start;
    a2.stored.remaining_early_finish = a2.stored.early_finish;
    s.activities.insert(a2.id, a2);
    s.relationships.insert(
        m::RelationshipId(901),
        m::Relationship {
            id: m::RelationshipId(901),
            predecessor: m::ActivityId(101),
            successor: m::ActivityId(103),
            predecessor_project: project,
            successor_project: project,
            kind: m::RelationshipType::FinishToStart,
            lag: m::Minutes::ZERO,
            source: None,
        },
    );
    let mut second = s.assignments[&m::AssignmentId(801)].clone();
    second.id = m::AssignmentId(803);
    second.activity = m::ActivityId(103);
    second.udfs.clear();
    second.source = None;
    second.amounts.budgeted_units = Some(120.0);
    second.amounts.actual_units = Some(0.0);
    second.amounts.remaining_units = Some(120.0);
    s.assignments.insert(second.id, second);
    s
}

fn run(args: &[&str]) -> (Option<i32>, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_totalfloat"))
        .args(args)
        .output()
        .unwrap();
    (
        out.status.code(),
        String::from_utf8(out.stdout).unwrap(),
        String::from_utf8(out.stderr).unwrap(),
    )
}

/// Set `TOTALFLOAT_UPDATE_CONTRACT=1` to rewrite the fixture after a
/// deliberate change, and record why in fixtures/xer/MANIFEST.md. The model
/// is written fresh and then calculated, so its stored dates are the engine's
/// and the file reads the same whether `serve` calculates it again or not.
#[test]
fn the_fixture_is_the_authored_model_written_fresh_and_calculated() {
    let mut options = FreshOptions::new("2026-01-01".parse().unwrap());
    options.seed = 42;
    let mut written = Vec::new();
    write(&authored(), options)
        .unwrap()
        .write(&mut written)
        .unwrap();
    let directory = tempfile::tempdir().unwrap();
    let fresh = directory.path().join("fresh.xer");
    let calculated = directory.path().join("calculated.xer");
    std::fs::write(&fresh, &written).unwrap();
    let (status, stdout, stderr) = run(&[
        "schedule",
        fresh.to_str().unwrap(),
        "-o",
        calculated.to_str().unwrap(),
    ]);
    assert_eq!(status, Some(0), "{stdout}{stderr}");
    let calculated = std::fs::read(calculated).unwrap();
    let path = root().join("fixtures/xer/usage.xer");
    if std::env::var_os("TOTALFLOAT_UPDATE_CONTRACT").is_some() {
        std::fs::write(&path, &calculated).unwrap();
        return;
    }
    assert!(
        std::fs::read(&path).unwrap() == calculated,
        "fixtures/xer/usage.xer is out of date; rerun with TOTALFLOAT_UPDATE_CONTRACT=1"
    );
}

#[test]
fn usage_by_week_as_text() {
    let (status, stdout, stderr) = run(&["query", FIXTURE, "--usage", "CREW", "--by", "week"]);
    assert_eq!(status, Some(0), "{stderr}");
    insta::assert_snapshot!(stdout);
}

#[test]
fn usage_by_month_as_json() {
    let (status, stdout, stderr) = run(&[
        "query", FIXTURE, "--usage", "crew", "--by", "month", "--json",
    ]);
    assert_eq!(status, Some(0), "{stderr}");
    insta::assert_snapshot!(stdout);
    let report: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let schema: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(root().join("docs/schemas/usage-1.schema.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(report["schema"], "totalfloat.usage/1");
    let keys = |value: &serde_json::Value| {
        let mut keys: Vec<String> = value.as_object().unwrap().keys().cloned().collect();
        keys.sort();
        keys
    };
    assert_eq!(keys(&report), keys(&schema["properties"]));
    assert_eq!(
        keys(&report["periods"][0]),
        keys(&schema["$defs"]["Usage"]["properties"])
    );
}

#[test]
fn a_resource_that_isn_t_there_is_named() {
    let (status, _, stderr) = run(&["query", FIXTURE, "--usage", "NOPE"]);
    assert_ne!(status, Some(0));
    assert!(
        stderr.contains("no-such-resource") && stderr.contains("CREW"),
        "{stderr}"
    );
}
