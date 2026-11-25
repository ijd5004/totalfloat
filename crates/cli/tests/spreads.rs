//! How an assignment's units spread over time, end to end: a hand-typed
//! spread read from an XER file shapes `totalfloat_engine::spread`,
//! and `edit` chooses a curve, an even spread or typed units through the
//! CLI, and `query --usage` shows the chosen shape (the MCP seam is
//! tested in `src/mcp/tests.rs`). The CLI crate is the one that sees the
//! codec, the edits and the engine together.

#![allow(clippy::unwrap_used, reason = "test code")]

use std::path::{Path, PathBuf};
use std::process::Command;

use totalfloat_calendar::WorkCalendar;
use totalfloat_engine::{
    Distribution, Period, assignment_calendar, remaining_distribution, spread,
};
use totalfloat_model::{self as m, Schedule};
use totalfloat_xer::rawdiff::{changed_cells, diff_lines};
use totalfloat_xer::{raw, to_model};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/xer")
        .join(name)
}

fn read(path: &Path) -> Schedule {
    let raw = raw::read(std::fs::read(path).unwrap()).unwrap();
    to_model(&raw).0.swap_remove(0)
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

/// A1's remaining units per day by its distribution, rounded to the
/// thousandth.
fn by_day(schedule: &Schedule) -> Vec<(String, f64)> {
    let assignment = schedule.assignments.values().next().unwrap();
    let activity = &schedule.activities[&assignment.activity];
    let calendar = assignment_calendar(schedule, assignment).unwrap();
    let calendar = WorkCalendar::new(&schedule.calendars[&calendar]).unwrap();
    spread(
        assignment.amounts.remaining_units.unwrap(),
        activity.stored.remaining_early_start.unwrap(),
        activity.stored.remaining_early_finish.unwrap(),
        remaining_distribution(schedule, assignment),
        &calendar,
        Period::Day,
    )
    .unwrap()
    .iter()
    .map(|p| (p.start.to_string(), (p.units * 1000.0).round() / 1000.0))
    .collect()
}

fn days(list: &[(&str, f64)]) -> Vec<(String, f64)> {
    list.iter().map(|&(d, u)| (d.to_owned(), u)).collect()
}

/// The fixture's typed spread gives its typed units per day.
#[test]
fn a_typed_spread_in_a_file_gives_its_typed_units_per_day() {
    let schedule = read(&fixture("spreads.xer"));
    let assignment = schedule.assignments.values().next().unwrap();
    assert!(matches!(
        remaining_distribution(&schedule, assignment),
        Distribution::Typed(_)
    ));
    assert_eq!(
        by_day(&schedule),
        days(&[
            ("2026-01-05", 12.0),
            ("2026-01-06", 4.0),
            ("2026-01-07", 0.0),
            ("2026-01-08", 12.0),
            ("2026-01-09", 12.0),
            ("2026-01-10", 0.0),
        ])
    );
}

/// `fixtures/xer/curves.xer` recalculated, so an edit's own recalculation
/// changes nothing else: A1 has R1 on the curve "Late", with 40 hours left
/// from Monday 2026-01-05 08:00 to Monday 2026-01-12 08:00.
fn calculated(directory: &Path) -> PathBuf {
    let base = directory.join("base.xer");
    let source = fixture("curves.xer");
    let (code, out, err) = run(&[
        "schedule",
        source.to_str().unwrap(),
        "-o",
        base.to_str().unwrap(),
    ]);
    assert_eq!(code, Some(0), "{out}{err}");
    base
}

/// Edits `base` with the flags and returns the new file, checking the only
/// changed line is A1's assignment row and the changed cells are `fields`.
fn edited(base: &Path, flags: &[&str], fields: &[&str]) -> PathBuf {
    static RUNS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = RUNS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let out = base.with_file_name(format!("edited-{n}.xer"));
    let mut args = vec!["edit", base.to_str().unwrap()];
    args.extend(flags);
    args.extend(["-o", out.to_str().unwrap()]);
    let (code, stdout, stderr) = run(&args);
    assert_eq!(code, Some(0), "{stdout}{stderr}");
    let (before, after) = (std::fs::read(base).unwrap(), std::fs::read(&out).unwrap());
    let d = diff_lines(&before, &after);
    assert_eq!((d.removed.len(), d.added.len()), (1, 1), "{d:?}");
    assert!(d.removed[0].1.starts_with("%R\t801\t"), "{d:?}");
    let raw = raw::read(after).unwrap();
    let table = raw.table("TASKRSRC").unwrap();
    let mut changed: Vec<&str> = changed_cells(&d.removed[0].1, &d.added[0].1)
        .iter()
        .map(|&c| table.fields[c - 1].as_str())
        .collect();
    changed.sort_unstable();
    let mut wanted = fields.to_vec();
    wanted.sort_unstable();
    assert_eq!(changed, wanted, "{flags:?}");
    out
}

fn cell(path: &Path, field: &str) -> String {
    let raw = raw::read(std::fs::read(path).unwrap()).unwrap();
    let table = raw.table("TASKRSRC").unwrap();
    table.rows[0]
        .get(table.field_index(field).unwrap())
        .unwrap()
        .to_owned()
}

/// `--curve` changes one `curv_id` cell, `--spread-evenly` clears it,
/// and `--spread` writes the marker and the spread cells the reader decodes, which
/// read back as the typed spread and spread by day as typed. A1 is under way
/// with no typed plan, so the planned cell stays empty (TF-040).
#[test]
fn edit_chooses_a_curve_an_even_spread_or_typed_units() {
    let directory = tempfile::tempdir().unwrap();
    let base = calculated(directory.path());
    assert_eq!(cell(&base, "curv_id"), "2");

    let linear = edited(&base, &["--curve", "A1=R1;linear"], &["curv_id"]);
    assert_eq!(cell(&linear, "curv_id"), "1");
    let schedule = read(&linear);
    assert_eq!(
        schedule.assignments.values().next().unwrap().curve,
        Some(m::ResourceCurveId(1))
    );
    // Linear is even over work time: 8 a day.
    let even_days = days(&[
        ("2026-01-05", 8.0),
        ("2026-01-06", 8.0),
        ("2026-01-07", 8.0),
        ("2026-01-08", 8.0),
        ("2026-01-09", 8.0),
        ("2026-01-10", 0.0),
        ("2026-01-11", 0.0),
        ("2026-01-12", 0.0),
    ]);
    assert_eq!(by_day(&schedule), even_days);

    let even = edited(&base, &["--spread-evenly", "A1=R1"], &["curv_id"]);
    assert_eq!(cell(&even, "curv_id"), "");
    assert_eq!(by_day(&read(&even)), even_days);

    let typed = edited(
        &base,
        &["--spread", "A1=R1;day=2026-01-05;12,4,0,12,12"],
        &["curv_id", "remain_crv"],
    );
    assert_eq!(cell(&typed, "curv_id"), "9");
    assert_eq!(cell(&typed, "remain_crv"), "12:8;4:8;0:8;12:8;12:8");
    assert_eq!(cell(&typed, "target_crv"), "");
    let schedule = read(&typed);
    let spread = schedule
        .assignments
        .values()
        .next()
        .unwrap()
        .typed_spread
        .clone()
        .unwrap();
    let hours = |u: f64| m::SpreadSegment {
        units: u,
        work: m::Minutes::new(480),
    };
    assert_eq!(
        spread.remaining,
        [
            hours(12.0),
            hours(4.0),
            hours(0.0),
            hours(12.0),
            hours(12.0)
        ]
    );
    assert_eq!(
        by_day(&schedule),
        days(&[
            ("2026-01-05", 12.0),
            ("2026-01-06", 4.0),
            ("2026-01-07", 0.0),
            ("2026-01-08", 12.0),
            ("2026-01-09", 12.0),
            ("2026-01-10", 0.0),
            ("2026-01-11", 0.0),
            ("2026-01-12", 0.0),
        ])
    );
    // Back to even from typed clears the marker and the spread cells.
    edited(
        &typed,
        &["--spread-evenly", "A1=R1"],
        &["curv_id", "remain_crv"],
    );
}

/// `base` with A1's expected finish cleared, so its dates follow its
/// remaining duration, and that set to two weeks: Monday 2026-01-05 08:00 to
/// Friday 2026-01-16 17:00.
fn two_weeks(base: &Path) -> PathBuf {
    let text = String::from_utf8(std::fs::read(base).unwrap()).unwrap();
    let mut table = "";
    let mut fields: Vec<&str> = Vec::new();
    let mut lines = Vec::new();
    for line in text.split('\n') {
        let cells: Vec<&str> = line.trim_end_matches('\r').split('\t').collect();
        match cells[0] {
            "%T" => table = cells[1],
            "%F" => fields = cells[1..].to_vec(),
            "%R" if table == "TASK" => {
                let at = |f: &str| 1 + fields.iter().position(|x| *x == f).unwrap();
                if cells[at("task_code")] == "A1" {
                    let mut cells = cells.clone();
                    cells[at("expect_end_date")] = "";
                    let end = if line.ends_with('\r') { "\r" } else { "" };
                    lines.push(format!("{}{end}", cells.join("\t")));
                    continue;
                }
            }
            _ => {}
        }
        lines.push(line.to_owned());
    }
    let cleared = base.with_file_name("no-expected-finish.xer");
    std::fs::write(&cleared, lines.join("\n")).unwrap();
    let out = base.with_file_name("two-weeks.xer");
    let (code, stdout, stderr) = run(&[
        "edit",
        cleared.to_str().unwrap(),
        "--remaining",
        "A1=10d",
        "-o",
        out.to_str().unwrap(),
    ]);
    assert_eq!(code, Some(0), "{stdout}{stderr}");
    out
}

/// R1's remaining units per week from `query --usage`.
fn weekly_usage(path: &Path) -> Vec<(String, f64)> {
    let (code, stdout, stderr) = run(&[
        "query",
        path.to_str().unwrap(),
        "--usage",
        "R1",
        "--by",
        "week",
        "--json",
    ]);
    assert_eq!(code, Some(0), "{stdout}{stderr}");
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    json["periods"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            (
                p["start"].as_str().unwrap().to_owned(),
                p["remaining"].as_f64().unwrap(),
            )
        })
        .collect()
}

