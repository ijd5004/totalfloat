//! `totalfloat history`: starting a history, reading it back and exporting.

#![allow(clippy::unwrap_used, reason = "test helpers outside #[test] functions")]

use std::path::{Path, PathBuf};
use std::process::Command;

const FIXTURE: &str = "../../fixtures/xer/batches/001/rel-fs-positive.xer";

/// Runs the binary with a fixed author and time, so IDs repeat.
fn run(args: &[&str]) -> (Option<i32>, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_totalfloat"))
        .args(args)
        .env("TOTALFLOAT_NOW", "2026-11-02T09:00:00Z")
        .env("TOTALFLOAT_AUTHOR", "Pat")
        .env_remove("TOTALFLOAT_AUTHOR_KIND")
        .output()
        .unwrap();
    (
        out.status.code(),
        String::from_utf8(out.stdout).unwrap(),
        String::from_utf8(out.stderr).unwrap(),
    )
}

/// A copy of the fixture as `plant.xer`, and its history started.
fn started(directory: &Path) -> PathBuf {
    let file = directory.join("plant.xer");
    std::fs::copy(FIXTURE, &file).unwrap();
    let (code, stdout, stderr) = run(&[
        "history",
        "init",
        file.to_str().unwrap(),
        "-m",
        "As submitted",
    ]);
    assert_eq!(code, Some(0), "{stderr}");
    assert!(stdout.contains("on main"), "{stdout}");
    directory.join("plant.tfdb")
}

/// `text` with the temporary `directory` and the separator after it written
/// as `<dir>/`, whether printed as is, in JSON or in Markdown, so snapshots
/// are the same on every OS. JSON escapes a Windows path's backslashes, and
/// Markdown escapes them and other markup characters such as `~`.
fn without_dir(text: &str, directory: &Path) -> String {
    let raw = format!("{}{}", directory.display(), std::path::MAIN_SEPARATOR);
    let json = raw.replace('\\', "\\\\");
    let markdown: String = raw
        .chars()
        .flat_map(|c| {
            let escaped = "\\`*_[]|~#<>&".contains(c);
            escaped.then_some('\\').into_iter().chain([c])
        })
        .collect();
    [markdown, json, raw]
        .iter()
        .fold(text.to_owned(), |text, form| text.replace(form, "<dir>/"))
}

fn keys(value: &serde_json::Value) -> Vec<String> {
    let mut keys: Vec<String> = value.as_object().unwrap().keys().cloned().collect();
    keys.sort();
    keys
}

fn schema(name: &str) -> serde_json::Value {
    let text = std::fs::read_to_string(format!("../../docs/schemas/{name}")).unwrap();
    serde_json::from_str(&text).unwrap()
}

#[test]
fn a_history_exports_the_file_it_started_from() {
    let directory = tempfile::tempdir().unwrap();
    let store = started(directory.path());
    let back = directory.path().join("back.xer");
    let (code, stdout, stderr) = run(&[
        "history",
        "export",
        store.to_str().unwrap(),
        "-o",
        back.to_str().unwrap(),
    ]);
    assert_eq!(code, Some(0), "{stderr}");
    assert!(stdout.starts_with("Wrote "), "{stdout}");
    assert_eq!(
        std::fs::read(&back).unwrap(),
        std::fs::read(FIXTURE).unwrap()
    );
    // Exporting again over the same path is refused.
    let (code, ..) = run(&[
        "history",
        "export",
        store.to_str().unwrap(),
        "-o",
        back.to_str().unwrap(),
    ]);
    assert_eq!(code, Some(1));
    // A second init beside the same file is refused too.
    let file = directory.path().join("plant.xer");
    let (code, _, stderr) = run(&["history", "init", file.to_str().unwrap()]);
    assert_eq!(code, Some(1));
    assert!(stderr.contains("output-exists"), "{stderr}");
}

#[test]
fn a_file_that_isn_t_an_xer_starts_nothing() {
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("notes.xer");
    std::fs::write(&file, "not a schedule\n").unwrap();
    let (code, _, stderr) = run(&["history", "init", file.to_str().unwrap()]);
    assert_eq!(code, Some(1));
    assert!(stderr.contains("not-an-xer"), "{stderr}");
    assert!(!directory.path().join("notes.tfdb").exists());
    let (code, _, stderr) = run(&["history", "log", file.to_str().unwrap()]);
    assert_eq!(code, Some(1));
    assert!(stderr.contains("not-a-store"), "{stderr}");
}

#[test]
fn a_time_that_isn_t_utc_starts_nothing() {
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("plant.xer");
    std::fs::copy(FIXTURE, &file).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_totalfloat"))
        .args(["history", "init", file.to_str().unwrap()])
        .env("TOTALFLOAT_NOW", "yesterday")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("invalid-date"));
    assert!(!directory.path().join("plant.tfdb").exists());
}

