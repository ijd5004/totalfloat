//! `totalfloat fmt` on schedule text files (ARCHITECTURE.md, Schedule as text).

#![allow(clippy::unwrap_used, reason = "test helpers outside #[test] functions")]

use std::path::Path;
use std::process::Command;

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

const MESSY: &str = "totalfloat-schedule 1\nwbs P.A   \"A\"\nactivity A1 \"One\"  5d   # first\nactivity A2 \"Two\" 3d after A1 FS\n";
const TIDY: &str = "totalfloat-schedule 1\n\nwbs P.A \"A\"\nactivity A1 \"One\" 5d # first\nactivity A2 \"Two\" 3d after A1\n";

fn write(directory: &Path, name: &str, text: &str) -> String {
    let path = directory.join(name);
    std::fs::write(&path, text).unwrap();
    path.to_str().unwrap().to_owned()
}

#[test]
fn fmt_rewrites_a_file_and_check_then_passes() {
    let directory = tempfile::tempdir().unwrap();
    let plan = write(directory.path(), "plan.tfs", MESSY);
    let (code, stdout, _) = run(&["fmt", &plan, "--check"]);
    assert_eq!(code, Some(3));
    assert_eq!(stdout, format!("{plan} would change\n"));
    assert_eq!(std::fs::read_to_string(&plan).unwrap(), MESSY);

    let (code, stdout, _) = run(&["fmt", &plan]);
    assert_eq!(code, Some(0));
    assert_eq!(stdout, format!("Formatted {plan}\n"));
    assert_eq!(std::fs::read_to_string(&plan).unwrap(), TIDY);

    let (code, stdout, _) = run(&["fmt", &plan, "--check"]);
    assert_eq!(code, Some(0));
    assert_eq!(stdout, "Every file is formatted\n");
    let (code, stdout, _) = run(&["fmt", &plan]);
    assert_eq!(code, Some(0));
    assert_eq!(stdout, format!("{plan} is already formatted\n"));
}

#[cfg(unix)]
#[test]
fn fmt_keeps_the_file_permissions() {
    use std::os::unix::fs::PermissionsExt;
    let directory = tempfile::tempdir().unwrap();
    let plan = write(directory.path(), "plan.tfs", MESSY);
    std::fs::set_permissions(&plan, std::fs::Permissions::from_mode(0o640)).unwrap();
    assert_eq!(run(&["fmt", &plan]).0, Some(0));
    let mode = std::fs::metadata(&plan).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o640);
}

#[test]
fn a_file_with_mistakes_is_left_alone_and_every_mistake_shown() {
    let directory = tempfile::tempdir().unwrap();
    let source = "totalfloat-schedule 1\nactivty A1 \"One\"\nactivity A2 \"Two\" 3x\n";
    let plan = write(directory.path(), "plan.tfs", source);
    let (code, stdout, stderr) = run(&["fmt", &plan]);
    assert_eq!(code, Some(1));
    assert_eq!(stdout, "");
    assert!(
        stderr.contains("error[unknown-keyword]: `activty` isn't a statement"),
        "{stderr}"
    );
    assert!(
        stderr.contains("hint: did you mean `activity`?"),
        "{stderr}"
    );
    assert!(stderr.contains(&format!("--> {plan}:3:19")), "{stderr}");
    assert!(stderr.contains("error[text-not-readable]"), "{stderr}");
    assert_eq!(std::fs::read_to_string(&plan).unwrap(), source);

    let (code, stdout, _) = run(&["fmt", &plan, "--json"]);
    assert_eq!(code, Some(1));
    let doc: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(doc["error"]["code"], "text-not-readable");
    let problems = doc["error"]["problems"].as_array().unwrap();
    assert_eq!(problems.len(), 2);
    assert_eq!(problems[0]["code"], "unknown-keyword");
    assert_eq!(problems[1]["source"], format!("{plan} line 3"));
}

#[test]
fn fmt_json_matches_its_schema() {
    let directory = tempfile::tempdir().unwrap();
    let plan = write(directory.path(), "plan.tfs", MESSY);
    let (code, stdout, _) = run(&["fmt", &plan, "--check", "--json"]);
    assert_eq!(code, Some(3));
    let doc: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let published: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string("../../docs/schemas/fmt-1.schema.json").unwrap(),
    )
    .unwrap();
    assert_eq!(doc["schema"], published["$id"]);
    let keys = |v: &serde_json::Value| {
        let mut k: Vec<String> = v.as_object().unwrap().keys().cloned().collect();
        k.sort();
        k
    };
    assert_eq!(keys(&doc), keys(&published["properties"]));
    assert_eq!(doc["changed"][0], plan.as_str());
}

#[test]
fn one_broken_file_does_not_stop_the_others() {
    let directory = tempfile::tempdir().unwrap();
    let messy = write(directory.path(), "a.tfs", MESSY);
    let broken = write(
        directory.path(),
        "b.tfs",
        "totalfloat-schedule 1\nactivty A1 \"One\"\n",
    );
    let other = write(directory.path(), "c.tfs", MESSY);
    let (code, stdout, stderr) = run(&["fmt", &messy, &broken, &other]);
    assert_eq!(code, Some(1));
    assert_eq!(stdout, format!("Formatted {messy}\nFormatted {other}\n"));
    assert!(
        stderr.contains("1 problem in 1 file left as they were"),
        "{stderr}"
    );
    assert_eq!(std::fs::read_to_string(&messy).unwrap(), TIDY);
    assert_eq!(std::fs::read_to_string(&other).unwrap(), TIDY);
}

