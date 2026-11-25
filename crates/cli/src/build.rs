//! `totalfloat import plan.tfs`: build a new schedule from a schedule text
//! file (ARCHITECTURE.md, Schedule as text), through the same path as `new --edits`.

use std::path::Path;

use totalfloat_text::{At, Compiled, Diagnostic};
use totalfloat_xer::RawXer;

use crate::edit;
use crate::error::{CliError, Problem, exit};
use crate::loaded::Loaded;
use crate::new;
use crate::output::{create_new, print_line, refuse_existing};

/// Whether `input` is a schedule text file: named `.tfs`, or starting with
/// the version line.
pub fn is_text(input: &Path) -> bool {
    if input
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("tfs"))
    {
        return true;
    }
    std::fs::read(input).is_ok_and(|bytes| {
        let start = bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(&bytes);
        let start = start.trim_ascii_start();
        start.starts_with(totalfloat_text::HEADER.as_bytes())
    })
}

/// A problem at a place in the text, as `--json` reports it.
fn placed_problem(
    file: &str,
    code: &'static str,
    message: String,
    hint: String,
    at: At,
) -> Problem {
    Problem {
        edit: None,
        source: format!("{file} line {}", at.line),
        code,
        message,
        hint,
        depends_on: None,
        line: Some(at.line),
        column: Some(at.column),
    }
}

/// The error for problems found in the text, printed compiler-style on
/// stderr unless `json`, where they're listed in the error instead.
pub fn text_error(
    file: &str,
    source: &str,
    mut diagnostics: Vec<Diagnostic>,
    json: bool,
) -> CliError {
    diagnostics.sort_by_key(|d| d.at);
    let count = diagnostics.len();
    let mut error = CliError::new(
        "text-refused",
        format!(
            "{file} has {count} problem{}, so nothing was built",
            if count == 1 { "" } else { "s" }
        ),
        "fix each problem at its line, then run again",
    );
    if json {
        error.problems = diagnostics
            .into_iter()
            .map(|d| placed_problem(file, d.code, d.message, d.hint, d.at))
            .collect();
    } else {
        for diagnostic in &diagnostics {
            eprintln!("{}", diagnostic.render(source));
        }
    }
    error
}

/// Reads and compiles a text file.
///
/// # Errors
///
/// `text-refused` with every problem in the text.
pub fn compile(file: &str, source: &str, json: bool) -> Result<Compiled, CliError> {
    let doc =
        totalfloat_text::parse(source, file).map_err(|d| text_error(file, source, d, json))?;
    totalfloat_text::compile(&doc, file).map_err(|d| text_error(file, source, d, json))
}

/// The refused edits as problems at the lines they came from.
fn refusals(file: &str, compiled: &Compiled, problems: &[Problem]) -> Vec<Diagnostic> {
    problems
        .iter()
        .map(|p| {
            let at = p
                .edit
                .and_then(|n| compiled.edits.get(n - 1))
                .map_or_else(At::default, |placed| placed.at);
            let hint = match p.depends_on.and_then(|n| compiled.edits.get(n - 1)) {
                Some(needed) => format!("it needs line {}, which was refused", needed.at.line),
                None => p.hint.clone(),
            };
            Diagnostic {
                code: p.code,
                message: p.message.clone(),
                hint,
                file: file.to_owned(),
                at,
            }
        })
        .collect()
}

/// Builds the schedule a compiled text describes. Its problems and the
/// edits' refusals are reported together, in line order, so one run shows
/// every mistake that reading didn't stop at.
///
/// # Errors
///
/// `text-refused` with every problem at its line, or what `new` refuses.
pub fn build(
    file: &str,
    source: &str,
    compiled: &Compiled,
    json: bool,
) -> Result<(RawXer, Vec<String>, Vec<String>), CliError> {
    let project = &compiled.project;
    let data_date = project.data_date.to_string();
    let must_finish = project.must_finish.map(|d| d.to_string());
    let spec = new::Spec {
        code: &project.code,
        name: &project.name,
        data_date: &data_date,
        must_finish: must_finish.as_deref(),
        calendar: Some(&project.calendar),
    };
    let raw = new::start(&spec)?;
    let mut problems = compiled.problems.clone();
    let mut built = (raw, Vec::new(), Vec::new());
    if !compiled.edits.is_empty() {
        let edits: Vec<_> = compiled.edits.iter().map(|p| p.edit.clone()).collect();
        let sources: Vec<String> = compiled
            .edits
            .iter()
            .map(|p| format!("{file} line {}", p.at.line))
            .collect();
        let loaded = Loaded::new(built.0);
        match edit::report_from(
            file,
            &loaded,
            (&edits, &sources),
            &[],
            edit::Options::default(),
        ) {
            Ok(done) => {
                let (applied, warnings) = done.report.notes();
                built = (done.raw, applied, warnings);
            }
            Err(error) if !error.problems.is_empty() => {
                problems.extend(refusals(file, compiled, &error.problems));
                return Err(text_error(file, source, problems, json));
            }
            // A failure of the whole batch most likely follows from the
            // text's own problems, so those are what to fix first.
            Err(_) if !problems.is_empty() => {
                return Err(text_error(file, source, problems, json));
            }
            Err(error) => return Err(error),
        }
    }
    if problems.is_empty() {
        Ok(built)
    } else {
        Err(text_error(file, source, problems, json))
    }
}

/// Runs `import` on a schedule text file.
///
/// # Errors
///
/// `file-not-readable`, `text-refused` with every problem at its line, or
/// what writing the output refuses.
pub fn run(input: &Path, output: &Path, json: bool) -> Result<u8, CliError> {
    refuse_existing(output)?;
    let source = std::fs::read_to_string(input).map_err(|error| {
        CliError::new(
            "file-not-readable",
            format!("can't read {}: {error}", input.display()),
            "check the path and that the file is text",
        )
    })?;
    let file = input.display().to_string();
    let compiled = compile(&file, &source, json)?;
    let (raw, applied, warnings) = build(&file, &source, &compiled, json)?;
    create_new(&raw, output)?;
    let report = new::report(output, &Loaded::new(raw), applied, warnings);
    if json {
        print_line(&serde_json::to_string_pretty(&report).unwrap_or_default());
    } else {
        print_line(&new::human(&report));
    }
    Ok(exit::OK)
}
