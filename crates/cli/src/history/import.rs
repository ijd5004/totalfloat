//! `history import`: a new export from P6 becomes the next point on the
//! checked-out branch, with what changed since the last one.

use std::collections::BTreeSet;
use std::path::Path;

use serde_json::{Value, json};
use totalfloat_store::Kind;

use super::{Asked, read_head, record_change};
use crate::error::{CliError, exit, read_xer};
use crate::loaded::Loaded;
use crate::output::print_line;

fn codes(loaded: &Loaded) -> BTreeSet<String> {
    loaded
        .schedules
        .iter()
        .map(|s| s.project.short_name.clone())
        .collect()
}

/// Records `file` as an import after the checked-out head.
///
/// # Errors
///
/// `different-projects` when the file shares no project with the branch
/// (without `force`), `branch-moved`, or what the file or store refuses.
pub fn run(
    path: &Path,
    file: &Path,
    force: bool,
    asked: Asked<'_>,
    json: bool,
) -> Result<u8, CliError> {
    let new = Loaded::new(read_xer(file)?);
    let (head, current) = read_head(path, None)?;
    let current = Loaded::new(current);
    let (was, now) = (codes(&current), codes(&new));
    if was.is_disjoint(&now) && !force {
        return Err(CliError::new(
            "different-projects",
            format!(
                "{} holds {}, and {} holds {}",
                file.display(),
                now.iter().cloned().collect::<Vec<_>>().join(", "),
                head.branch,
                was.iter().cloned().collect::<Vec<_>>().join(", ")
            ),
            "check it's the right file; --force records it anyway",
        ));
    }
    let mut warnings = Vec::new();
    if !was.is_disjoint(&now) && was != now {
        let gone: Vec<&String> = was.difference(&now).collect();
        let came: Vec<&String> = now.difference(&was).collect();
        warnings.push(format!("projects differ: left out {gone:?}, new {came:?}"));
    }
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let file_name = file.file_name().unwrap_or_default().to_string_lossy();
    let mut diff = crate::diff::report(
        &format!("{name}@{}", head.branch),
        &current,
        &file_name,
        &new,
    )?;
    diff.limit(25);
    let commands = json!({ "import": { "file": file_name } }).to_string();
    let report = serde_json::to_value(&diff).unwrap_or(Value::Null);
    let parts = (commands, "[]".to_owned(), report.to_string());
    let bytes = new.raw.source().to_vec();
    let recorded = record_change(path, &head, (Kind::Import, None), bytes, parts, asked)?;
    if json {
        let doc = json!({
            "schema": "totalfloat.history-import/1",
            "file": file.display().to_string(),
            "recorded": recorded,
            "warnings": warnings,
            "diff": report,
        });
        print_line(&serde_json::to_string_pretty(&doc).unwrap_or_default());
        return Ok(exit::OK);
    }
    let mut lines: Vec<String> = warnings.iter().map(|w| format!("warning: {w}")).collect();
    lines.push(crate::diff::human(&diff));
    lines.push(recorded.as_ref().map_or_else(
        || "The file is the same as the branch's, so nothing was recorded".to_owned(),
        |r| format!("Recorded {} on {}", r.short_id, r.branch),
    ));
    print_line(&lines.join("\n"));
    Ok(exit::OK)
}