#[cfg(unix)]
#[test]
fn fmt_through_a_link_formats_its_target() {
    let directory = tempfile::tempdir().unwrap();
    let target = write(directory.path(), "shared.tfs", MESSY);
    let link = directory.path().join("plan.tfs");
    std::os::unix::fs::symlink(&target, &link).unwrap();
    assert_eq!(run(&["fmt", link.to_str().unwrap()]).0, Some(0));
    assert!(
        std::fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(std::fs::read_to_string(&target).unwrap(), TIDY);
}

const BUILD: &str = r#"totalfloat-schedule 1

project PLANT "New Plant"
  data-date 2026-03-02 08:00
  must-finish 2026-06-30 17:00

calendar "Standard 5-Day"
  week mon-fri 08:00-12:00 13:00-17:00
  holiday 2026-03-10

wbs PLANT.CIV "Civil works"

activity A1010 "Excavate" 5d
activity A1020 "Pour foundations" 10d after A1010
activity A1030 "Cure" 7d after A1020 ss+2d
activity A1040 "Backfill" 3d after A1030, A1020 ff
milestone M100 "Civil complete" after A1040
  finish-on-or-before 2026-06-30 17:00
"#;

/// The same schedule as edit commands, written by hand.
const BUILD_JSON: &str = r#"[
  {"op": "add_calendar_exception", "calendar": "Standard 5-Day", "date": "2026-03-10"},
  {"op": "add_wbs", "code": "CIV", "name": "Civil works", "parent": "PLANT"},
  {"op": "add_activity", "code": "A1010", "name": "Excavate", "wbs": "CIV", "duration": "5d"},
  {"op": "add_activity", "code": "A1020", "name": "Pour foundations", "wbs": "CIV", "duration": "10d"},
  {"op": "add_activity", "code": "A1030", "name": "Cure", "wbs": "CIV", "duration": "7d"},
  {"op": "add_activity", "code": "A1040", "name": "Backfill", "wbs": "CIV", "duration": "3d"},
  {"op": "add_activity", "code": "M100", "name": "Civil complete", "wbs": "CIV", "activity_type": "finish-milestone"},
  {"op": "add_link", "predecessor": "A1010", "successor": "A1020"},
  {"op": "add_link", "predecessor": "A1020", "successor": "A1030", "link_type": "SS", "lag": "2d"},
  {"op": "add_link", "predecessor": "A1030", "successor": "A1040"},
  {"op": "add_link", "predecessor": "A1020", "successor": "A1040", "link_type": "FF"},
  {"op": "add_link", "predecessor": "A1040", "successor": "M100"},
  {"op": "set_constraint", "activity": "M100", "constraint": "finish-on-or-before", "date": "2026-06-30 17:00"}
]"#;

fn dates(file: &str) -> serde_json::Value {
    let (code, stdout, stderr) = run(&["schedule", file, "--json"]);
    assert_eq!(code, Some(0), "{stderr}");
    let doc: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    doc["projects"].clone()
}

#[test]
fn text_builds_the_same_schedule_as_edit_commands() {
    let directory = tempfile::tempdir().unwrap();
    let plan = write(directory.path(), "plan.tfs", BUILD);
    let edits = write(directory.path(), "edits.json", BUILD_JSON);
    let from_text = directory.path().join("text.xer");
    let from_json = directory.path().join("json.xer");
    let (code, stdout, stderr) = run(&["import", &plan, "-o", from_text.to_str().unwrap()]);
    assert_eq!(code, Some(0), "{stderr}");
    assert!(stdout.contains("5 activities, 5 relationships"), "{stdout}");
    let (code, _, stderr) = run(&[
        "new",
        "--code",
        "PLANT",
        "--name",
        "New Plant",
        "--data-date",
        "2026-03-02 08:00",
        "--must-finish",
        "2026-06-30 17:00",
        "--edits",
        &edits,
        "-o",
        from_json.to_str().unwrap(),
    ]);
    assert_eq!(code, Some(0), "{stderr}");
    let (text, json) = (
        dates(from_text.to_str().unwrap()),
        dates(from_json.to_str().unwrap()),
    );
    assert_eq!(text, json);
    assert_eq!(text[0]["scheduled_finish"], "2026-03-25 17:00");
}

