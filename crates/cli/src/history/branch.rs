//! `history branch`, `switch` and `branches`: named lines of work for
//! what-ifs. A branch is only a name for a changeset; nothing is copied.

use std::path::Path;

use serde_json::json;
use totalfloat_store::{Moving, RefKind, StoreError};

use super::{branch_head, open, resolve_point, store_error, summary};
use crate::error::{CliError, exit};
use crate::output::print_line;

/// Refuses a name a ref can't have: empty, with characters other than
/// letters, digits, `-`, `_`, `/` and `.`, or one that could be read as a
/// changeset ID.
///
/// # Errors
///
/// `invalid-name`, saying why.
pub fn check_name(name: &str) -> Result<(), CliError> {
    let bad = |why: &str| {
        Err(CliError::new(
            "invalid-name",
            format!("{name} can't name a branch or baseline: {why}"),
            "use letters, digits, -, _, / and ., such as what-if/steel-late",
        ))
    };
    if name.is_empty() {
        return bad("it's empty");
    }
    if !name
        .chars()
        .all(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '/' | '.'))
    {
        return bad("it holds other characters");
    }
    if name.starts_with(['-', '.', '/']) || name.ends_with(['/', '.']) || name.contains("..") {
        return bad("it starts or ends oddly");
    }
    if name.len() >= 6 && name.bytes().all(|b| b.is_ascii_hexdigit()) {
        return bad("it looks like a changeset ID");
    }
    Ok(())
}

/// Makes a ref of `kind` named `name` at `at` (default the checked-out head).
///
/// # Errors
///
/// `invalid-name`, `name-taken`, or what the store refuses.
pub fn make_ref(
    path: &Path,
    name: &str,
    kind: RefKind,
    at: Option<&str>,
) -> Result<String, CliError> {
    check_name(name)?;
    let mut store = open(path)?;
    let target = resolve_point(&store, path, at)?;
    let moving = Moving {
        name,
        kind,
        expected: None,
    };
    store
        .set_ref(moving, &target)
        .map_err(|error| match error {
            StoreError::Moved { .. } | StoreError::WrongKind { .. } => CliError::new(
                "name-taken",
                format!("{name} already names something in {}", path.display()),
                "`history branches` lists the names in use; pick another",
            ),
            other => store_error(other),
        })?;
    Ok(target.short())
}

/// `history branch`: make a branch, or delete one.
///
/// # Errors
///
/// As [`make_ref`]; deleting refuses the checked-out branch and anything not
/// a branch.
pub fn branch(
    path: &Path,
    name: &str,
    at: Option<&str>,
    delete: bool,
    json: bool,
) -> Result<u8, CliError> {
    if delete {
        let store = open(path)?;
        match store.get_ref(name).map_err(store_error)? {
            Some(found) if found.kind == RefKind::Branch => {}
            Some(_) => {
                return Err(CliError::new(
                    "wrong-ref-kind",
                    format!("{name} isn't a branch"),
                    "`history branches` lists branches",
                ));
            }
            None => return Err(store_error(StoreError::NotFound(name.to_owned()))),
        }
        store.delete_ref(name).map_err(store_error)?;
        let doc = json!({ "schema": "totalfloat.history-branch/1", "branch": name, "deleted": true, "at": null });
        say(
            json,
            &doc,
            &format!("Deleted branch {name}; its changesets stay in the store"),
        );
        return Ok(exit::OK);
    }
    let short = make_ref(path, name, RefKind::Branch, at)?;
    let doc = json!({ "schema": "totalfloat.history-branch/1", "branch": name, "deleted": false, "at": short });
    say(
        json,
        &doc,
        &format!("Made branch {name} at {short}; `history switch` to work on it"),
    );
    Ok(exit::OK)
}

fn say(json: bool, doc: &serde_json::Value, text: &str) {
    if json {
        print_line(&serde_json::to_string_pretty(doc).unwrap_or_default());
    } else {
        print_line(text);
    }
}

/// `history switch`: check out a branch, so edits record there.
///
/// # Errors
///
/// `no-such-ref`, or `wrong-ref-kind` for a baseline or proposal.
pub fn switch(path: &Path, name: &str, json: bool) -> Result<u8, CliError> {
    let store = open(path)?;
    store.set_current(name).map_err(|error| match error {
        StoreError::WrongKind { actual, .. } => CliError::new(
            "wrong-ref-kind",
            format!("{name} is a {actual}, and only a branch can be checked out"),
            format!("make a branch from it: `history branch STORE NEW-NAME --at {name}`"),
        ),
        other => store_error(other),
    })?;
    let (_, head) = branch_head(&store, Some(name))?;
    let doc =
        json!({ "schema": "totalfloat.history-switch/1", "branch": name, "head": head.as_str() });
    say(json, &doc, &format!("On {name} at {}", head.short()));
    Ok(exit::OK)
}

/// `history branches`: each branch, its head and its latest change.
///
/// # Errors
///
/// What the store refuses.
pub fn branches(path: &Path, json: bool) -> Result<u8, CliError> {
    let store = open(path)?;
    let current = store.current().map_err(store_error)?;
    let mut rows = Vec::new();
    for named in store.refs().map_err(store_error)? {
        if named.kind != RefKind::Branch {
            continue;
        }
        let head = store.changeset(&named.target).map_err(store_error)?;
        let what = if head.message.is_empty() {
            summary(&head)
        } else {
            head.message.lines().next().unwrap_or_default().to_owned()
        };
        rows.push(json!({
            "name": named.name,
            "current": current.as_deref() == Some(named.name.as_str()),
            "head": head.id.as_str(),
            "short_id": head.id.short(),
            "created": head.created,
            "author": head.author,
            "last_change": what,
        }));
    }
    if json {
        let doc = json!({
            "schema": "totalfloat.history-branches/1",
            "store": path.display().to_string(),
            "current": current,
            "branches": rows,
        });
        print_line(&serde_json::to_string_pretty(&doc).unwrap_or_default());
        return Ok(exit::OK);
    }
    let lines: Vec<String> = rows
        .iter()
        .map(|row| {
            format!(
                "{} {}  {}  {}  {}",
                if row["current"] == true { "*" } else { " " },
                row["name"].as_str().unwrap_or_default(),
                row["short_id"].as_str().unwrap_or_default(),
                row["created"].as_str().unwrap_or_default(),
                row["last_change"].as_str().unwrap_or_default(),
            )
        })
        .collect();
    print_line(&lines.join("\n"));
    Ok(exit::OK)
}

#[cfg(test)]
mod tests {
    use super::check_name;

    #[test]
    fn names_are_checked() {
        for good in ["main", "what-if/steel-late", "rev.2", "plan_B", "cafe"] {
            assert!(check_name(good).is_ok(), "{good}");
        }
        for bad in [
            "",
            "has space",
            "a@b",
            "-x",
            "x/",
            "a..b",
            "deadbeef",
            "ABCDEF12",
        ] {
            assert!(check_name(bad).is_err(), "{bad}");
        }
    }
}
