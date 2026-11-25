//! `totalfloat report`: templated reports, on a synthetic plan built
//! from schedule text.

#![allow(clippy::unwrap_used, reason = "test helpers outside #[test] functions")]

use std::path::Path;
use std::process::Command;

/// A synthetic plan: two WBS nodes, a critical chain, work off it, and a
/// name with a comma and a bar in it.
const PLAN: &str = r#"totalfloat-schedule 1

project PLANT "New Plant"
  data-date 2026-03-02 08:00

wbs PLANT.CIV "Civil works"

activity A1010 "Excavate" 5d
activity A1020 "Pour foundations" 10d after A1010
activity A1030 "Cure, then strip | forms" 7d after A1020
activity A1040 "Survey" 2d

wbs PLANT.MEC "Mechanical"

activity A2010 "Order pumps" 3d
activity A2020 "Install pumps" 6d after A2010, A1030
milestone M100 "Plant complete" after A2020
"#;

/// Runs the binary in `directory`, so reports name the file as given.
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

/// The plan, built into `plan.xer` in a new directory.
fn plan() -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("plan.tfs"), PLAN).unwrap();
    let (code, _, stderr) = run(directory.path(), &["import", "plan.tfs", "-o", "plan.xer"]);
    assert_eq!(code, Some(0), "{stderr}");
    directory
}

#[test]
fn each_built_in_template_in_each_format() {
    let directory = plan();
    for template in ["look-ahead", "critical-path"] {
        for format in ["markdown", "csv", "html"] {
            let (code, stdout, stderr) = run(
                directory.path(),
                &["report", template, "plan.xer", "--format", format],
            );
            assert_eq!(code, Some(0), "{template} {format}: {stderr}");
            insta::assert_snapshot!(format!("report_{template}_{format}"), stdout);
        }
    }
}

#[test]
fn a_template_file_groups_filters_and_sorts_as_it_says() {
    let directory = plan();
    std::fs::write(
        directory.path().join("float.toml"),
        r#"title = "Work with float"
columns = ["code", "name", "total_float_hours"]
group = "wbs"
sort = [{ column = "total_float_hours", descending = true }]

[filter]
float_min = 1
"#,
    )
    .unwrap();
    let (code, stdout, stderr) = run(directory.path(), &["report", "float.toml", "plan.xer"]);
    assert_eq!(code, Some(0), "{stderr}");
    insta::assert_snapshot!("report_template_file", stdout);
    // To a new file, which is never overwritten.
    let (code, stdout, stderr) = run(
        directory.path(),
        &[
            "report",
            "float.toml",
            "plan.xer",
            "--format",
            "csv",
            "-o",
            "float.csv",
        ],
    );
    assert_eq!(code, Some(0), "{stderr}");
    assert_eq!(stdout, "Wrote float.csv\n");
    let written = std::fs::read_to_string(directory.path().join("float.csv")).unwrap();
    assert_eq!(
        written,
        "wbs,code,name,total_float_hours\nCIV,A1040,Survey,208.0\nMEC,A2010,Order pumps,152.0\n"
    );
    let (code, _, stderr) = run(
        directory.path(),
        &["report", "float.toml", "plan.xer", "-o", "float.csv"],
    );
    assert_ne!(code, Some(0));
    assert!(stderr.contains("exists"), "{stderr}");
}

#[test]
fn a_template_that_can_t_be_read_is_refused_with_why() {
    let directory = plan();
    std::fs::write(
        directory.path().join("bad.toml"),
        "title = \"T\"\ncolumns = [\"code\"]\n[filter]\nlate = true\n",
    )
    .unwrap();
    std::fs::write(
        directory.path().join("column.toml"),
        "title = \"T\"\ncolumns = [\"colour\"]\n",
    )
    .unwrap();
    for (template, said) in [
        ("look-behind", "no-such-template"),
        ("bad.toml", "filter.late"),
        ("column.toml", "colour"),
    ] {
        let (code, _, stderr) = run(directory.path(), &["report", template, "plan.xer"]);
        assert_ne!(code, Some(0), "{template}");
        assert!(stderr.contains(said), "{template}: {stderr}");
    }
}