#[test]
fn every_problem_in_a_text_build_is_reported_at_its_line_in_one_run() {
    let directory = tempfile::tempdir().unwrap();
    let source = r#"totalfloat-schedule 1

project PLANT "New Plant"
  data-date 2026-03-02 08:00

activity A1010 "Excavate" 5d
activity A1020 "Pour" 10d after A1010
activity A1030 "Cure" 7d after A1025 ss+2d
  start-on-or-after
activity A1040 "Loop" 7d after A1050
activity A1050 "Loop back" 7d after A1040
"#;
    let plan = write(directory.path(), "plan.tfs", source);
    let output = directory.path().join("plan.xer");
    let (code, stdout, stderr) = run(&["import", &plan, "-o", output.to_str().unwrap()]);
    assert_eq!(code, Some(1));
    assert_eq!(stdout, "");
    assert!(!output.exists());
    insta::assert_snapshot!(stderr.replace(&plan, "plan.tfs"));

    let (code, stdout, _) = run(&["import", &plan, "-o", output.to_str().unwrap(), "--json"]);
    assert_eq!(code, Some(1));
    let doc: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let found: Vec<(String, u64, u64)> = doc["error"]["problems"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            (
                p["code"].as_str().unwrap().to_owned(),
                p["line"].as_u64().unwrap(),
                p["column"].as_u64().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        found,
        [
            ("unknown-activity".to_owned(), 8, 32),
            ("constraint-needs-date".to_owned(), 9, 3),
            ("circular-logic".to_owned(), 11, 37),
        ]
    );
}

#[test]
fn a_new_schedule_gets_its_code_types_and_resources() {
    let directory = tempfile::tempdir().unwrap();
    let source = "totalfloat-schedule 1\n\nproject P \"p\"\n  data-date 2026-03-02 08:00\n\ncode-type Phase\n  value CON \"Construction\"\n\nresource ELEC \"Electricians\" labor\nresource CRANE \"Tower crane\" nonlabor\n  units-per-hour 0.5\n\nactivity A \"a\" 1d\n  code Phase=CON\n  resource ELEC\n";
    let plan = write(directory.path(), "plan.tfs", source);
    let output = directory.path().join("plan.xer");
    let (code, _, stderr) = run(&["import", &plan, "-o", output.to_str().unwrap()]);
    assert_eq!(code, Some(0), "{stderr}");
    let (_, inspect) = json_of(&["inspect", output.to_str().unwrap(), "--json"]);
    assert_eq!(inspect["activity_codes"][0]["name"], "Phase");
    let resources: Vec<&str> = inspect["resources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["short_name"].as_str().unwrap())
        .collect();
    assert_eq!(resources, ["ELEC", "CRANE"]);
    // Written out again, the text is what was built, a resource no activity
    // uses included.
    let again = directory.path().join("again.tfs");
    let (code, _, stderr) = run(&[
        "export",
        output.to_str().unwrap(),
        "--to",
        "tfs",
        "-o",
        again.to_str().unwrap(),
    ]);
    assert_eq!(code, Some(0), "{stderr}");
    let text = std::fs::read_to_string(&again).unwrap();
    assert!(
        text.contains("resource CRANE \"Tower crane\" nonlabor\n  calendar \"Standard 5-Day\"\n  units-per-hour 0.5\n"),
        "{text}"
    );
    assert!(
        text.contains("  code Phase=CON\n  resource ELEC units 8\n"),
        "{text}"
    );
}

/// Copying a calendar keeps its hours per day (TF-033), so a
/// ten-hour copy of the 8-hour starter counts `15d` as 120 hours until a
/// `hours-per-day` line says otherwise. A calendar with no `copy-of` is its
/// lines, so its day is its working day.
#[test]
fn a_calendar_s_day_is_its_hours_per_day() {
    let directory = tempfile::tempdir().unwrap();
    let tens = "  week mon-thu 07:00-12:00 13:00-18:00\n  week fri off\n";
    let source = format!(
        "totalfloat-schedule 1\n\nproject P \"p\"\n  data-date 2026-03-02 08:00\n\n\
         calendar \"Ten hour\" copy-of \"Standard 5-Day\"\n{tens}\n\
         calendar \"Ten hour set\" copy-of \"Standard 5-Day\"\n  hours-per-day 10\n{tens}\n\
         calendar \"Four tens\"\n  week mon-thu 07:00-12:00 13:00-18:00\n\n\
         activity A \"a\" 15d\n  calendar \"Ten hour\"\n\
         activity B \"b\" 15d\n  calendar \"Ten hour set\"\n\
         activity C \"c\" 15d\n  calendar \"Four tens\"\n\
         activity R \"r\" 10h\n  type resource\n  calendar \"Four tens\"\n"
    );
    let plan = write(directory.path(), "plan.tfs", &source);
    let output = directory.path().join("plan.xer");
    let (code, stdout, stderr) = run(&["import", &plan, "-o", output.to_str().unwrap()]);
    assert_eq!(code, Some(0), "{stdout}{stderr}");
    let (code, stdout, stderr) = run(&[
        "query",
        output.to_str().unwrap(),
        "--columns",
        "code,original_duration_hours",
        "--json",
    ]);
    assert_eq!(code, Some(0), "{stderr}");
    let doc: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let hours: Vec<(String, f64)> = doc["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            (
                r["code"].as_str().unwrap().to_owned(),
                r["original_duration_hours"].as_f64().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        hours,
        [
            ("A".to_owned(), 120.0),
            ("B".to_owned(), 150.0),
            ("C".to_owned(), 150.0),
            ("R".to_owned(), 10.0)
        ]
    );
    // Written out, the copy that kept 8 hours says so, and spans are in days.
    let again = directory.path().join("again.tfs");
    export_text(&output, None, &again);
    let text = std::fs::read_to_string(&again).unwrap();
    assert!(
        text.contains("calendar \"Ten hour\"\n  hours-per-day 8\n  week mon-thu"),
        "{text}"
    );
    assert!(
        text.contains("calendar \"Four tens\"\n  week mon-thu"),
        "{text}"
    );
    assert!(text.contains("activity B \"b\" 15d\n"), "{text}");
    // Resource-dependent work stays in hours, as edits need it.
    assert!(text.contains("activity R \"r\" 10h\n"), "{text}");
    // Applied, a changed day reads the text's spans in the new day.
    std::fs::write(
        &again,
        text.replace("  hours-per-day 8\n", "  hours-per-day 10\n"),
    )
    .unwrap();
    let applied = directory.path().join("applied.xer");
    let (code, stdout, stderr) = run(&[
        "edit",
        output.to_str().unwrap(),
        "--apply",
        again.to_str().unwrap(),
        "-o",
        applied.to_str().unwrap(),
    ]);
    assert_eq!(code, Some(0), "{stdout}{stderr}");
    let (_, doc) = json_of(&[
        "query",
        applied.to_str().unwrap(),
        "--columns",
        "code,original_duration_hours",
        "--json",
    ]);
    assert_eq!(doc["rows"][0]["code"], "A");
    assert_eq!(doc["rows"][0]["original_duration_hours"], 150.0);
}

#[test]
fn a_code_or_resource_the_file_doesn_t_define_is_a_problem() {
    let head = "totalfloat-schedule 1\n\nproject P \"p\"\n  data-date 2026-03-02 08:00\n\ncode-type Phase\n  value CON\n\nresource ELEC \"Electricians\"\n";
    let activity = |detail: &str| format!("{head}\nactivity A \"a\" 1d\n  {detail}\n");
    assert_eq!(
        problem_codes(&activity("code Phse=CON")),
        ["unknown-code-type"]
    );
    assert_eq!(
        problem_codes(&activity("code Phase=COM")),
        ["unknown-code-value"]
    );
    assert_eq!(
        problem_codes(&activity("resource ELC")),
        ["unknown-resource"]
    );
    let twice = format!("{head}resource elec \"Again\"\n");
    assert_eq!(problem_codes(&twice), ["defined-twice"]);
}

fn problem_codes(source: &str) -> Vec<String> {
    let directory = tempfile::tempdir().unwrap();
    let plan = write(directory.path(), "plan.tfs", source);
    let output = directory.path().join("plan.xer");
    let (code, stdout, _) = run(&["import", &plan, "-o", output.to_str().unwrap(), "--json"]);
    assert_eq!(code, Some(1));
    let doc: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    doc["error"]["problems"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["code"].as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn one_mistake_in_a_text_build_is_one_problem() {
    let head = "totalfloat-schedule 1\n\nproject P \"p\"\n  data-date 2026-03-02 08:00\n\ncalendar Night\n  week mon-fri 20:00-24:00\n";
    // A mistyped calendar on an activity others follow.
    let typo = format!(
        "{head}\nactivity A1 \"a\" 5d\n  calendar Nite\n  start-on-or-after 2026-03-03 08:00\nactivity A2 \"b\" 5d after A1\n"
    );
    assert_eq!(problem_codes(&typo), ["unknown-calendar"]);
    // A refused WBS line with activities under it.
    let wbs = format!(
        "{head}\nwbs Q.A \"wrong root\"\n\nactivity A1 \"a\" 5d\n  start-on-or-after 2026-03-03 08:00\nactivity A2 \"b\" 5d after A1\n"
    );
    assert_eq!(problem_codes(&wbs), ["wrong-wbs-root"]);
    // A calendar copied from one that doesn't exist, used by an activity.
    let copy =
        format!("{head}\ncalendar Day copy-of Nihgt\n\nactivity A1 \"a\" 5d\n  calendar Day\n");
    assert_eq!(problem_codes(&copy), ["unknown-calendar"]);
}

fn json_of(args: &[&str]) -> (Option<i32>, serde_json::Value) {
    let (code, stdout, stderr) = run(args);
    let doc =
        serde_json::from_str(&stdout).unwrap_or_else(|_| panic!("{args:?}: {stdout}{stderr}"));
    (code, doc)
}

/// Calculated dates and float for every activity of a project, by code;
/// `None` if the engine refuses to schedule the file.
fn calculated(file: &str, project: &str, directory: &Path) -> Option<Vec<serde_json::Value>> {
    let out = directory.join(format!("calc-{}.xer", std::process::id()));
    let _ = std::fs::remove_file(&out);
    let (code, _, stderr) = run(&["schedule", file, "-o", out.to_str().unwrap()]);
    if code == Some(1) && stderr.contains("schedule-refused") {
        return None;
    }
    assert_eq!(code, Some(0), "{file}: {stderr}");
    let (_, doc) = json_of(&[
        "query",
        out.to_str().unwrap(),
        "--project",
        project,
        "--columns",
        "code,status,early_start,early_finish,late_start,late_finish,total_float_hours",
        "--limit",
        "100000",
        "--json",
    ]);
    let mut rows = doc["rows"].as_array().unwrap().clone();
    rows.sort_by_key(|r| r["code"].as_str().unwrap().to_owned());
    Some(rows)
}

fn fixtures(directory: &Path, found: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            fixtures(&path, found);
        } else if path.extension().is_some_and(|e| e == "xer") {
            found.push(path);
        }
    }
}

/// Writes `from` out as text at `to`, for `project` if given; returns the
/// export's note kinds.
fn export_text(from: &Path, project: Option<&str>, to: &Path) -> Vec<String> {
    let mut args = vec![
        "export",
        from.to_str().unwrap(),
        "--to",
        "tfs",
        "-o",
        to.to_str().unwrap(),
        "--json",
    ];
    if let Some(project) = project {
        args.extend(["--project", project]);
    }
    let (code, report) = json_of(&args);
    assert_eq!(code, Some(0), "{}: {report}", from.display());
    report["notes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["kind"].as_str().unwrap().to_owned())
        .collect()
}

/// Builds `text` into `to`; every problem if refused.
fn build_text(text: &Path, to: &Path) -> Result<(), String> {
    let (code, stdout, stderr) = run(&[
        "import",
        text.to_str().unwrap(),
        "-o",
        to.to_str().unwrap(),
        "--json",
    ]);
    if code == Some(0) {
        Ok(())
    } else {
        Err(format!("{}: {stdout}{stderr}", text.display()))
    }
}

/// What a project's round trip through text came to.
enum Trip {
    Exact,
    Settled,
}

fn round_trip(file: &str, id: &str, dir: &Path) -> Trip {
    let stem = format!(
        "{}-{id}",
        Path::new(file).file_stem().unwrap().to_str().unwrap()
    );
    let path = |suffix: &str| dir.join(format!("{stem}{suffix}"));
    let kinds = export_text(Path::new(file), Some(id), &path(".tfs"));
    let said = |kind: &str| kinds.iter().any(|k| k == kind);
    build_text(&path(".tfs"), &path(".xer")).unwrap();
    export_text(&path(".xer"), None, &path(".2.tfs"));
    let read = |suffix: &str| std::fs::read_to_string(path(suffix)).unwrap();
    let trip = if read(".tfs") == read(".2.tfs") {
        Trip::Exact
    } else {
        assert!(
            said("calendar-day-length") || said("assignment-units") || said("resource-no-calendar"),
            "{file} {id} changed:\n{}\n{}",
            read(".tfs"),
            read(".2.tfs")
        );
        // Once built, the text holds still.
        build_text(&path(".2.tfs"), &path(".3.xer")).unwrap();
        export_text(&path(".3.xer"), None, &path(".3.tfs"));
        assert_eq!(read(".3.tfs"), read(".2.tfs"), "{file} {id}");
        Trip::Settled
    };
    let rebuilt = path(".xer");
    // An original the engine won't schedule, such as one with links to
    // other projects, has no dates to compare.
    let before = calculated(file, id, dir);
    if before.is_some() && before != calculated(rebuilt.to_str().unwrap(), "1", dir) {
        assert!(
            said("activity-type") || said("schedule-options"),
            "{file} {id}: dates differ with no note saying why"
        );
    }
    trip
}

/// Every fixture's projects written out as text, built back, and written out
/// again. Every one builds. Where the round trip can't be exact, the
/// export's own notes must say why: a calendar with no day length, an
/// assignment with no units, activity types the format writes as tasks, or
/// scheduling options.
#[test]
fn every_fixture_round_trips_through_text() {
    let directory = tempfile::tempdir().unwrap();
    let mut files = Vec::new();
    fixtures(Path::new("../../fixtures"), &mut files);
    files.sort();
    let (mut exact, mut built) = (0, 0);
    for file in &files {
        let file = file.to_str().unwrap();
        let (_, inspect) = json_of(&["inspect", file, "--json"]);
        for project in inspect["projects"].as_array().unwrap() {
            match round_trip(file, &project["id"].to_string(), directory.path()) {
                Trip::Exact => {
                    exact += 1;
                    built += 1;
                }
                Trip::Settled => built += 1,
            }
        }
    }
    assert!(built >= 44, "only {built} projects built");
    assert!(exact >= 30, "only {exact} exact");
}

#[test]
fn the_same_file_always_gives_the_same_text() {
    let directory = tempfile::tempdir().unwrap();
    let (a, b) = (
        directory.path().join("a.tfs"),
        directory.path().join("b.tfs"),
    );
    for out in [&a, &b] {
        let (code, _, stderr) = run(&[
            "export",
            "../../fixtures/xer/two-projects.xer",
            "--to",
            "tfs",
            "--project",
            "2",
            "-o",
            out.to_str().unwrap(),
        ]);
        assert_eq!(code, Some(0), "{stderr}");
    }
    assert_eq!(std::fs::read(&a).unwrap(), std::fs::read(&b).unwrap());
    insta::assert_snapshot!(std::fs::read_to_string(&a).unwrap());
}

const APPLY_FIXTURE: &str = "../../fixtures/xer/batches/001/rel-fs-positive.xer";

/// The fixture written out as text in `dir`, and the copy to apply it to.
fn exported(dir: &Path) -> (String, String) {
    let file = dir.join("plant.xer");
    std::fs::copy(APPLY_FIXTURE, &file).unwrap();
    let text = dir.join("plant.tfs");
    export_text(&file, None, &text);
    (
        file.to_str().unwrap().to_owned(),
        text.to_str().unwrap().to_owned(),
    )
}

fn edit_text(path: &str, from: &str, to: &str) {
    let text = std::fs::read_to_string(path).unwrap();
    assert!(text.contains(from), "{text}");
    std::fs::write(path, text.replace(from, to)).unwrap();
}

#[test]
fn applying_text_gives_exactly_the_edits_it_needs() {
    let directory = tempfile::tempdir().unwrap();
    let (file, text) = exported(directory.path());
    edit_text(
        &text,
        "activity A1 \"Synthetic activity 1\" 1d\n",
        "activity A0 \"Synthetic mobilise\" 2d\n  calendar \"Synthetic weekdays\"\nactivity A1 \"Synthetic activity 1\" 3d\n",
    );
    edit_text(&text, "after A1 fs+1h", "after A0, A1 ss+1h");
    let out = directory.path().join("out.xer");
    let commands = directory.path().join("commands.json");
    let (code, _, stderr) = run(&[
        "edit",
        &file,
        "--apply",
        &text,
        "-o",
        out.to_str().unwrap(),
        "--emit-commands",
        commands.to_str().unwrap(),
    ]);
    assert_eq!(code, Some(0), "{stderr}");
    insta::assert_snapshot!(std::fs::read_to_string(&commands).unwrap());
    // The result written out again is the text that was applied.
    let again = directory.path().join("again.tfs");
    export_text(&out, None, &again);
    assert_eq!(
        std::fs::read_to_string(&again).unwrap(),
        std::fs::read_to_string(&text).unwrap()
    );
}

#[test]
fn applied_text_adds_code_types_and_resources() {
    let directory = tempfile::tempdir().unwrap();
    let (file, text) = exported(directory.path());
    edit_text(
        &text,
        "activity A1 \"Synthetic activity 1\" 1d\n  calendar \"Synthetic weekdays\"\n",
        "code-type Phase\n  value CON \"Construction\"\n\nresource ELEC \"Electricians\" labor\n\nactivity A1 \"Synthetic activity 1\" 1d\n  calendar \"Synthetic weekdays\"\n  code Phase=CON\n  resource ELEC units 8\n",
    );
    let out = directory.path().join("out.xer");
    let (code, stdout, stderr) =
        run(&["edit", &file, "--apply", &text, "-o", out.to_str().unwrap()]);
    assert_eq!(code, Some(0), "{stdout}{stderr}");
    let again = directory.path().join("again.tfs");
    export_text(&out, None, &again);
    let written = std::fs::read_to_string(&again).unwrap();
    for line in [
        "code-type Phase\n  value CON \"Construction\"\n",
        "resource ELEC \"Electricians\" labor\n",
        "  code Phase=CON\n  resource ELEC units 8\n",
    ] {
        assert!(written.contains(line), "{line}\n{written}");
    }
    // Applied again, there's nothing to do; a changed resource is refused,
    // as no edit changes one yet.
    let (code, stdout, _) = run(&[
        "edit",
        out.to_str().unwrap(),
        "--apply",
        again.to_str().unwrap(),
        "--dry-run",
    ]);
    assert_eq!(code, Some(0));
    assert!(stdout.contains("nothing to change"), "{stdout}");
    edit_text(
        again.to_str().unwrap(),
        "\"Electricians\" labor",
        "\"Electricians\" material",
    );
    let (code, doc) = json_of(&[
        "edit",
        out.to_str().unwrap(),
        "--apply",
        again.to_str().unwrap(),
        "--dry-run",
        "--json",
    ]);
    assert_eq!(code, Some(1));
    assert_eq!(doc["error"]["problems"][0]["code"], "unsupported-change");
}

#[test]
fn applying_an_unchanged_export_changes_nothing() {
    let directory = tempfile::tempdir().unwrap();
    let (file, text) = exported(directory.path());
    let (code, _, _) = run(&["history", "init", &file, "-m", "As submitted"]);
    assert_eq!(code, Some(0));
    let store = file.replace(".xer", ".tfdb");
    let (code, stdout, stderr) = run(&["edit", &store, "--apply", &text, "-m", "No change"]);
    assert_eq!(code, Some(0), "{stderr}");
    assert!(
        stdout.contains("matches the schedule; nothing to change"),
        "{stdout}"
    );
    let (_, log) = json_of(&["history", "log", &store, "--json"]);
    assert_eq!(log["changesets"].as_array().unwrap().len(), 1);
}

#[test]
fn what_the_text_leaves_out_needs_a_choice() {
    let directory = tempfile::tempdir().unwrap();
    let (file, text) = exported(directory.path());
    let full = std::fs::read_to_string(&text).unwrap();
    let cut = full.find("activity A2").unwrap();
    std::fs::write(&text, &full[..cut]).unwrap();
    let (code, doc) = json_of(&["edit", &file, "--apply", &text, "--dry-run", "--json"]);
    assert_eq!(code, Some(1));
    assert_eq!(doc["error"]["code"], "missing-from-text");
    assert_eq!(doc["error"]["problems"][0]["source"], "activity A2");
    let (code, stdout, _) = run(&[
        "edit",
        &file,
        "--apply",
        &text,
        "--dry-run",
        "--keep-missing",
    ]);
    assert_eq!(code, Some(0), "{stdout}");
    assert!(stdout.contains("nothing to change"), "{stdout}");
    let (code, doc) = json_of(&[
        "edit",
        &file,
        "--apply",
        &text,
        "--dry-run",
        "--delete-missing",
        "--json",
    ]);
    assert_eq!(code, Some(0), "{doc}");
    let applied = doc["applied"].as_array().unwrap();
    assert_eq!(applied.len(), 1, "{doc}");
    assert!(
        applied[0].as_str().unwrap().starts_with("A2 deleted"),
        "{doc}"
    );
}

#[test]
fn applying_text_to_a_store_records_one_changeset_that_replays() {
    let directory = tempfile::tempdir().unwrap();
    let (file, text) = exported(directory.path());
    assert_eq!(
        run(&["history", "init", &file, "-m", "As submitted"]).0,
        Some(0)
    );
    let store = file.replace(".xer", ".tfdb");
    edit_text(
        &text,
        "activity A1 \"Synthetic activity 1\" 1d",
        "activity A1 \"Synthetic activity 1\" 4d",
    );
    let (code, _, stderr) = run(&["edit", &store, "--apply", &text, "-m", "Longer dig"]);
    assert_eq!(code, Some(0), "{stderr}");
    let (_, log) = json_of(&["history", "log", &store, "--json"]);
    assert_eq!(log["changesets"].as_array().unwrap().len(), 2);
    let (code, verify) = json_of(&["history", "verify", &store, "--replay", "--json"]);
    assert_eq!(code, Some(0), "{verify}");
}

#[test]
fn a_mistake_in_applied_text_is_reported_at_its_line() {
    let directory = tempfile::tempdir().unwrap();
    let (file, text) = exported(directory.path());
    edit_text(&text, "after A1 fs+1h", "after A9");
    edit_text(
        &text,
        "activity A1 \"Synthetic activity 1\" 1d",
        "activity A1 \"Synthetic activity 1\" 1d\n  start-on-or-after",
    );
    let (code, doc) = json_of(&["edit", &file, "--apply", &text, "--dry-run", "--json"]);
    assert_eq!(code, Some(1), "{doc}");
    let problems: Vec<(String, u64)> = doc["error"]["problems"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            (
                p["code"].as_str().unwrap().to_owned(),
                p["line"].as_u64().unwrap(),
            )
        })
        .collect();
    assert_eq!(problems, [("unknown-activity".to_owned(), 21)]);
    // With the name right, the edit's own refusal comes back at its line.
    edit_text(&text, "after A9", "after A1");
    let (code, doc) = json_of(&["edit", &file, "--apply", &text, "--dry-run", "--json"]);
    assert_eq!(code, Some(1), "{doc}");
    assert_eq!(doc["error"]["code"], "constraint-needs-date");
    assert_eq!(doc["error"]["problems"][0]["line"], 19);
}

#[test]
fn an_unchanged_apply_still_writes_what_was_asked_for() {
    let directory = tempfile::tempdir().unwrap();
    let (file, text) = exported(directory.path());
    let out = directory.path().join("out.xer");
    let commands = directory.path().join("commands.json");
    let (code, stdout, stderr) = run(&[
        "edit",
        &file,
        "--apply",
        &text,
        "-o",
        out.to_str().unwrap(),
        "--emit-commands",
        commands.to_str().unwrap(),
    ]);
    assert_eq!(code, Some(0), "{stderr}");
    assert!(stdout.contains("nothing to change"), "{stdout}");
    assert_eq!(std::fs::read(&out).unwrap(), std::fs::read(&file).unwrap());
    let doc: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&commands).unwrap()).unwrap();
    assert_eq!(doc["edits"].as_array().unwrap().len(), 0);
}

