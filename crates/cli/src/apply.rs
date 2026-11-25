//! `edit --apply plan.tfs`: make a schedule match a schedule text file
//! (ARCHITECTURE.md, Schedule as text). The difference becomes ordinary edits, so the rest of `edit`
//! (all or nothing, dry runs, proposals, history) works the same.

use totalfloat_edit::Edit;
use totalfloat_model::Schedule;
use totalfloat_text::{At, Leaving};

use crate::build::text_error;
use crate::error::{CliError, Problem};
use crate::loaded::Loaded;

/// What to do with what the text leaves out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Choice {
    /// Refuse, listing it, until one of the others is chosen.
    #[default]
    Refuse,
    /// Keep it.
    Keep,
    /// Delete it.
    Delete,
}

/// The edits a text file asks for.
#[derive(Debug, Default)]
pub struct FromText {
    /// The edits, in a safe order.
    pub edits: Vec<Edit>,
    /// Where each came from, such as `plan.tfs line 14`.
    pub sources: Vec<String>,
    /// The place in the text each came from.
    pub places: Vec<At>,
    /// The text's data date, when it differs from the schedule's.
    pub data_date: Option<String>,
    /// Deleted and added activities that look like one renamed.
    pub warnings: Vec<String>,
}

fn one_project(loaded: &Loaded, project: Option<u64>) -> Result<&Schedule, CliError> {
    let reach: Vec<&Schedule> = loaded
        .schedules
        .iter()
        .filter(|s| project.is_none_or(|id| s.project.id.0 == id))
        .collect();
    match reach.as_slice() {
        [one] => Ok(one),
        [] => Err(CliError::new(
            "no-such-project",
            "there's no such project to apply the text to",
            "`inspect` lists the projects in the file",
        )),
        many => Err(CliError::new(
            "needs-project",
            format!(
                "the file has {} projects, and a text file holds one",
                many.len()
            ),
            "name the project: --project, or `project` over MCP",
        )),
    }
}

/// Works out the edits that make `loaded`'s project match `source`.
///
/// # Errors
///
/// `text-refused` with every problem in the text at its line, or
/// `missing-from-text` listing what the text leaves out when `choice` is
/// [`Choice::Refuse`].
pub fn from_text(
    loaded: &Loaded,
    project: Option<u64>,
    (file, source): (&str, &str),
    choice: Choice,
    json: bool,
) -> Result<FromText, CliError> {
    let schedule = one_project(loaded, project)?;
    let doc =
        totalfloat_text::parse(source, file).map_err(|d| text_error(file, source, d, json))?;
    let leaving = if choice == Choice::Delete {
        Leaving::Delete
    } else {
        Leaving::Keep
    };
    let diff = totalfloat_text::difference(schedule, &doc, file, leaving);
    if !diff.problems.is_empty() {
        return Err(text_error(file, source, diff.problems, json));
    }
    if choice == Choice::Refuse && !diff.missing.is_empty() {
        let mut error = CliError::new(
            "missing-from-text",
            format!(
                "{file} leaves out {} thing{} the schedule has",
                diff.missing.len(),
                if diff.missing.len() == 1 { "" } else { "s" }
            ),
            "say what to do with them: --delete-missing deletes them, --keep-missing keeps them",
        );
        error.problems = diff
            .missing
            .iter()
            .map(|m| Problem {
                edit: None,
                source: format!("{} {}", m.kind, m.name),
                code: "missing-from-text",
                message: format!("{file} leaves out the {} {}", m.kind, m.name),
                hint: "add it back, or choose --delete-missing or --keep-missing".to_owned(),
                depends_on: None,
                line: None,
                column: None,
            })
            .collect();
        return Err(error);
    }
    if choice == Choice::Delete && diff.missing.iter().any(|m| m.kind == "wbs") {
        let paths: Vec<&str> = diff
            .missing
            .iter()
            .filter(|m| m.kind == "wbs")
            .map(|m| m.name.as_str())
            .collect();
        return Err(CliError::new(
            "unsupported-change",
            format!(
                "{file} leaves out WBS {}, and the edit commands can't delete WBS nodes yet",
                paths.join(", ")
            ),
            "put them back in the text, or use --keep-missing",
        ));
    }
    let sources = diff
        .edits
        .iter()
        .map(|p| {
            if p.at.line == 0 {
                format!("{file} (left out)")
            } else {
                format!("{file} line {}", p.at.line)
            }
        })
        .collect();
    Ok(FromText {
        places: diff.edits.iter().map(|p| p.at).collect(),
        edits: diff.edits.into_iter().map(|p| p.edit).collect(),
        sources,
        data_date: diff.data_date.map(|d| d.to_string()),
        warnings: diff.warnings,
    })
}

/// Puts each refused text edit's line and column on its problem. Text edits
/// come first in the batch, so their places are the first ones.
pub fn place_problems(mut error: CliError, places: &[At]) -> CliError {
    for problem in &mut error.problems {
        if let Some(at) = problem.edit.and_then(|n| places.get(n - 1))
            && at.line > 0
        {
            problem.line = Some(at.line);
            problem.column = Some(at.column);
        }
    }
    error
}