#[test]
fn log_and_show_read_the_history_back() {
    let directory = tempfile::tempdir().unwrap();
    let store = started(directory.path());
    let store = store.to_str().unwrap();
    let clean = |text: String| without_dir(&text, directory.path());
    let (code, stdout, _) = run(&["history", "log", store]);
    assert_eq!(code, Some(0));
    insta::assert_snapshot!("history_log_human", clean(stdout));
    let (_, stdout, _) = run(&["history", "log", store, "--json"]);
    insta::assert_snapshot!("history_log_json", clean(stdout.clone()));
    let log: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let short = log["changesets"][0]["short_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let (code, stdout, _) = run(&["history", "show", store, &short]);
    assert_eq!(code, Some(0));
    insta::assert_snapshot!("history_show_human", clean(stdout));
    let (_, stdout, _) = run(&["history", "show", store, "main", "--json"]);
    insta::assert_snapshot!("history_show_json", clean(stdout));
    let (code, _, stderr) = run(&["history", "show", store, "nowhere"]);
    assert_eq!(code, Some(1));
    assert!(stderr.contains("no-such-ref"), "{stderr}");
}

/// Every key the commands print is in the published schemas, and the reverse.
#[test]
fn history_json_matches_the_published_schemas() {
    let directory = tempfile::tempdir().unwrap();
    let store = started(directory.path());
    let store = store.to_str().unwrap();
    let (_, stdout, _) = run(&["history", "log", store, "--json"]);
    let doc: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let published = schema("history-log-1.schema.json");
    assert_eq!(doc["schema"], published["$id"]);
    assert_eq!(keys(&doc), keys(&published["properties"]));
    assert_eq!(
        keys(&doc["changesets"][0]),
        keys(&published["$defs"]["entry"]["properties"])
    );
    let (_, stdout, _) = run(&["history", "show", store, "main", "--json"]);
    let doc: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let published = schema("changeset-1.schema.json");
    assert_eq!(doc["schema"], published["$id"]);
    assert_eq!(keys(&doc), keys(&published["properties"]));
    let back = directory.path().join("back.xer");
    let (_, stdout, _) = run(&[
        "history",
        "export",
        store,
        "-o",
        back.to_str().unwrap(),
        "--json",
    ]);
    let doc: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let published = schema("history-export-1.schema.json");
    assert_eq!(doc["schema"], published["$id"]);
    assert_eq!(keys(&doc), keys(&published["properties"]));
}

fn log_ids(store: &str) -> Vec<(String, Option<String>)> {
    let (_, stdout, _) = run(&["history", "log", store, "--json"]);
    let doc: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    doc["changesets"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            (
                c["id"].as_str().unwrap().to_owned(),
                c["parent"].as_str().map(str::to_owned),
            )
        })
        .collect()
}

#[test]
fn edits_on_a_store_are_recorded_and_match_editing_files() {
    let directory = tempfile::tempdir().unwrap();
    let store = started(directory.path());
    let store = store.to_str().unwrap();
    let (code, stdout, stderr) = run(&["edit", store, "--duration", "A1=3d", "-m", "Steel late"]);
    assert_eq!(code, Some(0), "{stderr}");
    assert!(stdout.contains("Recorded "), "{stdout}");
    let (code, stdout, _) = run(&["edit", store, "--duration", "A2=2d", "--json"]);
    assert_eq!(code, Some(0));
    let doc: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(doc["recorded"]["branch"], "main");
    let ids = log_ids(store);
    assert_eq!(ids.len(), 3);
    assert_eq!(ids[0].1.as_deref(), Some(ids[1].0.as_str()));
    assert_eq!(ids[1].1.as_deref(), Some(ids[2].0.as_str()));
    assert_eq!(ids[2].1, None);
    // The same edits, file to file.
    let (one, two) = (
        directory.path().join("one.xer"),
        directory.path().join("two.xer"),
    );
    let (one, two) = (one.to_str().unwrap(), two.to_str().unwrap());
    run(&["edit", FIXTURE, "--duration", "A1=3d", "-o", one]);
    run(&["edit", one, "--duration", "A2=2d", "-o", two]);
    let head = directory.path().join("head.xer");
    run(&["history", "export", store, "-o", head.to_str().unwrap()]);
    assert_eq!(std::fs::read(&head).unwrap(), std::fs::read(two).unwrap());
    // The stored commands replay as an --edits document.
    let (_, stdout, _) = run(&["history", "show", store, "main", "--json"]);
    let shown: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(shown["commands"]["schema"], "totalfloat.command/1");
    assert_eq!(shown["commands"]["edits"][0]["activity"], "A2");
    assert_eq!(shown["report"]["schema"], "totalfloat.edit/1");
}

#[test]
fn refused_looked_at_and_unchanged_edits_record_nothing() {
    let directory = tempfile::tempdir().unwrap();
    let store = started(directory.path());
    let store = store.to_str().unwrap();
    let (code, ..) = run(&["edit", store, "--duration", "A9=3d"]);
    assert_eq!(code, Some(1));
    let (code, stdout, _) = run(&["edit", store, "--duration", "A1=3d", "--dry-run"]);
    assert_eq!(code, Some(0));
    assert!(stdout.contains("Nothing recorded (dry run)"), "{stdout}");
    // The same edit twice: the second changes nothing, so isn't recorded.
    run(&["edit", store, "--duration", "A1=3d"]);
    let (code, stdout, _) = run(&["edit", store, "--duration", "A1=3d"]);
    assert_eq!(code, Some(0));
    assert!(stdout.contains("nothing was recorded"), "{stdout}");
    let out = directory.path().join("out.xer");
    let (code, _, stderr) = run(&[
        "edit",
        store,
        "--duration",
        "A1=3d",
        "-o",
        out.to_str().unwrap(),
    ]);
    assert_eq!(code, Some(1));
    assert!(stderr.contains("store-output"), "{stderr}");
    assert_eq!(log_ids(store).len(), 2);
}

#[test]
fn skipped_edits_are_recorded_with_the_changeset() {
    let directory = tempfile::tempdir().unwrap();
    let store = started(directory.path());
    let store = store.to_str().unwrap();
    let (code, ..) = run(&[
        "edit",
        store,
        "--skip-failed",
        "--duration",
        "A9=3d",
        "--duration",
        "A1=3d",
    ]);
    assert_eq!(code, Some(3));
    let (_, stdout, _) = run(&["history", "show", store, "main", "--json"]);
    let shown: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(shown["skipped"][0]["source"], "--duration A9=3d");
    assert_eq!(shown["commands"]["edits"].as_array().unwrap().len(), 1);
}

#[test]
fn flags_that_don_t_fit_the_input_are_refused() {
    let directory = tempfile::tempdir().unwrap();
    let store = started(directory.path());
    let store = store.to_str().unwrap();
    // -o on a store is refused as such, even when the path exists.
    let (code, _, stderr) = run(&["edit", store, "--duration", "A1=3d", "-o", FIXTURE]);
    assert_eq!(code, Some(1));
    assert!(stderr.contains("store-output"), "{stderr}");
    // A reason on a plain file would be lost, so it's refused.
    let (code, _, stderr) = run(&["edit", FIXTURE, "--duration", "A1=3d", "-m", "why"]);
    assert_eq!(code, Some(1));
    assert!(stderr.contains("not-a-store"), "{stderr}");
    // A dry run leaves no files, commands included.
    let saved = directory.path().join("commands.json");
    let (code, ..) = run(&[
        "edit",
        store,
        "--duration",
        "A1=3d",
        "--dry-run",
        "--emit-commands",
        saved.to_str().unwrap(),
    ]);
    assert_eq!(code, Some(0));
    assert!(!saved.exists());
}