#[test]
fn the_text_and_data_date_flag_must_agree() {
    let directory = tempfile::tempdir().unwrap();
    let (file, text) = exported(directory.path());
    edit_text(
        &text,
        "data-date 2026-01-12 08:00",
        "data-date 2026-01-19 08:00",
    );
    let (code, doc) = json_of(&[
        "edit",
        &file,
        "--apply",
        &text,
        "--data-date",
        "2026-01-26 08:00",
        "--dry-run",
        "--json",
    ]);
    assert_eq!(code, Some(1));
    assert_eq!(doc["error"]["code"], "data-date-conflict");
    let (code, doc) = json_of(&["edit", &file, "--apply", &text, "--dry-run", "--json"]);
    assert_eq!(code, Some(0), "{doc}");
    assert_eq!(doc["projects"][0]["data_date"], "2026-01-19 08:00", "{doc}");
}

/// Roles in text: a role under another, and work staffed by role with
/// no resource, built, written out again and applied.
#[test]
fn roles_are_built_written_and_applied() {
    let source = "totalfloat-schedule 1\n\nproject P \"p\"\n  data-date 2026-03-02 08:00\n\nrole SUPER \"Superintendent\"\nrole FOREMAN \"Site foreman\"\n  parent SUPER\n\nactivity A \"a\" 2d\n  role FOREMAN units 16\n";
    let directory = tempfile::tempdir().unwrap();
    let plan = write(directory.path(), "plan.tfs", source);
    let built = directory.path().join("plan.xer");
    build_text(Path::new(&plan), &built).unwrap();
    let again = directory.path().join("again.tfs");
    export_text(&built, None, &again);
    let written = std::fs::read_to_string(&again).unwrap();
    for line in [
        "role SUPER \"Superintendent\"\nrole FOREMAN \"Site foreman\"\n  parent SUPER\n",
        "  role FOREMAN units 16\n",
    ] {
        assert!(written.contains(line), "{line}\n{written}");
    }

    // Applied: the role moves to other work, and a new one is added.
    edit_text(
        again.to_str().unwrap(),
        "  parent SUPER\n",
        "  parent SUPER\nrole RIGGER \"Rigger\"\n",
    );
    edit_text(
        again.to_str().unwrap(),
        "  role FOREMAN units 16\n",
        "  role RIGGER units 8\n",
    );
    let out = directory.path().join("out.xer");
    let (code, stdout, stderr) = run(&[
        "edit",
        built.to_str().unwrap(),
        "--apply",
        again.to_str().unwrap(),
        "-o",
        out.to_str().unwrap(),
    ]);
    assert_eq!(code, Some(0), "{stdout}{stderr}");
    for said in [
        "added role RIGGER",
        "A unassigned role FOREMAN",
        "A assigned role RIGGER",
    ] {
        assert!(stdout.contains(said), "{said}\n{stdout}");
    }

    // A role's name or parent can't be changed this way; a role the file
    // doesn't define is a problem.
    edit_text(again.to_str().unwrap(), "  parent SUPER\n", "");
    let (code, doc) = json_of(&[
        "edit",
        out.to_str().unwrap(),
        "--apply",
        again.to_str().unwrap(),
        "--dry-run",
        "--json",
    ]);
    assert_eq!(code, Some(1));
    assert_eq!(doc["error"]["problems"][0]["code"], "unsupported-change");
    let unknown = source.replace("  role FOREMAN units 16", "  role FORMAN");
    assert_eq!(problem_codes(&unknown), ["unknown-role"]);
    let orphan = source.replace("  parent SUPER", "  parent BOSS");
    assert_eq!(problem_codes(&orphan), ["unknown-role"]);
}

