//! User thresholds and the thresholds a file stores, in `check`, on
//! synthetic files.

#![allow(clippy::unwrap_used, reason = "test helpers outside #[test] functions")]

use std::path::Path;
use std::process::Command;

/// A plan with float on some work: A1010 → A1020 → A2020, and A1040 beside.
const PLAN: &str = r#"totalfloat-schedule 1

project PLANT "New Plant"
  data-date 2026-03-02 08:00

wbs PLANT.CIV "Civil works"

activity A1010 "Excavate" 5d
activity A1020 "Pour foundations" 10d after A1010
activity A1040 "Survey" 2d

wbs PLANT.MEC "Mechanical"

activity A2020 "Install pumps" 6d after A1020
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

fn plan() -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("plan.tfs"), PLAN).unwrap();
    let (code, _, stderr) = run(directory.path(), &["import", "plan.tfs", "-o", "plan.xer"]);
    assert_eq!(code, Some(0), "{stderr}");
    directory
}

const CONFIG: &str = r#"logic_percent = 100.0

[[threshold]]
name = "Float under a week"
column = "total_float_hours"
comparison = "<"
value = 40
scope = { status = ["not-started", "in-progress"] }

[[threshold]]
column = "early_finish"
comparison = ">"
value = "2026-03-20 08:00"
scope = { wbs = "MEC" }
"#;

#[test]
fn a_user_s_thresholds_are_reported_as_checks() {
    let directory = plan();
    std::fs::write(directory.path().join("limits.toml"), CONFIG).unwrap();
    let (code, stdout, stderr) = run(
        directory.path(),
        &["check", "plan.xer", "--config", "limits.toml", "--json"],
    );
    assert_eq!(code, Some(3), "{stderr}");
    let report: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let thresholds = report["thresholds"].as_array().unwrap();
    assert_eq!(thresholds.len(), 2, "{report}");
    let float = &thresholds[0];
    assert_eq!(float["id"], "threshold-1");
    assert_eq!(float["name"], "Float under a week");
    assert_eq!(float["outcome"], "fail");
    // The critical chain has no float; the survey has plenty.
    assert_eq!(
        float["listed"],
        serde_json::json!(["A1010", "A1020", "A2020"])
    );
    assert_eq!(
        (float["offenders"].as_u64(), float["population"].as_u64()),
        (Some(3), Some(4))
    );
    assert_eq!(
        float["measure"],
        "3 of 4 activities have total_float_hours < 40"
    );
    let late = &thresholds[1];
    assert_eq!(late["name"], "early_finish > 2026-03-20 08:00");
    assert_eq!(late["listed"], serde_json::json!(["A2020"]));
    assert_eq!(late["population"], 1);
    // They count among the failed checks, and read as checks do.
    assert!(report["failed"].as_u64().unwrap() >= 2);
    let (_, text, _) = run(
        directory.path(),
        &["check", "plan.xer", "--config", "limits.toml"],
    );
    assert!(text.contains("Float under a week"), "{text}");
    assert!(text.contains("A1010, A1020, A2020"), "{text}");
    let (_, markdown, _) = run(
        directory.path(),
        &["check", "plan.xer", "--config", "limits.toml", "--markdown"],
    );
    assert!(markdown.contains("Float under a week"), "{markdown}");
}

#[test]
fn a_threshold_that_can_t_be_measured_is_refused() {
    let directory = plan();
    for (body, said) in [
        (
            "[[threshold]]\ncolumn = \"total_float_hours\"\ncomparison = \"<<\"\nvalue = 1\n",
            "comparison",
        ),
        (
            "[[threshold]]\ncolumn = \"total_float_hours\"\ncomparison = \"<\"\nvalue = 1\nscope = { colour = \"red\" }\n",
            "colour",
        ),
        (
            "[[threshold]]\ncolumn = \"total_float_hours\"\ncomparison = \"<\"\nvalue = [1]\n",
            "number, text or true/false",
        ),
        (
            "[[threshold]]\ncolumn = \"total_float_hours\"\ncomparison = \"<\"\nvalue = \"40\"\n",
            "holds numbers, and the value is text",
        ),
        (
            "[[threshold]]\ncolumn = \"floatiness\"\ncomparison = \"<\"\nvalue = 1\n",
            "floatiness",
        ),
    ] {
        std::fs::write(directory.path().join("bad.toml"), body).unwrap();
        let (code, _, stderr) = run(
            directory.path(),
            &["check", "plan.xer", "--config", "bad.toml"],
        );
        assert_eq!(code, Some(1), "{body}");
        assert!(stderr.contains(said), "{body}: {stderr}");
    }
}

