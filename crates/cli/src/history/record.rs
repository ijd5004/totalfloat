//! Edits through a store: read the checked-out branch's file, and record
//! what the edits made as a changeset on it.

use std::path::Path;

use serde::Serialize;
use totalfloat_store::{BlobHash, ChangesetId, Kind, Moving, NewChangeset, RefKind, State, Store};
use totalfloat_xer::RawXer;

use super::{author, branch_head, now, open, store_error};
use crate::error::{CliError, read_xer_bytes};

/// What a change run under [`pinned`] expects of the branch it records on.
#[derive(Debug, Clone)]
pub struct Pin {
    /// The branch.
    pub branch: String,
    /// The head the caller last saw on it, if they gave one.
    pub head: Option<String>,
    /// Whether the change must record on this branch and no other, as a
    /// revert or merge on the checked-out branch must.
    pub only: bool,
}

thread_local! {
    /// What this thread's changes expect, while [`pinned`] runs.
    static PIN: std::cell::RefCell<Option<Pin>> = const { std::cell::RefCell::new(None) };
}

/// Runs `work` with every head it reads checked against `pin`: a change on
/// the pinned branch whose head isn't the one expected is refused with
/// `branch-moved`, and with `only`, a change on another branch with
/// `not-checked-out`. The change then records only if the branch is still at
/// that head, so the check holds from the caller's view to the store.
/// `serve` runs each history change from the app this way.
pub fn pinned<T>(pin: Pin, work: impl FnOnce() -> T) -> T {
    /// Puts back what was there, even if `work` panics.
    struct Restore(Option<Pin>);
    impl Drop for Restore {
        fn drop(&mut self) {
            let before = self.0.take();
            PIN.with_borrow_mut(|pin| *pin = before);
        }
    }
    let _restore = Restore(PIN.with_borrow_mut(|before| before.replace(pin)));
    work()
}

/// Refuses a head the pin, if any, doesn't expect.
fn check_pin(branch: &str, id: &ChangesetId) -> Result<(), CliError> {
    PIN.with_borrow(|pin| {
        let Some(pin) = pin else {
            return Ok(());
        };
        if branch != pin.branch {
            if pin.only {
                return Err(CliError::new(
                    "not-checked-out",
                    format!(
                        "this acts on the checked-out branch, {branch}, not {}, the one shown",
                        pin.branch
                    ),
                    "switch to the branch shown first: POST /api/1/history/branch {\"action\": \"switch\", \"name\": ...}",
                ));
            }
            return Ok(());
        }
        match &pin.head {
            Some(head) if *head != id.to_string() => Err(CliError::new(
                "branch-moved",
                format!("{branch} has moved on since the page saw it"),
                "reload to see what changed, then try again",
            )),
            _ => Ok(()),
        }
    })
}

/// The checked-out branch's head, as an edit starts from it.
#[derive(Debug)]
pub struct Head {
    /// The branch.
    pub branch: String,
    /// Its head.
    pub id: ChangesetId,
    /// The file there, by hash.
    pub snapshot: BlobHash,
}

/// A changeset an edit recorded.
#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct Recorded {
    /// Its ID.
    pub id: String,
    /// Its first ten digits.
    pub short_id: String,
    /// The branch it's on, or for a proposal the branch it's for.
    pub branch: String,
    /// For a proposal, the ref naming it until it's reviewed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proposal: Option<String>,
}

/// Reads a branch's file: the one named, or the checked-out one.
///
/// # Errors
///
/// `not-a-branch` for a baseline, proposal or changeset ID, which edits
/// can't record on; `branch-moved` or `not-checked-out` under [`pinned`];
/// what the store refuses; or a stored file the reader
/// can't read.
pub fn read_head(path: &Path, branch: Option<&str>) -> Result<(Head, RawXer), CliError> {
    let store = open(path)?;
    let head = head_in(&store, branch)?;
    let bytes = store.blob(&head.snapshot).map_err(store_error)?;
    Ok((head, read_xer_bytes(bytes, path)?))
}

/// The file at `head`, read from the store at `path`.
///
/// # Errors
///
/// What reading the store refuses.
pub fn read_snapshot(path: &Path, head: &Head) -> Result<RawXer, CliError> {
    let bytes = open(path)?.blob(&head.snapshot).map_err(store_error)?;
    read_xer_bytes(bytes, path)
}

