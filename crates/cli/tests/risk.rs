//! `totalfloat risk`: Monte Carlo schedule risk through the command.

#![allow(clippy::unwrap_used, reason = "test helpers outside #[test] functions")]

use std::process::Command;

use serde_json::Value;

/// Two activities, A1 then A2, finish to start.
const FIXTURE: &str = "../../fixtures/xer/batches/001/rel-fs-positive.xer";

fn risk(args: &[&str]) -> (Option<i32>, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_totalfloat"))
        .arg("risk")
        .args(args)
        .output()
        .unwrap();
    (
        out.status.code(),
        String::from_utf8(out.stdout).unwrap(),
        String::from_utf8(out.stderr).unwrap(),
    )
}

fn json(args: &[&str]) -> Value {
    let mut all = args.to_vec();
    all.push("--json");
    let (code, stdout, stderr) = risk(&all);
    assert_eq!(code, Some(0), "{stderr}");
    serde_json::from_str(&stdout).unwrap()
}

fn inputs(text: &str) -> (tempfile::TempDir, String) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("risk.json");
    std::fs::write(&path, text).unwrap();
    let path = path.to_str().unwrap().to_owned();
    (dir, path)
}

#[test]
fn the_text_report() {
    let (code, stdout, stderr) = risk(&[
        FIXTURE,
        "--range=-10,25",
        "--iterations",
        "500",
        "--target",
        "2026-01-13 18:00",
    ]);
    assert_eq!(code, Some(0), "{stderr}");
    insta::assert_snapshot!(stdout);
}

#[test]
fn no_uncertainty_gives_the_deterministic_finish() {
    let report = json(&[FIXTURE, "--range=0,0", "--iterations", "50"]);
    let finish = &report["deterministic_finish"];
    assert_eq!(report["deterministic_probability"], 1.0);
    for p in report["percentiles"].as_array().unwrap() {
        assert_eq!(&p["finish"], finish);
    }
    assert_eq!(report["sensitivity"], Value::Array(vec![]));
}

#[test]
fn the_json_follows_its_schema() {
    let report = json(&[FIXTURE, "--range=-10,25", "--iterations", "100"]);
    let schema: Value = serde_json::from_str(
        &std::fs::read_to_string("../../docs/schemas/risk-analysis-1.schema.json").unwrap(),
    )
    .unwrap();
    assert_eq!(report["schema"], "totalfloat.risk-analysis/1");
    let keys = |value: &Value| {
        let mut keys: Vec<String> = value.as_object().unwrap().keys().cloned().collect();
        keys.sort();
        keys
    };
    assert_eq!(keys(&report), keys(&schema["properties"]));
    assert_eq!(report["percentiles"].as_array().unwrap().len(), 19);
}

#[test]
fn the_seed_decides_the_result() {
    let run = |seed: &str| {
        json(&[
            FIXTURE,
            "--range=-10,25",
            "--iterations",
            "200",
            "--seed",
            seed,
        ])
    };
    assert_eq!(run("7"), run("7"));
    assert_ne!(run("7")["percentiles"], run("8")["percentiles"]);
}

#[test]
fn estimates_and_correlation_from_a_file() {
    // A1 alone carries the risk: it's the only activity sampled.
    let (_dir, path) = inputs(
        r#"{
            "schema": "totalfloat.risk-inputs/1",
            "estimates": {"A1": {"min": "4h", "max": "16h", "distribution": "pert"}},
            "correlations": [{"activities": ["A1"], "coefficient": 0.5}]
        }"#,
    );
    let report = json(&[FIXTURE, "--inputs", &path, "--iterations", "200"]);
    assert_eq!(report["sampled_activities"], 1);
    assert_eq!(report["sensitivity"][0]["code"], "A1");
}