/// A planner sees the chosen spread in units per period: `query
/// --usage` puts the late curve's units in the second week, the linear
/// curve's and an even spread's half in each, and typed weeks as typed.
#[test]
fn query_usage_follows_the_chosen_spread() {
    let directory = tempfile::tempdir().unwrap();
    let long = two_weeks(&calculated(directory.path()));
    let weeks = |first: f64, second: f64| days(&[("2026-01-05", first), ("2026-01-12", second)]);
    assert_eq!(weekly_usage(&long), weeks(0.0, 40.0));
    let chosen = |flag: &str, value: &str, name: &str| {
        let out = long.with_file_name(name);
        let (code, stdout, stderr) = run(&[
            "edit",
            long.to_str().unwrap(),
            flag,
            value,
            "-o",
            out.to_str().unwrap(),
        ]);
        assert_eq!(code, Some(0), "{stdout}{stderr}");
        weekly_usage(&out)
    };
    assert_eq!(
        chosen("--curve", "A1=R1;linear", "linear.xer"),
        weeks(20.0, 20.0)
    );
    assert_eq!(
        chosen("--spread-evenly", "A1=R1", "even.xer"),
        weeks(20.0, 20.0)
    );
    assert_eq!(
        chosen("--spread", "A1=R1;week=2026-01-05;30,10", "typed.xer"),
        weeks(30.0, 10.0)
    );
}

