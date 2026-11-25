//! `history revert` and `history undo`: a new changeset whose file is the
//! branch as if an earlier changeset had never happened. Nothing is deleted.

use std::path::Path;

use serde_json::{Value, json};
use totalfloat_store::{BlobHash, Changeset, ChangesetId, Kind, Store};
use totalfloat_xer::RawXer;

use super::replay::{cancel_reverts, conflict_error, replay, reverts};
use super::{Asked, read_head, record_change, resolve_point, store_error};
use crate::error::{CliError, exit, read_xer_bytes};
use crate::loaded::Loaded;
use crate::output::print_line;

/// The changesets after `target` on the first-parent line to `head`, oldest
/// first, or `None` if `target` isn't on that line.
fn after(
    store: &Store,
    head: &ChangesetId,
    target: &ChangesetId,
) -> Result<Option<Vec<Changeset>>, CliError> {
    if !store.is_ancestor(target, head).map_err(store_error)? {
        return Ok(None);
    }
    let mut later = Vec::new();
    let mut next = Some(head.clone());
    while let Some(id) = next.take() {
        if id == *target {
            later.reverse();
            return Ok(Some(later));
        }
        let changeset = store.changeset(&id).map_err(store_error)?;
        next.clone_from(&changeset.parent);
        later.push(changeset);
    }
    Ok(None)
}

/// The file once `target` is taken out: its parent's file, with the later
/// changesets replayed on it after reverts and what they took out cancel.
fn without(
    store: &Store,
    path: &Path,
    target: &Changeset,
    later: Vec<Changeset>,
) -> Result<(Vec<u8>, RawXer, Vec<Changeset>), CliError> {
    let Some(parent) = &target.parent else {
        return Err(CliError::new(
            "nothing-before",
            format!("{} is where the history starts", target.id.short()),
            "there's nothing before it to go back to",
        ));
    };
    if let Some(undo) = later
        .iter()
        .find(|c| reverts(c).as_deref() == Some(target.id.as_str()))
    {
        return Err(CliError::new(
            "already-reverted",
            format!(
                "{} was already taken out by {}",
                target.id.short(),
                undo.id.short()
            ),
            format!("revert {} to put it back", undo.id.short()),
        ));
    }
    let before = store.changeset(parent).map_err(store_error)?;
    let bytes = store.blob(&before.snapshot).map_err(store_error)?;
    let start = read_xer_bytes(bytes.clone(), path)?;
    let replayed = cancel_reverts(later);
    if replayed.is_empty() {
        return Ok((bytes, start, replayed));
    }
    let what = format!("can't take out {}", target.id.short());
    let rebuilt =
        replay(start, &replayed).map_err(|c| conflict_error("revert-conflict", &what, c))?;
    let mut out = Vec::new();
    rebuilt
        .write(&mut out)
        .map_err(|e| CliError::new("write-failed", e.to_string(), "try again"))?;
    Ok((out, rebuilt, replayed))
}

/// Takes a changeset back out of the checked-out branch, as a new changeset:
/// the one named, or with `None` the head as just read.
///
/// # Errors
///
/// `not-on-branch`, `nothing-before`, `already-reverted`, `revert-conflict`
/// naming the later changeset that no longer applies, or what the store
/// refuses.
pub fn revert(path: &Path, at: Option<&str>, asked: Asked<'_>, json: bool) -> Result<u8, CliError> {
    let (head, current) = read_head(path, None)?;
    let store = super::open(path)?;
    let target_id = match at {
        Some(at) => resolve_point(&store, path, Some(at))?,
        None => head.id.clone(),
    };
    let target = store.changeset(&target_id).map_err(store_error)?;
    let Some(later) = after(&store, &head.id, &target_id)? else {
        return Err(CliError::new(
            "not-on-branch",
            format!("{} isn't part of {}", target_id.short(), head.branch),
            "`history log` lists the changesets on the branch",
        ));
    };
    let (result, rebuilt, replayed) = without(&store, path, &target, later)?;
    let changed = BlobHash::of(&result) != head.snapshot;
    let impact = if changed {
        let label = path.file_name().unwrap_or_default().to_string_lossy();
        crate::diff::report(
            &format!("{label}@{}", head.branch),
            &Loaded::new(current),
            &format!("{label}@revert"),
            &Loaded::new(rebuilt),
        )
        .map_or(Value::Null, |mut r| {
            r.limit(25);
            serde_json::to_value(&r).unwrap_or(Value::Null)
        })
    } else {
        Value::Null
    };
    let commands = json!({ "reverts": target_id.as_str() }).to_string();
    let parts = (commands, "[]".to_owned(), impact.to_string());
    let recorded = record_change(path, &head, (Kind::Revert, None), result, parts, asked)?;
    let doc = json!({
        "schema": "totalfloat.history-revert/1",
        "reverted": target_id.as_str(),
        "replayed": replayed.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
        "recorded": recorded,
        "impact": impact,
    });
    if json {
        print_line(&serde_json::to_string_pretty(&doc).unwrap_or_default());
    } else {
        let mut lines = vec![format!(
            "Took out {} ({}), replaying {} later changeset{}",
            target_id.short(),
            super::summary(&target),
            replayed.len(),
            if replayed.len() == 1 { "" } else { "s" }
        )];
        lines.push(recorded.as_ref().map_or_else(
            || "That leaves the file as it was, so nothing was recorded".to_owned(),
            |r| format!("Recorded {} on {}", r.short_id, r.branch),
        ));
        print_line(&lines.join("\n"));
    }
    Ok(exit::OK)
}
