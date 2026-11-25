//! `totalfloat.command/1` is a public contract (ARCHITECTURE.md, History and changesets): these tests fail
//! when its meaning or its published schema would change.

#![allow(clippy::unwrap_used, reason = "test helpers outside #[test] functions")]

use std::path::{Path, PathBuf};

use proptest::prelude::*;
use serde_json::Value;
use totalfloat_edit::{COMMAND_SCHEMA, Commands, Edit};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn golden_path() -> PathBuf {
    root().join("crates/edit/tests/golden/command-1.json")
}

fn golden() -> Commands {
    serde_json::from_str(&std::fs::read_to_string(golden_path()).unwrap()).unwrap()
}

/// Set `TOTALFLOAT_UPDATE_CONTRACT=1` to rewrite a file after a deliberate,
/// compatible change, and say why in the PR.
fn updating() -> bool {
    std::env::var_os("TOTALFLOAT_UPDATE_CONTRACT").is_some()
}

fn pretty(value: &impl serde::Serialize) -> String {
    serde_json::to_string_pretty(value).unwrap() + "\n"
}

/// The published schema: generated from the types, with the version as `$id`.
fn generated_schema() -> Value {
    let mut schema = serde_json::to_value(schemars::schema_for!(Commands)).unwrap();
    let object = schema.as_object_mut().unwrap();
    object.insert("$id".into(), Value::from(COMMAND_SCHEMA));
    object.insert("title".into(), Value::from("totalfloat.command/1"));
    schema
}

#[test]
fn the_golden_document_reads_and_writes_back_unchanged() {
    let text = std::fs::read_to_string(golden_path()).unwrap();
    let commands = golden();
    assert!(commands.readable());
    let written = pretty(&commands);
    if updating() {
        std::fs::write(golden_path(), &written).unwrap();
        return;
    }
    assert_eq!(
        written, text,
        "the golden command document changed meaning or form"
    );
}

#[test]
fn the_golden_document_holds_every_kind_of_edit() {
    let schema = generated_schema();
    let mut ops: Vec<String> = schema["$defs"]["Edit"]["oneOf"]
        .as_array()
        .unwrap()
        .iter()
        .map(|variant| {
            variant["properties"]["op"]["const"]
                .as_str()
                .unwrap()
                .to_owned()
        })
        .collect();
    ops.sort();
    let mut held: Vec<String> = golden()
        .edits
        .iter()
        .map(|edit| {
            serde_json::to_value(edit).unwrap()["op"]
                .as_str()
                .unwrap()
                .to_owned()
        })
        .collect();
    held.sort();
    held.dedup();
    assert_eq!(
        held, ops,
        "add an example of each new edit to the golden file"
    );
}

#[test]
fn the_published_schema_matches_the_types() {
    let path = root().join("docs/schemas/command-1.schema.json");
    let generated = pretty(&generated_schema());
    if updating() {
        std::fs::write(&path, &generated).unwrap();
        return;
    }
    let published = std::fs::read_to_string(&path).unwrap_or_default();
    assert_eq!(
        published, generated,
        "docs/schemas/command-1.schema.json is out of date; rerun with TOTALFLOAT_UPDATE_CONTRACT=1"
    );
}

#[test]
fn a_bare_list_is_still_a_list_of_edits() {
    let edits: Vec<Edit> =
        serde_json::from_str(r#"[{"op": "set_name", "activity": "A1", "name": "x"}]"#).unwrap();
    assert_eq!(edits.len(), 1);
}

const TEXT: [&str; 13] = [
    "activity",
    "name",
    "code",
    "wbs",
    "parent",
    "calendar",
    "resource",
    "code_type",
    "value",
    "description",
    "predecessor",
    "successor",
    "copy_of",
];
const SPANS: [&str; 4] = ["original", "remaining", "duration", "lag"];

/// A golden edit with its names and spans replaced.
fn varied(edit: &Edit, words: &[String], spans: &[(i32, u16, bool)]) -> Value {
    let mut value = serde_json::to_value(edit).unwrap();
    let object = value.as_object_mut().unwrap();
    for (n, key) in TEXT.iter().enumerate() {
        if let Some(cell) = object.get_mut(*key) {
            *cell = Value::from(words[n % words.len()].clone());
        }
    }
    for (n, key) in SPANS.iter().enumerate() {
        if let Some(cell) = object.get_mut(*key) {
            let (whole, part, days) = spans[n % spans.len()];
            let unit = if days { 'd' } else { 'h' };
            *cell = Value::from(format!("{whole}.{part:03}{unit}"));
        }
    }
    value
}

proptest! {
    #[test]
    fn every_edit_survives_writing_and_reading(
        words in prop::collection::vec(".{0,12}", 1..4),
        spans in prop::collection::vec((-9999i32..9999, 0u16..1000, any::<bool>()), 1..4),
    ) {
        for edit in golden().edits {
            let value = varied(&edit, &words, &spans);
            let read: Edit = serde_json::from_value(value).unwrap();
            let again: Edit = serde_json::from_value(serde_json::to_value(&read).unwrap()).unwrap();
            prop_assert_eq!(again, read);
        }
    }
}
