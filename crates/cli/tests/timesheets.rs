//! Timesheets: `timesheet` writes each team member's work for a
//! period, and `edit --from-timesheet` applies the hours and progress they
//! fill in, changing only those cells, refusing a sheet applied twice, and
//! naming every bad cell at its file, sheet, row and column.

#![allow(clippy::unwrap_used, reason = "test helpers outside #[test] functions")]

use std::path::Path;
use std::process::Command;

use rust_xlsxwriter::Workbook;
use serde_json::Value;

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

const EDITS: &str = r#"[
  {"op": "add_resource", "resource": "ELEC", "name": "Electrician", "resource_type": "labor"},
  {"op": "assign_resource", "activity": "A1", "resource": "ELEC"}
]"#;

/// The usage fixture with a second team member on A1, scheduled so that a
/// run's recalculation changes nothing else.
fn settled(at: &Path) {
    std::fs::copy("../../fixtures/xer/usage.xer", at.join("plan.xer")).unwrap();
    std::fs::write(at.join("edits.json"), EDITS).unwrap();
    let (code, stdout, stderr) = run(
        at,
        &["edit", "plan.xer", "--edits", "edits.json", "-o", "two.xer"],
    );
    assert_eq!(code, Some(0), "{stdout}{stderr}");
    let (code, _, stderr) = run(at, &["schedule", "two.xer", "-o", "settled.xer"]);
    assert_eq!(code, Some(0), "{stderr}");
}

/// The week's timesheets as CSV files, read back as rows of cells.
fn week(at: &Path, folder: &str) -> Vec<(String, Vec<Vec<String>>)> {
    let (code, stdout, stderr) = run(
        at,
        &[
            "timesheet",
            "settled.xer",
            "--from",
            "2026-01-05",
            "--to",
            "2026-01-11",
            "--csv",
            folder,
        ],
    );
    assert_eq!(code, Some(0), "{stdout}{stderr}");
    let mut sheets: Vec<_> = std::fs::read_dir(at.join(folder))
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    sheets.sort();
    sheets
        .iter()
        .map(|path| {
            let text = std::fs::read_to_string(path).unwrap();
            let rows = text
                .lines()
                .map(|line| line.split(',').map(str::to_owned).collect())
                .collect();
            (
                path.file_stem().unwrap().to_string_lossy().into_owned(),
                rows,
            )
        })
        .collect()
}

/// Sets `column` on the row for `activity`.
fn fill(rows: &mut [Vec<String>], activity: &str, cells: &[(&str, &str)]) {
    let header = rows[0].clone();
    let row = rows.iter().position(|r| r[0] == activity).unwrap();
    for (column, value) in cells {
        let k = header.iter().position(|c| c == column).unwrap();
        (*value).clone_into(&mut rows[row][k]);
    }
}

fn write_csv(path: &Path, rows: &[Vec<String>]) {
    let text: String = rows.iter().map(|r| r.join(",") + "\n").collect();
    std::fs::write(path, text).unwrap();
}

/// The cells two XER files differ in, as `TABLE.field` once for each.
fn changed_cells(before: &Path, after: &Path) -> Vec<String> {
    let read = |path: &Path| std::fs::read_to_string(path).unwrap();
    let (a, b) = (read(before), read(after));
    let (a, b): (Vec<&str>, Vec<&str>) = (a.lines().collect(), b.lines().collect());
    assert_eq!(a.len(), b.len(), "the files have different rows");
    let (mut table, mut fields) = ("", Vec::new());
    let mut changed = Vec::new();
    for (x, y) in a.iter().zip(&b) {
        if let Some(name) = x.strip_prefix("%T\t") {
            table = name;
        } else if let Some(names) = x.strip_prefix("%F\t") {
            fields = names.split('\t').collect();
        } else if x != y {
            let cells = x.split('\t').skip(1).zip(y.split('\t').skip(1));
            for (k, (p, q)) in cells.enumerate() {
                if p != q {
                    changed.push(format!("{table}.{}", fields.get(k).unwrap_or(&"?")));
                }
            }
        }
    }
    changed.sort();
    changed
}

/// The problems a refused run names, as (where, code), the file left off.
fn problems(stdout: &str) -> Vec<(String, String)> {
    let error: Value = serde_json::from_str(stdout).unwrap();
    error["error"]["problems"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            let source = p["source"].as_str().unwrap();
            // The file's name, after either separator.
            let at = source
                .rsplit_once(['/', '\\'])
                .map_or(source, |(_, rest)| rest);
            (at.to_owned(), p["code"].as_str().unwrap().to_owned())
        })
        .collect()
}