fn duration_of_a1(location: &str) -> String {
    let (code, stdout, stderr) = run(&[
        "query",
        location,
        "--text",
        "A1",
        "--columns",
        "original_duration_hours",
        "--json",
    ]);
    assert_eq!(code, Some(0), "{location}: {stderr}");
    let doc: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    doc["rows"][0]["original_duration_hours"].to_string()
}

#[test]
fn read_commands_take_a_store_and_any_point_in_it() {
    let directory = tempfile::tempdir().unwrap();
    let store = started(directory.path());
    let store = store.to_str().unwrap();
    run(&["edit", store, "--duration", "A1=3d"]);
    let ids = log_ids(store);
    let first = &ids[1].0[..8];
    assert_eq!(duration_of_a1(store), "24.0");
    assert_eq!(duration_of_a1(&format!("{store}@main")), "24.0");
    assert_eq!(duration_of_a1(&format!("{store}@{first}")), "8.0");
    let (code, stdout, stderr) = run(&[
        "diff",
        &format!("{store}@{first}"),
        &format!("{store}@main"),
        "--json",
    ]);
    assert_eq!(code, Some(0), "{stderr}");
    let doc: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert!(
        doc["projects"][0]["counts"]["changed"].as_u64().unwrap() >= 1,
        "{doc}"
    );
    let (code, stdout, stderr) = run(&[
        "query",
        store,
        "--baseline",
        &format!("{store}@{first}"),
        "--columns",
        "code,original_duration_hours",
    ]);
    assert_eq!(code, Some(0), "{stderr}");
    assert!(stdout.contains("A1"), "{stdout}");
    // A point the store doesn't have names the ones it does.
    let (code, _, stderr) = run(&["query", &format!("{store}@nowhere")]);
    assert_eq!(code, Some(1));
    assert!(
        stderr.contains("no-such-ref") && stderr.contains("branches: main"),
        "{stderr}"
    );
    // A file whose name holds @ still opens as a file.
    let odd = directory.path().join("plant@v2.xer");
    std::fs::copy(FIXTURE, &odd).unwrap();
    assert_eq!(duration_of_a1(odd.to_str().unwrap()), "8.0");
}

#[test]
fn an_edit_records_on_the_branch_named_and_only_on_a_branch() {
    let directory = tempfile::tempdir().unwrap();
    let store = started(directory.path());
    let store = store.to_str().unwrap();
    let (code, stdout, stderr) = run(&["edit", &format!("{store}@main"), "--duration", "A1=3d"]);
    assert_eq!(code, Some(0), "{stderr}");
    assert!(stdout.contains("on main"), "{stdout}");
    let first = log_ids(store)[1].0[..8].to_owned();
    let (code, _, stderr) = run(&["edit", &format!("{store}@{first}"), "--duration", "A1=2d"]);
    assert_eq!(code, Some(1));
    assert!(stderr.contains("not-a-branch"), "{stderr}");
    // An unknown short name isn't mistaken for a short ID.
    let (code, _, stderr) = run(&["query", &format!("{store}@cafe")]);
    assert_eq!(code, Some(1));
    assert!(stderr.contains("no-such-ref"), "{stderr}");
}

#[test]
fn a_database_that_isn_t_a_store_isn_t_read_as_one() {
    let directory = tempfile::tempdir().unwrap();
    let other = directory.path().join("other.db");
    rusqlite::Connection::open(&other)
        .unwrap()
        .execute_batch("CREATE TABLE things (x INTEGER);")
        .unwrap();
    let (code, _, stderr) = run(&["inspect", other.to_str().unwrap()]);
    assert_eq!(code, Some(1));
    assert!(stderr.contains("not-an-xer"), "{stderr}");
}

fn export_head(store: &str, directory: &Path, name: &str) -> Vec<u8> {
    let out = directory.join(name);
    let (code, _, stderr) = run(&["history", "export", store, "-o", out.to_str().unwrap()]);
    assert_eq!(code, Some(0), "{stderr}");
    std::fs::read(out).unwrap()
}

#[test]
fn undo_takes_the_last_edit_out_and_undo_again_puts_it_back() {
    let directory = tempfile::tempdir().unwrap();
    let store = started(directory.path());
    let store = store.to_str().unwrap();
    run(&["edit", store, "--duration", "A1=3d"]);
    let edited = export_head(store, directory.path(), "edited.xer");
    let (code, stdout, stderr) = run(&["history", "undo", store, "-m", "wrong activity"]);
    assert_eq!(code, Some(0), "{stderr}");
    assert!(stdout.contains("Recorded "), "{stdout}");
    assert_eq!(
        export_head(store, directory.path(), "undone.xer"),
        std::fs::read(FIXTURE).unwrap()
    );
    let (code, stdout, _) = run(&["history", "undo", store, "--json"]);
    assert_eq!(code, Some(0));
    let doc: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let published = schema("history-revert-1.schema.json");
    assert_eq!(doc["schema"], published["$id"]);
    assert_eq!(keys(&doc), keys(&published["properties"]));
    assert_eq!(export_head(store, directory.path(), "redone.xer"), edited);
    // Nothing is deleted: import, edit, undo, undo.
    assert_eq!(log_ids(store).len(), 4);
}

