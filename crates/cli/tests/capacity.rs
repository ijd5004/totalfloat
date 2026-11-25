//! `totalfloat portfolio capacity`: each resource's demand across a
//! portfolio's projects against what it can supply.

#![allow(clippy::unwrap_used, reason = "test helpers outside #[test] functions")]

use std::fmt::Write as _;
use std::path::Path;
use std::process::Command;

use serde_json::Value;

/// The crew can work 1 unit an hour, 40 hours a week. Its demand by week:
/// 16 (actual), 24, 60 and 60 (`query --usage CREW --by week`).
const USAGE: &str = "../../fixtures/xer/usage.xer";

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

/// A portfolio of `files`, each a copy of the usage fixture.
fn portfolio(files: &[&str]) -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    let mut text =
        "schema = \"totalfloat.portfolio/1\"\n[[eps]]\nid = \"E\"\nname = \"E\"\n".to_owned();
    for file in files {
        std::fs::copy(USAGE, directory.path().join(file)).unwrap();
        writeln!(text, "[[project]]\nfile = \"{file}\"\neps = \"E\"").unwrap();
    }
    std::fs::write(directory.path().join("plan.portfolio.toml"), text).unwrap();
    directory
}

fn capacity(directory: &Path, more: &[&str]) -> Value {
    let mut args = vec!["portfolio", "capacity", "plan.portfolio.toml", "--json"];
    args.extend_from_slice(more);
    let (code, stdout, stderr) = run(directory, &args);
    assert_eq!(code, Some(0), "{stderr}");
    serde_json::from_str(&stdout).unwrap()
}

#[test]
fn one_project_s_demand_is_its_usage() {
    let directory = portfolio(&["a.xer"]);
    let report = capacity(directory.path(), &[]);
    let crew = &report["resources"][0];
    assert_eq!(crew["short_name"], "CREW");
    let (code, stdout, stderr) = run(
        directory.path(),
        &[
            "query", "a.xer", "--usage", "CREW", "--by", "week", "--json",
        ],
    );
    assert_eq!(code, Some(0), "{stderr}");
    let usage: Value = serde_json::from_str(&stdout).unwrap();
    let ours: Vec<(Value, Value, Value)> = crew["periods"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| (p["start"].clone(), p["demand"].clone(), p["supply"].clone()))
        .collect();
    let theirs: Vec<(Value, Value, Value)> = usage["periods"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| (p["start"].clone(), p["total"].clone(), p["limit"].clone()))
        .collect();
    assert_eq!(ours, theirs);
    // Over in the weeks of 60, not in those of 16 and 24.
    let over: Vec<bool> = crew["periods"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["over"].as_bool().unwrap())
        .collect();
    assert_eq!(over, [false, false, true, true]);
    assert_eq!(crew["periods_over"], 2);
}

#[test]
fn two_projects_on_one_crew_add_up_against_one_supply() {
    // The same crew (by short name) in two files: demand doubles, supply
    // doesn't. 32 and 48 against 40: over in the second week, not the
    // first; the worst week is 120 against 40, 60 from each project.
    let directory = portfolio(&["a.xer", "b.xer"]);
    let report = capacity(directory.path(), &[]);
    let crew = &report["resources"][0];
    let demand: Vec<f64> = crew["periods"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["demand"].as_f64().unwrap())
        .collect();
    assert_eq!(demand, [32.0, 48.0, 120.0, 120.0]);
    let over: Vec<bool> = crew["periods"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["over"].as_bool().unwrap())
        .collect();
    assert_eq!(over, [false, true, true, true]);
    let peak = &crew["peak"];
    assert_eq!(peak["demand"], 120.0);
    assert_eq!(peak["supply"], 40.0);
    assert_eq!(peak["over_by"], 80.0);
    let shares: Vec<(Value, Value)> = peak["projects"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| (s["file"].clone(), s["demand"].clone()))
        .collect();
    assert_eq!(
        shares,
        [
            (Value::from("a.xer"), Value::from(60.0)),
            (Value::from("b.xer"), Value::from(60.0))
        ]
    );
    assert_eq!(crew["total_demand"], 320.0);
    assert_eq!(crew["assignments"], 4);
}

