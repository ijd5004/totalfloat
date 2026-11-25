//! `history diff`: what changed between two points in a history, with the
//! changesets that made the change.

use std::path::Path;

use serde_json::{Value, json};
use totalfloat_store::{Changeset, ChangesetId, Store};

use super::{branch_head, log_entry, open, resolve_point, snapshot_at, store_error};
use crate::error::{CliError, exit, read_xer_bytes};
use crate::loaded::Loaded;
use crate::output::print_line;

/// The changesets from `to` back to `from` through first parents, newest
/// first, or `None` if `from` isn't on that line.
fn between(
    store: &Store,
    from: &ChangesetId,
    to: &ChangesetId,
) -> Result<Option<Vec<Changeset>>, CliError> {
    if !store.is_ancestor(from, to).map_err(store_error)? {
        return Ok(None);
    }
    let mut out = Vec::new();
    let mut next = Some(to.clone());
    while let Some(id) = next.take() {
        if id == *from {
            return Ok(Some(out));
        }
        let changeset = store.changeset(&id).map_err(store_error)?;
        next.clone_from(&changeset.parent);
        out.push(changeset);
    }
    Ok(None)
}

/// The two points to compare: given, or the head and its parent.
fn points(
    store: &Store,
    path: &Path,
    from: Option<&str>,
    to: Option<&str>,
) -> Result<(ChangesetId, ChangesetId), CliError> {
    let to = match to {
        Some(to) => resolve_point(store, path, Some(to))?,
        None => branch_head(store, None)?.1,
    };
    let from = match from {
        Some(from) => resolve_point(store, path, Some(from))?,
        None => store
            .changeset(&to)
            .map_err(store_error)?
            .parent
            .ok_or_else(|| {
                CliError::new(
                    "nothing-before",
                    format!("{} is where the history starts", to.short()),
                    "name the two points to compare",
                )
            })?,
    };
    Ok((from, to))
}

/// Runs `history diff`.
///
/// # Errors
///
/// `no-such-ref`, `nothing-before`, or what the store or the files refuse.
pub fn run(
    path: &Path,
    (from, to): (Option<&str>, Option<&str>),
    limit: usize,
    json: bool,
) -> Result<u8, CliError> {
    let store = open(path)?;
    let (from, to) = points(&store, path, from, to)?;
    let line = between(&store, &from, &to)?;
    let read = |id: &ChangesetId| -> Result<Loaded, CliError> {
        let (_, bytes) = snapshot_at(&store, path, Some(id.as_str()))?;
        Ok(Loaded::new(read_xer_bytes(bytes, path)?))
    };
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let mut report = crate::diff::report(
        &format!("{name}@{}", from.short()),
        &read(&from)?,
        &format!("{name}@{}", to.short()),
        &read(&to)?,
    )?;
    // 0 asks for every item.
    if limit > 0 {
        report.limit(limit);
    }
    let changesets: Vec<Value> = line.iter().flatten().map(log_entry).collect();
    if json {
        let doc = json!({
            "schema": "totalfloat.history-diff/1",
            "from": from.as_str(),
            "to": to.as_str(),
            "ancestor": line.is_some(),
            "changesets": changesets,
            "diff": report,
        });
        print_line(&serde_json::to_string_pretty(&doc).unwrap_or_default());
        return Ok(exit::OK);
    }
    let mut lines = Vec::new();
    match &line {
        Some(made) => {
            lines.push(format!(
                "From {} to {}, {} changeset{}:",
                from.short(),
                to.short(),
                made.len(),
                if made.len() == 1 { "" } else { "s" }
            ));
            for changeset in made {
                let what = changeset
                    .message
                    .lines()
                    .next()
                    .filter(|l| !l.is_empty())
                    .map_or_else(|| super::summary(changeset), str::to_owned);
                lines.push(format!(
                    "  {}  {}  {what}",
                    changeset.id.short(),
                    changeset.author
                ));
            }
        }
        None => lines.push(format!(
            "{} isn't before {} on one line of history, so this compares the two files only",
            from.short(),
            to.short()
        )),
    }
    lines.push(String::new());
    lines.push(crate::diff::human(&report));
    print_line(&lines.join("\n"));
    Ok(exit::OK)
}