/// [`read_head`] without reading the file: the branch, its head and the
/// snapshot's hash, so a caller already holding that snapshot can skip it.
///
/// # Errors
///
/// As [`read_head`].
pub fn head_of(path: &Path, branch: Option<&str>) -> Result<Head, CliError> {
    head_in(&open(path)?, branch)
}

/// [`head_of`] in a store already open.
fn head_in(store: &Store, branch: Option<&str>) -> Result<Head, CliError> {
    if let Some(name) = branch {
        let named = store.get_ref(name).map_err(store_error)?;
        if named.is_none_or(|r| r.kind != RefKind::Branch) {
            return Err(CliError::new(
                "not-a-branch",
                format!("{name} isn't a branch, and edits are recorded on a branch"),
                "make one from it with `history branch`, or export it and edit the file with -o",
            ));
        }
    }
    let (branch, id) = branch_head(store, branch)?;
    check_pin(&branch, &id)?;
    let changeset = store.changeset(&id).map_err(store_error)?;
    Ok(Head {
        branch,
        id,
        snapshot: changeset.snapshot,
    })
}

/// What an edit asks to record, besides its result.
#[derive(Debug, Clone, Copy)]
pub struct Asked<'a> {
    /// Why.
    pub message: Option<&'a str>,
    /// Who, if given on the command line.
    pub author: Option<&'a str>,
    /// Record it as a proposal awaiting review, leaving the branch where it
    /// is.
    pub propose: bool,
}

/// Records the edited file on the branch it was read from, if it changed.
/// The branch moves only if it's still where it was read.
///
/// # Errors
///
/// `branch-moved` if someone recorded first, or what the store refuses.
pub fn record_edit(
    path: &Path,
    head: &Head,
    raw: &RawXer,
    parts: (String, String, String),
    asked: Asked<'_>,
) -> Result<Option<Recorded>, CliError> {
    let mut bytes = Vec::with_capacity(raw.source().len());
    raw.write(&mut bytes).map_err(|e| {
        CliError::new(
            "write-failed",
            format!("can't write the edited file: {e}"),
            "try again",
        )
    })?;
    record_change(path, head, (Kind::Edit, None), bytes, parts, asked)
}

/// Records a file as a changeset of `kind` after `head` on its branch, with a
/// second parent for a merge, if it differs from the head's. The branch moves only if it's still at `head`.
///
/// # Errors
///
/// `branch-moved` if someone recorded first, or what the store refuses.
pub fn record_change(
    path: &Path,
    head: &Head,
    (kind, second_parent): (Kind, Option<ChangesetId>),
    bytes: Vec<u8>,
    (commands, skipped, report): (String, String, String),
    asked: Asked<'_>,
) -> Result<Option<Recorded>, CliError> {
    let created = now()?;
    let (author, author_kind) = author(asked.author);
    let new = NewChangeset {
        parent: Some(head.id.clone()),
        second_parent,
        kind,
        state: State::Accepted,
        author,
        author_kind,
        message: asked.message.unwrap_or_default().to_owned(),
        created,
        tool_version: env!("CARGO_PKG_VERSION").to_owned(),
        commands,
        skipped,
        report,
        snapshot: bytes,
    };
    let mut store = open(path)?;
    if asked.propose {
        return propose(&mut store, new, head);
    }
    let moving = Moving {
        name: &head.branch,
        kind: RefKind::Branch,
        expected: Some(&head.id),
    };
    // A merge is recorded even when it changes nothing, so the branches
    // share a point from then on.
    let recorded = if kind == Kind::Merge {
        store.record(&new, Some(moving)).map_err(store_error)?
    } else {
        match store
            .record_changed(&new, Some(moving), &head.snapshot)
            .map_err(store_error)?
        {
            Some(recorded) => recorded,
            None => return Ok(None),
        }
    };
    Ok(Some(Recorded {
        id: recorded.id.to_string(),
        short_id: recorded.id.short(),
        branch: head.branch.clone(),
        proposal: None,
    }))
}