#[test]
fn reverting_a_middle_edit_replays_the_later_ones() {
    let directory = tempfile::tempdir().unwrap();
    let store = started(directory.path());
    let store = store.to_str().unwrap();
    run(&["edit", store, "--duration", "A1=3d"]);
    run(&["edit", store, "--name", "A2=Second"]);
    run(&["edit", store, "--name", "A1=First"]);
    let middle = log_ids(store)[1].0[..8].to_owned();
    let (code, stdout, stderr) = run(&["history", "revert", store, &middle]);
    assert_eq!(code, Some(0), "{stderr}");
    assert!(stdout.contains("replaying 1 later changeset"), "{stdout}");
    let (one, two) = (
        directory.path().join("one.xer"),
        directory.path().join("two.xer"),
    );
    run(&[
        "edit",
        FIXTURE,
        "--duration",
        "A1=3d",
        "-o",
        one.to_str().unwrap(),
    ]);
    run(&[
        "edit",
        one.to_str().unwrap(),
        "--name",
        "A1=First",
        "-o",
        two.to_str().unwrap(),
    ]);
    assert_eq!(
        export_head(store, directory.path(), "head.xer"),
        std::fs::read(&two).unwrap()
    );
}

#[test]
fn a_revert_that_a_later_edit_needs_is_refused() {
    let directory = tempfile::tempdir().unwrap();
    let store = started(directory.path());
    let store = store.to_str().unwrap();
    let (code, _, stderr) = run(&["edit", store, "--add", "B1;Strip forms;rel-fs-positive;2d"]);
    assert_eq!(code, Some(0), "{stderr}");
    run(&["edit", store, "--link", "A1>B1"]);
    let added = log_ids(store)[1].0[..8].to_owned();
    let (code, _, stderr) = run(&["history", "revert", store, &added]);
    assert_eq!(code, Some(1));
    assert!(
        stderr.contains("revert-conflict") && stderr.contains("B1"),
        "{stderr}"
    );
    assert_eq!(log_ids(store).len(), 3);
    // The start can't be taken out.
    let start = log_ids(store)[2].0[..8].to_owned();
    let (code, _, stderr) = run(&["history", "revert", store, &start]);
    assert_eq!(code, Some(1));
    assert!(stderr.contains("nothing-before"), "{stderr}");
}

#[test]
fn an_earlier_edit_can_be_reverted_past_an_undo() {
    let directory = tempfile::tempdir().unwrap();
    let store = started(directory.path());
    let store = store.to_str().unwrap();
    run(&["edit", store, "--duration", "A1=3d"]);
    run(&["edit", store, "--name", "A2=Second"]);
    run(&["history", "undo", store]);
    let first = log_ids(store)[2].0[..8].to_owned();
    let (code, stdout, stderr) = run(&["history", "revert", store, &first]);
    assert_eq!(code, Some(0), "{stderr}");
    // The name and its undo cancel, so nothing is left to replay.
    assert!(stdout.contains("replaying 0 later changesets"), "{stdout}");
    assert_eq!(
        export_head(store, directory.path(), "head.xer"),
        std::fs::read(FIXTURE).unwrap()
    );
    // Taking out the name again says it's already out.
    let named = log_ids(store)[2].0[..8].to_owned();
    let (code, _, stderr) = run(&["history", "revert", store, &named]);
    assert_eq!(code, Some(1));
    assert!(stderr.contains("already-reverted"), "{stderr}");
}

#[test]
fn a_what_if_branch_leaves_main_alone() {
    let directory = tempfile::tempdir().unwrap();
    let store = started(directory.path());
    let store = store.to_str().unwrap();
    let main_before = log_ids(store)[0].0.clone();
    let (code, stdout, stderr) = run(&["history", "branch", store, "what-if/steel-late"]);
    assert_eq!(code, Some(0), "{stderr}");
    assert!(
        stdout.contains("Made branch what-if/steel-late"),
        "{stdout}"
    );
    let (code, _, stderr) = run(&["history", "switch", store, "what-if/steel-late"]);
    assert_eq!(code, Some(0), "{stderr}");
    let (_, stdout, _) = run(&["edit", store, "--duration", "A1=3d", "-m", "Steel late"]);
    assert!(stdout.contains("on what-if/steel-late"), "{stdout}");
    run(&["history", "switch", store, "main"]);
    assert_eq!(log_ids(store)[0].0, main_before);
    assert_eq!(duration_of_a1(store), "8.0");
    assert_eq!(
        duration_of_a1(&format!("{store}@what-if/steel-late")),
        "24.0"
    );
    let (_, stdout, _) = run(&["history", "branches", store]);
    insta::assert_snapshot!(
        "history_branches_human",
        without_dir(&stdout, directory.path())
    );
    let (_, stdout, _) = run(&["history", "branches", store, "--json"]);
    let doc: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let published = schema("history-branches-1.schema.json");
    assert_eq!(keys(&doc), keys(&published["properties"]));
    assert_eq!(
        keys(&doc["branches"][0]),
        keys(&published["$defs"]["branch"]["properties"])
    );
    // A changeset on the what-if isn't on main, so main can't revert it.
    let what_if = &doc["branches"][1]["short_id"];
    let (code, _, stderr) = run(&["history", "revert", store, what_if.as_str().unwrap()]);
    assert_eq!(code, Some(1));
    assert!(stderr.contains("not-on-branch"), "{stderr}");
}

#[test]
fn branch_names_are_checked_and_the_current_branch_stays() {
    let directory = tempfile::tempdir().unwrap();
    let store = started(directory.path());
    let store = store.to_str().unwrap();
    for bad in ["has space", "deadbeef", "x@y"] {
        let (code, _, stderr) = run(&["history", "branch", store, bad]);
        assert_eq!(code, Some(1), "{bad}");
        assert!(stderr.contains("invalid-name"), "{bad}: {stderr}");
    }
    let (code, stdout, _) = run(&["history", "branch", store, "plan-b", "--json"]);
    assert_eq!(code, Some(0));
    let doc: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(
        keys(&doc),
        keys(&schema("history-branch-1.schema.json")["properties"])
    );
    let (code, _, stderr) = run(&["history", "branch", store, "plan-b"]);
    assert_eq!(code, Some(1));
    assert!(stderr.contains("name-taken"), "{stderr}");
    let (code, _, stderr) = run(&["history", "branch", store, "main", "--delete"]);
    assert_eq!(code, Some(1));
    assert!(stderr.contains("current-branch"), "{stderr}");
    let (code, stdout, _) = run(&["history", "switch", store, "plan-b", "--json"]);
    assert_eq!(code, Some(0));
    let doc: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(
        keys(&doc),
        keys(&schema("history-switch-1.schema.json")["properties"])
    );
    let (code, ..) = run(&["history", "branch", store, "main", "--delete"]);
    assert_eq!(code, Some(0));
    let (code, _, stderr) = run(&["history", "switch", store, "main"]);
    assert_eq!(code, Some(1));
    assert!(stderr.contains("no-such-ref"), "{stderr}");
}

