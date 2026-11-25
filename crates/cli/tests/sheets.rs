//! `import` from a spreadsheet: a workbook and a folder of CSV files
//! build the same schedule as the equivalent command document, and each
//! mistake is reported at its sheet and row.

#![allow(clippy::unwrap_used, reason = "test helpers outside #[test] functions")]

use std::path::Path;
use std::process::Command;

use rust_xlsxwriter::{ExcelDateTime, Format, Workbook};
use serde_json::Value;

/// The plan, sheet by sheet: a header, then rows. Empty cells are left blank.
const PLAN: [(&str, &[&[&str]]); 5] = [
    (
        "Project",
        &[
            &["code", "name", "data_date"],
            &["PLANT", "New Plant", "2026-03-02 08:00"],
        ],
    ),
    (
        "WBS",
        &[
            &["wbs_path", "name"],
            &["PLANT.CIV", "Civil works"],
            &["PLANT.MEC", "Mechanical"],
        ],
    ),
    (
        "Resources",
        &[
            &["resource", "name", "type", "units_per_hour"],
            &["CREW", "Civil crew", "labor", "1"],
        ],
    ),
    (
        "Activities",
        &[
            &[
                "code",
                "name",
                "wbs_path",
                "type",
                "duration",
                "original_duration_hours",
                "constraint",
                "constraint_date",
                "resources",
            ],
            &[
                "A1010",
                "Excavate",
                "PLANT.CIV",
                "task",
                "5d",
                "",
                "",
                "",
                "CREW",
            ],
            &[
                "A1020",
                "Pour, then cure",
                "PLANT.CIV",
                "",
                "",
                "80",
                "",
                "",
                "",
            ],
            &[
                "A2010",
                "Order pumps",
                "PLANT.MEC.PUMPS",
                "",
                "3d",
                "",
                "start-on-or-after",
                "2026-03-05 08:00",
                "",
            ],
            &[
                "M100",
                "Plant complete",
                "PLANT.MEC",
                "finish-milestone",
                "",
                "",
                "",
                "",
                "",
            ],
        ],
    ),
    (
        "Relationships",
        &[
            &["predecessor", "successor", "link_type", "lag"],
            &["A1010", "A1020", "FS", ""],
            &["A1020", "M100", "", ""],
            &["A2010", "M100", "FF", "1d"],
        ],
    ),
];

/// The same plan as a command document for `new --edits`.
const COMMANDS: &str = r#"[
  {"op": "add_wbs", "code": "CIV", "name": "Civil works", "parent": "PLANT"},
  {"op": "add_wbs", "code": "MEC", "name": "Mechanical", "parent": "PLANT"},
  {"op": "add_wbs", "code": "PUMPS", "name": "PUMPS", "parent": "MEC"},
  {"op": "add_resource", "resource": "CREW", "name": "Civil crew", "resource_type": "labor", "units_per_hour": 1},
  {"op": "add_activity", "code": "A1010", "name": "Excavate", "wbs": "CIV", "activity_type": "task", "duration": "5d"},
  {"op": "add_activity", "code": "A1020", "name": "Pour, then cure", "wbs": "CIV", "duration": "80h"},
  {"op": "add_activity", "code": "A2010", "name": "Order pumps", "wbs": "PUMPS", "duration": "3d"},
  {"op": "add_activity", "code": "M100", "name": "Plant complete", "wbs": "MEC", "activity_type": "finish-milestone"},
  {"op": "assign_resource", "activity": "A1010", "resource": "CREW"},
  {"op": "set_constraint", "activity": "A2010", "constraint": "start-on-or-after", "date": "2026-03-05 08:00"},
  {"op": "add_link", "predecessor": "A1010", "successor": "A1020", "link_type": "FS"},
  {"op": "add_link", "predecessor": "A1020", "successor": "M100"},
  {"op": "add_link", "predecessor": "A2010", "successor": "M100", "link_type": "FF", "lag": "1d"}
]"#;

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

/// Writes `sheets` as a workbook. The data date goes in as an Excel date,
/// as a spreadsheet user would type it; numbers go in as numbers.
fn workbook(path: &Path, sheets: &[(&str, &[&[&str]])]) {
    let mut book = Workbook::new();
    let dated = Format::new().set_num_format("yyyy-mm-dd hh:mm");
    for (name, rows) in sheets {
        let sheet = book.add_worksheet().set_name(*name).unwrap();
        for (r, row) in rows.iter().enumerate() {
            let r = u32::try_from(r).unwrap();
            for (c, text) in row.iter().enumerate() {
                let c = u16::try_from(c).unwrap();
                if text.is_empty() {
                    continue;
                }
                let date = (*name == "Project" && r == 1 && c == 2)
                    .then(|| ExcelDateTime::parse_from_str(text).ok())
                    .flatten();
                if let Some(date) = date {
                    sheet
                        .write_datetime_with_format(r, c, &date, &dated)
                        .unwrap();
                } else if let Ok(number) = text.parse::<f64>() {
                    sheet.write_number(r, c, number).unwrap();
                } else {
                    sheet.write_string(r, c, *text).unwrap();
                }
            }
        }
    }
    book.save(path).unwrap();
}

