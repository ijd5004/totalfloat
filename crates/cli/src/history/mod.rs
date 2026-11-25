//! `totalfloat history`: a schedule's history in a store (ARCHITECTURE.md, History and changesets), recording
//! who changed what, why and when, with every version of the file.

mod baseline;
mod branch;
mod clock;
mod compare;
mod import;
mod merge;
mod proposal;
mod record;
mod replay;
mod revert;
mod verify;
mod words;

use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::{Value, json};
use totalfloat_store::{
    Changeset, ChangesetId, Kind, Moving, NewChangeset, RefKind, State, Store, StoreError,
};

use crate::error::{CliError, exit, read_xer};
use crate::loaded::Loaded;
use crate::output::{create_new_bytes, print_line};

pub use clock::{as_agent, as_person, author, now};
pub use record::{
    Asked, Head, Pin, Recorded, head_of, pinned, proposal_branch, read_head, read_snapshot,
    record_change, record_edit,
};

/// Items of each kind `history diff` lists per project unless told otherwise,
/// so an answer stays short enough for a model's context.
pub const DIFF_LIMIT: usize = 25;

/// The branch a new history starts on.
pub const FIRST_BRANCH: &str = "main";

/// What `history` does.
#[derive(Debug, clap::Subcommand)]
pub enum Action {
    /// Start a history from an XER file, in a store beside it
    /// (`plant.xer` gives `plant.tfdb`) on branch `main`.
    Init {
        /// The XER file.
        file: PathBuf,
        /// Where to make the store instead; it mustn't exist.
        #[arg(long)]
        store: Option<PathBuf>,
        /// Why, such as `Contract baseline as submitted`.
        #[arg(short, long)]
        message: Option<String>,
        /// Who; default `TOTALFLOAT_AUTHOR`, else the account's user name.
        #[arg(long)]
        author: Option<String>,
        /// Print JSON (schema `totalfloat.changeset/1`).
        #[arg(long)]
        json: bool,
    },
    /// List changesets, newest first.
    Log {
        /// The store.
        store: PathBuf,
        /// The branch; default the checked-out one.
        #[arg(long)]
        branch: Option<String>,
        /// At most this many.
        #[arg(long, default_value_t = 20)]
        limit: usize,
        /// Every changeset in the store, newest first, including proposals
        /// and rejected ones, instead of one branch's line.
        #[arg(long, conflicts_with = "branch")]
        all: bool,
        /// Print JSON (schema `totalfloat.history-log/1`).
        #[arg(long)]
        json: bool,
    },
    /// Show one changeset in full: its commands and its schedule impact.
    Show {
        /// The store.
        store: PathBuf,
        /// A branch, baseline or changeset ID (6 or more characters).
        at: String,
        /// Print JSON (schema `totalfloat.changeset/1`).
        #[arg(long)]
        json: bool,
    },
    /// Take a changeset back out of the checked-out branch, as a new
    /// changeset. Later edits are replayed without it; nothing is deleted.
    Revert {
        /// The store.
        store: PathBuf,
        /// The changeset, by ID (6 or more characters).
        at: String,
        /// Why.
        #[arg(short, long)]
        message: Option<String>,
        /// Who; default `TOTALFLOAT_AUTHOR`, else the account's user name.
        #[arg(long)]
        author: Option<String>,
        /// Print JSON (schema `totalfloat.history-revert/1`).
        #[arg(long)]
        json: bool,
    },
    /// Take the checked-out branch's latest changeset back out. Undo again to
    /// undo the undo.
    Undo {
        /// The store.
        store: PathBuf,
        /// Why.
        #[arg(short, long)]
        message: Option<String>,
        /// Who; default `TOTALFLOAT_AUTHOR`, else the account's user name.
        #[arg(long)]
        author: Option<String>,
        /// Print JSON (schema `totalfloat.history-revert/1`).
        #[arg(long)]
        json: bool,
    },
    /// Make a branch for a what-if, at the checked-out head or `--at` a point,
    /// or delete one with `--delete`. Nothing is copied.
    Branch {
        /// The store.
        store: PathBuf,
        /// The branch's name, such as `what-if/steel-late`.
        name: String,
        /// Where it starts: a branch, baseline or changeset ID.
        #[arg(long, conflicts_with = "delete")]
        at: Option<String>,
        /// Delete the branch instead. Its changesets stay in the store.
        #[arg(long)]
        delete: bool,
        /// Print JSON (schema `totalfloat.history-branch/1`).
        #[arg(long)]
        json: bool,
    },
    /// Check out a branch, so edits record there.
    Switch {
        /// The store.
        store: PathBuf,
        /// The branch.
        name: String,
        /// Print JSON (schema `totalfloat.history-switch/1`).
        #[arg(long)]
        json: bool,
    },
    /// List branches, marking the checked-out one.
    Branches {
        /// The store.
        store: PathBuf,
        /// Print JSON (schema `totalfloat.history-branches/1`).
        #[arg(long)]
        json: bool,
    },
    /// What changed between two points, and the changesets between them.
    /// Default: the checked-out head against its parent.
    Diff {
        /// The store.
        store: PathBuf,
        /// The earlier point: a branch, baseline or changeset ID.
        from: Option<String>,
        /// The later point; default the checked-out head.
        to: Option<String>,
        /// At most this many of each kind per project (changed, added and
        /// deleted activities, links, and so on); 0 lists every one. The
        /// counts are always complete.
        #[arg(long, default_value_t = DIFF_LIMIT)]
        limit: usize,
        /// Print JSON (schema `totalfloat.history-diff/1`).
        #[arg(long)]
        json: bool,
    },
    /// Bring a new export from P6 in as the next point on the checked-out
    /// branch, and show what changed since the last.
    Import {
        /// The store.
        store: PathBuf,
        /// The new XER export.
        file: PathBuf,
        /// Why, such as `Update 07 from P6`.
        #[arg(short, long)]
        message: Option<String>,
        /// Who; default `TOTALFLOAT_AUTHOR`, else the account's user name.
        #[arg(long)]
        author: Option<String>,
        /// Record it even if it shares no project with the branch.
        #[arg(long)]
        force: bool,
        /// Print JSON (schema `totalfloat.history-import/1`).
        #[arg(long)]
        json: bool,
    },
    /// Name a point as a baseline, at the checked-out head or `--at` a point,
    /// or delete one with `--delete`. A baseline never moves; compare with
    /// `--baseline STORE@NAME`.
    Baseline {
        /// The store.
        store: PathBuf,
        /// The baseline's name, such as `contract`.
        name: String,
        /// The point: a branch, baseline or changeset ID.
        #[arg(long, conflicts_with = "delete")]
        at: Option<String>,
        /// Delete the baseline's name instead. The point stays.
        #[arg(long)]
        delete: bool,
        /// Print JSON (schema `totalfloat.history-baseline/1`).
        #[arg(long)]
        json: bool,
    },
    /// List baselines and the points they name.
    Baselines {
        /// The store.
        store: PathBuf,
        /// Print JSON (schema `totalfloat.history-baselines/1`).
        #[arg(long)]
        json: bool,
    },
    /// Bring another branch's edits onto the checked-out one, replaying them.
    /// Refused, listing every conflict, when a changeset can't be replayed or
    /// both branches changed the same thing differently.
    Merge {
        /// The store.
        store: PathBuf,
        /// The branch to merge in.
        branch: String,
        /// Why.
        #[arg(short, long)]
        message: Option<String>,
        /// Who; default `TOTALFLOAT_AUTHOR`, else the account's user name.
        #[arg(long)]
        author: Option<String>,
        /// Say what would happen, and record nothing.
        #[arg(long)]
        dry_run: bool,
        /// Print JSON (schema `totalfloat.history-merge/1`).
        #[arg(long)]
        json: bool,
    },
    /// List proposals waiting for review, with who made them and their
    /// impact.
    Proposals {
        /// The store.
        store: PathBuf,
        /// Print JSON (schema `totalfloat.history-proposals/1`).
        #[arg(long)]
        json: bool,
    },
    /// Accept a proposal onto the branch it was made for, replaying it if
    /// the branch has moved on.
    Accept {
        /// The store.
        store: PathBuf,
        /// The proposal, by ID or `proposal/…` name.
        at: String,
        /// A note on the decision.
        #[arg(short, long)]
        message: Option<String>,
        /// Who decided; default `TOTALFLOAT_AUTHOR`, else the account's user
        /// name.
        #[arg(long)]
        author: Option<String>,
        /// Print JSON (schema `totalfloat.history-review/1`).
        #[arg(long)]
        json: bool,
    },
    /// Reject a proposal, with a reason. It stays in the store.
    Reject {
        /// The store.
        store: PathBuf,
        /// The proposal, by ID or `proposal/…` name.
        at: String,
        /// Why; required.
        #[arg(short, long)]
        message: Option<String>,
        /// Who decided; default `TOTALFLOAT_AUTHOR`, else the account's user
        /// name.
        #[arg(long)]
        author: Option<String>,
        /// Print JSON (schema `totalfloat.history-review/1`).
        #[arg(long)]
        json: bool,
    },
    /// Check the store: every file and changeset against its hash, and with
    /// `--replay` that each recorded edit replays to the file recorded.
    /// Exits 1 if damaged, 3 if anything replays differently.
    Verify {
        /// The store.
        store: PathBuf,
        /// Also replay each edit on its parent's file and compare.
        #[arg(long)]
        replay: bool,
        /// Print JSON (schema `totalfloat.history-verify/1`).
        #[arg(long)]
        json: bool,
    },
    /// Write the file as it was at a point in history, byte for byte.
    Export {
        /// The store.
        store: PathBuf,
        /// A branch, baseline or changeset ID; default the checked-out branch.
        #[arg(long)]
        at: Option<String>,
        /// New output file; existing paths are refused.
        #[arg(short, long)]
        output: PathBuf,
        /// Print JSON (schema `totalfloat.history-export/1`).
        #[arg(long)]
        json: bool,
    },
}

