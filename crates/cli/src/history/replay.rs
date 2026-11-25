//! Replaying recorded edits: each changeset's commands, applied in order with
//! the same edit path as `edit`, recalculating as they go. Reverts, merges
//! and proposals use it to rebuild a branch on another point.

use totalfloat_edit::{Commands, Missing, NameKind, rename};
use totalfloat_store::{Changeset, Kind};
use totalfloat_xer::RawXer;

use crate::edit::{self, Edited, Options};
use crate::error::{CliError, Problem};
use crate::loaded::Loaded;

/// A recorded changeset that couldn't be replayed, and why.
#[derive(Debug, Clone)]
pub struct Conflict {
    /// The changeset, by short ID.
    pub changeset: String,
    /// What went wrong.
    pub problems: Vec<Problem>,
}

impl Conflict {
    fn whole(changeset: &Changeset, code: &'static str, message: String, hint: &str) -> Self {
        Self {
            changeset: changeset.id.short(),
            problems: vec![Problem {
                edit: None,
                source: format!("changeset {}", changeset.id.short()),
                code,
                message,
                hint: hint.to_owned(),
                depends_on: None,
                line: None,
                column: None,
            }],
        }
    }
}

/// The conflict for names a changeset used that are gone here.
fn gone(changeset: &Changeset, missing: &[Missing]) -> Conflict {
    let problems = missing
        .iter()
        .map(|m| {
            let what = match m.name.kind {
                NameKind::Activity => "activity",
                NameKind::Wbs => "WBS node",
                NameKind::Calendar => "calendar",
                NameKind::Resource => "resource",
                NameKind::Role => "role",
            };
            Problem {
                edit: None,
                source: format!("changeset {}", changeset.id.short()),
                code: "missing-here",
                message: format!(
                    "{what} {} (ID {} in project {}) was deleted here",
                    m.name.name, m.name.id, m.name.project
                ),
                hint: "the changeset needs it; take it out, or put the work back first".to_owned(),
                depends_on: None,
                line: None,
                column: None,
            }
        })
        .collect();
    Conflict {
        changeset: changeset.id.short(),
        problems,
    }
}

/// The commands an edit changeset holds.
fn commands_of(changeset: &Changeset) -> Result<Commands, Conflict> {
    if changeset.kind != Kind::Edit {
        return Err(Conflict::whole(
            changeset,
            "not-replayable",
            format!(
                "{} is {} {}, which holds a whole file rather than edits",
                changeset.id.short(),
                if changeset.kind == Kind::Import {
                    "an"
                } else {
                    "a"
                },
                changeset.kind
            ),
            "only edits can be replayed onto another point",
        ));
    }
    serde_json::from_str(&changeset.commands).map_err(|error| {
        Conflict::whole(
            changeset,
            "store-damaged",
            format!("its commands can't be read: {error}"),
            "`history verify` checks the store",
        )
    })
}

/// Applies each changeset's commands in order to `start`, stopping at the
/// first that no longer applies.
///
/// # Errors
///
/// The changeset that failed, with each of its refused edits.
pub fn replay(start: RawXer, changesets: &[Changeset]) -> Result<RawXer, Conflict> {
    let mut raw = start;
    for changeset in changesets {
        let commands = commands_of(changeset)?;
        let options = Options::from_document(&commands.options);
        let label = format!("replay of {}", changeset.id.short());
        let loaded = Loaded::new(raw);
        // Names go by their source IDs, so a rename here doesn't lose them.
        let edits = rename(&commands.edits, &commands.resolved, &loaded.schedules)
            .map_err(|missing| gone(changeset, &missing))?;
        let batch = (edits.as_slice(), &[][..]);
        match edit::report_from(&label, &loaded, batch, &[], options) {
            Ok(Edited { raw: next, .. }) => raw = next,
            Err(error) => {
                let problems = if error.problems.is_empty() {
                    vec![Problem {
                        edit: None,
                        source: format!("changeset {}", changeset.id.short()),
                        code: error.code,
                        message: error.message,
                        hint: error.hint,
                        depends_on: None,
                        line: None,
                        column: None,
                    }]
                } else {
                    error.problems
                };
                return Err(Conflict {
                    changeset: changeset.id.short(),
                    problems,
                });
            }
        }
    }
    Ok(raw)
}

/// A conflict as the error a command reports. A changeset that can't be
/// replayed at all keeps its own reason; one whose edits no longer apply
/// lists them.
pub fn conflict_error(code: &'static str, what: &str, conflict: Conflict) -> CliError {
    let whole = matches!(
        conflict.problems.as_slice(),
        [one] if matches!(one.code, "not-replayable" | "store-damaged")
    );
    let mut error = if whole {
        let one = &conflict.problems[0];
        CliError::new(
            one.code,
            format!("{what}: {}", one.message),
            one.hint.clone(),
        )
    } else {
        CliError::new(
            code,
            format!(
                "{what}: changeset {} no longer applies, so nothing was recorded",
                conflict.changeset
            ),
            "each problem says what it needed; undo the changesets that depend on it first",
        )
    };
    error.problems = conflict.problems;
    error
}

/// The changeset a revert took out, if `changeset` is a revert.
pub fn reverts(changeset: &Changeset) -> Option<String> {
    if changeset.kind != Kind::Revert {
        return None;
    }
    let commands: serde_json::Value = serde_json::from_str(&changeset.commands).ok()?;
    commands["reverts"].as_str().map(str::to_owned)
}

/// The changesets left to replay once each revert in the list and what it
/// took out cancel, newest first, so undoing an undo brings its edit back.
/// A revert of something before the list stays, and can't be replayed.
pub fn cancel_reverts(changesets: Vec<Changeset>) -> Vec<Changeset> {
    let mut cancelled = std::collections::BTreeSet::new();
    for changeset in changesets.iter().rev() {
        if cancelled.contains(changeset.id.as_str()) {
            continue;
        }
        if let Some(target) = reverts(changeset)
            && changesets.iter().any(|c| c.id.as_str() == target)
        {
            cancelled.insert(target);
            cancelled.insert(changeset.id.to_string());
        }
    }
    changesets
        .into_iter()
        .filter(|c| !cancelled.contains(c.id.as_str()))
        .collect()
}