#[test]
fn history_diff_shows_the_change_and_who_made_it() {
    let directory = tempfile::tempdir().unwrap();
    let store = started(directory.path());
    let store = store.to_str().unwrap();
    run(&["edit", store, "--duration", "A1=3d", "-m", "Steel late"]);
    run(&["edit", store, "--name", "A2=Second"]);
    let (code, stdout, stderr) = run(&["history", "diff", store, "--json"]);
    assert_eq!(code, Some(0), "{stderr}");
    let doc: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(
        keys(&doc),
        keys(&schema("history-diff-1.schema.json")["properties"])
    );
    assert_eq!(doc["diff"]["schema"], "totalfloat.diff/1");
    assert_eq!(doc["changesets"].as_array().unwrap().len(), 1);
    assert_eq!(doc["ancestor"], true);
    let start = log_ids(store)[2].0[..8].to_owned();
    let (code, stdout, _) = run(&["history", "diff", store, &start, "main"]);
    assert_eq!(code, Some(0));
    assert!(
        stdout.contains("2 changesets") && stdout.contains("Steel late"),
        "{stdout}"
    );
    // Two points on different branches compare as files.
    run(&["history", "branch", store, "side", "--at", &start]);
    let (code, stdout, _) = run(&["history", "diff", store, "main", "side"]);
    assert_eq!(code, Some(0));
    assert!(stdout.contains("compares the two files only"), "{stdout}");
}

/// `history diff` lists 25 of each kind by default, `--limit N` at most N,
/// and `--limit 0` every one; the counts are always complete.
#[test]
fn history_diff_lists_as_many_as_asked() {
    let directory = tempfile::tempdir().unwrap();
    let store = started(directory.path());
    let store = store.to_str().unwrap();
    let (code, _, stderr) = run(&["edit", store, "--name", "A1=One", "--name", "A2=Two"]);
    assert_eq!(code, Some(0), "{stderr}");
    let listed = |extra: &[&str]| {
        let mut args = vec!["history", "diff", store, "--json"];
        args.extend(extra);
        let (code, stdout, stderr) = run(&args);
        assert_eq!(code, Some(0), "{extra:?}: {stderr}");
        let doc: serde_json::Value = serde_json::from_str(&stdout).unwrap();
        let project = &doc["diff"]["projects"][0];
        (
            project["changed"].as_array().unwrap().len(),
            project["counts"]["changed"].as_u64().unwrap(),
        )
    };
    assert_eq!(listed(&[]), (2, 2));
    assert_eq!(listed(&["--limit", "1"]), (1, 2));
    assert_eq!(listed(&["--limit", "0"]), (2, 2));
    let (code, _, stderr) = run(&["history", "diff", store, "--limit", "some"]);
    assert_ne!(code, Some(0));
    assert!(stderr.contains("limit"), "{stderr}");
}

#[test]
fn next_month_s_export_comes_in_as_the_next_point() {
    let directory = tempfile::tempdir().unwrap();
    let store = started(directory.path());
    let store = store.to_str().unwrap();
    let update = directory.path().join("update-07.xer");
    run(&[
        "edit",
        FIXTURE,
        "--duration",
        "A1=3d",
        "-o",
        update.to_str().unwrap(),
    ]);
    let (code, stdout, stderr) = run(&[
        "history",
        "import",
        store,
        update.to_str().unwrap(),
        "-m",
        "Update 07",
        "--json",
    ]);
    assert_eq!(code, Some(0), "{stderr}");
    let doc: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(
        keys(&doc),
        keys(&schema("history-import-1.schema.json")["properties"])
    );
    assert_eq!(doc["diff"]["projects"][0]["counts"]["changed"], 1, "{doc}");
    assert_eq!(log_ids(store).len(), 2);
    assert_eq!(duration_of_a1(store), "24.0");
    // The same file again records nothing.
    let (code, stdout, _) = run(&["history", "import", store, update.to_str().unwrap()]);
    assert_eq!(code, Some(0));
    assert!(stdout.contains("nothing was recorded"), "{stdout}");
    // Another project's file is refused unless forced.
    let other = "../../fixtures/xer/batches/001/loe.xer";
    let (code, _, stderr) = run(&["history", "import", store, other]);
    assert_eq!(code, Some(1));
    assert!(stderr.contains("different-projects"), "{stderr}");
    assert_eq!(log_ids(store).len(), 2);
    let (code, ..) = run(&["history", "import", store, other, "--force"]);
    assert_eq!(code, Some(0));
    assert_eq!(log_ids(store).len(), 3);
}

