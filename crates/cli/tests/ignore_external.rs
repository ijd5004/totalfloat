//! `schedule --ignore-external`: relationships to and from other
//! projects, and the external dates they leave on activities, ignored for
//! one run (TF-030).

#![allow(clippy::unwrap_used, reason = "test helpers outside #[test] functions")]

use std::path::Path;
use std::process::Command;

use serde_json::Value;

const FIXTURE: &str = "../../fixtures/xer/two-projects.xer";

fn schedule(file: &Path, args: &[&str]) -> (Option<i32>, Value, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_totalfloat"))
        .arg("schedule")
        .arg(file)
        .args(args)
        .arg("--json")
        .output()
        .unwrap();
    let stdout = String::from_utf8(out.stdout).unwrap();
    let stderr = String::from_utf8(out.stderr).unwrap();
    (
        out.status.code(),
        serde_json::from_str(&stdout).unwrap_or(Value::Null),
        stderr,
    )
}

/// Each project's code and calculated finish.
fn finishes(report: &Value) -> Vec<(String, String)> {
    report["projects"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            (
                p["code"].as_str().unwrap().to_owned(),
                p["scheduled_finish"].as_str().unwrap().to_owned(),
            )
        })
        .collect()
}

/// The fixture with `from` replaced by `to`, in a new directory.
fn variant(from: &str, to: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let text = std::fs::read_to_string(FIXTURE).unwrap();
    assert!(text.contains(from), "the fixture has no {from:?}");
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("plan.xer");
    std::fs::write(&file, text.replace(from, to)).unwrap();
    (directory, file)
}

#[test]
fn a_link_from_another_project_is_ignored_only_when_asked() {
    // Project 2's B1 follows project 1's A1, finish to start with 16 hours' lag.
    let (_dir, file) = variant(
        "%R\t1002\t2\t201\t1\t101\tPR_SS\t-1",
        "%R\t1002\t2\t201\t1\t101\tPR_FS\t16",
    );
    let (code, counted, stderr) = schedule(&file, &[]);
    assert_eq!(code, Some(0), "{stderr}");
    let (code, ignored, stderr) = schedule(&file, &["--ignore-external"]);
    assert_eq!(code, Some(0), "{stderr}");
    let sy2 = |r: &Value| {
        finishes(r)
            .into_iter()
            .find(|(c, _)| c == "SYN2")
            .unwrap()
            .1
    };
    assert_eq!(sy2(&counted), "2026-01-15 13:15");
    assert_eq!(sy2(&ignored), "2026-01-06 13:15");
    // Project 1 has no link in, so it's the same either way.
    let sy1 = |r: &Value| {
        finishes(r)
            .into_iter()
            .find(|(c, _)| c == "SYN1")
            .unwrap()
            .1
    };
    assert_eq!(sy1(&counted), sy1(&ignored));
    assert_eq!(ignored["ignore_external"], true);
    assert!(counted.get("ignore_external").is_none());
}

#[test]
fn an_external_early_start_is_ignored_only_when_asked() {
    // B1 can't start before its external early start, a week in.
    let (_dir, file) = variant(
        "2026-01-01 08:00\t2026-02-01 17:00",
        "2026-01-12 08:00\t2026-02-01 17:00",
    );
    let (_, counted, stderr) = schedule(&file, &["--project", "2"]);
    assert_eq!(
        finishes(&counted),
        [("SYN2".to_owned(), "2026-01-13 13:15".to_owned())],
        "{stderr}"
    );
    let (_, ignored, stderr) = schedule(&file, &["--project", "2", "--ignore-external"]);
    assert_eq!(
        finishes(&ignored),
        [("SYN2".to_owned(), "2026-01-06 13:15".to_owned())],
        "{stderr}"
    );
}

#[test]
fn a_link_to_a_project_not_in_the_file_can_be_ignored() {
    let file = Path::new("../../fixtures/xer/model-mapping.xer");
    let (code, refused, _) = schedule(file, &[]);
    assert_ne!(code, Some(0));
    let message = refused["error"]["message"].as_str().unwrap();
    assert!(message.contains("external relationships"), "{message}");
    let (code, report, stderr) = schedule(file, &["--ignore-external"]);
    assert_eq!(code, Some(0), "{stderr}");
    assert_ne!(finishes(&report), []);
}

#[test]
fn the_text_report_says_so_and_the_file_keeps_its_setting() {
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("out.xer");
    let out = Command::new(env!("CARGO_BIN_EXE_totalfloat"))
        .args(["schedule", FIXTURE, "--ignore-external", "-o"])
        .arg(&output)
        .output()
        .unwrap();
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(
        stdout.starts_with("Relationships to and from other projects ignored"),
        "{stdout}"
    );
    // Nothing in the file says the option was used: its SCHEDOPTIONS rows are as they were.
    let rows = |text: &str| {
        text.lines()
            .skip_while(|l| !l.starts_with("%T\tSCHEDOPTIONS"))
            .take_while(|l| !l.starts_with("%T\t") || l.starts_with("%T\tSCHEDOPTIONS"))
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    let before = std::fs::read_to_string(FIXTURE).unwrap();
    let after = std::fs::read_to_string(&output).unwrap();
    assert_eq!(rows(&after), rows(&before));
}
