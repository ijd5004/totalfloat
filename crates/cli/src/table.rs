//! `edit --progress-table`: a status update as a CSV table, one activity a row.
//!
//! The header names the columns: `activity`, and any of `actual_start`,
//! `actual_finish`, `remaining` and `percent`. A blank cell leaves that value as
//! it is. Each row becomes a `set_progress` edit.

use std::path::Path;

use totalfloat_edit::{Edit, Span};

use crate::error::CliError;

const COLUMNS: [&str; 5] = [
    "activity",
    "actual_start",
    "actual_finish",
    "remaining",
    "percent",
];

/// One row's edit and the line it came from.
#[derive(Debug)]
pub struct Row {
    /// The file's line number, counting the header as 1.
    pub line: usize,
    /// The edit the row asks for.
    pub edit: Edit,
}

fn refused(path: &Path, problems: &[String]) -> CliError {
    CliError::new(
        "table-refused",
        format!(
            "{} has rows that can't be used, so nothing was changed:\n{}",
            path.display(),
            problems.join("\n")
        ),
        format!("correct those rows; the columns are {}", COLUMNS.join(", ")),
    )
}

/// Splits CSV text into records, each with the line it starts on. Quoted
/// cells can hold commas, line breaks and doubled quotes.
pub(crate) fn records(text: &str) -> Result<Vec<(usize, Vec<String>)>, String> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut out = Vec::new();
    let (mut record, mut cell) = (Vec::new(), String::new());
    let (mut line, mut starts, mut quoted) = (1, 1, false);
    let mut letters = text.chars().peekable();
    while let Some(letter) = letters.next() {
        match (letter, quoted) {
            ('"', true) if letters.peek() == Some(&'"') => {
                letters.next();
                cell.push('"');
            }
            ('"', true) => quoted = false,
            ('"', false) if cell.is_empty() => quoted = true,
            (',', false) => record.push(std::mem::take(&mut cell)),
            ('\r', false) if letters.peek() == Some(&'\n') => {}
            ('\n', false) => {
                record.push(std::mem::take(&mut cell));
                out.push((starts, std::mem::take(&mut record)));
                line += 1;
                starts = line;
            }
            ('\n', true) => {
                line += 1;
                cell.push('\n');
            }
            (other, _) => cell.push(other),
        }
    }
    if quoted {
        return Err(format!("line {starts}: a quoted cell never closes"));
    }
    if !cell.is_empty() || !record.is_empty() {
        record.push(cell);
        out.push((starts, record));
    }
    // Blank lines carry nothing.
    out.retain(|(_, cells)| cells.iter().any(|c| !c.trim().is_empty()));
    Ok(out)
}

fn some(cell: Option<&String>) -> Option<String> {
    cell.map(|c| c.trim().to_owned()).filter(|c| !c.is_empty())
}