/// Expense lines build expenses, are written back as built, and when
/// applied add, change and delete expenses.
#[test]
fn expenses_are_built_written_and_applied() {
    let source = "totalfloat-schedule 1\n\nproject P \"p\"\n  data-date 2026-03-02 08:00\n\nactivity A \"a\" 2d\n  expense \"Building permit\" 1500 accrual start\n  expense \"Skip hire\" 1200 actual 400\n";
    let directory = tempfile::tempdir().unwrap();
    let plan = write(directory.path(), "plan.tfs", source);
    let built = directory.path().join("plan.xer");
    build_text(Path::new(&plan), &built).unwrap();
    let again = directory.path().join("again.tfs");
    export_text(&built, None, &again);
    let written = std::fs::read_to_string(&again).unwrap();
    for line in [
        "  expense \"Building permit\" 1500 accrual start\n",
        "  expense \"Skip hire\" 1200 actual 400\n",
    ] {
        assert!(written.contains(line), "{line}\n{written}");
    }

    // Applied: one changes, one goes, and one comes.
    edit_text(
        again.to_str().unwrap(),
        "  expense \"Skip hire\" 1200 actual 400\n",
        "  expense \"Skip hire\" 1200 actual 900 remaining 300\n  expense \"Survey\" 650\n",
    );
    edit_text(
        again.to_str().unwrap(),
        "  expense \"Building permit\" 1500 accrual start\n",
        "",
    );
    let out = directory.path().join("out.xer");
    let (code, stdout, stderr) = run(&[
        "edit",
        built.to_str().unwrap(),
        "--apply",
        again.to_str().unwrap(),
        "-o",
        out.to_str().unwrap(),
    ]);
    assert_eq!(code, Some(0), "{stdout}{stderr}");
    for said in [
        "A expense Building permit deleted",
        "A has expense Survey, 650",
        "A expense Skip hire: budget 1200, actual 900, remaining 300",
    ] {
        assert!(stdout.contains(said), "{said}\n{stdout}");
    }
    let (code, stdout, stderr) = run(&[
        "query",
        out.to_str().unwrap(),
        "--columns",
        "code,expenses,budgeted_expense_cost,actual_expense_cost,remaining_expense_cost",
    ]);
    assert_eq!(code, Some(0), "{stderr}");
    assert!(stdout.contains("Skip hire, Survey"), "{stdout}");
    assert!(stdout.contains("1850"), "{stdout}");
    assert!(stdout.contains("950"), "{stdout}");
}

