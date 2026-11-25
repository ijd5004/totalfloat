//! A resource's limit, set when it's added or later, from the command
//! line, a JSON edit and text, is what `query --usage` measures against, and
//! it's saved in `RSRCRATE.max_qty_per_hr`.

#![allow(clippy::unwrap_used, reason = "test code")]

use std::path::{Path, PathBuf};
use std::process::Command;

use totalfloat_xer::{raw, to_model};

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

fn ok(args: &[&str]) -> String {
    let (status, stdout, stderr) = run(args);
    assert_eq!(status, Some(0), "{args:?}\n{stdout}{stderr}");
    stdout
}

fn path(dir: &Path, name: &str) -> PathBuf {
    dir.join(name)
}

fn text(p: &Path) -> &str {
    p.to_str().unwrap()
}

/// Two weeks of work on a 40-hour week, from Monday 2026-12-28, with no
/// holidays on the new calendar.
const BUILD: &str = r#"[
  {"op": "add_activity", "code": "A1", "name": "Pull cable", "wbs": "PLANT", "duration": "10d"}
]"#;

/// A fresh schedule with one ten-day activity.
fn fresh(dir: &Path, extra: &str) -> PathBuf {
    let edits = path(dir, "build.json");
    let build = if extra.is_empty() {
        BUILD.to_owned()
    } else {
        BUILD.replacen("\n]", &format!(",\n  {extra}\n]"), 1)
    };
    std::fs::write(&edits, build).unwrap();
    let out = path(dir, "s.xer");
    ok(&[
        "new",
        "--code",
        "PLANT",
        "--name",
        "Plant",
        "--data-date",
        "2026-12-28 08:00",
        "--edits",
        text(&edits),
        "-o",
        text(&out),
    ]);
    out
}

/// 240 hours over ten days is 3 an hour, so 120 a week.
fn usage(file: &Path) -> String {
    ok(&["query", text(file), "--usage", "ELEC", "--by", "week"])
}

/// The limits of each `RSRCRATE` row, as written.
fn rate_cells(file: &Path) -> Vec<(String, String)> {
    let xer = raw::read(std::fs::read(file).unwrap()).unwrap();
    let table = xer.tables().iter().find(|t| t.name == "RSRCRATE").unwrap();
    let at = |name: &str| table.field_index(name).unwrap();
    let (start, limit) = (at("start_date"), at("max_qty_per_hr"));
    table
        .rows
        .iter()
        .map(|row| {
            let cell = |at: usize| row.get(at).unwrap_or_default().to_owned();
            (cell(start), cell(limit))
        })
        .collect()
}

#[test]
fn a_limit_set_on_the_command_line_is_what_usage_measures() {
    let dir = tempfile::tempdir().unwrap();
    let s = fresh(dir.path(), "");
    let t = path(dir.path(), "t.xer");
    ok(&[
        "edit",
        text(&s),
        "--add-resource",
        "ELEC;Electricians",
        "--resource-limit",
        "ELEC=2",
        "--assign",
        "A1=ELEC:240",
        "-o",
        text(&t),
    ]);
    let report = usage(&t);
    insta::assert_snapshot!(report);
    assert_eq!(
        rate_cells(&t),
        [("2026-12-28 00:00".to_owned(), "2".to_owned())]
    );
    // The saved file reads back with the limit.
    let (schedules, _) = to_model(&raw::read(std::fs::read(&t).unwrap()).unwrap());
    let elec = schedules[0]
        .resources
        .values()
        .find(|r| r.short_name == "ELEC")
        .unwrap();
    assert_eq!(elec.rates[0].max_units_per_hour, Some(2.0));
    // An unedited file still writes back byte for byte.
    ok(&["roundtrip", text(&t)]);
}

#[test]
fn a_limit_given_with_new_edits_is_the_same() {
    let dir = tempfile::tempdir().unwrap();
    let s = fresh(
        dir.path(),
        r#"{"op": "add_resource", "resource": "ELEC", "name": "Electricians", "max_units_per_hour": 2},
  {"op": "assign_resource", "activity": "A1", "resource": "ELEC", "units": 240}"#,
    );
    let t = path(dir.path(), "t.xer");
    ok(&["schedule", text(&s), "-o", text(&t)]);
    let other = tempfile::tempdir().unwrap();
    let cli = fresh(other.path(), "");
    let edited = path(other.path(), "t.xer");
    ok(&[
        "edit",
        text(&cli),
        "--add-resource",
        "ELEC;Electricians",
        "--resource-limit",
        "ELEC=2",
        "--assign",
        "A1=ELEC:240",
        "-o",
        text(&edited),
    ]);
    assert_eq!(usage(&t), usage(&edited));
    assert_eq!(
        rate_cells(&t),
        [("2026-12-28 00:00".to_owned(), "2".to_owned())]
    );
}

