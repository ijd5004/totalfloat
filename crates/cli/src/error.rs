//! Errors in the agent-first format: a stable code, a message and a hint.

use serde::Serialize;

/// Exit codes (docs/ARCHITECTURE.md "CLI conventions").
pub mod exit {
    /// Success.
    pub const OK: u8 = 0;
    /// The command failed.
    pub const ERROR: u8 = 1;
    /// A validation finding (e.g. a round-trip that isn't byte-identical).
    pub const FINDINGS: u8 = 3;
}

/// A command failure an agent can act on.
#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct CliError {
    /// Stable, machine-readable code, e.g. `file-not-readable`.
    pub code: &'static str,
    /// What went wrong.
    pub message: String,
    /// What to try next.
    pub hint: String,
    /// Each problem, when one failure has several, such as the refused edits
    /// of a batch.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub problems: Vec<Problem>,
}

/// One of several problems behind an error.
#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct Problem {
    /// The edit's place in the batch, counting from 1, if it got that far.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edit: Option<usize>,
    /// Where it came from: a flag and its text, `edit 3` for an item of a
    /// list, or `line 4` for a table row.
    pub source: String,
    /// Stable code, such as `no-such-activity`.
    pub code: &'static str,
    /// What's wrong.
    pub message: String,
    /// What to do about it.
    pub hint: String,
    /// The refused edit this one needed, by its place in the batch.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub depends_on: Option<usize>,
    /// The line it came from, from 1, for a schedule text file.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
    /// The column it came from, from 1, for a schedule text file.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column: Option<usize>,
}

impl CliError {
    /// A new error.
    pub fn new(code: &'static str, message: impl Into<String>, hint: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            hint: hint.into(),
            problems: Vec::new(),
        }
    }
}

/// The `--json` error document.
#[derive(Serialize)]
struct ErrorDocument<'a> {
    schema: &'static str,
    error: &'a CliError,
}

/// Prints `error` in the requested format: JSON on stdout (so agents read one
/// stream), or `error[code]: message` plus a hint on stderr.
pub fn report(error: &CliError, json: bool) {
    if json {
        let doc = ErrorDocument {
            schema: "totalfloat.error/1",
            error,
        };
        crate::output::print_line(&serde_json::to_string_pretty(&doc).unwrap_or_default());
    } else {
        eprintln!("error[{}]: {}", error.code, error.message);
        match error.problems.as_slice() {
            [one] if one.code == error.code && one.message == error.message => {
                eprintln!("  from: {}", one.source);
            }
            several => {
                for problem in several {
                    eprintln!("  {}: {}", problem.source, problem.message);
                    let needed = problem
                        .depends_on
                        .and_then(|edit| several.iter().find(|p| p.edit == Some(edit)));
                    match needed {
                        Some(needed) => eprintln!("    needs {}, which was refused", needed.source),
                        None => eprintln!("    hint: {}", problem.hint),
                    }
                }
            }
        }
        eprintln!("  hint: {}", error.hint);
    }
}

/// Parses a date argument.
pub fn parse_date(text: &str) -> Result<totalfloat_model::DateTime, CliError> {
    text.parse().map_err(|_| {
        CliError::new(
            "invalid-date",
            format!("{text} isn't a valid date"),
            "use the form YYYY-MM-DD HH:MM",
        )
    })
}

/// Parses a day argument.
pub fn parse_day(text: &str) -> Result<totalfloat_model::Date, CliError> {
    text.parse().map_err(|_| {
        CliError::new(
            "invalid-date",
            format!("{text} isn't a valid date"),
            "use the form YYYY-MM-DD",
        )
    })
}

/// Reads an XER file into the raw codec, mapping failures to CLI errors.
/// Commands that also read history stores use [`crate::location::read`].
pub fn read_xer(path: &std::path::Path) -> Result<totalfloat_xer::RawXer, CliError> {
    let bytes = std::fs::read(path).map_err(|e| {
        CliError::new(
            "file-not-readable",
            format!("can't read {}: {e}", path.display()),
            "check the path and that the file is readable",
        )
    })?;
    read_xer_bytes(bytes, path)
}

/// Reads XER bytes, such as a file kept in a history store, naming `path` in
/// failures.
pub fn read_xer_bytes(
    bytes: Vec<u8>,
    path: &std::path::Path,
) -> Result<totalfloat_xer::RawXer, CliError> {
    totalfloat_xer::read(bytes).map_err(|e| {
        CliError::new(
            "not-an-xer",
            format!("{} is not a readable XER file: {e}", path.display()),
            "pass a Primavera P6 .xer export; the first line must start with ERMHDR",
        )
    })
}
