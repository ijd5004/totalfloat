//! Scheduling linked projects together: projects with no stored dates,
//! loops through links between projects, and float measured from the finish
//! of the projects open together (TF-031).

#![allow(clippy::unwrap_used, reason = "test helpers outside #[test] functions")]

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

const FIXTURE: &str = "../../fixtures/xer/two-projects.xer";

/// B1's stored dates and floats, in `TASK` column order.
const B1_STORED: &str = "2026-02-01 17:00\t2026-01-05 08:00\t2026-01-06 13:15\t2026-01-05 08:00\t2026-01-06 13:15\t2026-01-05 08:00\t2026-01-06 13:15\t2026-01-05 08:00\t2026-01-06 13:15\t0\t0\t";

/// M1's two constraints.
const M1_CONSTRAINTS: &str = "CS_MEOB\t2026-02-01 17:00\tCS_MSOA\t2026-01-20 08:00";

/// The link from project 1's A1 to project 2's B1.
const A1_TO_B1: &str = "%R\t1002\t2\t201\t1\t101\tPR_SS\t-1";

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

/// The fixture with each `(from, to)` replaced, in a new directory.
fn variant(replacements: &[(&str, &str)]) -> (tempfile::TempDir, PathBuf) {
    let mut text = std::fs::read_to_string(FIXTURE).unwrap();
    for (from, to) in replacements {
        assert!(text.contains(from), "the fixture has no {from:?}");
        text = text.replace(from, to);
    }
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("plan.xer");
    std::fs::write(&file, text).unwrap();
    (directory, file)
}

/// A project's report by its code.
fn project<'a>(report: &'a Value, code: &str) -> &'a Value {
    report["projects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["code"] == code)
        .unwrap()
}

/// Project 1's link from A1 to M1.
const A1_TO_M1: &str = "%R\t1001\t1\t102\t1\t101\tPR_FS\t0.5";

/// A1's expected finish.
const A1_EXPECTED: (&str, &str) = (
    "2026-01-01 08:00\t\t\t\t2026-01-10 17:00",
    "2026-01-01 08:00\t\t\t\t",
);

/// Project 1's M1, unconstrained, follows project 2's B1 instead of A1,
/// finish to start with 16 hours' lag, and B1 has no stored dates. A1 has
/// no expected finish, so it finishes first.
fn m1_after_undated_b1() -> (tempfile::TempDir, PathBuf) {
    variant(&[
        (M1_CONSTRAINTS, "\t\t\t"),
        A1_EXPECTED,
        (A1_TO_M1, "%R\t1001\t1\t102\t2\t201\tPR_FS\t16"),
        (B1_STORED, &format!("2026-02-01 17:00{}", "\t".repeat(11))),
    ])
}

#[test]
fn a_project_with_no_stored_dates_is_calculated_with_the_project_it_links_to() {
    let (_dir, file) = m1_after_undated_b1();
    let (code, report, stderr) = schedule(&file, &[]);
    assert_eq!(code, Some(0), "{stderr}");
    // B1 works 12.25 hours from the data date, 2026-01-05 08:00, on an
    // 08:00-12:00, 13:00-17:00 week: it finishes 2026-01-06 13:15. Sixteen
    // hours' lag runs 3.75 that day, 8 on the 7th and 4.25 on the 8th.
    assert_eq!(
        project(&report, "SYN2")["scheduled_finish"],
        "2026-01-06 13:15"
    );
    assert_eq!(
        project(&report, "SYN1")["scheduled_finish"],
        "2026-01-08 13:15"
    );
}

#[test]
fn a_project_not_chosen_still_stands_in_with_its_stored_dates() {
    // With only project 1 open, B1 isn't calculated, so there are no dates to read.
    let (_dir, file) = m1_after_undated_b1();
    let (code, refused, _) = schedule(&file, &["--project", "1"]);
    assert_ne!(code, Some(0));
    let message = refused["error"]["message"].as_str().unwrap();
    assert!(message.contains("no stored dates"), "{message}");
}

#[test]
fn a_change_ripples_through_projects_in_both_directions() {
    // A1 (project 1) drives B1 (project 2), which drives M1 (project 1).
    // A1's expected finish is taken off, so its remaining work sets its finish.
    let (_dir, file) = variant(&[
        (M1_CONSTRAINTS, "\t\t\t"),
        A1_EXPECTED,
        (A1_TO_M1, "%R\t1001\t1\t102\t2\t201\tPR_FS\t0"),
        (A1_TO_B1, "%R\t1002\t2\t201\t1\t101\tPR_FS\t8"),
        (B1_STORED, &format!("2026-02-01 17:00{}", "\t".repeat(11))),
    ]);
    let (code, report, stderr) = schedule(&file, &[]);
    assert_eq!(code, Some(0), "{stderr}");
    // A1 has 4 hours left from 2026-01-05 08:00, to 12:00. Eight hours' lag,
    // on B1's calendar as project 2's option says, runs to 2026-01-06 12:00,
    // which isn't work, so B1 starts 13:00 and works
    // 12.25 hours: 4 on the 6th, 8 on the 7th, 0.25 on the 8th.
    assert_eq!(
        project(&report, "SYN2")["scheduled_finish"],
        "2026-01-08 08:15"
    );
    assert_eq!(
        project(&report, "SYN1")["scheduled_finish"],
        "2026-01-08 08:15"
    );
}