#[test]
fn a_baseline_is_a_point_that_stays_put() {
    let directory = tempfile::tempdir().unwrap();
    let store = started(directory.path());
    let store = store.to_str().unwrap();
    let (code, stdout, stderr) = run(&["history", "baseline", store, "contract", "--json"]);
    assert_eq!(code, Some(0), "{stderr}");
    let doc: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(
        keys(&doc),
        keys(&schema("history-baseline-1.schema.json")["properties"])
    );
    run(&["edit", store, "--duration", "A1=3d"]);
    run(&["edit", store, "--duration", "A2=3d"]);
    let (code, stdout, stderr) = run(&[
        "variance",
        store,
        "--baseline",
        &format!("{store}@contract"),
        "--json",
    ]);
    assert_eq!(code, Some(0), "{stderr}");
    let doc: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(
        doc["projects"][0]["counts"]["durations_changed"], 2,
        "{doc}"
    );
    let (_, stdout, _) = run(&["history", "baselines", store, "--json"]);
    let doc: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let published = schema("history-baselines-1.schema.json");
    assert_eq!(keys(&doc), keys(&published["properties"]));
    assert_eq!(
        keys(&doc["baselines"][0]),
        keys(&published["$defs"]["baseline"]["properties"])
    );
    // A baseline can't be checked out or edited, or moved by naming it again.
    let (code, _, stderr) = run(&["history", "switch", store, "contract"]);
    assert_eq!(code, Some(1));
    assert!(stderr.contains("wrong-ref-kind"), "{stderr}");
    let (code, _, stderr) = run(&["edit", &format!("{store}@contract"), "--duration", "A1=1d"]);
    assert_eq!(code, Some(1));
    assert!(stderr.contains("not-a-branch"), "{stderr}");
    let (code, _, stderr) = run(&["history", "baseline", store, "contract"]);
    assert_eq!(code, Some(1));
    assert!(stderr.contains("name-taken"), "{stderr}");
    // A branch from it is how to change it.
    let (code, ..) = run(&["history", "branch", store, "rebaseline", "--at", "contract"]);
    assert_eq!(code, Some(0));
    let (code, ..) = run(&["history", "baseline", store, "contract", "--delete"]);
    assert_eq!(code, Some(0));
    let (code, ..) = run(&["history", "baseline", store, "main", "--delete"]);
    assert_eq!(code, Some(1));
}

#[test]
fn a_replayed_edit_finds_an_activity_p6_renamed() {
    let directory = tempfile::tempdir().unwrap();
    let store = started(directory.path());
    let store = store.to_str().unwrap();
    // A1 is renamed A1-R in a new export, and the export comes in.
    let text = std::fs::read(FIXTURE).unwrap();
    let text = String::from_utf8(text)
        .unwrap()
        .replace("\tA1\tSynthetic activity 1", "\tA1-R\tSynthetic activity 1");
    let renamed = directory.path().join("renamed.xer");
    std::fs::write(&renamed, text).unwrap();
    let (code, _, stderr) = run(&["history", "import", store, renamed.to_str().unwrap()]);
    assert_eq!(code, Some(0), "{stderr}");
    let (code, _, stderr) = run(&["edit", store, "--duration", "A1-R=3d"]);
    assert_eq!(code, Some(0), "{stderr}");
    let (_, stdout, _) = run(&["history", "show", store, "main", "--json"]);
    let shown: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(shown["commands"]["resolved"][0]["name"], "A1-R");
    // Taking the import out replays the edit where the activity is still A1.
    let import = log_ids(store)[1].0[..8].to_owned();
    let (code, _, stderr) = run(&["history", "revert", store, &import]);
    assert_eq!(code, Some(0), "{stderr}");
    assert_eq!(duration_of_a1(store), "24.0");
}

#[test]
fn merging_a_branch_ahead_moves_forward_without_a_changeset() {
    let directory = tempfile::tempdir().unwrap();
    let store = started(directory.path());
    let store = store.to_str().unwrap();
    run(&["history", "branch", store, "ahead"]);
    run(&["edit", &format!("{store}@ahead"), "--duration", "A1=3d"]);
    let before = log_ids(store).len();
    let (code, stdout, stderr) = run(&["history", "merge", store, "ahead"]);
    assert_eq!(code, Some(0), "{stderr}");
    assert!(stdout.contains("forward"), "{stdout}");
    assert_eq!(
        log_ids(store).len(),
        before + 1,
        "main now has the branch's edit"
    );
    assert_eq!(duration_of_a1(store), "24.0");
    let (_, stdout, _) = run(&["history", "merge", store, "ahead"]);
    assert!(stdout.contains("already has everything"), "{stdout}");
}

/// The log gives a merge's second parent, the head of the branch merged in,
/// so a page can draw where it came from; every other changeset has none.
#[test]
fn the_log_gives_a_merge_its_second_parent() {
    let directory = tempfile::tempdir().unwrap();
    let store = started(directory.path());
    let store = store.to_str().unwrap();
    run(&["history", "branch", store, "side"]);
    run(&["edit", store, "--duration", "A1=3d"]);
    run(&["edit", &format!("{store}@side"), "--name", "A2=Second"]);
    let (_, stdout, _) = run(&["history", "log", store, "--branch", "side", "--json"]);
    let side: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let side_head = side["head"].as_str().unwrap().to_owned();
    let (code, _, stderr) = run(&["history", "merge", store, "side", "-m", "Take the rename"]);
    assert_eq!(code, Some(0), "{stderr}");
    for args in [
        vec!["history", "log", store, "--json"],
        vec!["history", "log", store, "--all", "--json"],
    ] {
        let (_, stdout, _) = run(&args);
        let log: serde_json::Value = serde_json::from_str(&stdout).unwrap();
        let changesets = log["changesets"].as_array().unwrap();
        let merges: Vec<_> = changesets.iter().filter(|c| c["kind"] == "merge").collect();
        assert_eq!(merges.len(), 1, "{args:?}");
        assert_eq!(merges[0]["second_parent"], side_head.as_str(), "{args:?}");
        for other in changesets.iter().filter(|c| c["kind"] != "merge") {
            assert_eq!(other["second_parent"], serde_json::Value::Null, "{args:?}");
        }
        let published = schema("history-log-1.schema.json");
        for changeset in changesets {
            assert_eq!(
                keys(changeset),
                keys(&published["$defs"]["entry"]["properties"])
            );
        }
    }
}

