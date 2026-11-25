//! `totalfloat portfolio`: the EPS kept in a portfolio file, and each
//! node's and project's figures rolled up the tree.

#![allow(clippy::unwrap_used, reason = "test helpers outside #[test] functions")]

use std::path::Path;
use std::process::Command;

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

/// Two schedule files in a new directory: `two.xer` (projects SYN1 and
/// SYN2) and `one.xer` (project rel-fs-positive).
fn files() -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    std::fs::copy(
        "../../fixtures/xer/two-projects.xer",
        directory.path().join("two.xer"),
    )
    .unwrap();
    std::fs::copy(
        "../../fixtures/xer/batches/001/rel-fs-positive.xer",
        directory.path().join("one.xer"),
    )
    .unwrap();
    directory
}

/// ENT > A (SYN1) and ENT > B (SYN2) > B1 (rel-fs-positive).
const TREE: &str = r#"schema = "totalfloat.portfolio/1"

[[eps]]
id = "ENT"
name = "Enterprise"

[[eps]]
id = "A"
name = "Alpha"
parent = "ENT"

[[eps]]
id = "B"
name = "Bravo"
parent = "ENT"

[[eps]]
id = "B1"
name = "Bravo one"
parent = "B"

[[project]]
file = "two.xer"
project = 1
eps = "A"

[[project]]
file = "two.xer"
project = 2
eps = "B"

[[project]]
file = "one.xer"
eps = "B1"
"#;

fn show(directory: &Path) -> Value {
    let (code, stdout, stderr) = run(
        directory,
        &["portfolio", "show", "plan.portfolio.toml", "--json"],
    );
    assert_eq!(code, Some(0), "{stderr}");
    serde_json::from_str(&stdout).unwrap()
}

