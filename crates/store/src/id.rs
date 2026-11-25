//! Changeset IDs: SHA-256 over every field that says what a changeset is,
//! except its state, which changes when a proposal is reviewed, and its place
//! in the order recorded.

use std::fmt;

use sha2::{Digest, Sha256};

use crate::{BlobHash, Changeset, NewChangeset, Result, StoreError};

/// A changeset's ID: 64 lowercase hex digits.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ChangesetId(String);

impl ChangesetId {
    /// Reads an ID kept in the store.
    pub(crate) fn from_stored(text: String) -> Result<Self> {
        if crate::blob::is_digest(&text) {
            Ok(Self(text))
        } else {
            Err(StoreError::Damaged(format!("{text} isn't a changeset ID")))
        }
    }

    /// The full ID.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The first ten digits, as history shows it.
    #[must_use]
    pub fn short(&self) -> String {
        self.0[..10].to_owned()
    }
}

impl fmt::Display for ChangesetId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// The parts an ID is worked out from, in a fixed order.
struct Parts<'a> {
    parent: Option<&'a ChangesetId>,
    second_parent: Option<&'a ChangesetId>,
    kind: &'a str,
    author: &'a str,
    author_kind: &'a str,
    message: &'a str,
    created: &'a str,
    tool_version: &'a str,
    commands: &'a str,
    skipped: &'a str,
    report: &'a str,
    snapshot: &'a BlobHash,
}

fn hash(parts: &Parts<'_>) -> ChangesetId {
    let mut hasher = Sha256::new();
    hasher.update(b"totalfloat.changeset/1\0");
    // Each field is its length then its bytes, so no two field lists encode
    // the same way. A missing parent is a marker no ID can be.
    let mut field = |bytes: &[u8]| {
        hasher.update((bytes.len() as u64).to_le_bytes());
        hasher.update(bytes);
    };
    for parent in [parts.parent, parts.second_parent] {
        field(parent.map_or(b"-".as_slice(), |p| p.as_str().as_bytes()));
    }
    for text in [
        parts.kind,
        parts.author,
        parts.author_kind,
        parts.message,
        parts.created,
        parts.tool_version,
        parts.commands,
        parts.skipped,
        parts.report,
        parts.snapshot.as_str(),
    ] {
        field(text.as_bytes());
    }
    ChangesetId(crate::blob::hex(&hasher.finalize()))
}

/// The ID a new changeset gets, with its file stored under `snapshot`.
pub(crate) fn changeset_id(new: &NewChangeset, snapshot: &BlobHash) -> ChangesetId {
    hash(&Parts {
        parent: new.parent.as_ref(),
        second_parent: new.second_parent.as_ref(),
        kind: new.kind.as_str(),
        author: &new.author,
        author_kind: new.author_kind.as_str(),
        message: &new.message,
        created: &new.created,
        tool_version: &new.tool_version,
        commands: &new.commands,
        skipped: &new.skipped,
        report: &new.report,
        snapshot,
    })
}

/// The ID a recorded changeset's contents give, to check against its own.
pub(crate) fn recorded_id(changeset: &Changeset) -> ChangesetId {
    let new = NewChangeset {
        parent: changeset.parent.clone(),
        second_parent: changeset.second_parent.clone(),
        kind: changeset.kind,
        state: changeset.state,
        author: changeset.author.clone(),
        author_kind: changeset.author_kind,
        message: changeset.message.clone(),
        created: changeset.created.clone(),
        tool_version: changeset.tool_version.clone(),
        commands: changeset.commands.clone(),
        skipped: changeset.skipped.clone(),
        report: changeset.report.clone(),
        snapshot: Vec::new(),
    };
    changeset_id(&new, &changeset.snapshot)
}