#[test]
fn edits_to_different_things_merge_as_if_done_in_turn() {
    let directory = tempfile::tempdir().unwrap();
    let store = started(directory.path());
    let store = store.to_str().unwrap();
    run(&["history", "branch", store, "side"]);
    run(&["edit", store, "--duration", "A1=3d"]);
    run(&["edit", &format!("{store}@side"), "--name", "A2=Second"]);
    let (code, stdout, stderr) = run(&["history", "merge", store, "side", "--dry-run", "--json"]);
    assert_eq!(code, Some(0), "{stderr}");
    let doc: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(
        keys(&doc),
        keys(&schema("history-merge-1.schema.json")["properties"])
    );
    assert_eq!(doc["recorded"], serde_json::Value::Null);
    let (code, stdout, stderr) = run(&["history", "merge", store, "side", "-m", "Take the rename"]);
    assert_eq!(code, Some(0), "{stderr}");
    assert!(stdout.contains("Merged side"), "{stdout}");
    let (_, stdout, _) = run(&["history", "show", store, "main", "--json"]);
    let shown: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(shown["kind"], "merge");
    assert!(shown["second_parent"].is_string());
    let (one, two) = (
        directory.path().join("one.xer"),
        directory.path().join("two.xer"),
    );
    run(&[
        "edit",
        FIXTURE,
        "--duration",
        "A1=3d",
        "-o",
        one.to_str().unwrap(),
    ]);
    run(&[
        "edit",
        one.to_str().unwrap(),
        "--name",
        "A2=Second",
        "-o",
        two.to_str().unwrap(),
    ]);
    assert_eq!(
        export_head(store, directory.path(), "head.xer"),
        std::fs::read(&two).unwrap()
    );
}

#[test]
fn both_branches_changing_one_duration_is_a_conflict() {
    let directory = tempfile::tempdir().unwrap();
    let store = started(directory.path());
    let store = store.to_str().unwrap();
    run(&["history", "branch", store, "side"]);
    run(&["edit", store, "--duration", "A1=3d"]);
    run(&["edit", &format!("{store}@side"), "--duration", "A1=5d"]);
    let before = log_ids(store).len();
    let (code, _, stderr) = run(&["history", "merge", store, "side"]);
    assert_eq!(code, Some(1));
    assert!(
        stderr.contains("merge-conflict") && stderr.contains("A1"),
        "{stderr}"
    );
    assert_eq!(log_ids(store).len(), before);
}

#[test]
fn a_branch_holding_an_import_can_t_be_merged() {
    let directory = tempfile::tempdir().unwrap();
    let store = started(directory.path());
    let store = store.to_str().unwrap();
    run(&["history", "branch", store, "side"]);
    run(&["edit", store, "--duration", "A1=3d"]);
    run(&["history", "switch", store, "side"]);
    let update = directory.path().join("update.xer");
    run(&[
        "edit",
        FIXTURE,
        "--name",
        "A2=From P6",
        "-o",
        update.to_str().unwrap(),
    ]);
    run(&["history", "import", store, update.to_str().unwrap()]);
    run(&["history", "switch", store, "main"]);
    let (code, _, stderr) = run(&["history", "merge", store, "side"]);
    assert_eq!(code, Some(1));
    assert!(stderr.contains("is an import"), "{stderr}");
}

#[test]
fn merge_conflicts_cover_deletes_and_repeats() {
    let directory = tempfile::tempdir().unwrap();
    let store = started(directory.path());
    let store = store.to_str().unwrap();
    let side = format!("{store}@side");
    run(&["history", "branch", store, "side"]);
    // One side deletes A2 while the other changes it.
    run(&["edit", store, "--delete", "A2"]);
    run(&["edit", &side, "--duration", "A2=3d"]);
    let (code, _, stderr) = run(&["history", "merge", store, "side"]);
    assert_eq!(code, Some(1));
    assert!(stderr.contains("one branch deleted it"), "{stderr}");
    // Both sides deleting it can't be replayed twice.
    let start = log_ids(store).last().unwrap().0[..8].to_owned();
    run(&["history", "branch", store, "both", "--at", &start]);
    run(&["edit", &format!("{store}@both"), "--delete", "A2"]);
    let (code, _, stderr) = run(&["history", "merge", store, "both"]);
    assert_eq!(code, Some(1));
    assert!(stderr.contains("can't be made twice"), "{stderr}");
    // A baseline isn't a branch to merge.
    run(&["history", "baseline", store, "contract"]);
    let (code, _, stderr) = run(&["history", "merge", store, "contract"]);
    assert_eq!(code, Some(1));
    assert!(stderr.contains("not-a-branch"), "{stderr}");
}

#[test]
fn a_merge_that_changes_nothing_is_still_recorded() {
    let directory = tempfile::tempdir().unwrap();
    let store = started(directory.path());
    let store = store.to_str().unwrap();
    run(&["history", "branch", store, "side"]);
    run(&["edit", store, "--duration", "A1=3d"]);
    run(&["edit", &format!("{store}@side"), "--duration", "A1=3d"]);
    let (code, stdout, stderr) = run(&["history", "merge", store, "side"]);
    assert_eq!(code, Some(0), "{stderr}");
    assert!(stdout.contains("recorded"), "{stdout}");
    let (_, stdout, _) = run(&["history", "merge", store, "side"]);
    assert!(stdout.contains("already has everything"), "{stdout}");
}

fn agent(args: &[&str]) -> (Option<i32>, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_totalfloat"))
        .args(args)
        .env("TOTALFLOAT_NOW", "2026-11-02T10:00:00Z")
        .env("TOTALFLOAT_AUTHOR", "planner-bot")
        .env("TOTALFLOAT_AUTHOR_KIND", "agent")
        .output()
        .unwrap();
    (
        out.status.code(),
        String::from_utf8(out.stdout).unwrap(),
        String::from_utf8(out.stderr).unwrap(),
    )
}

#[test]
fn an_agent_s_proposal_waits_and_a_person_accepts_it() {
    let directory = tempfile::tempdir().unwrap();
    let store = started(directory.path());
    let store = store.to_str().unwrap();
    let head = log_ids(store)[0].0.clone();
    let (code, stdout, stderr) = agent(&[
        "edit",
        store,
        "--duration",
        "A1=3d",
        "--propose",
        "-m",
        "Crash A1",
    ]);
    assert_eq!(code, Some(0), "{stderr}");
    assert!(stdout.contains("Proposed "), "{stdout}");
    assert_eq!(log_ids(store)[0].0, head, "main didn't move");
    let (_, stdout, _) = run(&["history", "proposals", store, "--json"]);
    let doc: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let published = schema("history-proposals-1.schema.json");
    assert_eq!(
        keys(&doc["proposals"][0]),
        keys(&published["$defs"]["proposal"]["properties"])
    );
    assert_eq!(doc["proposals"][0]["author_kind"], "agent");
    assert_eq!(doc["proposals"][0]["for_branch"], "main");
    let name = doc["proposals"][0]["name"].as_str().unwrap().to_owned();
    let (code, stdout, stderr) =
        run(&["history", "accept", store, &name, "-m", "Agreed", "--json"]);
    assert_eq!(code, Some(0), "{stderr}");
    let decided: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(
        keys(&decided),
        keys(&schema("history-review-1.schema.json")["properties"])
    );
    assert_eq!(decided["replayed"], false);
    assert_eq!(duration_of_a1(store), "24.0");
    let (_, stdout, _) = run(&["history", "proposals", store]);
    assert!(stdout.contains("No proposals"), "{stdout}");
    let (_, stdout, _) = run(&["history", "show", store, "main", "--json"]);
    let shown: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(shown["author_kind"], "agent");
    assert_eq!(shown["reviews"][0]["author"], "Pat");
    assert_eq!(shown["reviews"][0]["message"], "Agreed");
}