/// Each project's look-ahead window starts at its own data date, and a
/// descending sort still puts empty values last.
#[test]
fn windows_follow_each_project_and_empty_values_sort_last() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::copy(
        "../../fixtures/xer/two-projects.xer",
        directory.path().join("pair.xer"),
    )
    .unwrap();
    let (code, _, stderr) = run(
        directory.path(),
        &[
            "edit",
            "pair.xer",
            "--project",
            "2",
            "--data-date",
            "2026-02-02 08:00",
            "-o",
            "moved.xer",
        ],
    );
    assert_eq!(code, Some(0), "{stderr}");
    let (code, stdout, stderr) = run(
        directory.path(),
        &["report", "look-ahead", "moved.xer", "--format", "csv"],
    );
    assert_eq!(code, Some(0), "{stderr}");
    let codes: Vec<&str> = stdout
        .lines()
        .skip(1)
        .map(|l| l.split(',').nth(1).unwrap())
        .collect();
    // B1 starts on project 2's data date, after project 1's window ends.
    assert_eq!(codes, ["A1", "B1"], "{stdout}");
    // A name only one project has is found across both, not refused.
    std::fs::write(
        directory.path().join("wp.toml"),
        "title = \"WP\"\ncolumns = [\"code\"]\ndays_from_data_date = 21\n[filter]\nwbs = \"WP\"\n",
    )
    .unwrap();
    let (code, stdout, stderr) = run(
        directory.path(),
        &["report", "wp.toml", "moved.xer", "--format", "csv"],
    );
    assert_eq!(code, Some(0), "{stderr}");
    assert_eq!(stdout, "code\nA1\n");
    std::fs::write(
        directory.path().join("desc.toml"),
        "title = \"By float\"\ncolumns = [\"code\", \"total_float_hours\"]\nsort = [{ column = \"total_float_hours\", descending = true }]\n",
    )
    .unwrap();
    let (code, stdout, stderr) = run(
        directory.path(),
        &["report", "desc.toml", "pair.xer", "--format", "csv"],
    );
    assert_eq!(code, Some(0), "{stderr}");
    assert_eq!(stdout, "code,total_float_hours\nB1,0.0\nA1,-1.5\nM1,\n");
}

/// Totals per group and for the report, in each format.
#[test]
fn totals_per_group_and_for_the_report_in_each_format() {
    let directory = plan();
    std::fs::write(
        directory.path().join("durations.toml"),
        r#"title = "Durations"
columns = ["code", "name", "original_duration_hours", "remaining_duration_hours"]
group = "wbs"
totals = ["original_duration_hours", "remaining_duration_hours"]
"#,
    )
    .unwrap();
    for format in ["markdown", "csv", "html"] {
        let (code, stdout, stderr) = run(
            directory.path(),
            &["report", "durations.toml", "plan.xer", "--format", format],
        );
        assert_eq!(code, Some(0), "{format}: {stderr}");
        insta::assert_snapshot!(format!("report_totals_{format}"), stdout);
    }
    // Without grouping, the one total row is the report's.
    std::fs::write(
        directory.path().join("flat.toml"),
        r#"title = "Durations"
columns = ["original_duration_hours", "code"]
sort = [{ column = "code" }]
totals = ["original_duration_hours"]

[filter]
wbs = "MEC"
"#,
    )
    .unwrap();
    let (code, stdout, stderr) = run(
        directory.path(),
        &["report", "flat.toml", "plan.xer", "--format", "csv"],
    );
    assert_eq!(code, Some(0), "{stderr}");
    assert_eq!(
        stdout,
        "original_duration_hours,code\n24.0,A2010\n48.0,A2020\n0.0,M100\n72.0,total\n"
    );
    let (code, stdout, stderr) = run(directory.path(), &["report", "flat.toml", "plan.xer"]);
    assert_eq!(code, Some(0), "{stderr}");
    assert!(stdout.ends_with("| **72.0** | **Total** |\n"), "{stdout}");
}

/// A total of something that doesn't add up, or isn't shown, is refused.
#[test]
fn a_total_that_doesn_t_add_up_is_refused() {
    let directory = plan();
    for (columns, total, said) in [
        (r#"["code", "name"]"#, "name", "isn't a number that adds up"),
        (
            r#"["code", "total_float_hours"]"#,
            "total_float_hours",
            "isn't a number that adds up",
        ),
        (
            r#"["code"]"#,
            "original_duration_hours",
            "isn't one of the report's columns",
        ),
        (
            r#"["original_duration_hours"]"#,
            "original_duration_hours",
            "leaves none to label",
        ),
    ] {
        std::fs::write(
            directory.path().join("t.toml"),
            format!("title = \"T\"\ncolumns = {columns}\ntotals = [\"{total}\"]\n"),
        )
        .unwrap();
        let (code, _, stderr) = run(directory.path(), &["report", "t.toml", "plan.xer"]);
        assert_ne!(code, Some(0), "{total}");
        assert!(stderr.contains("bad-template"), "{total}: {stderr}");
        assert!(stderr.contains(&format!("totals.{total}")), "{stderr}");
        assert!(stderr.contains(said), "{total}: {stderr}");
    }
}

/// A user field is totalled by its type, not by the values that happen to
/// match: a text field is refused even where every matching value is empty.
#[test]
fn a_text_user_field_is_refused_as_a_total_whatever_matches() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::copy(
        "../../fixtures/xer/fresh-writer.xer",
        directory.path().join("plan.xer"),
    )
    .unwrap();
    std::fs::write(
        directory.path().join("t.toml"),
        "title = \"T\"\ncolumns = [\"code\", \"udf:Synthetic note\"]\ntotals = [\"udf:Synthetic note\"]\n[filter]\ntext = \"milestone\"\n",
    )
    .unwrap();
    let (code, _, stderr) = run(directory.path(), &["report", "t.toml", "plan.xer"]);
    assert_ne!(code, Some(0));
    assert!(stderr.contains("is a text field"), "{stderr}");
}