#[test]
fn a_filled_timesheet_changes_only_its_cells_and_is_refused_a_second_time() {
    let directory = tempfile::tempdir().unwrap();
    let at = directory.path();
    settled(at);
    let mut sheets = week(at, "week");
    let names: Vec<&str> = sheets.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, ["CREW", "ELEC"]);
    // A2 starts the week after, so only A1 is on the sheets.
    for (_, rows) in &sheets {
        assert_eq!(rows.len(), 2, "{rows:?}");
        assert_eq!(rows[1][0], "A1");
    }
    let crew = &mut sheets[0].1;
    fill(
        crew,
        "A1",
        &[("hours", "8"), ("remaining_hours", "16"), ("percent", "60")],
    );
    write_csv(&at.join("week/CREW.csv"), crew);
    let (code, stdout, stderr) = run(
        at,
        &[
            "edit",
            "settled.xer",
            "--from-timesheet",
            "week",
            "-o",
            "out.xer",
        ],
    );
    assert_eq!(code, Some(0), "{stdout}{stderr}");
    // ELEC reported nothing, so its actuals on A1 are still to record.
    assert!(
        stdout.contains("A1's assignments have no new actual units"),
        "{stdout}"
    );
    // The progress, the assignment's units, and the activity's work totals,
    // which follow its assignments'.
    assert_eq!(
        changed_cells(&at.join("settled.xer"), &at.join("out.xer")),
        [
            "TASK.act_work_qty",
            "TASK.phys_complete_pct",
            "TASK.remain_work_qty",
            "TASKRSRC.act_reg_qty",
            "TASKRSRC.remain_qty",
            "TASKRSRC.remain_qty_per_hr",
        ]
    );
    // The same rows in a workbook, a sheet a resource, give the same file.
    let mut book = Workbook::new();
    for (name, rows) in &sheets {
        let sheet = book.add_worksheet();
        sheet.set_name(name).unwrap();
        for (r, row) in rows.iter().enumerate() {
            for (c, cell) in row.iter().enumerate() {
                let (r, c) = (u32::try_from(r).unwrap(), u16::try_from(c).unwrap());
                match cell.parse::<f64>() {
                    Ok(n) => sheet.write_number(r, c, n).map(|_| ()),
                    Err(_) if cell.is_empty() => Ok(()),
                    Err(_) => sheet.write_string(r, c, cell).map(|_| ()),
                }
                .unwrap();
            }
        }
    }
    book.save(at.join("week.xlsx")).unwrap();
    let (code, stdout, stderr) = run(
        at,
        &[
            "edit",
            "settled.xer",
            "--from-timesheet",
            "week.xlsx",
            "-o",
            "book.xer",
        ],
    );
    assert_eq!(code, Some(0), "{stdout}{stderr}");
    assert_eq!(
        std::fs::read(at.join("out.xer")).unwrap(),
        std::fs::read(at.join("book.xer")).unwrap()
    );
    // Applied again: the hours recorded have moved on, so the row is refused.
    let (code, stdout, _) = run(
        at,
        &[
            "edit",
            "out.xer",
            "--from-timesheet",
            "week",
            "-o",
            "again.xer",
            "--json",
        ],
    );
    assert_eq!(code, Some(1), "{stdout}");
    assert!(!at.join("again.xer").exists());
    assert_eq!(
        problems(&stdout),
        [(
            "CREW.csv CREW row 2 column actual_hours".to_owned(),
            "timesheet-out-of-date".to_owned()
        )]
    );
}