impl Action {
    /// True if the caller asked for JSON.
    pub fn json(&self) -> bool {
        match self {
            Self::Init { json, .. }
            | Self::Log { json, .. }
            | Self::Show { json, .. }
            | Self::Revert { json, .. }
            | Self::Undo { json, .. }
            | Self::Branch { json, .. }
            | Self::Switch { json, .. }
            | Self::Branches { json, .. }
            | Self::Diff { json, .. }
            | Self::Import { json, .. }
            | Self::Baseline { json, .. }
            | Self::Baselines { json, .. }
            | Self::Merge { json, .. }
            | Self::Proposals { json, .. }
            | Self::Accept { json, .. }
            | Self::Reject { json, .. }
            | Self::Verify { json, .. }
            | Self::Export { json, .. } => *json,
        }
    }
}

/// Runs a history command.
///
/// # Errors
///
/// What the store or the file refuses, with a code and a hint.
pub fn run(action: &Action) -> Result<u8, CliError> {
    match action {
        Action::Init {
            file,
            store,
            message,
            author,
            json,
        } => init(
            file,
            store.as_deref(),
            message.as_deref(),
            author.as_deref(),
            *json,
        ),
        Action::Log {
            store,
            branch,
            limit,
            all,
            json,
        } => log(store, (branch.as_deref(), *all), *limit, *json),
        Action::Show { store, at, json } => show(store, at, *json),
        Action::Diff {
            store,
            from,
            to,
            limit,
            json,
        } => compare::run(store, (from.as_deref(), to.as_deref()), *limit, *json),
        Action::Export {
            store,
            at,
            output,
            json,
        } => export(store, at.as_deref(), output, *json),
        Action::Branch { .. }
        | Action::Switch { .. }
        | Action::Branches { .. }
        | Action::Baseline { .. }
        | Action::Baselines { .. } => names(action),
        Action::Revert { .. }
        | Action::Undo { .. }
        | Action::Import { .. }
        | Action::Merge { .. } => change(action),
        Action::Proposals { store, json } => proposal::list(store, *json),
        Action::Verify {
            store,
            replay,
            json,
        } => verify::run(store, *replay, *json),
        Action::Accept {
            store,
            at,
            message,
            author,
            json,
        } => proposal::accept(
            store,
            at,
            asked(message.as_deref(), author.as_deref()),
            *json,
        ),
        Action::Reject {
            store,
            at,
            message,
            author,
            json,
        } => proposal::reject(
            store,
            at,
            asked(message.as_deref(), author.as_deref()),
            *json,
        ),
    }
}

