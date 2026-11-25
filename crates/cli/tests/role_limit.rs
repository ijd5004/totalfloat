//! A role's limit, set from the command line or JSON, is saved in
//! `ROLERATE.max_qty_per_hr`, on the rates the file holds and on new rows,
//! and `portfolio capacity` measures the role against it.

#![allow(clippy::unwrap_used, reason = "test code")]

use std::path::{Path, PathBuf};
use std::process::Command;

use totalfloat_xer::raw;

const ROLES: &str = "../../fixtures/xer/roles.xer";

fn ok(dir: &Path, args: &[&str]) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_totalfloat"))
        .current_dir(dir)
        .args(args)
        .output()
        .unwrap();
    let (stdout, stderr) = (
        String::from_utf8(out.stdout).unwrap(),
        String::from_utf8(out.stderr).unwrap(),
    );
    assert_eq!(out.status.code(), Some(0), "{args:?}\n{stdout}{stderr}");
    stdout
}

fn copy(dir: &Path) -> PathBuf {
    let held = dir.join("roles.xer");
    std::fs::copy(ROLES, &held).unwrap();
    held
}

/// Each `ROLERATE` row as written: rate, role, start and limit.
fn rate_rows(file: &Path) -> Vec<[String; 4]> {
    let xer = raw::read(std::fs::read(file).unwrap()).unwrap();
    let table = xer.tables().iter().find(|t| t.name == "ROLERATE").unwrap();
    let at = |name: &str| table.field_index(name).unwrap();
    let fields = [
        at("role_rate_id"),
        at("role_id"),
        at("start_date"),
        at("max_qty_per_hr"),
    ];
    table
        .rows
        .iter()
        .map(|row| fields.map(|at| row.get(at).unwrap_or_default().to_owned()))
        .collect()
}

fn row(cells: [&str; 4]) -> [String; 4] {
    cells.map(str::to_owned)
}

#[test]
fn a_role_s_limit_is_saved_on_the_rates_the_file_holds() {
    let dir = tempfile::tempdir().unwrap();
    copy(dir.path());
    ok(
        dir.path(),
        &[
            "edit",
            "roles.xer",
            "--role-limit",
            "FOREMAN=3",
            "-o",
            "t.xer",
        ],
    );
    assert_eq!(
        rate_rows(&dir.path().join("t.xer")),
        [
            row(["1", "1", "2026-01-01 00:00", "1"]),
            row(["2", "2", "2026-01-01 00:00", "3"]),
            row(["3", "2", "2026-07-01 00:00", "3"]),
        ]
    );
    // Beside what recalculating changes, only those two cells changed.
    ok(dir.path(), &["schedule", "roles.xer", "-o", "base.xer"]);
    let before = std::fs::read_to_string(dir.path().join("base.xer")).unwrap();
    let after = std::fs::read_to_string(dir.path().join("t.xer")).unwrap();
    let changed: Vec<_> = before
        .lines()
        .zip(after.lines())
        .filter(|(a, b)| a != b)
        .collect();
    assert_eq!(changed.len(), 2, "{changed:?}");
    assert_eq!(before.lines().count(), after.lines().count());
    // A saved file, unedited, writes back byte for byte.
    ok(dir.path(), &["roundtrip", "t.xer"]);
}

#[test]
fn a_dated_role_limit_and_a_new_role_s_limit_are_new_rows() {
    let dir = tempfile::tempdir().unwrap();
    copy(dir.path());
    std::fs::write(
        dir.path().join("e.json"),
        r#"[{"op": "set_role_limit", "role": "SUPER", "max_units_per_hour": 2, "from": "2026-03-02"},
            {"op": "add_role", "role": "ENG", "name": "Engineer", "max_units_per_hour": 0.5}]"#,
    )
    .unwrap();
    ok(
        dir.path(),
        &["edit", "roles.xer", "--edits", "e.json", "-o", "t.xer"],
    );
    let rows = rate_rows(&dir.path().join("t.xer"));
    assert_eq!(rows[..3], rate_rows(Path::new(ROLES))[..]);
    assert_eq!(rows[3], row(["4", "1", "2026-03-02 00:00", "2"]));
    assert_eq!(rows[4][3], "0.5");
    assert_eq!(rows.len(), 5);
    ok(dir.path(), &["roundtrip", "t.xer"]);
    // Cleared: the file takes the empty cell on every rate of the role.
    ok(
        dir.path(),
        &["edit", "t.xer", "--role-limit", "SUPER=", "-o", "u.xer"],
    );
    let rows = rate_rows(&dir.path().join("u.xer"));
    assert_eq!(rows[0][3], "");
    assert_eq!(rows[3][3], "");
    assert_eq!(rows[1][3], "1");
}

/// A role's limit in text builds into `ROLERATE`, exports back as the same
/// line, and a changed line sets it on the file's role.
#[test]
fn a_role_s_limit_in_text_builds_exports_and_changes() {
    let dir = tempfile::tempdir().unwrap();
    let source = "\
totalfloat-schedule 1

project PLANT \"Plant\"
  data-date 2026-12-28 08:00

role ENG \"Engineer\"
  max-units-per-hour 2

wbs PLANT.CIV \"Civil\"

activity A1 \"Design\" 10d
  role ENG units 80
";
    std::fs::write(dir.path().join("plan.tfs"), source).unwrap();
    ok(dir.path(), &["import", "plan.tfs", "-o", "built.xer"]);
    let rows = rate_rows(&dir.path().join("built.xer"));
    assert_eq!(rows.len(), 1);
    assert_eq!(
        (rows[0][2].as_str(), rows[0][3].as_str()),
        ("2026-12-28 00:00", "2")
    );
    ok(
        dir.path(),
        &["export", "built.xer", "--to", "tfs", "-o", "out.tfs"],
    );
    let exported = std::fs::read_to_string(dir.path().join("out.tfs")).unwrap();
    assert!(
        exported.contains("role ENG \"Engineer\"\n  max-units-per-hour 2\n"),
        "{exported}"
    );
    std::fs::write(
        dir.path().join("changed.tfs"),
        exported.replace("max-units-per-hour 2", "max-units-per-hour 3"),
    )
    .unwrap();
    ok(
        dir.path(),
        &[
            "edit",
            "built.xer",
            "--apply",
            "changed.tfs",
            "-o",
            "applied.xer",
        ],
    );
    let rows = rate_rows(&dir.path().join("applied.xer"));
    assert_eq!(rows[0][3], "3");
}