#[test]
fn by_month_and_text() {
    let directory = portfolio(&["a.xer", "b.xer"]);
    let report = capacity(directory.path(), &["--by", "month"]);
    let crew = &report["resources"][0];
    // The actuals begin on 2026-01-01, so January is the first month.
    assert_eq!(crew["periods"][0]["start"], "2026-01-01");
    assert_eq!(crew["total_demand"], 320.0);
    let (code, stdout, stderr) = run(
        directory.path(),
        &[
            "portfolio",
            "capacity",
            "plan.portfolio.toml",
            "--resource",
            "crew",
        ],
    );
    assert_eq!(code, Some(0), "{stderr}");
    insta::assert_snapshot!(stdout);
    let (code, stdout, stderr) = run(
        directory.path(),
        &[
            "portfolio",
            "capacity",
            "plan.portfolio.toml",
            "--resource",
            "crew",
            "--by",
            "month",
        ],
    );
    assert_eq!(code, Some(0), "{stderr}");
    insta::assert_snapshot!("by_month_text", stdout);
}

#[test]
fn a_resource_no_project_asks_for_is_refused() {
    let directory = portfolio(&["a.xer"]);
    let (code, _, stderr) = run(
        directory.path(),
        &[
            "portfolio",
            "capacity",
            "plan.portfolio.toml",
            "--resource",
            "NOPE",
        ],
    );
    assert_ne!(code, Some(0));
    assert!(
        stderr.contains("no project in the portfolio has demand on NOPE"),
        "{stderr}"
    );
    let (code, _, stderr) = run(
        directory.path(),
        &[
            "portfolio",
            "capacity",
            "plan.portfolio.toml",
            "--role",
            "NOPE",
        ],
    );
    assert_ne!(code, Some(0));
    assert!(
        stderr.contains("error[no-such-role]: no project in the portfolio has demand on NOPE"),
        "{stderr}"
    );
}

#[test]
fn the_json_follows_its_schema() {
    let directory = portfolio(&["a.xer"]);
    let report = capacity(directory.path(), &[]);
    let schema: Value = serde_json::from_str(
        &std::fs::read_to_string("../../docs/schemas/portfolio-capacity-1.schema.json").unwrap(),
    )
    .unwrap();
    let keys = |value: &Value| {
        let mut keys: Vec<String> = value.as_object().unwrap().keys().cloned().collect();
        keys.sort();
        keys
    };
    assert_eq!(keys(&report), keys(&schema["properties"]));
    assert_eq!(report["schema"], "totalfloat.portfolio-capacity/1");
}

#[test]
fn work_staffed_by_role_alone_counts_against_the_role() {
    // An engineer role, with no maximum, on A2 for 40 hours with no resource
    // named: it shows under roles, never over, as nothing limits it.
    let directory = portfolio(&["a.xer"]);
    std::fs::write(
        directory.path().join("roles.json"),
        r#"[{"op": "add_role", "role": "ENG", "name": "Engineer"},
            {"op": "assign_role", "activity": "A2", "role": "ENG", "units": 40}]"#,
    )
    .unwrap();
    let (code, _, stderr) = run(
        directory.path(),
        &["edit", "a.xer", "--edits", "roles.json", "-o", "b.xer"],
    );
    assert_eq!(code, Some(0), "{stderr}");
    std::fs::write(
        directory.path().join("plan.portfolio.toml"),
        "schema = \"totalfloat.portfolio/1\"\n[[eps]]\nid = \"E\"\nname = \"E\"\n[[project]]\nfile = \"b.xer\"\neps = \"E\"\n",
    )
    .unwrap();
    let report = capacity(directory.path(), &["--role", "eng"]);
    assert_eq!(report["resources"], Value::Array(vec![]));
    let eng = &report["roles"][0];
    assert_eq!(eng["short_name"], "ENG");
    assert_eq!(eng["total_demand"], 40.0);
    assert_eq!(eng["periods_over"], 0);
    assert!(
        eng["periods"]
            .as_array()
            .unwrap()
            .iter()
            .all(|p| p["supply"].is_null())
    );
}