/// A threshold's scope and `check`'s project both hold; names found in any
/// project are found; each activity counts towards its own project; and
/// empty text never breaks a threshold.
#[test]
fn thresholds_across_projects() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::copy(
        "../../fixtures/xer/two-projects.xer",
        directory.path().join("pair.xer"),
    )
    .unwrap();
    let measured = |body: &str, extra: &[&str]| -> Vec<(u64, String, Vec<String>, u64)> {
        std::fs::write(
            directory.path().join("th.toml"),
            format!("[[threshold]]\n{body}"),
        )
        .unwrap();
        let mut args = vec!["check", "pair.xer", "--config", "th.toml", "--json"];
        args.extend(extra);
        let (_, stdout, stderr) = run(directory.path(), &args);
        let report: serde_json::Value =
            serde_json::from_str(&stdout).unwrap_or_else(|_| panic!("{body}: {stderr}"));
        report["thresholds"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| {
                let listed = t["listed"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|c| c.as_str().unwrap().to_owned())
                    .collect();
                (
                    t["project"].as_u64().unwrap(),
                    t["outcome"].as_str().unwrap().to_owned(),
                    listed,
                    t["population"].as_u64().unwrap(),
                )
            })
            .collect()
    };
    let long = "column = \"original_duration_hours\"\ncomparison = \">\"\nvalue = 1\n";
    // The scope's project: project 2's B1 isn't raised.
    assert_eq!(
        measured(&format!("{long}scope = {{ project = 1 }}\n"), &[]),
        [
            (1, "fail".into(), vec!["A1".into()], 2),
            (2, "not-applicable".into(), vec![], 0)
        ]
    );
    // A WBS node only project 1 has, and a code type only project 2 has.
    assert_eq!(
        measured(&format!("{long}scope = {{ wbs = \"WP\" }}\n"), &[])[0].2,
        ["A1"]
    );
    assert_eq!(
        measured(
            "column = \"code:Synthetic local codes\"\ncomparison = \"=\"\nvalue = \"X\"\n",
            &[]
        )
        .len(),
        2
    );
    // A relation reaching project 2 doesn't make B1 project 1's.
    assert_eq!(
        measured(
            &format!("{long}scope = {{ succs = \"A1\" }}\n"),
            &["--project", "1"]
        ),
        [(1, "pass".into(), vec![], 1)]
    );
    // M1 has no resources: empty text doesn't break `!=`.
    let resources = measured(
        "column = \"resources\"\ncomparison = \"!=\"\nvalue = \"R1\"\n",
        &[],
    );
    assert!(
        resources
            .iter()
            .all(|(_, _, listed, _)| !listed.contains(&"M1".to_owned())),
        "{resources:?}"
    );
}

#[test]
fn a_true_false_threshold_and_text_kept_as_written() {
    let directory = plan();
    std::fs::write(
        directory.path().join("critical.toml"),
        "[[threshold]]\nname = \"Critical | *watch*\"\ncolumn = \"critical\"\ncomparison = \"=\"\nvalue = true\n",
    )
    .unwrap();
    let (code, stdout, stderr) = run(
        directory.path(),
        &["check", "plan.xer", "--config", "critical.toml", "--json"],
    );
    assert_eq!(code, Some(3), "{stderr}");
    let report: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(
        report["thresholds"][0]["listed"],
        serde_json::json!(["A1010", "A1020", "A2020"])
    );
    let (_, markdown, _) = run(
        directory.path(),
        &[
            "check",
            "plan.xer",
            "--config",
            "critical.toml",
            "--markdown",
        ],
    );
    assert!(
        markdown.contains("| Critical \\| \\*watch\\* | FAIL |"),
        "{markdown}"
    );
}

/// The synthetic file with P6's stored thresholds, scheduled into
/// `scheduled.xer` in a new directory.
fn monitoring() -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    std::fs::copy(
        "../../fixtures/xer/monitoring.xer",
        directory.path().join("monitoring.xer"),
    )
    .unwrap();
    let (code, _, stderr) = run(
        directory.path(),
        &["schedule", "monitoring.xer", "-o", "scheduled.xer"],
    );
    assert_eq!(code, Some(0), "{stderr}");
    directory
}

#[test]
fn the_file_s_own_thresholds_are_measured_by_check() {
    let directory = monitoring();
    let (code, stdout, stderr) = run(directory.path(), &["check", "scheduled.xer", "--json"]);
    assert_eq!(code, Some(3), "{stderr}");
    let report: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let thresholds = report["thresholds"].as_array().unwrap();
    let by_id = |id: &str| thresholds.iter().find(|t| t["id"] == id).unwrap().clone();
    // Total float outside 8 to 40 hours: both activities, at 0 hours.
    let float = by_id("p6-threshold-1");
    assert_eq!(float["outcome"], "fail");
    assert_eq!(float["listed"], serde_json::json!(["TF-001", "TF-002"]));
    assert_eq!(float["name"], "P6 threshold: Total float, hours");
    // One on WBS nodes as a whole, and one the file doesn't enable: not measured, saying why.
    let wbs = by_id("p6-threshold-2");
    assert_eq!(wbs["outcome"], "not-applicable");
    assert!(wbs["measure"].as_str().unwrap().contains("WBS nodes"));
    let off = by_id("p6-threshold-3");
    assert_eq!(off["outcome"], "not-applicable");
    assert!(off["measure"].as_str().unwrap().contains("TS_Disable"));
}