/// What each new flag says in `edit --help`.
#[test]
fn the_new_flags_are_in_the_help() {
    let (code, stdout, _) = run(&["edit", "--help"]);
    assert_eq!(code, Some(0));
    let lines: Vec<&str> = stdout.lines().collect();
    let mut shown = String::new();
    for flag in ["--curve ", "--spread-evenly ", "--spread "] {
        let at = lines
            .iter()
            .position(|l| l.trim_start().starts_with(flag))
            .unwrap();
        shown.push_str(lines[at].trim());
        shown.push('\n');
        shown.push_str(lines[at + 1].trim());
        shown.push('\n');
    }
    insta::assert_snapshot!(shown);
}

/// An unknown curve (with a suggestion), an unknown assignment, units
/// that don't add up, and periods outside the dates are refused, and a flag
/// that can't be read says its form.
#[test]
fn spreads_that_can_t_be_chosen_are_refused() {
    let directory = tempfile::tempdir().unwrap();
    let base = calculated(directory.path());
    let base = base.to_str().unwrap();
    let mut all = String::new();
    for (flag, value, code) in [
        ("--curve", "A1=R1;Lineer", "no-such-curve"),
        ("--curve", "M1=R1;Linear", "not-assigned"),
        ("--spread-evenly", "A1=NOBODY", "no-such-resource"),
        ("--spread", "A1=R1;day=2026-01-05;8,8,8,8,9", "spread-units"),
        (
            "--spread",
            "A1=R1;day=2026-01-05;8,8,8,8,8,0,0,0,0",
            "spread-outside-dates",
        ),
        (
            "--spread",
            "A1=R1;day=2026-01-06;10,10,10,10",
            "spread-outside-dates",
        ),
        ("--spread", "A1=R1;fortnight=2026-01-05;40", "invalid-edit"),
        ("--curve", "A1=R1", "invalid-edit"),
    ] {
        let (status, stdout, stderr) = run(&["edit", base, flag, value, "--dry-run"]);
        assert_eq!(status, Some(1), "{flag} {value}: {stdout}{stderr}");
        assert!(stderr.contains(code), "{flag} {value}: {stderr}");
        all.push_str(&stderr);
    }
    insta::assert_snapshot!(all);
}
