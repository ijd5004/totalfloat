//! MCP history tools: each runs the same `history` command as the CLI, with
//! its JSON as the answer, recorded as the client's work, marked as an
//! agent's.

use std::path::PathBuf;

use schemars::JsonSchema;
use serde::Deserialize;

use crate::error::CliError;
use crate::history::{Action, as_agent, as_person, run};
use crate::output::captured;

/// Runs a history command as `client`, and gives its JSON document.
///
/// # Errors
///
/// The command's own error, or `tool-failed` if it printed no document.
pub fn answer_of(action: &Action, client: &str) -> Result<(u8, serde_json::Value), CliError> {
    document(captured(|| as_agent(client, || run(action))))
}

/// Runs a history command as a person, as `serve` does for the person using
/// the app, and gives its JSON document.
///
/// # Errors
///
/// The command's own error, or `tool-failed` if it printed no document.
pub fn answer_as_person(action: &Action) -> Result<(u8, serde_json::Value), CliError> {
    document(captured(|| as_person(|| run(action))))
}

/// A history command's exit code and the JSON document it printed.
fn document(
    (result, printed): (Result<u8, CliError>, String),
) -> Result<(u8, serde_json::Value), CliError> {
    let code = result?;
    let doc = serde_json::from_str(&printed).map_err(|e| {
        CliError::new(
            "tool-failed",
            format!("the history command gave no document: {e}"),
            "this is a bug; the store is unaffected",
        )
    })?;
    Ok((code, doc))
}

fn store(path: &str) -> PathBuf {
    PathBuf::from(path)
}

/// Needs a reason, as every change an agent records does.
///
/// # Errors
///
/// `reason-needed`.
pub fn reason(given: Option<String>) -> Result<String, CliError> {
    given.filter(|r| !r.trim().is_empty()).ok_or_else(|| {
        CliError::new(
            "reason-needed",
            "an agent's change to a history needs a reason",
            "say why in `reason`, so whoever reviews it knows",
        )
    })
}

/// `history_init`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InitParams {
    /// The XER file to start from.
    pub file: String,
    /// Where to make the store; default beside the file, `plant.tfdb`.
    pub store: Option<String>,
    /// Why, such as "Contract baseline as submitted".
    pub reason: Option<String>,
}

impl InitParams {
    /// The command.
    ///
    /// # Errors
    ///
    /// `reason-needed`.
    pub fn action(self) -> Result<Action, CliError> {
        Ok(Action::Init {
            file: PathBuf::from(self.file),
            store: self.store.map(PathBuf::from),
            message: Some(reason(self.reason)?),
            author: None,
            json: true,
        })
    }
}

/// `history_log`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LogParams {
    /// The store.
    pub store: String,
    /// The branch; default the checked-out one.
    pub branch: Option<String>,
    /// At most this many; default 20.
    pub limit: Option<usize>,
    /// Every changeset, proposals and rejected ones included.
    #[serde(default)]
    pub all: bool,
}

impl LogParams {
    /// The command.
    pub fn action(self) -> Action {
        Action::Log {
            store: store(&self.store),
            branch: self.branch,
            limit: self.limit.unwrap_or(20).min(200),
            all: self.all,
            json: true,
        }
    }
}

/// `history_show`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ShowParams {
    /// The store.
    pub store: String,
    /// A branch, baseline, proposal or changeset ID.
    pub at: String,
}

impl ShowParams {
    /// The command.
    pub fn action(self) -> Action {
        Action::Show {
            store: store(&self.store),
            at: self.at,
            json: true,
        }
    }
}

/// `history_diff`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DiffParams {
    /// The store.
    pub store: String,
    /// The earlier point; default the head's parent.
    pub from: Option<String>,
    /// The later point; default the checked-out head.
    pub to: Option<String>,
    /// At most this many of each kind per project; 0 lists every one.
    /// Default 25. The counts are always complete.
    #[serde(default)]
    pub limit: Option<usize>,
}

impl DiffParams {
    /// The command.
    pub fn action(self) -> Action {
        Action::Diff {
            store: store(&self.store),
            from: self.from,
            to: self.to,
            limit: self.limit.unwrap_or(crate::history::DIFF_LIMIT),
            json: true,
        }
    }
}

/// `history_revert`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RevertParams {
    /// The store.
    pub store: String,
    /// The changeset to take out; default the checked-out head (undo).
    pub at: Option<String>,
    /// Why; required.
    pub reason: Option<String>,
}

impl RevertParams {
    /// The command.
    ///
    /// # Errors
    ///
    /// `reason-needed`.
    pub fn action(self) -> Result<Action, CliError> {
        let message = Some(reason(self.reason)?);
        Ok(match self.at {
            Some(at) => Action::Revert {
                store: store(&self.store),
                at,
                message,
                author: None,
                json: true,
            },
            None => Action::Undo {
                store: store(&self.store),
                message,
                author: None,
                json: true,
            },
        })
    }
}

/// What to do with a branch or baseline.
#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RefAction {
    /// Make one, at `at` or the checked-out head.
    Create,
    /// Check a branch out.
    Switch,
    /// List them.
    List,
    /// Remove the name; the changesets stay.
    Delete,
}