#[test]
fn two_team_members_progress_applies_when_it_agrees_and_is_refused_when_it_differs() {
    let directory = tempfile::tempdir().unwrap();
    let at = directory.path();
    settled(at);
    let mut sheets = week(at, "week");
    fill(&mut sheets[0].1, "A1", &[("hours", "8"), ("percent", "60")]);
    fill(&mut sheets[1].1, "A1", &[("hours", "4"), ("percent", "60")]);
    for (name, rows) in &sheets {
        write_csv(&at.join(format!("week/{name}.csv")), rows);
    }
    std::fs::create_dir(at.join("crew")).unwrap();
    std::fs::create_dir(at.join("elec")).unwrap();
    write_csv(&at.join("crew/CREW.csv"), &sheets[0].1);
    write_csv(&at.join("elec/ELEC.csv"), &sheets[1].1);
    // Each team member's file given on its own.
    let (code, stdout, stderr) = run(
        at,
        &[
            "edit",
            "settled.xer",
            "--from-timesheet",
            "crew/CREW.csv",
            "--from-timesheet",
            "elec",
            "-o",
            "agreed.xer",
        ],
    );
    assert_eq!(code, Some(0), "{stdout}{stderr}");
    assert!(stdout.contains("actual units 16 to 24"), "{stdout}");
    assert!(stdout.contains("actual units 0 to 4"), "{stdout}");
    // The batch sets A1's actual units itself, so there's nothing to record
    // in P6.
    assert!(!stdout.contains("record them in P6"), "{stdout}");
    fill(&mut sheets[1].1, "A1", &[("percent", "70")]);
    write_csv(&at.join("elec/ELEC.csv"), &sheets[1].1);
    let (code, stdout, _) = run(
        at,
        &[
            "edit",
            "settled.xer",
            "--from-timesheet",
            "crew/CREW.csv",
            "--from-timesheet",
            "elec",
            "-o",
            "differ.xer",
            "--json",
        ],
    );
    assert_eq!(code, Some(1), "{stdout}");
    assert!(!at.join("differ.xer").exists());
    assert_eq!(
        problems(&stdout),
        [(
            "ELEC.csv ELEC row 2 column percent".to_owned(),
            "conflicting-progress".to_owned()
        )]
    );
}

#[test]
fn bad_cells_are_refused_at_their_row_and_column() {
    let directory = tempfile::tempdir().unwrap();
    let at = directory.path();
    settled(at);
    let header = "activity,name,resource,planned_start,planned_finish,budget_hours,actual_hours,hours,remaining_hours,actual_start,actual_finish,percent";
    let text = [
        header,
        "NOPE,,CREW,,,,0,1,,,,",
        "A1,,NOBODY,,,,0,1,,,,",
        "A1,,CREW,,,,16,-2,,,,",
        "A2,,CREW,,,,0,,,soon,,",
        "A2,,ELEC,,,,0,,,,,",
    ]
    .join("\n");
    std::fs::write(at.join("bad.csv"), text).unwrap();
    let (code, stdout, _) = run(
        at,
        &[
            "edit",
            "settled.xer",
            "--from-timesheet",
            "bad.csv",
            "-o",
            "out.xer",
            "--json",
        ],
    );
    assert_eq!(code, Some(1), "{stdout}");
    assert!(!at.join("out.xer").exists());
    let at_row = |row: u32, column: &str, code: &str| {
        (
            format!("bad.csv bad row {row} column {column}"),
            code.to_owned(),
        )
    };
    assert_eq!(
        problems(&stdout),
        [
            at_row(2, "activity", "no-such-activity"),
            at_row(3, "resource", "no-such-resource"),
            at_row(4, "hours", "bad-hours"),
            at_row(5, "actual_start", "bad-date"),
        ]
    );
}

/// A row with only hours, or only remaining hours, makes
/// no progress edit, and an edit the schedule refuses is named at its cells.
#[test]
fn hours_or_remaining_alone_apply_and_a_refused_edit_names_its_cells() {
    let directory = tempfile::tempdir().unwrap();
    let at = directory.path();
    settled(at);
    let mut sheets = week(at, "week");
    fill(&mut sheets[0].1, "A1", &[("hours", "8")]);
    fill(&mut sheets[1].1, "A1", &[("remaining_hours", "12")]);
    for (name, rows) in &sheets {
        write_csv(&at.join(format!("week/{name}.csv")), rows);
    }
    let (code, stdout, stderr) = run(
        at,
        &[
            "edit",
            "settled.xer",
            "--from-timesheet",
            "week",
            "-o",
            "units.xer",
        ],
    );
    assert_eq!(code, Some(0), "{stdout}{stderr}");
    assert!(stdout.contains("actual units 16 to 24"), "{stdout}");
    assert!(stdout.contains("remaining units"), "{stdout}");
    // A finish before the activity's start is the schedule's to refuse.
    fill(
        &mut sheets[0].1,
        "A1",
        &[("hours", ""), ("actual_finish", "2025-12-01 08:00")],
    );
    write_csv(&at.join("week/CREW.csv"), &sheets[0].1);
    let (code, stdout, _) = run(
        at,
        &[
            "edit",
            "settled.xer",
            "--from-timesheet",
            "week",
            "-o",
            "early.xer",
            "--json",
        ],
    );
    assert_eq!(code, Some(1), "{stdout}");
    assert!(!at.join("early.xer").exists());
    assert!(
        stdout.contains("CREW row 2 column actual_finish"),
        "{stdout}"
    );
}