/// Who asked, and why.
fn asked<'a>(message: Option<&'a str>, author: Option<&'a str>) -> Asked<'a> {
    Asked {
        message,
        author,
        propose: false,
    }
}

/// Runs a command that names points: branches and baselines.
fn names(action: &Action) -> Result<u8, CliError> {
    match action {
        Action::Branch {
            store,
            name,
            at,
            delete,
            json,
        } => branch::branch(store, name, at.as_deref(), *delete, *json),
        Action::Switch { store, name, json } => branch::switch(store, name, *json),
        Action::Branches { store, json } => branch::branches(store, *json),
        Action::Baseline {
            store,
            name,
            at,
            delete,
            json,
        } => baseline::baseline(store, name, at.as_deref(), *delete, *json),
        Action::Baselines { store, json } => baseline::baselines(store, *json),
        _ => Err(misrouted()),
    }
}

/// Runs a command that records a changeset from replayed or imported work.
fn change(action: &Action) -> Result<u8, CliError> {
    match action {
        Action::Revert {
            store,
            at,
            message,
            author,
            json,
        } => revert::revert(
            store,
            Some(at),
            asked(message.as_deref(), author.as_deref()),
            *json,
        ),
        Action::Undo {
            store,
            message,
            author,
            json,
        } => revert::revert(
            store,
            None,
            asked(message.as_deref(), author.as_deref()),
            *json,
        ),
        Action::Import {
            store,
            file,
            message,
            author,
            force,
            json,
        } => import::run(
            store,
            file,
            *force,
            asked(message.as_deref(), author.as_deref()),
            *json,
        ),
        Action::Merge {
            store,
            branch,
            message,
            author,
            dry_run,
            json,
        } => {
            let merging = merge::Merging {
                other: branch,
                dry_run: *dry_run,
                asked: asked(message.as_deref(), author.as_deref()),
            };
            merge::run(store, merging, *json)
        }
        _ => Err(misrouted()),
    }
}