/// A credit, and an item with spaces at its ends, as a file can hold them,
/// build, write back as they were, and apply back unchanged; a second line for
/// an item is refused rather than dropped.
#[test]
fn a_credit_and_an_item_with_spaces_at_its_ends_build_and_write_back() {
    let source = "totalfloat-schedule 1\n\nproject P \"p\"\n  data-date 2026-03-02 08:00\n\nactivity A \"a\" 2d\n  expense \" Skip hire \" 1200\n  expense \"Refund\" -500 actual -200\n";
    let directory = tempfile::tempdir().unwrap();
    let plan = write(directory.path(), "plan.tfs", source);
    let built = directory.path().join("plan.xer");
    build_text(Path::new(&plan), &built).unwrap();
    let again = directory.path().join("again.tfs");
    export_text(&built, None, &again);
    let written = std::fs::read_to_string(&again).unwrap();
    for line in [
        "  expense \" Skip hire \" 1200\n",
        "  expense \"Refund\" -500 actual -200\n",
    ] {
        assert!(written.contains(line), "{line}\n{written}");
    }
    let apply = |out: &str| {
        let out = directory.path().join(out);
        run(&[
            "edit",
            built.to_str().unwrap(),
            "--apply",
            again.to_str().unwrap(),
            "-o",
            out.to_str().unwrap(),
        ])
    };
    let (code, stdout, stderr) = apply("same.xer");
    assert_eq!(code, Some(0), "{stdout}{stderr}");
    assert!(!stdout.contains("expense"), "{stdout}");

    edit_text(
        again.to_str().unwrap(),
        "  expense \" Skip hire \" 1200\n",
        "  expense \" Skip hire \" 1200\n  expense \"skip hire\" 2500\n",
    );
    let (code, stdout, stderr) = apply("twice.xer");
    assert_ne!(code, Some(0), "{stdout}{stderr}");
    assert!(stderr.contains("duplicate-expense"), "{stdout}{stderr}");
}