/// Records a changeset as a proposal: off the branch, named
/// `proposal/<branch>/<short id>` until someone accepts or rejects it, in
/// one transaction.
fn propose(
    store: &mut Store,
    mut new: NewChangeset,
    head: &Head,
) -> Result<Option<Recorded>, CliError> {
    new.state = State::Proposed;
    let id = Store::id_for(&new);
    let name = format!("{PROPOSAL}{}/{}", head.branch, id.short());
    let moving = Moving {
        name: &name,
        kind: RefKind::Proposal,
        expected: None,
    };
    let Some(recorded) = store
        .record_changed(&new, Some(moving), &head.snapshot)
        .map_err(store_error)?
    else {
        return Ok(None);
    };
    Ok(Some(Recorded {
        id: recorded.id.to_string(),
        short_id: recorded.id.short(),
        branch: head.branch.clone(),
        proposal: Some(name),
    }))
}

/// The start of every proposal's name.
pub const PROPOSAL: &str = "proposal/";

/// The branch a proposal's name says it's for: `proposal/<branch>/<id>`.
pub fn proposal_branch(name: &str) -> Option<&str> {
    name.strip_prefix(PROPOSAL)?
        .rsplit_once('/')
        .map(|(branch, _)| branch)
}

#[cfg(test)]
mod tests {
    use totalfloat_store::{AuthorKind, Store};

    use super::*;

    /// A store with the fixture imported on main, checked out.
    fn started(directory: &Path) -> std::path::PathBuf {
        let path = directory.join("plan.tfdb");
        let fixture = std::fs::read("../../fixtures/xer/batches/001/rel-fs-positive.xer").unwrap();
        let mut store = Store::create(&path).unwrap();
        let start = NewChangeset {
            parent: None,
            second_parent: None,
            kind: Kind::Import,
            state: State::Accepted,
            author: "Pat".into(),
            author_kind: AuthorKind::Human,
            message: String::new(),
            created: "2026-11-02T09:00:00Z".into(),
            tool_version: "0.0.0".into(),
            commands: "{}".into(),
            skipped: "[]".into(),
            report: "{}".into(),
            snapshot: fixture,
        };
        let moving = Moving {
            name: "main",
            kind: RefKind::Branch,
            expected: None,
        };
        store.record(&start, Some(moving)).unwrap();
        store.set_current("main").unwrap();
        path
    }

    #[test]
    fn a_second_edit_from_the_same_head_is_told_the_branch_moved() {
        let directory = tempfile::tempdir().unwrap();
        let path = started(directory.path());
        // Two edits both read main at the import.
        let (first, raw) = read_head(&path, None).unwrap();
        let (second, _) = read_head(&path, None).unwrap();
        let mut changed = raw.clone();
        changed.set_cell(0, 0, 0, "changed").unwrap();
        let parts = || ("{}".to_owned(), "[]".to_owned(), "{}".to_owned());
        let asked = Asked {
            message: Some("mine"),
            author: Some("Pat"),
            propose: false,
        };
        assert!(
            record_edit(&path, &first, &changed, parts(), asked)
                .unwrap()
                .is_some()
        );
        let late = record_edit(&path, &second, &changed, parts(), asked).unwrap_err();
        assert_eq!(late.code, "branch-moved");
        // An edit that changes nothing records nothing.
        let (now_head, now_raw) = read_head(&path, None).unwrap();
        assert!(
            record_edit(&path, &now_head, &now_raw, parts(), asked)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn a_pinned_change_reads_only_the_head_expected() {
        let directory = tempfile::tempdir().unwrap();
        let path = started(directory.path());
        let (head, _) = read_head(&path, None).unwrap();
        let pin = |branch: &str, head: Option<String>, only: bool| Pin {
            branch: branch.to_owned(),
            head,
            only,
        };
        let seen = Some(head.id.to_string());
        assert!(pinned(pin("main", seen.clone(), true), || read_head(&path, None)).is_ok());
        assert!(pinned(pin("main", None, true), || read_head(&path, None)).is_ok());
        let stale = pinned(pin("main", Some("stale".to_owned()), false), || {
            read_head(&path, None)
        });
        assert_eq!(stale.unwrap_err().code, "branch-moved");
        // Only the branch pinned may be read, when it's the only one.
        let elsewhere = pinned(pin("what-if", seen.clone(), true), || {
            read_head(&path, None)
        });
        assert_eq!(elsewhere.unwrap_err().code, "not-checked-out");
        assert!(pinned(pin("what-if", seen, false), || read_head(&path, None)).is_ok());
        // The pin goes when the work is done.
        assert!(read_head(&path, None).is_ok());
        pinned(pin("main", Some("stale".to_owned()), false), || {});
        assert!(read_head(&path, None).is_ok());
    }
}