/// [`run`] sends each action to the one handler that takes it, so this
/// can't happen; a new action without a handler is caught by `run`'s match.
fn misrouted() -> CliError {
    CliError::new(
        "internal",
        "a history command reached the wrong handler",
        "please report this",
    )
}

/// The CLI error for a store's.
#[allow(
    clippy::needless_pass_by_value,
    reason = "taken by value so it reads as `map_err(store_error)`"
)]
pub fn store_error(error: StoreError) -> CliError {
    let message = error.to_string();
    let (code, hint) = match &error {
        StoreError::Exists(_) => ("output-exists", "choose a new path for the store"),
        StoreError::NotAStore(_) => (
            "not-a-store",
            "start one with `totalfloat history init FILE.xer`",
        ),
        StoreError::UnknownFormat(_) => (
            "unsupported-schema",
            "a newer TotalFloat may read this store",
        ),
        StoreError::Moved { .. } => (
            "branch-moved",
            "someone recorded a change first; run the command again on the new head",
        ),
        StoreError::NotFound(_) => (
            "no-such-ref",
            "`history log` shows changesets, and `history show` takes a branch, baseline or ID",
        ),
        StoreError::Ambiguous(..) | StoreError::TooShort(_) => {
            ("ambiguous-id", "give more of the changeset ID")
        }
        StoreError::Damaged(_) => (
            "store-damaged",
            "`history verify` lists what's wrong; export an earlier point with `history export --at`",
        ),
        StoreError::WrongKind { .. } => (
            "wrong-ref-kind",
            "a baseline never moves; make a branch from it to change it",
        ),
        StoreError::Current(_) => ("current-branch", "switch to another branch first"),
        StoreError::Duplicate(_) => ("already-recorded", "nothing new to record"),
        StoreError::Decided(_) => (
            "not-a-proposal",
            "someone decided it first; `history show` says how",
        ),
        StoreError::Unknown(..) | StoreError::Database(_) | StoreError::Io(_) => (
            "store-error",
            "check the store file and that it isn't open elsewhere",
        ),
    };
    CliError::new(code, message, hint)
}

/// Opens a store, with CLI errors.
pub fn open(path: &Path) -> Result<Store, CliError> {
    Store::open(path).map_err(|error| match error {
        StoreError::Io(message) => CliError::new(
            "file-not-readable",
            message,
            "check the path and that the file is readable",
        ),
        other => store_error(other),
    })
}

/// The branch to read, given or checked out, and its head.
pub fn branch_head(store: &Store, branch: Option<&str>) -> Result<(String, ChangesetId), CliError> {
    let name = match branch {
        Some(name) => name.to_owned(),
        None => store.current().map_err(store_error)?.ok_or_else(|| {
            CliError::new(
                "no-such-ref",
                "no branch is checked out",
                "name one with --branch",
            )
        })?,
    };
    let found = store.get_ref(&name).map_err(store_error)?;
    match found {
        Some(found) => Ok((name, found.target)),
        None => Err(store_error(StoreError::NotFound(name))),
    }
}