/// Writes `sheets` as a folder of CSV files.
fn folder(path: &Path, sheets: &[(&str, &[&[&str]])]) {
    std::fs::create_dir(path).unwrap();
    for (name, rows) in sheets {
        let text: String = rows
            .iter()
            .map(|row| {
                row.iter()
                    .map(|c| {
                        if c.contains(',') {
                            format!("\"{c}\"")
                        } else {
                            (*c).to_owned()
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(",")
                    + "\n"
            })
            .collect();
        std::fs::write(path.join(format!("{}.csv", name.to_lowercase())), text).unwrap();
    }
}

#[test]
fn a_workbook_and_csv_files_build_what_the_command_document_builds() {
    let directory = tempfile::tempdir().unwrap();
    let at = directory.path();
    std::fs::write(at.join("plan.json"), COMMANDS).unwrap();
    let (code, _, stderr) = run(
        at,
        &[
            "new",
            "--code",
            "PLANT",
            "--name",
            "New Plant",
            "--data-date",
            "2026-03-02 08:00",
            "--edits",
            "plan.json",
            "-o",
            "commands.xer",
        ],
    );
    assert_eq!(code, Some(0), "{stderr}");
    workbook(&at.join("plan.xlsx"), &PLAN);
    let (code, stdout, stderr) = run(at, &["import", "plan.xlsx", "-o", "workbook.xer"]);
    assert_eq!(code, Some(0), "{stderr}");
    assert!(stdout.contains("4 activities, 3 relationships"), "{stdout}");
    folder(&at.join("plan"), &PLAN);
    let (code, _, stderr) = run(at, &["import", "plan", "-o", "csv.xer"]);
    assert_eq!(code, Some(0), "{stderr}");
    let read = |name: &str| std::fs::read(at.join(name)).unwrap();
    assert!(
        read("workbook.xer") == read("commands.xer"),
        "the workbook built something else"
    );
    assert!(
        read("csv.xer") == read("commands.xer"),
        "the CSV files built something else"
    );
}

/// The plan with one sheet's rows replaced.
fn with<'a>(sheet: &str, rows: &'a [&'a [&'a str]]) -> Vec<(&'a str, &'a [&'a [&'a str]])> {
    PLAN.iter()
        .map(|(name, plan_rows)| {
            if *name == sheet {
                (*name, rows)
            } else {
                (*name, *plan_rows)
            }
        })
        .collect()
}

