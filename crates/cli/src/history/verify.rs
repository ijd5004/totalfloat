//! `history verify`: is the store whole, and does replaying each recorded
//! edit today give the file that was recorded? The second can differ after
//! an engine fix; the record of what was produced at the time stands, and
//! this says where today's TotalFloat would differ.

use std::path::Path;

use serde_json::{Value, json};
use totalfloat_store::{Changeset, Kind, Store};

use super::replay::replay;
use super::{open, store_error};
use crate::error::{CliError, exit, read_xer_bytes};
use crate::loaded::Loaded;
use crate::output::print_line;

/// Replays one edit changeset on its parent's file and compares the result
/// with what was recorded: `None` if the same, else why not.
fn replays_the_same(
    store: &Store,
    path: &Path,
    changeset: &Changeset,
) -> Result<Option<Value>, CliError> {
    let Some(parent) = &changeset.parent else {
        return Ok(None);
    };
    let before = store.changeset(parent).map_err(store_error)?;
    let start = read_xer_bytes(store.blob(&before.snapshot).map_err(store_error)?, path)?;
    let recorded = store.blob(&changeset.snapshot).map_err(store_error)?;
    let differs = |summary: String| {
        Some(json!({
            "id": changeset.id.as_str(),
            "short_id": changeset.id.short(),
            "tool_version": changeset.tool_version,
            "summary": summary,
        }))
    };
    let rebuilt = match replay(start, std::slice::from_ref(changeset)) {
        Ok(rebuilt) => rebuilt,
        Err(conflict) => {
            let why: Vec<String> = conflict
                .problems
                .iter()
                .map(|p| p.message.clone())
                .collect();
            return Ok(differs(format!(
                "its edits no longer apply: {}",
                why.join("; ")
            )));
        }
    };
    let mut bytes = Vec::new();
    rebuilt
        .write(&mut bytes)
        .map_err(|e| CliError::new("write-failed", e.to_string(), "try again"))?;
    if bytes == recorded {
        return Ok(None);
    }
    let summary = crate::diff::report(
        "recorded",
        &Loaded::new(read_xer_bytes(recorded, path)?),
        "replayed",
        &Loaded::new(rebuilt),
    )
    .map_or_else(
        |_| "the files differ".to_owned(),
        |report| {
            let value = serde_json::to_value(&report).unwrap_or(Value::Null);
            let changed: u64 = value["projects"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|p| p["counts"]["changed"].as_u64())
                .sum();
            format!("replaying today changes {changed} activities from what was recorded")
        },
    );
    Ok(differs(summary))
}

/// Runs `history verify`.
///
/// # Errors
///
/// What the store refuses to open. Damage and differences are reported, not
/// errors: exit 1 for damage, 3 for differences, 0 when clean.
pub fn run(path: &Path, with_replay: bool, json: bool) -> Result<u8, CliError> {
    let store = open(path)?;
    let damage: Vec<Value> = store
        .verify()
        .map_err(store_error)?
        .into_iter()
        .map(|d| json!({ "subject": d.subject, "problem": d.problem }))
        .collect();
    let mut replayed = Value::Null;
    let mut differing = Vec::new();
    if with_replay && damage.is_empty() {
        let edits: Vec<Changeset> = store
            .all()
            .map_err(store_error)?
            .into_iter()
            .filter(|c| c.kind == Kind::Edit)
            .collect();
        for changeset in &edits {
            differing.extend(replays_the_same(&store, path, changeset)?);
        }
        replayed = json!({ "checked": edits.len(), "differing": differing });
    }
    let code = if !damage.is_empty() {
        exit::ERROR
    } else if !differing.is_empty() {
        exit::FINDINGS
    } else {
        exit::OK
    };
    if json {
        let doc = json!({
            "schema": "totalfloat.history-verify/1",
            "store": path.display().to_string(),
            "damage": damage,
            "replay": replayed,
        });
        print_line(&serde_json::to_string_pretty(&doc).unwrap_or_default());
        return Ok(code);
    }
    let mut lines = Vec::new();
    if damage.is_empty() {
        lines.push(format!(
            "{} is whole: every file and changeset matches its hash",
            path.display()
        ));
    } else {
        lines.push(format!("{} is damaged:", path.display()));
        for d in &damage {
            lines.push(format!(
                "  {}: {}",
                d["subject"].as_str().unwrap_or_default(),
                d["problem"].as_str().unwrap_or_default()
            ));
        }
    }
    if with_replay && damage.is_empty() {
        let checked = replayed["checked"].as_u64().unwrap_or(0);
        if differing.is_empty() {
            lines.push(format!("All {checked} edits replay to the files recorded"));
        } else {
            lines.push(format!(
                "{} of {checked} edits replay differently today:",
                differing.len()
            ));
            for d in &differing {
                lines.push(format!(
                    "  {} (recorded by {}): {}",
                    d["short_id"].as_str().unwrap_or_default(),
                    d["tool_version"].as_str().unwrap_or_default(),
                    d["summary"].as_str().unwrap_or_default()
                ));
            }
        }
    }
    print_line(&lines.join("\n"));
    Ok(code)
}