/// On an activity whose percent complete is by units,
/// the hours set the percent, so a percent given as well is refused.
#[test]
fn a_percent_on_an_activity_complete_by_units_is_refused() {
    let directory = tempfile::tempdir().unwrap();
    let at = directory.path();
    settled(at);
    let text = std::fs::read_to_string(at.join("settled.xer")).unwrap();
    assert!(text.contains("\tCP_Phys\tTT_Task\t"));
    std::fs::write(
        at.join("units.xer"),
        text.replace("\tCP_Phys\tTT_Task\t", "\tCP_Units\tTT_Task\t"),
    )
    .unwrap();
    let mut sheets = week(at, "week");
    fill(&mut sheets[0].1, "A1", &[("hours", "8"), ("percent", "50")]);
    write_csv(&at.join("week/CREW.csv"), &sheets[0].1);
    let (code, stdout, _) = run(
        at,
        &[
            "edit",
            "units.xer",
            "--from-timesheet",
            "week",
            "-o",
            "out.xer",
            "--json",
        ],
    );
    assert_eq!(code, Some(1), "{stdout}");
    assert_eq!(
        problems(&stdout),
        [(
            "CREW.csv CREW row 2 column percent".to_owned(),
            "percent-by-units".to_owned()
        )]
    );
    // Hours alone apply.
    fill(&mut sheets[0].1, "A1", &[("percent", "")]);
    write_csv(&at.join("week/CREW.csv"), &sheets[0].1);
    let (code, stdout, stderr) = run(
        at,
        &[
            "edit",
            "units.xer",
            "--from-timesheet",
            "week",
            "-o",
            "out.xer",
        ],
    );
    assert_eq!(code, Some(0), "{stdout}{stderr}");
    assert!(stdout.contains("complete by units"), "{stdout}");
}

/// Excel stores 50% as 0.5, so a percent below 1 in a
/// number cell is refused, at its own column wherever the sheet starts; a
/// sheet with none of the timesheet's columns is skipped.
#[test]
fn a_workbook_percent_typed_with_a_percent_sign_is_refused_and_notes_are_skipped() {
    let directory = tempfile::tempdir().unwrap();
    let at = directory.path();
    settled(at);
    let sheets = week(at, "week");
    let crew = &sheets[0].1;
    let mut book = Workbook::new();
    let sheet = book.add_worksheet();
    sheet.set_name("CREW").unwrap();
    for (r, row) in crew.iter().enumerate() {
        for (c, cell) in row.iter().enumerate() {
            // The sheet starts at column B.
            let (r, c) = (u32::try_from(r).unwrap(), u16::try_from(c + 1).unwrap());
            match cell.parse::<f64>() {
                Ok(n) => sheet.write_number(r, c, n).map(|_| ()),
                Err(_) if cell.is_empty() => Ok(()),
                Err(_) => sheet.write_string(r, c, cell).map(|_| ()),
            }
            .unwrap();
        }
    }
    let header = &crew[0];
    let column =
        |name: &str| u16::try_from(header.iter().position(|c| c == name).unwrap() + 1).unwrap();
    sheet.write_number(1, column("hours"), 8).unwrap();
    sheet.write_number(1, column("percent"), 0.5).unwrap();
    book.add_worksheet()
        .set_name("Notes")
        .unwrap()
        .write_string(0, 0, "Rained on Tuesday")
        .unwrap();
    book.save(at.join("week.xlsx")).unwrap();
    let (code, stdout, _) = run(
        at,
        &[
            "edit",
            "settled.xer",
            "--from-timesheet",
            "week.xlsx",
            "-o",
            "out.xer",
            "--json",
        ],
    );
    assert_eq!(code, Some(1), "{stdout}");
    assert_eq!(
        problems(&stdout),
        [(
            "week.xlsx CREW row 2 column percent".to_owned(),
            "bad-percent".to_owned()
        )]
    );
    let error: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(
        error["error"]["problems"][0]["column"],
        u64::from(column("percent")) + 1
    );
}