#[test]
fn supply_comes_from_the_first_project_holding_the_resource() {
    // The first file holds the crew at 2 units an hour but gives it no work;
    // the second has all the work. Supply is the first file's, 80 hours a
    // week, so no week is over.
    let directory = portfolio(&["first.xer", "b.xer"]);
    let text = std::fs::read_to_string(directory.path().join("first.xer")).unwrap();
    let text: String = text
        .lines()
        .filter(|l| !l.starts_with("%R\t801\t") && !l.starts_with("%R\t803\t"))
        .map(|l| {
            if l.starts_with("%R\t1\t501\t1\t50\t") {
                l.replacen("\t501\t1\t", "\t501\t2\t", 1)
            } else {
                l.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(directory.path().join("first.xer"), text).unwrap();
    let report = capacity(directory.path(), &[]);
    let crew = &report["resources"][0];
    let supply: Vec<f64> = crew["periods"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["supply"].as_f64().unwrap())
        .collect();
    assert_eq!(supply[2], 80.0, "{crew}");
    assert_eq!(crew["periods_over"], 0);
    assert_eq!(crew["assignments"], 2);
}

/// `b.xer` with its crew renamed or retyped.
fn with_crew(directory: &Path, short: &str, kind: &str) {
    let text = std::fs::read_to_string(directory.join("b.xer")).unwrap();
    let text = text.replace(
        "Synthetic labor\tCREW\t",
        &format!("Synthetic labor\t{short}\t"),
    );
    let text = text.replace("\tRT_Labor\t", &format!("\t{kind}\t"));
    std::fs::write(directory.join("b.xer"), text).unwrap();
}

#[test]
fn review_fixes_on_matching_resources() {
    // A short name matches whatever its case: `Crew` in b.xer is a.xer's
    // `CREW`, so their demand adds up on one line.
    let directory = portfolio(&["a.xer", "b.xer"]);
    with_crew(directory.path(), "Crew", "RT_Labor");
    let report = capacity(directory.path(), &[]);
    assert_eq!(report["resources"].as_array().unwrap().len(), 1, "{report}");
    assert_eq!(report["resources"][0]["total_demand"], 320.0);

    // Units of material never add to hours: a material `CREW` is its own line.
    let directory = portfolio(&["a.xer", "b.xer"]);
    with_crew(directory.path(), "CREW", "RT_Mat");
    let report = capacity(directory.path(), &[]);
    let units: Vec<&str> = report["resources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["unit"].as_str().unwrap())
        .collect();
    assert_eq!(units, ["hours", "units"]);

    // An assignment naming a resource the file doesn't hold is reported, not
    // dropped.
    let directory = portfolio(&["a.xer"]);
    let text = std::fs::read_to_string(directory.path().join("a.xer")).unwrap();
    let row = text
        .lines()
        .find(|l| l.starts_with("%R\t803\t"))
        .unwrap()
        .to_owned();
    let cells: Vec<&str> = row.split('\t').collect();
    assert_eq!(cells[7], "501");
    let mut changed = cells.clone();
    changed[7] = "999";
    std::fs::write(
        directory.path().join("a.xer"),
        text.replace(&row, &changed.join("\t")),
    )
    .unwrap();
    // Mapping leaves that assignment out, so the portfolio refuses the file
    // as short rather than undercount.
    let (code, _, stderr) = run(
        directory.path(),
        &["portfolio", "capacity", "plan.portfolio.toml"],
    );
    assert_ne!(code, Some(0));
    assert!(stderr.contains("project 1 lost 1"), "{stderr}");
}

#[test]
fn a_role_s_limit_set_by_an_edit_is_its_supply() {
    // The engineer role, limited to half a unit an hour from the
    // command line, supplies 20 hours in a full 40-hour week.
    let directory = portfolio(&["a.xer"]);
    std::fs::write(
        directory.path().join("roles.json"),
        r#"[{"op": "add_role", "role": "ENG", "name": "Engineer"},
            {"op": "assign_role", "activity": "A2", "role": "ENG", "units": 40}]"#,
    )
    .unwrap();
    let (code, _, stderr) = run(
        directory.path(),
        &[
            "edit",
            "a.xer",
            "--edits",
            "roles.json",
            "--role-limit",
            "ENG=0.5",
            "-o",
            "b.xer",
        ],
    );
    assert_eq!(code, Some(0), "{stderr}");
    std::fs::write(
        directory.path().join("plan.portfolio.toml"),
        "schema = \"totalfloat.portfolio/1\"\n[[eps]]\nid = \"E\"\nname = \"E\"\n[[project]]\nfile = \"b.xer\"\neps = \"E\"\n",
    )
    .unwrap();
    let report = capacity(directory.path(), &["--role", "eng"]);
    let eng = &report["roles"][0];
    assert_eq!(eng["short_name"], "ENG");
    let periods = eng["periods"].as_array().unwrap();
    assert!(periods.iter().all(|p| !p["supply"].is_null()), "{eng}");
    assert!(periods.iter().any(|p| p["supply"] == 20.0), "{eng}");
}
