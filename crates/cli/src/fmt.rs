//! `totalfloat fmt`: rewrite schedule text files (`.tfs`, ARCHITECTURE.md, Schedule as text) in the
//! canonical layout, or with `--check`, say which would change.

use std::io::Write as _;
use std::path::{Path, PathBuf};

use crate::error::{CliError, Problem, exit};
use crate::output::print_line;

fn read_error(path: &Path, error: &std::io::Error) -> CliError {
    CliError::new(
        "file-not-readable",
        format!("can't read {}: {error}", path.display()),
        "check the path and that the file is text",
    )
}

/// What reading one file found.
enum Read {
    /// The file and its canonical layout.
    Text(String, String),
    /// Its problems, already printed unless this is `--json`.
    Problems(Vec<Problem>),
}

fn problem(
    name: &str,
    line: Option<usize>,
    code: &'static str,
    message: String,
    hint: String,
) -> Problem {
    Problem {
        edit: None,
        source: line.map_or_else(|| name.to_owned(), |line| format!("{name} line {line}")),
        code,
        message,
        hint,
        depends_on: None,
        line: None,
        column: None,
    }
}

/// The file in the canonical layout, or every problem reading it.
fn formatted(path: &Path, json: bool) -> Read {
    let name = path.display().to_string();
    let source = match std::fs::read_to_string(path) {
        Ok(source) => source,
        Err(e) => {
            let error = read_error(path, &e);
            if !json {
                eprintln!(
                    "error[{}]: {}\n  = hint: {}\n",
                    error.code, error.message, error.hint
                );
            }
            return Read::Problems(vec![problem(
                &name,
                None,
                error.code,
                error.message,
                error.hint,
            )]);
        }
    };
    match totalfloat_text::parse(&source, &name) {
        Ok(doc) => Read::Text(source, totalfloat_text::format(&doc)),
        Err(diagnostics) => {
            if !json {
                for diagnostic in &diagnostics {
                    eprintln!("{}", diagnostic.render(&source));
                }
            }
            Read::Problems(
                diagnostics
                    .into_iter()
                    .map(|d| problem(&name, Some(d.at.line), d.code, d.message, d.hint))
                    .collect(),
            )
        }
    }
}

/// Replaces a file's text, keeping its permissions; a reader never sees half
/// a file. A symbolic link keeps pointing where it did, and its target gets
/// the new text.
fn replace(path: &Path, text: &str) -> Result<(), CliError> {
    let failed = |e: std::io::Error| {
        CliError::new(
            "write-failed",
            format!("can't write {}: {e}", path.display()),
            "check the directory is writable",
        )
    };
    let target = std::fs::canonicalize(path).map_err(failed)?;
    let permissions = std::fs::metadata(&target).map_err(failed)?.permissions();
    let parent = target.parent().unwrap_or_else(|| Path::new("."));
    let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(failed)?;
    temporary.write_all(text.as_bytes()).map_err(failed)?;
    temporary
        .as_file()
        .set_permissions(permissions)
        .map_err(failed)?;
    temporary.persist(&target).map_err(|e| failed(e.error))?;
    Ok(())
}

/// Runs `fmt`. Every file is read, even after one with problems, so one
/// broken file doesn't hide the others.
///
/// # Errors
///
/// `text-not-readable`, with every problem in every file, or `write-failed`.
pub fn run(files: &[PathBuf], check: bool, json: bool) -> Result<u8, CliError> {
    let mut changed = Vec::new();
    let mut tidy = Vec::new();
    let mut problems = Vec::new();
    let mut broken = 0;
    for path in files {
        let name = path.display().to_string();
        match formatted(path, json) {
            Read::Text(source, text) if source == text => tidy.push(name),
            Read::Text(_, text) => {
                if !check {
                    replace(path, &text)?;
                }
                changed.push(name);
            }
            Read::Problems(found) => {
                broken += 1;
                problems.extend(found);
            }
        }
    }
    if !json {
        for name in &changed {
            print_line(&if check {
                format!("{name} would change")
            } else {
                format!("Formatted {name}")
            });
        }
        if check && changed.is_empty() && problems.is_empty() {
            print_line("Every file is formatted");
        }
        if !check {
            for name in &tidy {
                print_line(&format!("{name} is already formatted"));
            }
        }
    }
    if !problems.is_empty() {
        let count = problems.len();
        let plural = |n: usize, word: &str| format!("{n} {word}{}", if n == 1 { "" } else { "s" });
        let mut error = CliError::new(
            "text-not-readable",
            format!(
                "{} in {} left as they were",
                plural(count, "problem"),
                plural(broken, "file")
            ),
            "fix each problem at its line, then run fmt again",
        );
        if json {
            error.problems = problems;
        }
        return Err(error);
    }
    if json {
        let doc = serde_json::json!({
            "schema": "totalfloat.fmt/1",
            "check": check,
            "changed": changed,
        });
        print_line(&serde_json::to_string_pretty(&doc).unwrap_or_default());
    }
    Ok(if check && !changed.is_empty() {
        exit::FINDINGS
    } else {
        exit::OK
    })
}