/// Reads a status table into `set_progress` edits, refusing the whole table if
/// any row can't be read, and naming each such row.
///
/// # Errors
///
/// `file-not-readable`, or `table-refused` listing every bad row.
pub fn read(path: &Path) -> Result<Vec<Row>, CliError> {
    let text = std::fs::read_to_string(path).map_err(|error| {
        CliError::new(
            "file-not-readable",
            format!("can't read {}: {error}", path.display()),
            "check the path and that the file is readable",
        )
    })?;
    let records = records(&text).map_err(|problem| refused(path, &[problem]))?;
    let Some(((_, header), rows)) = records.split_first() else {
        return Err(refused(path, &["the table is empty".to_owned()]));
    };
    let names: Vec<String> = header.iter().map(|h| h.trim().to_lowercase()).collect();
    let unknown: Vec<String> = names
        .iter()
        .filter(|name| !COLUMNS.contains(&name.as_str()))
        .map(|name| format!("line 1: there's no column {name}"))
        .collect();
    if !unknown.is_empty() {
        return Err(refused(path, &unknown));
    }
    if !names.iter().any(|name| name == "activity") {
        return Err(refused(
            path,
            &["line 1: the header needs an activity column".to_owned()],
        ));
    }
    let mut problems = Vec::new();
    let mut out = Vec::new();
    for (line, cells) in rows {
        let at = |name: &str| {
            some(
                names
                    .iter()
                    .position(|n| n == name)
                    .and_then(|i| cells.get(i)),
            )
        };
        let mut wrong = |what: String| problems.push(format!("line {line}: {what}"));
        if cells.len() > names.len() {
            wrong(format!("{} cells for {} columns", cells.len(), names.len()));
            continue;
        }
        let Some(activity) = at("activity") else {
            wrong("no activity".to_owned());
            continue;
        };
        let remaining = match at("remaining").map(|r| r.parse::<Span>()) {
            Some(Err(_)) => {
                wrong(format!(
                    "{activity}'s remaining isn't a span like 5d or 40h"
                ));
                continue;
            }
            Some(Ok(span)) => Some(span),
            None => None,
        };
        let percent = match at("percent").map(|p| p.trim_end_matches('%').parse::<f64>()) {
            Some(Err(_)) => {
                wrong(format!("{activity}'s percent isn't a number"));
                continue;
            }
            Some(Ok(p)) => Some(p),
            None => None,
        };
        let (actual_start, actual_finish) = (at("actual_start"), at("actual_finish"));
        if [&actual_start, &actual_finish].iter().all(|d| d.is_none())
            && remaining.is_none()
            && percent.is_none()
        {
            wrong(format!("{activity} has no progress in it"));
            continue;
        }
        out.push(Row {
            line: *line,
            edit: Edit::SetProgress {
                activity,
                actual_start,
                actual_finish,
                remaining,
                percent,
            },
        });
    }
    if problems.is_empty() {
        Ok(out)
    } else {
        Err(refused(path, &problems))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoted_cells_hold_commas_quotes_and_breaks() {
        let held = records("a,b\r\n\"x, \"\"y\"\"\",\"two\nlines\"\n\nlast,1\n").unwrap();
        assert_eq!(held.len(), 3);
        assert_eq!(
            held[1],
            (2, vec!["x, \"y\"".to_owned(), "two\nlines".to_owned()])
        );
        // The quoted break and the blank line both count, so this is line 5.
        assert_eq!(held[2], (5, vec!["last".to_owned(), "1".to_owned()]));
        assert!(records("a\n\"open").is_err());
    }

    fn table(text: &str) -> Result<Vec<Row>, CliError> {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("status.csv");
        std::fs::write(&path, text).unwrap();
        read(&path)
    }

    #[test]
    fn rows_become_progress_with_blanks_left_alone() {
        let rows =
            table("activity,actual_start,remaining,percent\nA1,2026-01-12 08:00,,40%\nA2,,8h,\n")
                .unwrap();
        assert_eq!(rows.len(), 2);
        let Edit::SetProgress {
            activity,
            actual_start,
            remaining,
            percent,
            ..
        } = &rows[0].edit
        else {
            panic!();
        };
        assert_eq!(
            (activity.as_str(), actual_start.as_deref()),
            ("A1", Some("2026-01-12 08:00"))
        );
        assert!(remaining.is_none());
        assert_eq!(*percent, Some(40.0));
        assert_eq!(rows[1].line, 3);
    }

    #[test]
    fn every_bad_row_is_named() {
        let Err(error) = table("activity,remaining,percent\nA1,soon,\n,8h,\nA3,,lots\nA4,,\n")
        else {
            panic!("a bad table was read");
        };
        assert_eq!(error.code, "table-refused");
        for line in ["line 2", "line 3", "line 4", "line 5"] {
            assert!(error.message.contains(line), "{line}: {}", error.message);
        }
        let Err(error) = table("activity,start\nA1,x\n") else {
            panic!("an unknown column was read");
        };
        assert!(
            error.message.contains("no column start"),
            "{}",
            error.message
        );
    }
}
