//! `schedule --log`: a log of each calculation, on a synthetic plan
//! with open ends, out-of-sequence progress, a constraint and negative float.

#![allow(clippy::unwrap_used, reason = "test helpers outside #[test] functions")]

use std::path::Path;
use std::process::Command;

const PLAN: &str = r#"totalfloat-schedule 1

project PLANT "New Plant"
  data-date 2026-03-02 08:00

wbs PLANT.CIV "Civil works"

activity A1010 "Excavate" 5d
activity A1020 "Pour foundations" 10d after A1010
activity A1030 "Cure" 5d after A1020
activity A1040 "Survey" 2d
milestone M100 "Civil complete" after A1030
"#;

fn run(directory: &Path, args: &[&str]) -> (Option<i32>, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_totalfloat"))
        .current_dir(directory)
        .args(args)
        .output()
        .unwrap();
    (
        out.status.code(),
        String::from_utf8(out.stdout).unwrap(),
        String::from_utf8(out.stderr).unwrap(),
    )
}

/// The plan, progressed out of sequence with a constraint it can't meet.
fn progressed() -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    let at = directory.path();
    std::fs::write(at.join("plan.tfs"), PLAN).unwrap();
    let (code, _, stderr) = run(at, &["import", "plan.tfs", "-o", "plan.xer"]);
    assert_eq!(code, Some(0), "{stderr}");
    let (code, _, stderr) = run(
        at,
        &[
            "edit",
            "plan.xer",
            "--actual-start",
            "A1010=2026-03-02 08:00",
            "--actual-start",
            "A1020=2026-03-04 08:00",
            "--data-date",
            "2026-03-06 08:00",
            "--constraint",
            "A1030=finish-on-or-before@2026-03-16 17:00",
            "-o",
            "progressed.xer",
        ],
    );
    assert_eq!(code, Some(0), "{stderr}");
    directory
}

#[test]
fn the_log_lists_settings_counts_every_problem_and_the_results() {
    let directory = progressed();
    let at = directory.path();
    let (code, _, stderr) = run(at, &["schedule", "progressed.xer", "--log", "run.log"]);
    assert_eq!(code, Some(0), "{stderr}");
    let log = std::fs::read_to_string(at.join("run.log")).unwrap();
    insta::assert_snapshot!("schedule_log", log);
    // The same run, the same bytes, with the output written as well.
    let (code, _, stderr) = run(
        at,
        &[
            "schedule",
            "progressed.xer",
            "--log",
            "again.log",
            "-o",
            "out.xer",
        ],
    );
    assert_eq!(code, Some(0), "{stderr}");
    assert_eq!(std::fs::read_to_string(at.join("again.log")).unwrap(), log);
}

#[test]
fn an_existing_log_or_one_named_as_the_output_is_refused() {
    let directory = progressed();
    let at = directory.path();
    std::fs::write(at.join("kept.log"), "mine").unwrap();
    let (code, _, stderr) = run(at, &["schedule", "progressed.xer", "--log", "kept.log"]);
    assert_ne!(code, Some(0));
    assert!(stderr.contains("output-exists"), "{stderr}");
    assert_eq!(
        std::fs::read_to_string(at.join("kept.log")).unwrap(),
        "mine"
    );
    let (code, _, stderr) = run(
        at,
        &[
            "schedule",
            "progressed.xer",
            "--log",
            "same.xer",
            "-o",
            "same.xer",
        ],
    );
    assert_ne!(code, Some(0));
    assert!(stderr.contains("same file"), "{stderr}");
    assert!(!at.join("same.xer").exists());
}