#[test]
fn every_problem_in_the_inputs_is_named() {
    let (_dir, path) = inputs(
        r#"{
            "estimates": {
                "NOPE": {"min": "1d", "max": "2d"},
                "A1": {"min": "16h", "likely": "8h", "max": "20h"}
            },
            "correlations": [{"activities": ["GONE"], "coefficient": 0.5}]
        }"#,
    );
    let (code, _, stderr) = risk(&[FIXTURE, "--inputs", &path]);
    assert_ne!(code, Some(0));
    assert!(stderr.contains("NOPE: no such activity"), "{stderr}");
    assert!(stderr.contains("GONE: no such activity"), "{stderr}");
    let (_dir, path) =
        inputs(r#"{"estimates": {"A1": {"min": "16h", "likely": "8h", "max": "20h"}}}"#);
    let (code, _, stderr) = risk(&[FIXTURE, "--inputs", &path]);
    assert_ne!(code, Some(0));
    assert!(
        stderr.contains("A1: minimum, most likely and maximum out of order"),
        "{stderr}"
    );
    let (_dir, path) = inputs(r#"{"estimates": {}, "extra": 1}"#);
    let (code, _, stderr) = risk(&[FIXTURE, "--inputs", &path]);
    assert_ne!(code, Some(0));
    assert!(stderr.contains("extra"), "{stderr}");
}

#[test]
fn nothing_to_sample_is_refused() {
    let (code, _, stderr) = risk(&[FIXTURE]);
    assert_ne!(code, Some(0));
    assert!(stderr.contains("nothing to sample"), "{stderr}");
}

#[test]
fn estimates_from_user_defined_fields() {
    // Add two number fields and give A1 an estimate in days with
    // `edit`.
    let dir = tempfile::tempdir().unwrap();
    let with_fields = dir.path().join("fields.xer");
    let out = Command::new(env!("CARGO_BIN_EXE_totalfloat"))
        .args([
            "edit",
            FIXTURE,
            "--add-udf-type",
            "Optimistic;number",
            "--add-udf-type",
            "Pessimistic;number",
            "--udf",
            "A1=Optimistic=0.5",
            "--udf",
            "A1=Pessimistic=3",
            "-o",
        ])
        .arg(&with_fields)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let (_inputs, path) = inputs(r#"{"udfs": {"min": "Optimistic", "max": "Pessimistic"}}"#);
    let report = json(&[
        with_fields.to_str().unwrap(),
        "--inputs",
        &path,
        "--iterations",
        "300",
    ]);
    assert_eq!(report["sampled_activities"], 1);
    assert_eq!(report["sensitivity"][0]["code"], "A1");
    // A1 with only its optimistic field set is a problem, unless the file
    // gives its estimate, which wins over the fields.
    let half = dir.path().join("half.xer");
    let out = Command::new(env!("CARGO_BIN_EXE_totalfloat"))
        .args([
            "edit",
            with_fields.to_str().unwrap(),
            "--clear-udf",
            "A1=Pessimistic",
            "-o",
        ])
        .arg(&half)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let (_inputs, path) = inputs(r#"{"udfs": {"min": "Optimistic", "max": "Pessimistic"}}"#);
    let (code, _, stderr) = risk(&[half.to_str().unwrap(), "--inputs", &path]);
    assert_ne!(code, Some(0));
    assert!(
        stderr.contains("A1: an estimate field is empty"),
        "{stderr}"
    );
    let (_inputs, path) = inputs(
        r#"{"udfs": {"min": "Optimistic", "max": "Pessimistic"},
            "estimates": {"A1": {"min": "4h", "max": "16h"}}}"#,
    );
    let report = json(&[
        half.to_str().unwrap(),
        "--inputs",
        &path,
        "--iterations",
        "50",
    ]);
    assert_eq!(report["sampled_activities"], 1);
    // A missing field is named.
    let (_inputs, path) = inputs(r#"{"udfs": {"min": "Optimistic", "max": "Nope"}}"#);
    let (code, _, stderr) = risk(&[with_fields.to_str().unwrap(), "--inputs", &path]);
    assert_ne!(code, Some(0));
    assert!(stderr.contains("udf Nope"), "{stderr}");
}

#[test]
fn review_fixes_on_inputs() {
    // An empty most-likely field means the remaining duration.
    let dir = tempfile::tempdir().unwrap();
    let with_fields = dir.path().join("fields.xer");
    let out = Command::new(env!("CARGO_BIN_EXE_totalfloat"))
        .args([
            "edit",
            FIXTURE,
            "--add-udf-type",
            "Optimistic;number",
            "--add-udf-type",
            "Likely;number",
            "--add-udf-type",
            "Pessimistic;number",
            "--udf",
            "A1=Optimistic=0.5",
            "--udf",
            "A1=Pessimistic=3",
            "-o",
        ])
        .arg(&with_fields)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let file = with_fields.to_str().unwrap();
    let (_i, path) =
        inputs(r#"{"udfs": {"min": "Optimistic", "likely": "Likely", "max": "Pessimistic"}}"#);
    let report = json(&[file, "--inputs", &path, "--iterations", "50"]);
    assert_eq!(report["sampled_activities"], 1);
    // Fields that exist but hold nothing say so.
    let (_i, path) = inputs(r#"{"udfs": {"min": "Likely", "max": "Likely"}}"#);
    let (code, _, stderr) = risk(&[file, "--inputs", &path]);
    assert_ne!(code, Some(0));
    assert!(
        stderr.contains("no activity has values in the named fields"),
        "{stderr}"
    );
    // A span in a period the calendar has no hours for names that period.
    let (_i, path) = inputs(r#"{"estimates": {"A1": {"min": "1w", "max": "2w"}}}"#);
    let (code, _, stderr) = risk(&[FIXTURE, "--inputs", &path]);
    assert_ne!(code, Some(0));
    assert!(stderr.contains("hours in a week for A1"), "{stderr}");
    // Iterations out of range are refused before anything runs.
    let (code, _, _) = risk(&[FIXTURE, "--range=-10,25", "--iterations", "0"]);
    assert_eq!(code, Some(2));
}

#[test]
fn an_expected_finish_activity_is_listed_not_refused() {
    // Project 1 of the two-project fixture: A1 has an expected finish, which
    // sets its duration, so neither the default range nor an estimate draws
    // it, and the report says so.
    let fixture = "../../fixtures/xer/two-projects.xer";
    let (_i, path) = inputs(
        r#"{"default": {"min": -10, "max": 25}, "estimates": {"A1": {"min": "2h", "max": "8h"}}}"#,
    );
    let report = json(&[
        fixture,
        "--project",
        "1",
        "--inputs",
        &path,
        "--iterations",
        "20",
    ]);
    assert_eq!(report["set_by_expected_finish"], serde_json::json!(["A1"]));
    assert_eq!(report["sampled_activities"], 0);
}

#[test]
fn risk_events_are_ranked() {
    let (_i, path) = inputs(
        r#"{"default": {"min": 0, "max": 0}, "risks": [
            {"code": "WEATHER", "probability": 0.3, "impacts": {"A2": {"min": "2h", "likely": "4h", "max": "16h"}}},
            {"code": "R-01", "probability": 1, "impacts": {"A1": {"min": "8h", "max": "8h"}}}
        ]}"#,
    );
    let report = json(&[FIXTURE, "--inputs", &path, "--iterations", "500"]);
    let risks = report["risks"].as_array().unwrap();
    assert_eq!(risks[0]["code"], "R-01");
    assert_eq!(risks[0]["occurred"], 1.0);
    // Eight working hours from 17:00 is 17:00 the next working day.
    assert_eq!(risks[0]["p80_effect_days"], 1.0);
    assert_eq!(risks[1]["code"], "WEATHER");
    let occurred = risks[1]["occurred"].as_f64().unwrap();
    assert!((occurred - 0.3).abs() < 0.06, "{occurred}");
    // Without `likely`, a delay is uniform whatever distribution is named:
    // P(delay happens) = 1 and a 0 to 16 hour uniform delay on A2 gives a
    // P50 effect near 8 working hours, not the 0 a peak at the minimum
    // would pull it toward.
    let (_i, path) = inputs(
        r#"{"default": {"min": 0, "max": 0}, "risks": [
            {"code": "R", "probability": 1, "impacts": {"A2": {"min": "0h", "max": "16h", "distribution": "pert"}}}
        ]}"#,
    );
    let report = json(&[FIXTURE, "--inputs", &path, "--iterations", "2000"]);
    let p50 = report["risks"][0]["p50_effect_days"].as_f64().unwrap();
    assert!(p50 > 0.3, "{p50}");
    // Two risks with one code are refused.
    let (_i, path) = inputs(
        r#"{"risks": [
            {"code": "R", "probability": 0, "impacts": {"A1": {"min": "1h", "max": "2h"}}},
            {"code": "r", "probability": 1, "impacts": {"A1": {"min": "1h", "max": "2h"}}}
        ]}"#,
    );
    let (code, _, stderr) = risk(&[FIXTURE, "--inputs", &path]);
    assert_ne!(code, Some(0));
    assert!(stderr.contains("r: two risks have this code"), "{stderr}");
    // A risk alone is enough to run, and a bad one is named.
    let (_i, path) = inputs(
        r#"{"risks": [{"code": "R9", "probability": 2, "impacts": {"NOPE": {"min": "1h", "max": "2h"}}}]}"#,
    );
    let (code, _, stderr) = risk(&[FIXTURE, "--inputs", &path]);
    assert_ne!(code, Some(0));
    assert!(stderr.contains("R9: NOPE: no such activity"), "{stderr}");
    // ...along with its probability, and a delay in weeks the calendar
    // can't measure names the week.
    assert!(
        stderr.contains("R9: probability 2 is outside 0 to 1"),
        "{stderr}"
    );
    let (_i, path) = inputs(
        r#"{"risks": [{"code": "R8", "probability": 0.5, "impacts": {"A1": {"min": "1w", "max": "2w"}}}]}"#,
    );
    let (code, _, stderr) = risk(&[FIXTURE, "--inputs", &path]);
    assert_ne!(code, Some(0));
    assert!(
        stderr.contains("R8: A1: there's no number of hours in a week for A1"),
        "{stderr}"
    );
}