/// `history_branch` and `history_baseline`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RefParams {
    /// The store.
    pub store: String,
    /// What to do.
    pub action: RefAction,
    /// The name; needed except to list.
    pub name: Option<String>,
    /// Where a new one starts: a branch, baseline or changeset ID.
    pub at: Option<String>,
}

fn named(name: Option<String>) -> Result<String, CliError> {
    name.ok_or_else(|| CliError::new("name-needed", "this needs a name", "give `name`"))
}

impl RefParams {
    /// The branch command.
    ///
    /// # Errors
    ///
    /// `name-needed`.
    pub fn branch_action(self) -> Result<Action, CliError> {
        let store = store(&self.store);
        Ok(match self.action {
            RefAction::List => Action::Branches { store, json: true },
            RefAction::Switch => Action::Switch {
                store,
                name: named(self.name)?,
                json: true,
            },
            RefAction::Create | RefAction::Delete => Action::Branch {
                store,
                name: named(self.name)?,
                at: self.at,
                delete: matches!(self.action, RefAction::Delete),
                json: true,
            },
        })
    }

    /// The baseline command.
    ///
    /// # Errors
    ///
    /// `name-needed`, or `wrong-ref-kind` for switch.
    pub fn baseline_action(self) -> Result<Action, CliError> {
        let store = store(&self.store);
        Ok(match self.action {
            RefAction::List => Action::Baselines { store, json: true },
            RefAction::Switch => {
                return Err(CliError::new(
                    "wrong-ref-kind",
                    "a baseline can't be checked out",
                    "make a branch from it with `history_branch`",
                ));
            }
            RefAction::Create | RefAction::Delete => Action::Baseline {
                store,
                name: named(self.name)?,
                at: self.at,
                delete: matches!(self.action, RefAction::Delete),
                json: true,
            },
        })
    }
}

/// `history_merge`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MergeParams {
    /// The store.
    pub store: String,
    /// The branch to merge into the checked-out one.
    pub branch: String,
    /// Why; required unless `dry_run`.
    pub reason: Option<String>,
    /// Say what would happen, and record nothing.
    #[serde(default)]
    pub dry_run: bool,
}

impl MergeParams {
    /// The command.
    ///
    /// # Errors
    ///
    /// `reason-needed`.
    pub fn action(self) -> Result<Action, CliError> {
        let message = if self.dry_run {
            self.reason
        } else {
            Some(reason(self.reason)?)
        };
        Ok(Action::Merge {
            store: store(&self.store),
            branch: self.branch,
            message,
            author: None,
            dry_run: self.dry_run,
            json: true,
        })
    }
}

/// A tool that takes only the store.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StoreParams {
    /// The store.
    pub store: String,
}

impl StoreParams {
    /// The `history proposals` command.
    pub fn proposals_action(self) -> Action {
        Action::Proposals {
            store: store(&self.store),
            json: true,
        }
    }
}

/// Accept or reject.
#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    /// Put it on its branch.
    Accept,
    /// Turn it down; it stays in the store.
    Reject,
}

/// `history_review`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReviewParams {
    /// The store.
    pub store: String,
    /// The proposal, by ID or `proposal/…` name.
    pub proposal: String,
    /// Accept or reject.
    pub decision: Decision,
    /// Why; required to reject.
    pub reason: Option<String>,
}

impl ReviewParams {
    /// The command.
    pub fn action(self) -> Action {
        let store = store(&self.store);
        match self.decision {
            Decision::Accept => Action::Accept {
                store,
                at: self.proposal,
                message: self.reason,
                author: None,
                json: true,
            },
            Decision::Reject => Action::Reject {
                store,
                at: self.proposal,
                message: self.reason,
                author: None,
                json: true,
            },
        }
    }
}

/// `history_export`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExportParams {
    /// The store.
    pub store: String,
    /// A branch, baseline or changeset ID; default the checked-out head.
    pub at: Option<String>,
    /// A new XER file; existing paths are refused.
    pub path: String,
}

impl ExportParams {
    /// The command.
    pub fn action(self) -> Action {
        Action::Export {
            store: store(&self.store),
            at: self.at,
            output: PathBuf::from(self.path),
            json: true,
        }
    }
}

/// `history_verify`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VerifyParams {
    /// The store.
    pub store: String,
    /// Also replay each edit and compare with the file recorded.
    #[serde(default)]
    pub replay: bool,
}

impl VerifyParams {
    /// The command.
    pub fn action(self) -> Action {
        Action::Verify {
            store: store(&self.store),
            replay: self.replay,
            json: true,
        }
    }
}

/// `history_import`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ImportParams {
    /// The store.
    pub store: String,
    /// The new XER export.
    pub file: String,
    /// Why; required.
    pub reason: Option<String>,
    /// Record it even if it shares no project with the branch.
    #[serde(default)]
    pub force: bool,
}

impl ImportParams {
    /// The command.
    ///
    /// # Errors
    ///
    /// `reason-needed`.
    pub fn action(self) -> Result<Action, CliError> {
        Ok(Action::Import {
            store: store(&self.store),
            file: PathBuf::from(self.file),
            message: Some(reason(self.reason)?),
            author: None,
            force: self.force,
            json: true,
        })
    }
}