#[test]
fn a_dated_limit_measures_the_weeks_after_it_against_the_new_one() {
    let dir = tempfile::tempdir().unwrap();
    let s = fresh(dir.path(), "");
    let t = path(dir.path(), "t.xer");
    ok(&[
        "edit",
        text(&s),
        "--add-resource",
        "ELEC;Electricians",
        "--resource-limit",
        "ELEC=2",
        "--assign",
        "A1=ELEC:240",
        "-o",
        text(&t),
    ]);
    let u = path(dir.path(), "u.xer");
    ok(&[
        "edit",
        text(&t),
        "--resource-limit",
        "ELEC=3@2027-01-04",
        "-o",
        text(&u),
    ]);
    let report = usage(&u);
    insta::assert_snapshot!(report);
    // The JSON gives the limit over time.
    let json: serde_json::Value =
        serde_json::from_str(&ok(&["query", text(&u), "--usage", "ELEC", "--json"])).unwrap();
    assert_eq!(
        json["limits"],
        serde_json::json!([
            {"from": null, "units_per_hour": 2.0},
            {"from": "2027-01-04 00:00", "units_per_hour": 3.0}
        ])
    );
    assert_eq!(
        rate_cells(&u),
        [
            ("2026-12-28 00:00".to_owned(), "2".to_owned()),
            ("2027-01-04 00:00".to_owned(), "3".to_owned())
        ]
    );
    // And cleared: the file takes the empty cell on every rate.
    let v = path(dir.path(), "v.xer");
    ok(&[
        "edit",
        text(&u),
        "--resource-limit",
        "ELEC=",
        "-o",
        text(&v),
    ]);
    assert_eq!(
        rate_cells(&v),
        [
            ("2026-12-28 00:00".to_owned(), String::new()),
            ("2027-01-04 00:00".to_owned(), String::new())
        ]
    );
}

#[test]
fn a_limit_that_isn_t_a_number_is_refused_with_its_form() {
    let dir = tempfile::tempdir().unwrap();
    let s = fresh(dir.path(), "");
    let t = path(dir.path(), "t.xer");
    let (status, _, stderr) = run(&[
        "edit",
        text(&s),
        "--add-resource",
        "ELEC;Electricians",
        "--resource-limit",
        "ELEC=two",
        "-o",
        text(&t),
    ]);
    assert_ne!(status, Some(0));
    assert!(stderr.contains("RESOURCE=2"), "{stderr}");
    let (status, _, stderr) = run(&[
        "edit",
        text(&s),
        "--add-resource",
        "ELEC;Electricians",
        "--resource-limit",
        "ELEC=-1",
        "-o",
        text(&t),
    ]);
    assert_ne!(status, Some(0));
    assert!(stderr.contains("invalid-units"), "{stderr}");
}

#[test]
fn a_limit_in_text_builds_and_exports_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let tfs = path(dir.path(), "plan.tfs");
    let source = "\
totalfloat-schedule 1

project PLANT \"Plant\"
  data-date 2026-12-28 08:00

resource ELEC \"Electricians\" labor
  max-units-per-hour 2

wbs PLANT.CIV \"Civil\"

activity A1 \"Pull cable\" 10d
  resource ELEC units 240
";
    std::fs::write(&tfs, source).unwrap();
    let built = path(dir.path(), "built.xer");
    ok(&["import", text(&tfs), "-o", text(&built)]);
    assert_eq!(
        rate_cells(&built),
        [("2026-12-28 00:00".to_owned(), "2".to_owned())]
    );
    let out = path(dir.path(), "out.tfs");
    ok(&["export", text(&built), "--to", "tfs", "-o", text(&out)]);
    let exported = std::fs::read_to_string(&out).unwrap();
    assert!(
        exported.contains("resource ELEC \"Electricians\" labor\n  calendar \"Standard 5-Day\"\n  max-units-per-hour 2\n"),
        "{exported}"
    );
}

/// A limit goes out to MS Project XML and comes back, as `MaxUnits`
/// when it holds throughout and as availability periods when it changes,
/// and `query --usage` measures the imported file against it.
#[test]
fn a_limit_comes_back_from_ms_project_xml() {
    let dir = tempfile::tempdir().unwrap();
    let s = fresh(dir.path(), "");
    let t = path(dir.path(), "t.xer");
    ok(&[
        "edit",
        text(&s),
        "--add-resource",
        "ELEC;Electricians",
        "--resource-limit",
        "ELEC=2",
        "--assign",
        "A1=ELEC:240",
        "-o",
        text(&t),
    ]);
    let u = path(dir.path(), "u.xer");
    ok(&[
        "edit",
        text(&t),
        "--resource-limit",
        "ELEC=3@2027-01-04",
        "-o",
        text(&u),
    ]);
    for (file, holds, name) in [
        (&t, "<MaxUnits>2</MaxUnits>", "t"),
        (
            &u,
            "<AvailableFrom>2027-01-04T00:00:00</AvailableFrom>",
            "u",
        ),
    ] {
        let xml = path(dir.path(), &format!("{name}.xml"));
        let back = path(dir.path(), &format!("{name}-back.xer"));
        ok(&["export", text(file), "--to", "msproject", "-o", text(&xml)]);
        let written = std::fs::read_to_string(&xml).unwrap();
        assert!(written.contains(holds), "{written}");
        ok(&["import", text(&xml), "-o", text(&back)]);
        assert_eq!(usage(&back), usage(file));
    }
}