#[test]
fn a_loop_through_links_between_projects_is_named() {
    let (_dir, file) = variant(&[
        (M1_CONSTRAINTS, "\t\t\t"),
        (A1_TO_B1, "%R\t1002\t1\t102\t2\t201\tPR_FS\t0"),
        (
            A1_TO_M1,
            "%R\t1001\t1\t102\t1\t101\tPR_FS\t0.5\n%R\t1003\t2\t201\t1\t102\tPR_FS\t0",
        ),
    ]);
    let (code, refused, _) = schedule(&file, &[]);
    assert_ne!(code, Some(0));
    let message = refused["error"]["message"].as_str().unwrap();
    assert!(message.contains("make a loop"), "{message}");
    assert!(message.contains("SYN1: M1"), "{message}");
    assert!(message.contains("SYN2: B1"), "{message}");
}

/// Both projects measure float from the opened projects' finish.
const OPENED: (&str, &str) = ("SD_Both\tY\tN", "SD_Both\tN\tN");

#[test]
fn float_from_the_opened_projects_finish() {
    let (_dir, file) = variant(&[OPENED]);
    let (code, together, stderr) = schedule(&file, &[]);
    assert_eq!(code, Some(0), "{stderr}");
    // Project 1's finish milestone is held to 2026-01-20 08:00, well after
    // project 2's own finish, 2026-01-06 13:15.
    let syn1 = project(&together, "SYN1")["scheduled_finish"].clone();
    assert_eq!(syn1, "2026-01-20 08:00");
    assert_eq!(project(&together, "SYN2")["scheduled_finish"], syn1);

    // Open alone, project 2 measures from its own finish.
    let (code, alone, stderr) = schedule(&file, &["--project", "2"]);
    assert_eq!(code, Some(0), "{stderr}");
    assert_eq!(
        project(&alone, "SYN2")["scheduled_finish"],
        "2026-01-06 13:15"
    );

    // From each project's finish, as the fixture has it.
    let (code, each, stderr) = schedule(Path::new(FIXTURE), &[]);
    assert_eq!(code, Some(0), "{stderr}");
    assert_eq!(
        project(&each, "SYN2")["scheduled_finish"],
        "2026-01-06 13:15"
    );
}

#[test]
fn the_written_file_holds_the_opened_projects_finish() {
    let (dir, file) = variant(&[OPENED]);
    let output = dir.path().join("out.xer");
    let (code, _, stderr) = schedule(&file, &["-o", output.to_str().unwrap()]);
    assert_eq!(code, Some(0), "{stderr}");
    let written = std::fs::read_to_string(&output).unwrap();
    let row = written
        .lines()
        .find(|l| l.starts_with("%R\t2\tSYN2\t"))
        .unwrap();
    assert!(row.ends_with("\t2026-01-20 08:00"), "{row}");
    // Read again, the file is settled: scheduling it changes nothing.
    let (code, again, stderr) = schedule(&output, &[]);
    assert_eq!(code, Some(0), "{stderr}");
    for p in again["projects"].as_array().unwrap() {
        assert_eq!(p["changed_dates_or_float"], 0, "{p}");
    }
}

#[test]
fn as_late_as_possible_work_doesn_t_hold_the_opened_finish() {
    // Both projects end in as-late-as-possible work whose stored early finish,
    // 2026-03-02 17:00, is left from an older calculation. Read back from the
    // file, each would hold the other's finish there. Their own work
    // finishes far earlier: project 1's A1 at 2026-01-12 08:00 (its expected
    // finish moved off the weekend), then M1 half an hour later.
    let (_dir, file) = variant(&[
        OPENED,
        (
            &format!("{M1_CONSTRAINTS}\t\t\t\t"),
            &format!("CS_ALAP{}2026-03-02 17:00", "\t".repeat(7)),
        ),
        (
            "2026-01-06 13:15\t\t\t\t\t\t\t\t\t\t2026-01-01 08:00",
            "2026-01-06 13:15\t\t\t\t\t\tCS_ALAP\t\t\t\t2026-01-01 08:00",
        ),
        (
            "2026-02-01 17:00\t2026-01-05 08:00\t2026-01-06 13:15\t",
            "2026-02-01 17:00\t2026-01-05 08:00\t2026-03-02 17:00\t",
        ),
    ]);
    let (code, report, stderr) = schedule(&file, &[]);
    assert_eq!(code, Some(0), "{stderr}");
    assert_eq!(
        project(&report, "SYN1")["scheduled_finish"],
        "2026-01-12 08:30"
    );
    assert_eq!(
        project(&report, "SYN2")["scheduled_finish"],
        "2026-01-12 08:30"
    );
}