/// The changeset a point names, with unknown names (including short
/// hex-looking ones) told apart from IDs that match several, and the store's
/// branches and baselines in the hint.
///
/// # Errors
///
/// `no-such-ref`, `ambiguous-id`, or what the store refuses.
pub fn resolve_point(
    store: &Store,
    path: &Path,
    at: Option<&str>,
) -> Result<ChangesetId, CliError> {
    let Some(name) = at else {
        return Ok(branch_head(store, None)?.1);
    };
    match store.resolve(name) {
        Ok(id) => Ok(id),
        Err(StoreError::NotFound(_) | StoreError::TooShort(_)) => Err(CliError::new(
            "no-such-ref",
            format!("{} has nothing called {name}", path.display()),
            known_points(store)?,
        )),
        Err(other) => Err(store_error(other)),
    }
}

/// The names a store's points go by, for a hint.
fn known_points(store: &Store) -> Result<String, CliError> {
    let refs = store.refs().map_err(store_error)?;
    let names = |kind: RefKind| {
        refs.iter()
            .filter(|r| r.kind == kind)
            .map(|r| r.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    };
    let mut parts = vec![format!("branches: {}", names(RefKind::Branch))];
    let baselines = names(RefKind::Baseline);
    if !baselines.is_empty() {
        parts.push(format!("baselines: {baselines}"));
    }
    Ok(format!(
        "{}; or a changeset ID from `history log`",
        parts.join("; ")
    ))
}

/// The changeset a point names and the file it holds.
///
/// # Errors
///
/// As [`resolve_point`], or a damaged file.
pub fn snapshot_at(
    store: &Store,
    path: &Path,
    at: Option<&str>,
) -> Result<(Changeset, Vec<u8>), CliError> {
    let id = resolve_point(store, path, at)?;
    let changeset = store.changeset(&id).map_err(store_error)?;
    let bytes = store.blob(&changeset.snapshot).map_err(store_error)?;
    Ok((changeset, bytes))
}

/// The store a file's history goes in by default: `plant.xer` gives
/// `plant.tfdb`.
fn store_beside(file: &Path) -> PathBuf {
    file.with_extension("tfdb")
}

/// What an import holds: each project, by code, with its activity count.
fn import_report(loaded: &Loaded) -> Value {
    let projects: Vec<Value> = loaded
        .schedules
        .iter()
        .map(|s| {
            json!({
                "id": s.project.id.0,
                "code": s.project.short_name,
                "activities": s.activities.len(),
            })
        })
        .collect();
    json!({ "projects": projects })
}

fn init(
    file: &Path,
    store: Option<&Path>,
    message: Option<&str>,
    who: Option<&str>,
    json: bool,
) -> Result<u8, CliError> {
    // A file the reader refuses never starts a history.
    let loaded = Loaded::new(read_xer(file)?);
    let created = now()?;
    let path = store.map_or_else(|| store_beside(file), Path::to_path_buf);
    let mut opened = Store::create(&path).map_err(store_error)?;
    let (author, author_kind) = author(who);
    let name = file
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    let new = NewChangeset {
        parent: None,
        second_parent: None,
        kind: Kind::Import,
        state: State::Accepted,
        author,
        author_kind,
        message: message.unwrap_or_default().to_owned(),
        created,
        tool_version: env!("CARGO_PKG_VERSION").to_owned(),
        commands: json!({ "import": { "file": name } }).to_string(),
        skipped: "[]".to_owned(),
        report: import_report(&loaded).to_string(),
        snapshot: loaded.raw.source().to_vec(),
    };
    let moving = Moving {
        name: FIRST_BRANCH,
        kind: RefKind::Branch,
        expected: None,
    };
    let recorded = opened
        .record(&new, Some(moving))
        .and_then(|c| opened.set_current(FIRST_BRANCH).map(|()| c));
    let recorded = match recorded {
        Ok(recorded) => recorded,
        Err(error) => {
            drop(opened);
            let _ = std::fs::remove_file(&path);
            return Err(store_error(error));
        }
    };
    if json {
        print_line(&pretty(&changeset_doc(&opened, &recorded)?));
    } else {
        print_line(&format!(
            "Started history for {} in {} on {FIRST_BRANCH} ({})",
            file.display(),
            path.display(),
            recorded.id.short()
        ));
    }
    Ok(exit::OK)
}

fn pretty(value: &impl Serialize) -> String {
    serde_json::to_string_pretty(value).unwrap_or_default()
}

/// JSON text kept in the store, read back as a value.
fn parsed(text: &str) -> Value {
    serde_json::from_str(text).unwrap_or_else(|_| Value::String(text.to_owned()))
}

/// One line for what a changeset did, when it has no reason given: its
/// first three edits in plain words ("A6 duration", "Added link A3 to A5").
pub fn summary(changeset: &Changeset) -> String {
    let commands = parsed(&changeset.commands);
    if let Some(file) = commands["import"]["file"].as_str() {
        return format!("import {file}");
    }
    if let Some(reverts) = commands["reverts"].as_str() {
        let short: String = reverts.chars().take(10).collect();
        return format!("revert {short}");
    }
    let edits = commands["edits"].as_array().cloned().unwrap_or_default();
    let named: Vec<String> = edits.iter().take(3).map(words::edit_words).collect();
    match edits.len() {
        0 => changeset.kind.to_string(),
        n if n <= 3 => named.join(", "),
        n => format!("{}, and {} more", named.join(", "), n - 3),
    }
}

/// Who made a changeset, marked when it's an agent.
pub fn who(changeset: &Changeset) -> String {
    match changeset.author_kind {
        totalfloat_store::AuthorKind::Agent => format!("{} (agent)", changeset.author),
        totalfloat_store::AuthorKind::Human => changeset.author.clone(),
    }
}

/// A changeset's reason, or a summary of its commands without one, on one
/// line.
pub fn reason(changeset: &Changeset) -> String {
    first_line(&changeset.message).unwrap_or_else(|| summary(changeset))
}

/// A message's first line, marked if there's more.
fn first_line(message: &str) -> Option<String> {
    let mut lines = message.lines().filter(|l| !l.trim().is_empty());
    let first = lines.next()?.trim().to_owned();
    Some(if lines.next().is_some() {
        format!("{first} …")
    } else {
        first
    })
}

/// `totalfloat.changeset/1`.
fn changeset_doc(store: &Store, changeset: &Changeset) -> Result<Value, CliError> {
    let refs: Vec<String> = store
        .refs()
        .map_err(store_error)?
        .into_iter()
        .filter(|r| r.target == changeset.id)
        .map(|r| r.name)
        .collect();
    let reviews: Vec<Value> = store
        .reviews(&changeset.id)
        .map_err(store_error)?
        .into_iter()
        .map(|r| {
            json!({
                "state": r.state.as_str(),
                "author": r.author,
                "author_kind": r.author_kind.as_str(),
                "message": r.message,
                "created": r.created,
            })
        })
        .collect();
    Ok(json!({
        "schema": "totalfloat.changeset/1",
        "id": changeset.id.as_str(),
        "short_id": changeset.id.short(),
        "parent": changeset.parent.as_ref().map(ChangesetId::as_str),
        "second_parent": changeset.second_parent.as_ref().map(ChangesetId::as_str),
        "kind": changeset.kind.as_str(),
        "state": changeset.state.as_str(),
        "author": changeset.author,
        "author_kind": changeset.author_kind.as_str(),
        "message": changeset.message,
        "summary": summary(changeset),
        "created": changeset.created,
        "tool_version": changeset.tool_version,
        "refs": refs,
        "commands": parsed(&changeset.commands),
        "skipped": parsed(&changeset.skipped),
        "report": parsed(&changeset.report),
        "snapshot": changeset.snapshot.as_str(),
        "reviews": reviews,
    }))
}

/// One changeset as a line of the log.
pub fn log_entry(changeset: &Changeset) -> Value {
    json!({
        "id": changeset.id.as_str(),
        "short_id": changeset.id.short(),
        "parent": changeset.parent.as_ref().map(ChangesetId::as_str),
        "second_parent": changeset.second_parent.as_ref().map(ChangesetId::as_str),
        "kind": changeset.kind.as_str(),
        "state": changeset.state.as_str(),
        "author": changeset.author,
        "author_kind": changeset.author_kind.as_str(),
        "message": changeset.message,
        "created": changeset.created,
        "summary": summary(changeset),
    })
}

fn log(
    path: &Path,
    (branch, all): (Option<&str>, bool),
    limit: usize,
    json: bool,
) -> Result<u8, CliError> {
    let store = open(path)?;
    let (name, head, changesets) = if all {
        (None, None, store.recent(limit).map_err(store_error)?)
    } else {
        let (name, head) = branch_head(&store, branch)?;
        let line = store.log(&head, limit).map_err(store_error)?;
        (Some(name), Some(head), line)
    };
    if json {
        let doc = json!({
            "schema": "totalfloat.history-log/1",
            "store": path.display().to_string(),
            "branch": name,
            "head": head.as_ref().map(ChangesetId::as_str),
            "changesets": changesets.iter().map(log_entry).collect::<Vec<_>>(),
        });
        print_line(&pretty(&doc));
        return Ok(exit::OK);
    }
    let title = match &name {
        None => format!("Every changeset in {}", path.display()),
        Some(name) => format!("History of {name} in {}", path.display()),
    };
    let mut lines = vec![title];
    for changeset in &changesets {
        let who = who(changeset);
        let what = first_line(&changeset.message).unwrap_or_else(|| summary(changeset));
        let state = match changeset.state {
            State::Accepted => String::new(),
            other => format!(" [{other}]"),
        };
        lines.push(format!(
            "{}  {}  {who}  {}{state}  {what}",
            changeset.id.short(),
            changeset.created,
            changeset.kind
        ));
    }
    print_line(&lines.join("\n"));
    Ok(exit::OK)
}

fn show(path: &Path, at: &str, json: bool) -> Result<u8, CliError> {
    let store = open(path)?;
    let id = resolve_point(&store, path, Some(at))?;
    let changeset = store.changeset(&id).map_err(store_error)?;
    let doc = changeset_doc(&store, &changeset)?;
    if json {
        print_line(&pretty(&doc));
        return Ok(exit::OK);
    }
    let mut lines = vec![
        format!("changeset {}", changeset.id),
        format!(
            "{} by {} ({}) at {}",
            changeset.kind, changeset.author, changeset.author_kind, changeset.created
        ),
    ];
    if let Some(parent) = &changeset.parent {
        lines.push(format!("parent {}", parent.short()));
    }
    if let Some(second) = &changeset.second_parent {
        lines.push(format!("merged {}", second.short()));
    }
    if let Some(refs) = doc["refs"].as_array().filter(|r| !r.is_empty()) {
        let names: Vec<&str> = refs.iter().filter_map(Value::as_str).collect();
        lines.push(format!("named {}", names.join(", ")));
    }
    if changeset.state != State::Accepted {
        lines.push(format!("state {}", changeset.state));
    }
    for review in doc["reviews"].as_array().into_iter().flatten() {
        lines.push(format!(
            "{} by {} at {}: {}",
            review["state"].as_str().unwrap_or_default(),
            review["author"].as_str().unwrap_or_default(),
            review["created"].as_str().unwrap_or_default(),
            review["message"].as_str().unwrap_or_default(),
        ));
    }
    if !changeset.message.is_empty() {
        lines.push(String::new());
        lines.push(format!("    {}", changeset.message));
    }
    lines.push(String::new());
    lines.push(format!("commands: {}", pretty(&doc["commands"])));
    if doc["skipped"].as_array().is_some_and(|s| !s.is_empty()) {
        lines.push(format!("skipped: {}", pretty(&doc["skipped"])));
    }
    lines.push(format!("report: {}", pretty(&doc["report"])));
    print_line(&lines.join("\n"));
    Ok(exit::OK)
}

fn export(path: &Path, at: Option<&str>, output: &Path, json: bool) -> Result<u8, CliError> {
    let store = open(path)?;
    let (changeset, bytes) = snapshot_at(&store, path, at)?;
    create_new_bytes(&bytes, output)?;
    if json {
        let doc = json!({
            "schema": "totalfloat.history-export/1",
            "store": path.display().to_string(),
            "changeset": changeset.id.as_str(),
            "short_id": changeset.id.short(),
            "output": output.display().to_string(),
        });
        print_line(&pretty(&doc));
    } else {
        print_line(&format!(
            "Wrote {} from {}",
            output.display(),
            changeset.id.short()
        ));
    }
    Ok(exit::OK)
}