#[test]
fn each_mistake_is_reported_at_its_sheet_and_row() {
    let directory = tempfile::tempdir().unwrap();
    let at = directory.path();
    let activities: &[&[&str]] = &[
        &["code", "name", "colour", "duration"],
        &["A1010", "Excavate", "red", "5d"],
        &["A1020", "Pour", "", "lots"],
    ];
    let relationships: &[&[&str]] = &[
        &["predecessor", "successor"],
        &["A1010", "A1020"],
        &["A1010", "Z9"],
    ];
    let project: &[&[&str]] = &[
        &["code", "name", "data_date"],
        &["PLANT", "New Plant", "2026-02-30 08:00"],
    ];
    let mut sheets = with("Activities", activities);
    sheets.retain(|(name, _)| *name != "Relationships" && *name != "Project" && *name != "WBS");
    sheets.push(("Relationships", relationships));
    sheets.push(("Project", project));
    workbook(&at.join("bad.xlsx"), &sheets);
    let (code, stdout, stderr) = run(at, &["import", "bad.xlsx", "-o", "bad.xer", "--json"]);
    assert_ne!(code, Some(0), "{stderr}");
    assert!(!at.join("bad.xer").exists());
    let error: Value = serde_json::from_str(&stdout).unwrap();
    let found: Vec<(String, String)> = error["error"]["problems"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            (
                p["source"].as_str().unwrap().to_owned(),
                p["code"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    let want = [
        ("bad.xlsx Project row 2", "bad-date"),
        ("bad.xlsx Activities row 1", "unknown-column"),
        ("bad.xlsx Activities row 3", "bad-row"),
        ("bad.xlsx Relationships row 3", "no-such-activity"),
    ];
    for (source, code) in want {
        assert!(
            found.iter().any(|(s, c)| s == source && c == code),
            "no {code} at {source} in {found:?}"
        );
    }
    assert_eq!(found.len(), want.len(), "{found:?}");
    // The same, as text on stderr, each at its row.
    let (_, _, stderr) = run(at, &["import", "bad.xlsx", "-o", "bad.xer"]);
    assert!(
        stderr.contains("bad.xlsx Relationships row 3: successor Z9 isn't on the Activities sheet"),
        "{stderr}"
    );
}

#[test]
fn a_spreadsheet_without_a_project_sheet_is_refused() {
    let directory = tempfile::tempdir().unwrap();
    let at = directory.path();
    let sheets: Vec<_> = PLAN
        .iter()
        .filter(|(n, _)| *n != "Project")
        .copied()
        .collect();
    folder(&at.join("plan"), &sheets);
    let (code, _, stderr) = run(at, &["import", "plan", "-o", "plan.xer"]);
    assert_ne!(code, Some(0));
    assert!(stderr.contains("there's no Project sheet"), "{stderr}");
}

/// The sources and codes of the problems an import of `sheets` reports.
fn problems_of(at: &Path, sheets: &[(&str, &[&[&str]])]) -> Vec<(String, String)> {
    let name = "case";
    let _ = std::fs::remove_dir_all(at.join(name));
    folder(&at.join(name), sheets);
    let (code, stdout, stderr) = run(at, &["import", name, "-o", "case.xer", "--json"]);
    assert_ne!(code, Some(0), "{stderr}");
    assert!(!at.join("case.xer").exists());
    let error: Value = serde_json::from_str(&stdout).unwrap();
    error["error"]["problems"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            (
                p["source"].as_str().unwrap().replace("case ", ""),
                p["code"].as_str().unwrap().to_owned(),
            )
        })
        .collect()
}

#[test]
fn blank_values_repeated_wbs_codes_and_bad_projects_are_reported_at_their_rows() {
    let directory = tempfile::tempdir().unwrap();
    let at = directory.path();
    // A blank code, and a node code used in two branches.
    let activities: &[&[&str]] = &[
        &["code", "name", "wbs_path"],
        &["", "No ID", "PLANT.CIV"],
        &["A1", "One", "PLANT.CIV.PHASE"],
        &["A2", "Two", "PLANT.MEC.PHASE"],
    ];
    let found = problems_of(at, &with("Activities", activities));
    assert!(
        found.contains(&("Activities row 2".to_owned(), "missing-value".to_owned())),
        "{found:?}"
    );
    assert!(
        found.contains(&(
            "Activities row 4".to_owned(),
            "repeated-wbs-code".to_owned()
        )),
        "{found:?}"
    );
    // A must-finish date before the data date is the Project row's.
    let project: &[&[&str]] = &[
        &["code", "name", "data_date", "must_finish"],
        &["PLANT", "New Plant", "2026-03-02 08:00", "2026-01-01 08:00"],
    ];
    let found = problems_of(at, &with("Project", project));
    assert_eq!(
        found,
        [(
            "Project row 2".to_owned(),
            "invalid-new-schedule".to_owned()
        )]
    );
    // Another problem on the Project sheet doesn't hide it.
    let project: &[&[&str]] = &[
        &["code", "name", "data_date", "must_finish", "colour"],
        &[
            "PLANT",
            "New Plant",
            "2026-03-02 08:00",
            "2026-01-01 08:00",
            "red",
        ],
    ];
    let found = problems_of(at, &with("Project", project));
    let has = |source: &str, code: &str| found.contains(&(source.to_owned(), code.to_owned()));
    assert!(has("Project row 1", "unknown-column"), "{found:?}");
    assert!(has("Project row 2", "invalid-new-schedule"), "{found:?}");
    // Only a Project sheet: there's nothing to build.
    let found = problems_of(at, &PLAN[..1]);
    assert_eq!(
        found,
        [("Activities row 1".to_owned(), "missing-sheet".to_owned())]
    );
}

#[test]
fn a_row_the_edits_refuse_is_reported_with_the_rows_that_couldnt_be_read() {
    let directory = tempfile::tempdir().unwrap();
    let at = directory.path();
    // An unknown column, and a link the edits refuse: from an activity to itself.
    let activities: &[&[&str]] = &[
        &["code", "name", "colour", "duration"],
        &["A1", "One", "red", "2d"],
        &["A2", "Two", "", "3d"],
    ];
    let relationships: &[&[&str]] = &[&["predecessor", "successor"], &["A1", "A1"]];
    let mut sheets = with("Activities", activities);
    sheets.retain(|(name, _)| *name != "Relationships");
    sheets.push(("Relationships", relationships));
    let found = problems_of(at, &sheets);
    assert!(
        found.contains(&("Activities row 1".to_owned(), "unknown-column".to_owned())),
        "{found:?}"
    );
    // The edits refuse a link from an activity to itself as a loop.
    assert!(
        found.contains(&(
            "Relationships row 2".to_owned(),
            "circular-logic".to_owned()
        )),
        "{found:?}"
    );
}

#[test]
fn hours_as_query_exports_them_are_read_to_the_minute() {
    let directory = tempfile::tempdir().unwrap();
    let at = directory.path();
    let activities: &[&[&str]] = &[
        &["code", "name", "original_duration_hours"],
        &["A1", "Twenty minutes", "0.3333333333333333"],
    ];
    let sheets: Vec<_> = with("Activities", activities)
        .into_iter()
        .filter(|(n, _)| *n == "Project" || *n == "Activities")
        .collect();
    folder(&at.join("plan"), &sheets);
    let (code, stdout, stderr) = run(at, &["import", "plan", "-o", "plan.xer"]);
    assert_eq!(code, Some(0), "{stderr}");
    // Twenty minutes, not a refused span.
    assert!(stdout.contains("finish: 2026-03-02 08:20"), "{stdout}");
}

/// What a spreadsheet user does: other sheets beside the plan, values in
/// capitals, a duration as hours and minutes, WBS rows in their own order,
/// and CSV files named after the tabs.
#[test]
fn a_spreadsheet_as_people_keep_them_builds() {
    let directory = tempfile::tempdir().unwrap();
    let at = directory.path();
    let mut book = Workbook::new();
    let rows: [(&str, &[&[&str]]); 5] = [
        (
            "Project",
            &[
                &["Code", "Name", "Data_Date"],
                &["PLANT", "New Plant", "2026-03-02 08:00"],
            ],
        ),
        (
            "WBS",
            &[
                &["wbs_path", "name"],
                &["PLANT.SITE", "Site"],
                &["PLANT.CIV", "Civil works"],
            ],
        ),
        (
            "Resources",
            &[
                &["resource", "name", "type"],
                &["CREW", "Civil crew", "Labor"],
            ],
        ),
        (
            "Activities",
            &[
                &[
                    "code",
                    "name",
                    "wbs_path",
                    "type",
                    "constraint",
                    "constraint_date",
                    "original_duration_hours",
                ],
                &[
                    "A1",
                    "Clear",
                    "PLANT.SITE",
                    "Task",
                    "Start-On-Or-After",
                    "2026-03-03 08:00",
                    "",
                ],
                &["M1", "Done", "PLANT.CIV", "Finish-Milestone", "", "", ""],
            ],
        ),
        (
            "Notes",
            &[&["anything", "at all"], &["#N/A", "=SUM(A1:A9)"]],
        ),
    ];
    let hours = Format::new().set_num_format("[h]:mm");
    for (name, sheet_rows) in rows {
        let sheet = book.add_worksheet().set_name(name).unwrap();
        for (r, row) in sheet_rows.iter().enumerate() {
            for (c, text) in row.iter().enumerate() {
                let (r, c) = (u32::try_from(r).unwrap(), u16::try_from(c).unwrap());
                if !text.is_empty() {
                    sheet.write_string(r, c, *text).unwrap();
                }
            }
        }
        if name == "Activities" {
            // Eight hours, typed as 8:00 in a duration cell.
            sheet
                .write_number_with_format(1, 6, 8.0 / 24.0, &hours)
                .unwrap();
        }
    }
    book.save(at.join("plan.xlsx")).unwrap();
    let (code, stdout, stderr) = run(at, &["import", "plan.xlsx", "-o", "plan.xer"]);
    assert_eq!(code, Some(0), "{stderr}");
    assert!(stdout.contains("A1 added, 8h long"), "{stdout}");
    // The WBS sheet's order, not the alphabet's.
    let site = stdout.find("WBS SITE added").unwrap();
    let civil = stdout.find("WBS CIV added").unwrap();
    assert!(site < civil, "{stdout}");

    // CSV files named after the tabs.
    let plan = at.join("tabs");
    std::fs::create_dir(&plan).unwrap();
    std::fs::write(
        plan.join("Project.csv"),
        "code,name,data_date\nPLANT,New Plant,2026-03-02 08:00\n",
    )
    .unwrap();
    std::fs::write(
        plan.join("Activities.CSV"),
        "code,name,duration\nA1,Clear,2d\n",
    )
    .unwrap();
    let (code, _, stderr) = run(at, &["import", "tabs", "-o", "tabs.xer"]);
    assert_eq!(code, Some(0), "{stderr}");
}

#[test]
fn one_bad_row_is_one_problem() {
    let directory = tempfile::tempdir().unwrap();
    let at = directory.path();
    // A misspelled type: the activity's link and resource don't add problems of their own.
    let activities: &[&[&str]] = &[
        &[
            "code",
            "name",
            "type",
            "resources",
            "constraint",
            "constraint_date",
            "duration",
        ],
        &[
            "A1010",
            "Excavate",
            "taks",
            "CREW",
            "start-on-or-after",
            "2026-03-03 08:00",
            "5d",
        ],
        &["A1020", "Pour", "", "", "", "", "10d"],
    ];
    let relationships: &[&[&str]] = &[&["predecessor", "successor"], &["A1010", "A1020"]];
    let mut sheets = with("Activities", activities);
    sheets.retain(|(name, _)| *name != "Relationships");
    sheets.push(("Relationships", relationships));
    let found = problems_of(at, &sheets);
    assert_eq!(
        found,
        [("Activities row 2".to_owned(), "bad-row".to_owned())]
    );
    // A repeated column, and a blank value on the Project sheet.
    let activities: &[&[&str]] = &[
        &["code", "name", "Name", "duration"],
        &["A1", "One", "Uno", "1d"],
    ];
    let mut sheets = with("Activities", activities);
    sheets.retain(|(name, _)| *name == "Project" || *name == "Activities");
    let found = problems_of(at, &sheets);
    assert_eq!(
        found,
        [("Activities row 1".to_owned(), "repeated-column".to_owned())]
    );
    let project: &[&[&str]] = &[&["code", "name", "data_date"], &["PLANT", "New Plant", ""]];
    let found = problems_of(at, &with("Project", project));
    assert_eq!(
        found,
        [("Project row 2".to_owned(), "missing-value".to_owned())]
    );
}

/// A CSV text's rows, for the plain cells `query --csv` writes here.
fn csv_rows(text: &str) -> Vec<Vec<String>> {
    text.lines()
        .map(|line| line.split(',').map(str::to_owned).collect())
        .collect()
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
    changed
}

/// Edits made in a sheet `query` exported read back into the same schedule
///: only the cells changed in the sheet change in the file, and a
/// workbook, a folder of CSV files and one CSV file give the same file.
#[test]
fn edits_in_an_exported_sheet_change_only_those_cells() {
    let directory = tempfile::tempdir().unwrap();
    let at = directory.path();
    std::fs::copy("../../fixtures/xer/fresh-writer.xer", at.join("plan.xer")).unwrap();
    // Scheduled first, so the run's recalculation changes nothing else.
    let (code, _, stderr) = run(at, &["schedule", "plan.xer", "-o", "settled.xer"]);
    assert_eq!(code, Some(0), "{stderr}");
    let columns = "code,name,original_duration_hours,calendar,code:Synthetic phase,udf:Synthetic note,early_start,early_finish,total_float_hours";
    let (code, exported, stderr) =
        run(at, &["query", "settled.xer", "--csv", "--columns", columns]);
    assert_eq!(code, Some(0), "{stderr}");
    let mut rows = csv_rows(&exported);
    let header = rows[0].clone();
    let column = |name: &str| header.iter().position(|c| c == name).unwrap();
    let row = |rows: &[Vec<String>], code: &str| rows.iter().position(|r| r[0] == code).unwrap();
    let (first, second) = (row(&rows, "TF-001"), row(&rows, "TF-002"));
    rows[first][column("name")] = "Revised work".to_owned();
    rows[second][column("name")] = "Revised finish".to_owned();
    rows[first][column("udf:Synthetic note")] = "Checked".to_owned();
    let cells: Vec<Vec<&str>> = rows
        .iter()
        .map(|r| r.iter().map(String::as_str).collect())
        .collect();
    let sheet: Vec<&[&str]> = cells.iter().map(Vec::as_slice).collect();
    workbook(&at.join("edited.xlsx"), &[("Activities", &sheet)]);
    folder(&at.join("edited"), &[("Activities", &sheet)]);
    let text: String = rows.iter().map(|r| r.join(",") + "\n").collect();
    std::fs::write(at.join("edited.csv"), text).unwrap();
    for (from, out) in [
        ("edited.xlsx", "a.xer"),
        ("edited", "b.xer"),
        ("edited.csv", "c.xer"),
    ] {
        let (code, stdout, stderr) = run(
            at,
            &["edit", "settled.xer", "--from-sheet", from, "-o", out],
        );
        assert_eq!(code, Some(0), "{from}: {stdout}{stderr}");
    }
    let mut changed = changed_cells(&at.join("settled.xer"), &at.join("a.xer"));
    changed.sort();
    assert_eq!(
        changed,
        ["TASK.task_name", "TASK.task_name", "UDFVALUE.udf_text"]
    );
    let read = |name: &str| std::fs::read(at.join(name)).unwrap();
    assert_eq!(read("a.xer"), read("b.xer"));
    assert_eq!(read("a.xer"), read("c.xer"));
}

/// An activity the schedule doesn't have, or a value that can't be read, is
/// refused at its sheet, row and column, and nothing is written.
#[test]
fn a_sheet_s_bad_cells_are_refused_at_their_row_and_column() {
    let directory = tempfile::tempdir().unwrap();
    let at = directory.path();
    std::fs::copy("../../fixtures/xer/fresh-writer.xer", at.join("plan.xer")).unwrap();
    let sheet: [&[&str]; 4] = [
        &["code", "name", "original_duration_hours", "constraint"],
        &["NOPE", "Nothing", "8", ""],
        &["TF-001", "Synthetic work", "lots", ""],
        &["TF-002", "Synthetic finish milestone", "0", "sometime"],
    ];
    workbook(&at.join("bad.xlsx"), &[("Activities", &sheet)]);
    let (code, stdout, stderr) = run(
        at,
        &[
            "edit",
            "plan.xer",
            "--from-sheet",
            "bad.xlsx",
            "-o",
            "out.xer",
            "--json",
        ],
    );
    assert_ne!(code, Some(0), "{stderr}");
    assert!(!at.join("out.xer").exists());
    let error: Value = serde_json::from_str(&stdout).unwrap();
    let problems: Vec<(String, String)> = error["error"]["problems"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            (
                p["source"].as_str().unwrap().replace("bad.xlsx ", ""),
                p["code"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    assert_eq!(
        problems,
        [
            (
                "Activities row 2 column code".to_owned(),
                "no-such-activity".to_owned()
            ),
            (
                "Activities row 3 column original_duration_hours".to_owned(),
                "bad-value".to_owned()
            ),
            (
                "Activities row 4 column constraint".to_owned(),
                "bad-value".to_owned()
            ),
        ],
        "{error}"
    );
}

/// A Relationships sheet adds, changes and removes links among the
/// Activities sheet's activities.
#[test]
fn a_relationships_sheet_adds_changes_and_removes_links() {
    let directory = tempfile::tempdir().unwrap();
    let at = directory.path();
    std::fs::copy("../../fixtures/xer/fresh-writer.xer", at.join("plan.xer")).unwrap();
    let activities: [&[&str]; 3] = [&["code"], &["TF-001"], &["TF-002"]];
    let (_, before, _) = run(
        at,
        &[
            "query",
            "plan.xer",
            "--succs",
            "TF-001",
            "--columns",
            "code,link_type,link_lag_hours",
            "--json",
        ],
    );
    let before: Value = serde_json::from_str(&before).unwrap();
    assert_eq!(before["rows"].as_array().unwrap().len(), 1, "{before}");
    // The FS link becomes an SS link with a lag: the FS one goes, the SS one comes.
    let links: [&[&str]; 2] = [
        &["predecessor", "successor", "link_type", "lag"],
        &["TF-001", "TF-002", "SS", "1d"],
    ];
    workbook(
        &at.join("links.xlsx"),
        &[("Activities", &activities), ("Relationships", &links)],
    );
    let (code, stdout, stderr) = run(
        at,
        &[
            "edit",
            "plan.xer",
            "--from-sheet",
            "links.xlsx",
            "-o",
            "out.xer",
        ],
    );
    assert_eq!(code, Some(0), "{stdout}{stderr}");
    let (_, after, _) = run(
        at,
        &[
            "query",
            "out.xer",
            "--succs",
            "TF-001",
            "--columns",
            "code,link_type,link_lag_hours",
            "--json",
        ],
    );
    let after: Value = serde_json::from_str(&after).unwrap();
    let rows = after["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 1, "{after}");
    assert_eq!(rows[0]["link_type"], "SS", "{after}");
    assert_eq!(rows[0]["link_lag_hours"], 8.0, "{after}");
}

/// `fresh-writer.xer` with its one link given `lag` hours, on a project that
/// counts lags on 24-hour days.
fn on_24_hour_lags(at: &Path, lag: &str) {
    let text = std::fs::read_to_string("../../fixtures/xer/fresh-writer.xer").unwrap();
    let text = text.replace("rcal_Predecessor", "rcal_24Hour").replace(
        "%R\t1\t2\t1\t1\t1\tPR_FS\t0",
        &format!("%R\t1\t2\t1\t1\t1\tPR_FS\t{lag}"),
    );
    std::fs::write(at.join("plan.xer"), text).unwrap();
}

/// Lags are counted on the lag calendar, and a link turned
/// round isn't a loop on the way.
#[test]
fn a_sheet_s_links_count_lags_as_link_edits_do() {
    let directory = tempfile::tempdir().unwrap();
    let at = directory.path();
    let link_of = |file: &str| -> Value {
        let (_, out, _) = run(
            at,
            &[
                "query",
                file,
                "--succs",
                "TF-001",
                "--columns",
                "code,link_type,link_lag_hours",
                "--json",
            ],
        );
        serde_json::from_str::<Value>(&out).unwrap()["rows"].clone()
    };
    let activities: [&[&str]; 3] = [&["code"], &["TF-001"], &["TF-002"]];
    // A day of lag is 24 hours here, so 1d is a change from 8h.
    on_24_hour_lags(at, "8");
    let links: [&[&str]; 2] = [
        &["predecessor", "successor", "link_type", "lag"],
        &["TF-001", "TF-002", "FS", "1d"],
    ];
    workbook(
        &at.join("lag.xlsx"),
        &[("Activities", &activities), ("Relationships", &links)],
    );
    let (code, stdout, stderr) = run(
        at,
        &[
            "edit",
            "plan.xer",
            "--from-sheet",
            "lag.xlsx",
            "-o",
            "lag.xer",
        ],
    );
    assert_eq!(code, Some(0), "{stdout}{stderr}");
    assert_eq!(link_of("lag.xer")[0]["link_lag_hours"], 24.0);
    // TF-001 to TF-002 turned round.
    let links: [&[&str]; 2] = [&["predecessor", "successor"], &["TF-002", "TF-001"]];
    workbook(
        &at.join("round.xlsx"),
        &[("Activities", &activities), ("Relationships", &links)],
    );
    let (code, stdout, stderr) = run(
        at,
        &[
            "edit",
            "plan.xer",
            "--from-sheet",
            "round.xlsx",
            "-o",
            "round.xer",
        ],
    );
    assert_eq!(code, Some(0), "{stdout}{stderr}");
    assert_eq!(link_of("round.xer").as_array().unwrap().len(), 0);
}

/// Text that looks like a number is text, and a constraint
/// date changes without its kind column.
#[test]
fn a_sheet_s_text_and_constraint_dates_read_back_as_edits_do() {
    let directory = tempfile::tempdir().unwrap();
    let at = directory.path();
    // A name of digits renamed to the same number written differently.
    std::fs::copy("../../fixtures/xer/fresh-writer.xer", at.join("plan.xer")).unwrap();
    let (code, _, stderr) = run(
        at,
        &[
            "edit",
            "plan.xer",
            "--name",
            "TF-001=001",
            "-o",
            "digits.xer",
        ],
    );
    assert_eq!(code, Some(0), "{stderr}");
    let named: [&[&str]; 2] = [&["code", "name"], &["TF-001", "1"]];
    folder(&at.join("named"), &[("Activities", &named)]);
    let (code, stdout, stderr) = run(
        at,
        &[
            "edit",
            "digits.xer",
            "--from-sheet",
            "named",
            "--dry-run",
            "--json",
        ],
    );
    assert_eq!(code, Some(0), "{stderr}");
    assert!(stdout.contains("TF-001"), "{stdout}");
    // A constraint date, with no constraint column, keeps the kind it has.
    let (code, _, stderr) = run(
        at,
        &[
            "edit",
            "plan.xer",
            "--constraint",
            "TF-001=start-on-or-after@2026-01-05 08:00",
            "-o",
            "held.xer",
        ],
    );
    assert_eq!(code, Some(0), "{stderr}");
    let dated: [&[&str]; 2] = [
        &["code", "constraint_date"],
        &["TF-001", "2026-01-07 08:00"],
    ];
    folder(&at.join("dated"), &[("Activities", &dated)]);
    let (code, stdout, stderr) = run(
        at,
        &[
            "edit",
            "held.xer",
            "--from-sheet",
            "dated",
            "-o",
            "dated.xer",
        ],
    );
    assert_eq!(code, Some(0), "{stdout}{stderr}");
    let (_, out, _) = run(
        at,
        &[
            "query",
            "dated.xer",
            "--text",
            "TF-001",
            "--columns",
            "constraint,constraint_date",
            "--json",
        ],
    );
    let row = serde_json::from_str::<Value>(&out).unwrap()["rows"][0].clone();
    assert_eq!(row["constraint"], "start-on-or-after", "{row}");
    assert_eq!(row["constraint_date"], "2026-01-07 08:00", "{row}");
}

/// `--skip-failed` can't split a sheet, an unchanged sheet
/// sits beside other edits, and a workbook's leading `'` is as typed.
#[test]
fn a_sheet_s_flags_and_quotes_read_back_as_edits_do() {
    let directory = tempfile::tempdir().unwrap();
    let at = directory.path();
    std::fs::copy("../../fixtures/xer/fresh-writer.xer", at.join("plan.xer")).unwrap();
    let dated: [&[&str]; 2] = [
        &["code", "constraint_date"],
        &["TF-001", "2026-01-07 08:00"],
    ];
    folder(&at.join("dated"), &[("Activities", &dated)]);
    // --skip-failed can't take part of a sheet.
    let (code, _, stderr) = run(
        at,
        &[
            "edit",
            "plan.xer",
            "--from-sheet",
            "dated",
            "--skip-failed",
            "--dry-run",
        ],
    );
    assert_eq!(code, Some(2), "{stderr}");
    // An unchanged sheet beside another edit: the other edit applies.
    let same: [&[&str]; 2] = [&["code", "name"], &["TF-001", "Synthetic work"]];
    folder(&at.join("same"), &[("Activities", &same)]);
    let (code, stdout, stderr) = run(
        at,
        &[
            "edit",
            "plan.xer",
            "--from-sheet",
            "same",
            "--name",
            "TF-002=Done",
            "--dry-run",
            "--json",
        ],
    );
    assert_eq!(code, Some(0), "{stdout}{stderr}");
    let (code, _, stderr) = run(
        at,
        &["edit", "plan.xer", "--from-sheet", "same", "--dry-run"],
    );
    assert_ne!(code, Some(0));
    assert!(
        stderr.contains("says what the schedule already says"),
        "{stderr}"
    );
    // A workbook's cell starting with ' is as typed.
    let (code, _, stderr) = run(
        at,
        &[
            "edit",
            "plan.xer",
            "--name",
            "TF-001='=Literal",
            "-o",
            "quoted.xer",
        ],
    );
    assert_eq!(code, Some(0), "{stderr}");
    let quoted: [&[&str]; 2] = [&["code", "name"], &["TF-001", "'=Literal"]];
    workbook(&at.join("quoted.xlsx"), &[("Activities", &quoted)]);
    let (code, _, stderr) = run(
        at,
        &[
            "edit",
            "quoted.xer",
            "--from-sheet",
            "quoted.xlsx",
            "--dry-run",
        ],
    );
    assert_ne!(code, Some(0));
    assert!(
        stderr.contains("says what the schedule already says"),
        "{stderr}"
    );
}

/// The rows `query --json` gives for `args` on `file`.
fn rows_of(at: &Path, file: &str, args: &[&str]) -> Value {
    let mut all = vec!["query", file, "--json"];
    all.extend_from_slice(args);
    let (_, out, _) = run(at, &all);
    serde_json::from_str::<Value>(&out).unwrap()["rows"].clone()
}

/// A Relationships sheet without a type or lag keeps the
/// link's own, and a lag can be written `+1d` as `--add-link` takes it.
#[test]
fn a_relationships_sheet_keeps_the_type_and_lag_it_leaves_out() {
    let directory = tempfile::tempdir().unwrap();
    let at = directory.path();
    on_24_hour_lags(at, "8");
    let activities: [&[&str]; 3] = [&["code"], &["TF-001"], &["TF-002"]];
    let ends: [&[&str]; 2] = [&["predecessor", "successor"], &["TF-001", "TF-002"]];
    folder(
        &at.join("ends"),
        &[("Activities", &activities), ("Relationships", &ends)],
    );
    let (code, _, stderr) = run(
        at,
        &["edit", "plan.xer", "--from-sheet", "ends", "--dry-run"],
    );
    assert_ne!(code, Some(0));
    assert!(
        stderr.contains("says what the schedule already says"),
        "{stderr}"
    );
    let plus: [&[&str]; 2] = [
        &[
            "predecessor",
            "successor",
            "link_type",
            "lag",
            "link_lag_hours",
        ],
        &["TF-001", "TF-002", "", "+1d", ""],
    ];
    folder(
        &at.join("plus"),
        &[("Activities", &activities), ("Relationships", &plus)],
    );
    let (code, stdout, stderr) = run(
        at,
        &["edit", "plan.xer", "--from-sheet", "plus", "-o", "plus.xer"],
    );
    assert_eq!(code, Some(0), "{stdout}{stderr}");
    let links = rows_of(
        at,
        "plus.xer",
        &[
            "--succs",
            "TF-001",
            "--columns",
            "code,link_type,link_lag_hours",
        ],
    );
    assert_eq!(links.as_array().unwrap().len(), 1, "{links}");
    assert_eq!(links[0]["link_type"], "FS", "{links}");
    assert_eq!(links[0]["link_lag_hours"], 24.0, "{links}");
}

/// A CSV export reads back unchanged even where a value
/// starts with `'` itself, a dated constraint becomes as-late-as-possible
/// from the constraint column alone, and an Activity ID may hold `=`.
#[test]
fn a_sheet_reads_back_quotes_late_constraints_and_ids_with_equals() {
    let directory = tempfile::tempdir().unwrap();
    let at = directory.path();
    let text = std::fs::read_to_string("../../fixtures/xer/fresh-writer.xer").unwrap();
    std::fs::write(at.join("plan.xer"), text.replace("TF-001", "A=1")).unwrap();
    let (code, _, stderr) = run(
        at,
        &[
            "edit",
            "plan.xer",
            "--name",
            "TF-002='-draft",
            "--constraint",
            "TF-002=start-on-or-after@2026-01-05 08:00",
            "-o",
            "held.xer",
        ],
    );
    assert_eq!(code, Some(0), "{stderr}");
    let (_, export, _) = run(
        at,
        &["query", "held.xer", "--csv", "--columns", "code,name"],
    );
    std::fs::write(at.join("export.csv"), export).unwrap();
    let (code, _, stderr) = run(
        at,
        &[
            "edit",
            "held.xer",
            "--from-sheet",
            "export.csv",
            "--dry-run",
        ],
    );
    assert_ne!(code, Some(0));
    assert!(
        stderr.contains("says what the schedule already says"),
        "{stderr}"
    );
    let late: [&[&str]; 2] = [&["code", "constraint"], &["TF-002", "as-late-as-possible"]];
    folder(&at.join("late"), &[("Activities", &late)]);
    let (code, stdout, stderr) = run(
        at,
        &["edit", "held.xer", "--from-sheet", "late", "-o", "late.xer"],
    );
    assert_eq!(code, Some(0), "{stdout}{stderr}");
    let equals: [&[&str]; 2] = [
        &["code", "constraint", "constraint_date"],
        &["A=1", "start-on-or-after", "2026-01-05 08:00"],
    ];
    folder(&at.join("equals"), &[("Activities", &equals)]);
    let (code, stdout, stderr) = run(
        at,
        &[
            "edit",
            "late.xer",
            "--from-sheet",
            "equals",
            "-o",
            "both.xer",
        ],
    );
    assert_eq!(code, Some(0), "{stdout}{stderr}");
    let rows = rows_of(
        at,
        "both.xer",
        &["--columns", "code,constraint,constraint_date"],
    );
    let row = |code: &str| {
        rows.as_array()
            .unwrap()
            .iter()
            .find(|r| r["code"] == code)
            .cloned()
            .unwrap()
    };
    assert_eq!(row("TF-002")["constraint"], "as-late-as-possible", "{rows}");
    assert_eq!(row("TF-002")["constraint_date"], Value::Null, "{rows}");
    assert_eq!(row("A=1")["constraint"], "start-on-or-after", "{rows}");
}

/// A Resources sheet's `max_units_per_hour` is the resource's limit, and a
/// cell that isn't a number is refused at its row.
#[test]
fn a_resources_sheet_gives_each_resource_its_limit() {
    let directory = tempfile::tempdir().unwrap();
    let at = directory.path();
    let resources: &[&[&str]] = &[
        &["resource", "name", "max_units_per_hour"],
        &["CREW", "Civil crew", "2"],
    ];
    workbook(&at.join("plan.xlsx"), &with("Resources", resources));
    let (code, _, stderr) = run(at, &["import", "plan.xlsx", "-o", "plan.xer"]);
    assert_eq!(code, Some(0), "{stderr}");
    let xer = totalfloat_xer::raw::read(std::fs::read(at.join("plan.xer")).unwrap()).unwrap();
    let (schedules, _) = totalfloat_xer::to_model(&xer);
    let crew = schedules[0].resources.values().next().unwrap();
    assert_eq!(crew.rates.len(), 1);
    assert_eq!(crew.rates[0].max_units_per_hour, Some(2.0));
    let bad: &[&[&str]] = &[
        &["resource", "name", "max_units_per_hour"],
        &["CREW", "Civil crew", "two"],
    ];
    let problems = problems_of(at, &with("Resources", bad));
    assert!(
        problems.contains(&("Resources row 2".to_owned(), "bad-number".to_owned())),
        "{problems:?}"
    );
}