#[test]
fn figures_roll_up_the_tree() {
    let directory = files();
    std::fs::write(directory.path().join("plan.portfolio.toml"), TREE).unwrap();
    let report = show(directory.path());
    let nodes: Vec<(&str, usize)> = report["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| {
            (
                n["id"].as_str().unwrap(),
                usize::try_from(n["depth"].as_u64().unwrap()).unwrap(),
            )
        })
        .collect();
    // Tree order: each node after its parent, siblings in file order.
    assert_eq!(nodes, [("ENT", 0), ("A", 1), ("B", 1), ("B1", 2)]);
    let node = |id: &str| {
        report["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["id"] == id)
            .unwrap()["totals"]
            .clone()
    };
    let project = |code: &str| {
        report["projects"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["code"] == code)
            .unwrap()["totals"]
            .clone()
    };
    // SYN1 has A1 and M1, SYN2 has B1, rel-fs-positive has A1 and A2.
    assert_eq!(project("SYN1")["activities"], 2);
    assert_eq!(project("SYN2")["activities"], 1);
    assert_eq!(project("rel-fs-positive")["activities"], 2);
    assert_eq!(node("B1")["activities"], 2);
    assert_eq!(node("B")["activities"], 3);
    assert_eq!(node("B")["projects"], 2);
    assert_eq!(node("ENT")["activities"], 5);
    assert_eq!(node("ENT")["projects"], 3);
    assert_eq!(node("ENT")["in_progress"], 1);
    // A node starts with its earliest project and finishes with its latest.
    let earliest = ["SYN1", "SYN2", "rel-fs-positive"]
        .iter()
        .filter_map(|c| project(c)["start"].as_str().map(str::to_owned))
        .min()
        .unwrap();
    let latest = ["SYN1", "SYN2", "rel-fs-positive"]
        .iter()
        .filter_map(|c| project(c)["finish"].as_str().map(str::to_owned))
        .max()
        .unwrap();
    assert_eq!(node("ENT")["start"], earliest);
    assert_eq!(node("ENT")["finish"], latest);
    // rel-fs-positive stores no results, so B's dates are SYN2's.
    assert_eq!(project("rel-fs-positive")["finish"], Value::Null);
    assert_eq!(node("B")["finish"], project("SYN2")["finish"]);
    // SYN1 must finish by 2026-02-01 17:00.
    let syn1 = report["projects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["code"] == "SYN1")
        .unwrap();
    assert_eq!(syn1["must_finish_by"], "2026-02-01 17:00");
    assert!(syn1["finish_variance_days"].is_number());
}

#[test]
fn new_groups_projects_by_the_eps_node_their_files_keep() {
    let directory = files();
    let (code, stdout, stderr) = run(
        directory.path(),
        &[
            "portfolio",
            "new",
            "two.xer",
            "one.xer",
            "-o",
            "plan.portfolio.toml",
        ],
    );
    assert_eq!(code, Some(0), "{stderr}");
    assert!(
        stdout.starts_with("Wrote plan.portfolio.toml: 3 projects"),
        "{stdout}"
    );
    let report = show(directory.path());
    let projects = report["projects"].as_array().unwrap();
    assert_eq!(projects.len(), 3);
    // two-projects.xer keeps no EPS parent for its root nodes, so its
    // projects sit at the top; every project is placed once.
    for p in projects {
        assert!(p["eps"].as_str().unwrap().starts_with('E'), "{p}");
    }
    let (code, _, stderr) = run(
        directory.path(),
        &["portfolio", "new", "two.xer", "-o", "plan.portfolio.toml"],
    );
    assert_ne!(code, Some(0));
    assert!(stderr.contains("exists"), "{stderr}");
}

#[test]
fn every_problem_is_named_with_its_line() {
    let directory = files();
    let bad = r#"schema = "totalfloat.portfolio/1"

[[eps]]
id = "ENT"
name = "Enterprise"

[[eps]]
id = "ENT"
name = "Again"

[[eps]]
id = "X"
name = "Lost"
parent = "NOPE"

[[eps]]
id = "L1"
name = "Loop one"
parent = "L2"

[[eps]]
id = "L2"
name = "Loop two"
parent = "L1"

[[project]]
file = "two.xer"
project = 1
eps = "ENT"

[[project]]
file = "two.xer"
eps = "ENT"

[[project]]
file = "gone.xer"
eps = "ENT"

[[project]]
file = "one.xer"
project = 99
eps = "Z"
"#;
    std::fs::write(directory.path().join("plan.portfolio.toml"), bad).unwrap();
    let (code, _, stderr) = run(
        directory.path(),
        &["portfolio", "show", "plan.portfolio.toml"],
    );
    assert_ne!(code, Some(0));
    for expected in [
        "line 8: two EPS nodes are ENT",
        "line 14: X has no EPS node NOPE to sit under",
        "is in a loop of parents",
        "line 32: project 1 of two.xer is placed twice",
        "line 36: gone.xer: can't read gone.xer",
        "line 42: there's no EPS node Z",
        "line 40: one.xer has no project 99",
    ] {
        assert!(
            stderr.contains(expected),
            "missing {expected:?} in {stderr}"
        );
    }
    std::fs::write(directory.path().join("plan.portfolio.toml"), "schema = 1\n").unwrap();
    let (code, _, stderr) = run(
        directory.path(),
        &["portfolio", "show", "plan.portfolio.toml"],
    );
    assert_ne!(code, Some(0));
    assert!(stderr.contains("line 1"), "{stderr}");
}

#[test]
fn the_text_report() {
    let directory = files();
    std::fs::write(directory.path().join("plan.portfolio.toml"), TREE).unwrap();
    let (code, stdout, stderr) = run(
        directory.path(),
        &["portfolio", "show", "plan.portfolio.toml"],
    );
    assert_eq!(code, Some(0), "{stderr}");
    insta::assert_snapshot!(stdout);
}

#[test]
fn the_json_follows_its_schema() {
    let directory = files();
    std::fs::write(directory.path().join("plan.portfolio.toml"), TREE).unwrap();
    let report = show(directory.path());
    let schema: Value = serde_json::from_str(
        &std::fs::read_to_string("../../docs/schemas/portfolio-show-1.schema.json").unwrap(),
    )
    .unwrap();
    let keys = |value: &Value| {
        let mut keys: Vec<String> = value.as_object().unwrap().keys().cloned().collect();
        keys.sort();
        keys
    };
    assert_eq!(keys(&report), keys(&schema["properties"]));
    assert_eq!(report["schema"], "totalfloat.portfolio-show/1");
}

#[test]
fn one_file_named_two_ways_is_still_placed_once() {
    // `./two.xer` and `two.xer` are one file.
    let directory = files();
    std::fs::write(
        directory.path().join("plan.portfolio.toml"),
        "schema = \"totalfloat.portfolio/1\"\n[[eps]]\nid = \"ENT\"\nname = \"E\"\n[[project]]\nfile = \"two.xer\"\nproject = 1\neps = \"ENT\"\n[[project]]\nfile = \"./two.xer\"\nproject = 1\neps = \"ENT\"\n",
    )
    .unwrap();
    let (code, _, stderr) = run(
        directory.path(),
        &["portfolio", "show", "plan.portfolio.toml"],
    );
    assert_ne!(code, Some(0));
    assert!(
        stderr.contains("line 10: project 1 of ./two.xer is placed twice"),
        "{stderr}"
    );
}

#[test]
fn a_project_short_of_rows_is_refused_not_undercounted() {
    // A2's remaining duration can't be held in whole minutes, so the model
    // leaves its row out; the project's figures would be short.
    let directory = files();
    let text = std::fs::read_to_string(directory.path().join("one.xer")).unwrap();
    let row = text
        .lines()
        .find(|l| l.starts_with("%R\t2\t1\t1\t200\t"))
        .unwrap();
    let cells: Vec<&str> = row.split('\t').collect();
    // remain_drtn_hr_cnt is the 19th field, after `%R`.
    assert_eq!(cells[19], "8");
    let mut changed = cells.clone();
    changed[19] = "8.123";
    std::fs::write(
        directory.path().join("one.xer"),
        text.replace(row, &changed.join("\t")),
    )
    .unwrap();
    std::fs::write(directory.path().join("plan.portfolio.toml"), TREE).unwrap();
    let (code, _, stderr) = run(
        directory.path(),
        &["portfolio", "show", "plan.portfolio.toml"],
    );
    assert_ne!(code, Some(0));
    assert!(stderr.contains("one.xer: project 1 lost"), "{stderr}");
}

#[test]
fn an_assignment_lost_without_its_project_still_counts_against_it() {
    // The assignment's `proj_id` is blank, so the model leaves it out; its
    // activity, A1, is project 1's.
    let directory = files();
    let text = std::fs::read_to_string(directory.path().join("two.xer")).unwrap();
    assert!(text.contains("%R\t801\t1\t101\t501"));
    std::fs::write(
        directory.path().join("two.xer"),
        text.replace("%R\t801\t1\t101\t501", "%R\t801\t\t101\t501"),
    )
    .unwrap();
    std::fs::write(directory.path().join("plan.portfolio.toml"), TREE).unwrap();
    let (code, _, stderr) = run(
        directory.path(),
        &["portfolio", "show", "plan.portfolio.toml"],
    );
    assert_ne!(code, Some(0));
    assert!(stderr.contains("two.xer: project 1 lost"), "{stderr}");
}

#[test]
fn review_fixes_on_reading_files() {
    let directory = files();
    // A duplicate assignment row is left out of the model, but its ID was
    // kept, so nothing is short and the project is shown.
    let text = std::fs::read_to_string(directory.path().join("two.xer")).unwrap();
    let row = text
        .lines()
        .find(|l| l.starts_with("%R\t801\t"))
        .unwrap()
        .to_owned();
    std::fs::write(
        directory.path().join("two.xer"),
        text.replacen(&row, &format!("{row}\n{row}"), 1),
    )
    .unwrap();
    std::fs::write(directory.path().join("plan.portfolio.toml"), TREE).unwrap();
    let (code, _, stderr) = run(
        directory.path(),
        &["portfolio", "show", "plan.portfolio.toml"],
    );
    assert_eq!(code, Some(0), "{stderr}");

    // A file that isn't a schedule says why, as the reader puts it.
    std::fs::write(directory.path().join("notes.txt"), "not a schedule\n").unwrap();
    std::fs::write(
        directory.path().join("plan.portfolio.toml"),
        "schema = \"totalfloat.portfolio/1\"\n[[eps]]\nid = \"E\"\nname = \"E\"\n[[project]]\nfile = \"notes.txt\"\neps = \"E\"\n",
    )
    .unwrap();
    let (code, _, stderr) = run(
        directory.path(),
        &["portfolio", "show", "plan.portfolio.toml"],
    );
    assert_ne!(code, Some(0));
    assert!(
        stderr.contains("line 6: notes.txt:") && stderr.contains("ERMHDR"),
        "{stderr}"
    );

    // `new` places a file given twice once, so `show` reads what it wrote.
    let (code, stdout, stderr) = run(
        directory.path(),
        &[
            "portfolio",
            "new",
            "one.xer",
            "./one.xer",
            "-o",
            "again.portfolio.toml",
        ],
    );
    assert_eq!(code, Some(0), "{stderr}");
    assert!(
        stdout.contains(": 1 projects") || stdout.contains(": 1 project"),
        "{stdout}"
    );
    let (code, _, stderr) = run(
        directory.path(),
        &["portfolio", "show", "again.portfolio.toml"],
    );
    assert_eq!(code, Some(0), "{stderr}");
}

#[test]
fn a_lost_task_counts_only_against_its_own_project() {
    // SYN2's B1 can't be held in whole minutes, so its TASK row is left out,
    // and its assignment, with its project blank, goes too. That's project
    // 2's loss, not project 1's.
    let directory = files();
    let text = std::fs::read_to_string(directory.path().join("two.xer")).unwrap();
    let text = text
        .replace(
            "TT_Rsrc\tDT_FixedRate\tCP_Units\tTK_NotStart\t12.25\t12.25",
            "TT_Rsrc\tDT_FixedRate\tCP_Units\tTK_NotStart\t12.25\t12.2501",
        )
        .replace("%R\t802\t2\t201\t502", "%R\t802\t\t201\t502");
    std::fs::write(directory.path().join("two.xer"), text).unwrap();
    std::fs::write(directory.path().join("plan.portfolio.toml"), TREE).unwrap();
    let (code, _, stderr) = run(
        directory.path(),
        &["portfolio", "show", "plan.portfolio.toml"],
    );
    assert_ne!(code, Some(0));
    assert!(
        stderr.contains("project 2 lost") || stderr.contains("project 2"),
        "{stderr}"
    );
    assert!(!stderr.contains("project 1 lost"), "{stderr}");
}