#[test]
fn a_proposal_on_a_moved_branch_is_replayed_and_rejected_ones_stay() {
    let directory = tempfile::tempdir().unwrap();
    let store = started(directory.path());
    let store = store.to_str().unwrap();
    let (_, stdout, _) = agent(&["edit", store, "--duration", "A1=3d", "--propose", "--json"]);
    let proposed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let first = proposed["recorded"]["short_id"]
        .as_str()
        .unwrap()
        .to_owned();
    run(&["edit", store, "--name", "A2=Second"]);
    let (code, stdout, stderr) = run(&["history", "accept", store, &first]);
    assert_eq!(code, Some(0), "{stderr}");
    assert!(stdout.contains("replayed on main"), "{stdout}");
    assert_eq!(duration_of_a1(store), "24.0");
    // Replayed, it's still the agent's work, accepted by a person.
    let (_, stdout, _) = run(&["history", "show", store, "main", "--json"]);
    let shown: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(shown["author_kind"], "agent");
    assert!(
        shown["message"]
            .as_str()
            .unwrap()
            .contains("accepted by Pat"),
        "{shown}"
    );
    assert_eq!(log_ids(store).len(), 3, "import, rename, replayed proposal");
    // A second proposal is rejected: a reason is needed, and it stays.
    let (_, stdout, _) = agent(&["edit", store, "--duration", "A2=9d", "--propose", "--json"]);
    let proposed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let second = proposed["recorded"]["short_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let (code, _, stderr) = run(&["history", "reject", store, &second]);
    assert_eq!(code, Some(1));
    assert!(stderr.contains("reason-needed"), "{stderr}");
    let (code, ..) = run(&["history", "reject", store, &second, "-m", "Too long"]);
    assert_eq!(code, Some(0));
    let (_, stdout, _) = run(&["history", "log", store, "--all"]);
    assert!(stdout.contains("[rejected]"), "{stdout}");
    let (code, _, stderr) = run(&["history", "accept", store, &second]);
    assert_eq!(code, Some(1));
    assert!(stderr.contains("not-a-proposal"), "{stderr}");
}

#[test]
fn a_clean_history_verifies_and_replays() {
    let directory = tempfile::tempdir().unwrap();
    let store = started(directory.path());
    let store = store.to_str().unwrap();
    run(&["edit", store, "--duration", "A1=3d"]);
    run(&[
        "edit",
        store,
        "--data-date",
        "2026-01-13 08:00",
        "--actual-start",
        "A1=2026-01-12 08:00",
    ]);
    run(&["edit", store, "--name", "A2=Second"]);
    run(&["history", "undo", store]);
    let (code, stdout, stderr) = run(&["history", "verify", store, "--replay"]);
    assert_eq!(code, Some(0), "{stdout}{stderr}");
    assert!(stdout.contains("All 3 edits replay"), "{stdout}");
    let (_, stdout, _) = run(&["history", "verify", store, "--json"]);
    let doc: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(
        keys(&doc),
        keys(&schema("history-verify-1.schema.json")["properties"])
    );
}

#[test]
fn verify_finds_a_damaged_file() {
    let directory = tempfile::tempdir().unwrap();
    let store = started(directory.path());
    let connection = rusqlite::Connection::open(&store).unwrap();
    connection
        .execute("UPDATE blobs SET data = x'00'", [])
        .unwrap();
    drop(connection);
    let (code, stdout, _) = run(&["history", "verify", store.to_str().unwrap(), "--replay"]);
    assert_eq!(code, Some(1), "{stdout}");
    assert!(stdout.contains("is damaged"), "{stdout}");
}

#[test]
fn verify_replay_finds_a_file_its_edits_don_t_give() {
    use totalfloat_store::{AuthorKind, Kind, Moving, NewChangeset, RefKind, State, Store};
    let directory = tempfile::tempdir().unwrap();
    let path = started(directory.path());
    run(&["edit", path.to_str().unwrap(), "--duration", "A1=3d"]);
    let mut store = Store::open(&path).unwrap();
    let head = store.resolve("main").unwrap();
    let edit = store.changeset(&head).unwrap();
    // The same commands, recorded with the file from before them.
    let before = store.changeset(edit.parent.as_ref().unwrap()).unwrap();
    let forged = NewChangeset {
        parent: Some(head.clone()),
        second_parent: None,
        kind: Kind::Edit,
        state: State::Accepted,
        author: "Pat".into(),
        author_kind: AuthorKind::Human,
        message: "forged".into(),
        created: "2026-11-02T11:00:00Z".into(),
        tool_version: "0.0.0".into(),
        commands: edit.commands.clone(),
        skipped: "[]".into(),
        report: "{}".into(),
        snapshot: store.blob(&before.snapshot).unwrap(),
    };
    let moving = Moving {
        name: "main",
        kind: RefKind::Branch,
        expected: Some(&head),
    };
    store.record(&forged, Some(moving)).unwrap();
    drop(store);
    let (code, stdout, _) = run(&["history", "verify", path.to_str().unwrap(), "--replay"]);
    assert_eq!(code, Some(3), "{stdout}");
    assert!(
        stdout.contains("1 of 2 edits replay differently"),
        "{stdout}"
    );
}