/// A stored threshold watches its own WBS node and those under it, found by
/// ID: another branch's node with the same short code isn't watched.
#[test]
fn a_stored_threshold_watches_its_own_branch_only() {
    let directory = monitoring();
    let text = std::fs::read_to_string(directory.path().join("scheduled.xer")).unwrap();
    let mut out = Vec::new();
    let mut table = String::new();
    let mut fields: Vec<String> = Vec::new();
    for line in text.split("\r\n") {
        let cells: Vec<&str> = line.split('\t').collect();
        match cells[0] {
            "%T" => table = cells[1].to_owned(),
            "%F" => fields = cells[1..].iter().map(|c| (*c).to_owned()).collect(),
            _ => {}
        }
        let at = |name: &str| fields.iter().position(|f| f == name).unwrap() + 1;
        if cells[0] == "%R" && table == "PROJWBS" {
            out.push(line.to_owned());
            // Two branches under the root, each with a node coded AREA.
            for (id, parent, code) in [
                ("11", "10", "NORTH"),
                ("12", "10", "SOUTH"),
                ("13", "11", "AREA"),
                ("14", "12", "AREA"),
            ] {
                let mut row: Vec<String> = cells.iter().map(|c| (*c).to_owned()).collect();
                row[at("wbs_id")] = id.to_owned();
                row[at("parent_wbs_id")] = parent.to_owned();
                row[at("wbs_short_name")] = code.to_owned();
                row[at("wbs_name")] = code.to_owned();
                row[at("proj_node_flag")] = "N".to_owned();
                row[at("guid")] = String::new();
                out.push(row.join("\t"));
            }
            continue;
        }
        if cells[0] == "%R" && table == "TASK" {
            let mut row: Vec<String> = cells.iter().map(|c| (*c).to_owned()).collect();
            row[at("wbs_id")] = if row[at("task_code")] == "TF-001" {
                "13"
            } else {
                "14"
            }
            .to_owned();
            out.push(row.join("\t"));
            continue;
        }
        if cells[0] == "%R" && table == "PROJTHRS" && cells[1] == "1" {
            let mut row: Vec<String> = cells.iter().map(|c| (*c).to_owned()).collect();
            row[at("wbs_id")] = "13".to_owned();
            out.push(row.join("\t"));
            continue;
        }
        out.push(line.to_owned());
    }
    std::fs::write(directory.path().join("branches.xer"), out.join("\r\n")).unwrap();
    let (_, stdout, stderr) = run(directory.path(), &["check", "branches.xer", "--json"]);
    let report: serde_json::Value =
        serde_json::from_str(&stdout).unwrap_or_else(|_| panic!("{stderr}"));
    let float = report["thresholds"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["id"] == "p6-threshold-1")
        .unwrap()
        .clone();
    assert_eq!(float["listed"], serde_json::json!(["TF-001"]), "{float}");
    assert_eq!(float["population"], 1);
}

/// A threshold with a window of dates isn't measured yet, and a config can
/// leave the file's thresholds out.
#[test]
fn a_windowed_threshold_is_listed_and_stored_ones_can_be_left_out() {
    let directory = monitoring();
    let text = std::fs::read_to_string(directory.path().join("scheduled.xer")).unwrap();
    let windowed = text.replacen(
        "TS_Enable\tPT_Normal\tTM_Task\t10\t\t8\t40\t\t\t",
        "TS_Enable\tPT_Normal\tTM_Task\t10\t\t8\t40\t2026-01-01 00:00\t2026-02-01 00:00\t",
        1,
    );
    assert_ne!(windowed, text);
    std::fs::write(directory.path().join("windowed.xer"), windowed).unwrap();
    let (_, stdout, _) = run(directory.path(), &["check", "windowed.xer", "--json"]);
    let report: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let first = report["thresholds"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["id"] == "p6-threshold-1")
        .unwrap()
        .clone();
    assert_eq!(first["outcome"], "not-applicable");
    assert!(
        first["measure"].as_str().unwrap().contains("window"),
        "{first}"
    );
    std::fs::write(
        directory.path().join("own.toml"),
        "stored_thresholds = false\n",
    )
    .unwrap();
    let (code, stdout, stderr) = run(
        directory.path(),
        &["check", "scheduled.xer", "--config", "own.toml", "--json"],
    );
    let report: serde_json::Value =
        serde_json::from_str(&stdout).unwrap_or_else(|_| panic!("{stderr}"));
    assert!(report.get("thresholds").is_none(), "{report}");
    assert_ne!(code, None);
}
