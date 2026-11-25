//! `history baseline` and `baselines`: named points that never move. P6
//! keeps a baseline as a full copy of the project; here it is a name for a
//! point already in the history.

use std::path::Path;

use serde_json::json;
use totalfloat_store::{RefKind, StoreError};

use super::branch::make_ref;
use super::{open, store_error, summary};
use crate::error::{CliError, exit};
use crate::output::print_line;

fn say(json: bool, doc: &serde_json::Value, text: &str) {
    if json {
        print_line(&serde_json::to_string_pretty(doc).unwrap_or_default());
    } else {
        print_line(text);
    }
}

/// `history baseline`: name a point as a baseline, or delete one.
///
/// # Errors
///
/// `invalid-name`, `name-taken`, `wrong-ref-kind` when deleting something
/// that isn't a baseline, or what the store refuses.
pub fn baseline(
    path: &Path,
    name: &str,
    at: Option<&str>,
    delete: bool,
    json: bool,
) -> Result<u8, CliError> {
    if delete {
        let store = open(path)?;
        match store.get_ref(name).map_err(store_error)? {
            Some(found) if found.kind == RefKind::Baseline => {}
            Some(_) => {
                return Err(CliError::new(
                    "wrong-ref-kind",
                    format!("{name} isn't a baseline"),
                    "`history baselines` lists baselines",
                ));
            }
            None => return Err(store_error(StoreError::NotFound(name.to_owned()))),
        }
        store.delete_ref(name).map_err(store_error)?;
        let doc = json!({ "schema": "totalfloat.history-baseline/1", "baseline": name, "deleted": true, "at": null });
        say(
            json,
            &doc,
            &format!("Deleted baseline {name}; the point stays in the history"),
        );
        return Ok(exit::OK);
    }
    let short = make_ref(path, name, RefKind::Baseline, at)?;
    let doc = json!({ "schema": "totalfloat.history-baseline/1", "baseline": name, "deleted": false, "at": short });
    say(
        json,
        &doc,
        &format!("Baseline {name} is {short}; compare with --baseline STORE@{name}"),
    );
    Ok(exit::OK)
}

/// `history baselines`: each baseline and the point it names.
///
/// # Errors
///
/// What the store refuses.
pub fn baselines(path: &Path, json: bool) -> Result<u8, CliError> {
    let store = open(path)?;
    let mut rows = Vec::new();
    for named in store.refs().map_err(store_error)? {
        if named.kind != RefKind::Baseline {
            continue;
        }
        let point = store.changeset(&named.target).map_err(store_error)?;
        let what = point
            .message
            .lines()
            .next()
            .filter(|l| !l.is_empty())
            .map_or_else(|| summary(&point), str::to_owned);
        rows.push(json!({
            "name": named.name,
            "at": point.id.as_str(),
            "short_id": point.id.short(),
            "created": point.created,
            "author": point.author,
            "point": what,
        }));
    }
    if json {
        let doc = json!({
            "schema": "totalfloat.history-baselines/1",
            "store": path.display().to_string(),
            "baselines": rows,
        });
        print_line(&serde_json::to_string_pretty(&doc).unwrap_or_default());
        return Ok(exit::OK);
    }
    let lines: Vec<String> = rows
        .iter()
        .map(|row| {
            format!(
                "{}  {}  {}  {}",
                row["name"].as_str().unwrap_or_default(),
                row["short_id"].as_str().unwrap_or_default(),
                row["created"].as_str().unwrap_or_default(),
                row["point"].as_str().unwrap_or_default(),
            )
        })
        .collect();
    if lines.is_empty() {
        print_line("No baselines yet; `history baseline STORE NAME` names one");
    } else {
        print_line(&lines.join("\n"));
    }
    Ok(exit::OK)
}
