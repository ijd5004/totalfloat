//! The history store (ARCHITECTURE.md, History and changesets): one SQLite file holding a schedule file's
//! changesets, the refs that name them, and every version of the file,
//! compressed and stored once per distinct content.
//!
//! The store knows nothing about schedules. Commands, skipped edits and
//! reports are JSON text it keeps as given, and files are bytes. It is the one
//! library crate that touches the filesystem, and only through its database
//! file. Its API holds no SQLite types, so another backend can sit behind the
//! same calls. Time is an input: callers give each changeset's time.

#![forbid(unsafe_code)]

mod blob;
mod id;

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags, OptionalExtension, TransactionBehavior, params};

pub use blob::BlobHash;
pub use id::ChangesetId;

/// The format this build writes and reads.
pub const FORMAT: &str = "totalfloat.store/1";

/// The first bytes of every SQLite database file.
const SQLITE_HEADER: &[u8; 16] = b"SQLite format 3\0";

/// The shortest ID prefix that names a changeset.
pub const MIN_PREFIX: usize = 6;

/// What went wrong with a store.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// `create` was given a path that already exists.
    #[error("{0} already exists")]
    Exists(PathBuf),
    /// The file isn't a TotalFloat store.
    #[error("{0} isn't a TotalFloat history store")]
    NotAStore(PathBuf),
    /// The store is in a format this build doesn't read.
    #[error("the store is in format {0}, and this build reads {FORMAT}")]
    UnknownFormat(String),
    /// A ref moved since the caller read it, so the write was refused.
    #[error("{name} moved: it was expected at {expected}, and is at {actual}")]
    Moved {
        /// The ref.
        name: String,
        /// Where the caller last saw it, or `none`.
        expected: String,
        /// Where it is, or `none`.
        actual: String,
    },
    /// Nothing has this name or ID.
    #[error("nothing in the store is called {0}")]
    NotFound(String),
    /// An ID prefix matches more than one changeset.
    #[error("{0} matches {1} changesets")]
    Ambiguous(String, usize),
    /// An ID prefix shorter than [`MIN_PREFIX`].
    #[error("{0} is too short to name a changeset; give at least {MIN_PREFIX} characters")]
    TooShort(String),
    /// Stored data doesn't match its hash or can't be read back.
    #[error("the store is damaged: {0}")]
    Damaged(String),
    /// A value that isn't one of the store's words.
    #[error("{0} isn't a known {1}")]
    Unknown(String, &'static str),
    /// The database failed.
    #[error("database: {0}")]
    Database(String),
    /// A ref of one kind was used as another, or a baseline was moved.
    #[error("{name} is a {actual}, not a {wanted}")]
    WrongKind {
        /// The ref.
        name: String,
        /// What it is.
        actual: RefKind,
        /// What the caller treated it as.
        wanted: RefKind,
    },
    /// The checked-out branch can't be deleted.
    #[error("{0} is the checked-out branch")]
    Current(String),
    /// A proposal already accepted or rejected.
    #[error("{0} has already been decided")]
    Decided(String),
    /// A changeset with exactly these contents is already recorded.
    #[error("changeset {0} is already recorded")]
    Duplicate(String),
    /// The file couldn't be read.
    #[error("{0}")]
    Io(String),
}

impl From<rusqlite::Error> for StoreError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Database(error.to_string())
    }
}

/// Results from the store.
pub type Result<T> = std::result::Result<T, StoreError>;

/// Defines a closed set of words kept as text in the database.
macro_rules! words {
    ($(#[$doc:meta])* $name:ident, $what:literal, { $($(#[$vdoc:meta])* $variant:ident = $text:literal),+ $(,)? }) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
        pub enum $name {
            $($(#[$vdoc])* $variant),+
        }

        impl $name {
            /// The word as stored.
            #[must_use]
            pub const fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $text),+
                }
            }

            /// Reads a stored word.
            ///
            /// # Errors
            ///
            /// [`StoreError::Unknown`] for any other text.
            pub fn parse(text: &str) -> Result<Self> {
                match text {
                    $($text => Ok(Self::$variant),)+
                    other => Err(StoreError::Unknown(other.to_owned(), $what)),
                }
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_str())
            }
        }
    };
}

words!(
    /// What a changeset is.
    Kind, "changeset kind", {
        /// A whole file brought in: the first, or a later export from P6.
        Import = "import",
        /// Commands applied to the parent.
        Edit = "edit",
        /// The parent with an earlier changeset taken back out.
        Revert = "revert",
        /// Another branch's changesets replayed onto the parent.
        Merge = "merge",
    }
);

words!(
    /// Where a changeset stands.
    State, "changeset state", {
        /// Part of history.
        Accepted = "accepted",
        /// Waiting for a person to accept or reject it.
        Proposed = "proposed",
        /// Turned down, and kept.
        Rejected = "rejected",
    }
);

words!(
    /// Who made a changeset.
    AuthorKind, "author kind", {
        /// A person.
        Human = "human",
        /// An agent.
        Agent = "agent",
    }
);

words!(
    /// What a ref is for.
    RefKind, "ref kind", {
        /// A line of work, which moves forward as changesets are recorded.
        Branch = "branch",
        /// A named point that never moves.
        Baseline = "baseline",
        /// A proposed changeset waiting for review.
        Proposal = "proposal",
    }
);

/// A changeset to record. Its ID is worked out from its contents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewChangeset {
    /// The changeset it follows; none for the first import.
    pub parent: Option<ChangesetId>,
    /// For a merge, the other branch's head.
    pub second_parent: Option<ChangesetId>,
    /// What it is.
    pub kind: Kind,
    /// Where it stands.
    pub state: State,
    /// Who made it.
    pub author: String,
    /// Whether that's a person or an agent.
    pub author_kind: AuthorKind,
    /// Why, in the author's words. May be empty.
    pub message: String,
    /// When, as the caller gives it (RFC 3339 by convention).
    pub created: String,
    /// The TotalFloat version that made it.
    pub tool_version: String,
    /// The commands, as JSON text.
    pub commands: String,
    /// What was skipped, as JSON text.
    pub skipped: String,
    /// The schedule impact, as JSON text.
    pub report: String,
    /// The resulting file.
    pub snapshot: Vec<u8>,
}

/// A recorded changeset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Changeset {
    /// Its ID.
    pub id: ChangesetId,
    /// Its place in the order changesets were recorded, from 1.
    pub seq: i64,
    /// The changeset it follows.
    pub parent: Option<ChangesetId>,
    /// For a merge, the other branch's head.
    pub second_parent: Option<ChangesetId>,
    /// What it is.
    pub kind: Kind,
    /// Where it stands.
    pub state: State,
    /// Who made it.
    pub author: String,
    /// Whether that's a person or an agent.
    pub author_kind: AuthorKind,
    /// Why.
    pub message: String,
    /// When.
    pub created: String,
    /// The TotalFloat version that made it.
    pub tool_version: String,
    /// The commands, as JSON text.
    pub commands: String,
    /// What was skipped, as JSON text.
    pub skipped: String,
    /// The schedule impact, as JSON text.
    pub report: String,
    /// The resulting file's hash.
    pub snapshot: BlobHash,
}

impl Changeset {
    /// Its parents, first then second.
    pub fn parents(&self) -> impl Iterator<Item = &ChangesetId> {
        self.parent.iter().chain(self.second_parent.iter())
    }
}

/// A name for a changeset.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Ref {
    /// The name.
    pub name: String,
    /// What it's for.
    pub kind: RefKind,
    /// The changeset it names.
    pub target: ChangesetId,
}

/// Where a recorded changeset moves a ref to.
#[derive(Debug, Clone, Copy)]
pub struct Moving<'a> {
    /// The ref's name.
    pub name: &'a str,
    /// What it's for, if it has to be made.
    pub kind: RefKind,
    /// Where the caller last saw it; `None` if it shouldn't exist yet.
    pub expected: Option<&'a ChangesetId>,
}

/// A person's or agent's decision on a proposed changeset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Review {
    /// The changeset reviewed.
    pub changeset: ChangesetId,
    /// What it became: accepted or rejected.
    pub state: State,
    /// Who decided.
    pub author: String,
    /// Whether that's a person or an agent.
    pub author_kind: AuthorKind,
    /// Why.
    pub message: String,
    /// When, as the caller gives it.
    pub created: String,
}

/// A review to record, and the refs it moves in the same transaction.
#[derive(Debug, Clone)]
pub struct Reviewing<'a> {
    /// The decision.
    pub review: Review,
    /// A proposal ref to remove, now it's decided.
    pub drop_ref: Option<&'a str>,
    /// A branch to move to the changeset, from where the caller saw it.
    pub advance: Option<Moving<'a>>,
}

/// Something `verify` found wrong.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Damage {
    /// What it's about: a changeset ID, a blob hash or a ref name.
    pub subject: String,
    /// What's wrong.
    pub problem: String,
}

/// True if the file at `path` starts as a SQLite database does, so it may be
/// a store. Missing or short files aren't.
#[must_use]
pub fn looks_like_store(path: &Path) -> bool {
    use std::io::Read as _;
    let mut header = [0u8; 16];
    std::fs::File::open(path)
        .and_then(|mut file| file.read_exact(&mut header))
        .is_ok_and(|()| &header == SQLITE_HEADER)
}

const SCHEMA: &str = "
CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE blobs (
    hash TEXT PRIMARY KEY,
    size INTEGER NOT NULL,
    data BLOB NOT NULL
);
CREATE TABLE changesets (
    id TEXT PRIMARY KEY,
    seq INTEGER NOT NULL UNIQUE,
    parent TEXT REFERENCES changesets(id),
    second_parent TEXT REFERENCES changesets(id),
    kind TEXT NOT NULL,
    state TEXT NOT NULL,
    author TEXT NOT NULL,
    author_kind TEXT NOT NULL,
    message TEXT NOT NULL,
    created TEXT NOT NULL,
    tool_version TEXT NOT NULL,
    commands TEXT NOT NULL,
    skipped TEXT NOT NULL,
    report TEXT NOT NULL,
    snapshot TEXT NOT NULL REFERENCES blobs(hash)
);
CREATE INDEX changesets_parent ON changesets(parent);
CREATE TABLE refs (
    name TEXT PRIMARY KEY,
    kind TEXT NOT NULL,
    target TEXT NOT NULL REFERENCES changesets(id)
);
";

const REVIEWS: &str = "
CREATE TABLE IF NOT EXISTS reviews (
    changeset TEXT NOT NULL REFERENCES changesets(id),
    seq INTEGER NOT NULL,
    state TEXT NOT NULL,
    author TEXT NOT NULL,
    author_kind TEXT NOT NULL,
    message TEXT NOT NULL,
    created TEXT NOT NULL
);
";

const CHANGESET_COLUMNS: &str = "id, seq, parent, second_parent, kind, state, author, \
    author_kind, message, created, tool_version, commands, skipped, report, snapshot";

/// An open store.
#[derive(Debug)]
pub struct Store {
    connection: Connection,
}

fn moved(name: &str, expected: Option<&ChangesetId>, actual: Option<&ChangesetId>) -> StoreError {
    let show = |id: Option<&ChangesetId>| id.map_or_else(|| "none".to_owned(), ChangesetId::short);
    StoreError::Moved {
        name: name.to_owned(),
        expected: show(expected),
        actual: show(actual),
    }
}

/// A ref row as read.
fn read_ref(row: &rusqlite::Row<'_>) -> rusqlite::Result<(String, String, String)> {
    Ok((row.get(0)?, row.get(1)?, row.get(2)?))
}

fn to_ref((name, kind, target): (String, String, String)) -> Result<Ref> {
    Ok(Ref {
        name,
        kind: RefKind::parse(&kind)?,
        target: ChangesetId::from_stored(target)?,
    })
}

fn get_ref_in(connection: &Connection, name: &str) -> Result<Option<Ref>> {
    let row = connection
        .query_row(
            "SELECT name, kind, target FROM refs WHERE name = ?1",
            [name],
            read_ref,
        )
        .optional()?;
    row.map(to_ref).transpose()
}

/// Refuses a move unless the ref is where the caller last saw it, is of the
/// kind the caller means, and isn't a baseline that already exists.
fn check_move(connection: &Connection, moving: &Moving<'_>) -> Result<()> {
    let actual = get_ref_in(connection, moving.name)?;
    if let Some(actual) = &actual {
        if actual.kind != moving.kind {
            return Err(StoreError::WrongKind {
                name: moving.name.to_owned(),
                actual: actual.kind,
                wanted: moving.kind,
            });
        }
        if actual.kind == RefKind::Baseline {
            return Err(StoreError::WrongKind {
                name: moving.name.to_owned(),
                actual: RefKind::Baseline,
                wanted: RefKind::Branch,
            });
        }
    }
    let actual = actual.map(|r| r.target);
    if actual.as_ref() == moving.expected {
        Ok(())
    } else {
        Err(moved(moving.name, moving.expected, actual.as_ref()))
    }
}

fn recorded(connection: &Connection, id: &ChangesetId) -> Result<bool> {
    let found: Option<i64> = connection
        .query_row(
            "SELECT seq FROM changesets WHERE id = ?1",
            [id.as_str()],
            |row| row.get(0),
        )
        .optional()?;
    Ok(found.is_some())
}

fn point(connection: &Connection, moving: &Moving<'_>, target: &ChangesetId) -> Result<()> {
    connection.execute(
        "INSERT INTO refs (name, kind, target) VALUES (?1, ?2, ?3)
         ON CONFLICT(name) DO UPDATE SET target = excluded.target",
        params![moving.name, moving.kind.as_str(), target.as_str()],
    )?;
    Ok(())
}

/// Records a changeset inside a transaction, moving a ref to it if asked.
fn record_in(
    connection: &Connection,
    new: &NewChangeset,
    moving: Option<&Moving<'_>>,
) -> Result<ChangesetId> {
    record_hashed(connection, new, moving, BlobHash::of(&new.snapshot))
}

/// [`record_in`], with the snapshot's hash already worked out.
fn record_hashed(
    connection: &Connection,
    new: &NewChangeset,
    moving: Option<&Moving<'_>>,
    hash: BlobHash,
) -> Result<ChangesetId> {
    if let Some(moving) = moving {
        check_move(connection, moving)?;
    }
    for parent in new.parent.iter().chain(new.second_parent.iter()) {
        if !recorded(connection, parent)? {
            return Err(StoreError::NotFound(parent.to_string()));
        }
    }
    let snapshot = blob::put_hashed(connection, &new.snapshot, hash)?;
    let id = id::changeset_id(new, &snapshot);
    if recorded(connection, &id)? {
        return Err(StoreError::Duplicate(id.short()));
    }
    connection.execute(
        "INSERT INTO changesets (id, seq, parent, second_parent, kind, state, author,
            author_kind, message, created, tool_version, commands, skipped, report, snapshot)
         VALUES (?1, (SELECT COALESCE(MAX(seq), 0) + 1 FROM changesets), ?2, ?3, ?4, ?5,
            ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
        params![
            id.as_str(),
            new.parent.as_ref().map(ChangesetId::as_str),
            new.second_parent.as_ref().map(ChangesetId::as_str),
            new.kind.as_str(),
            new.state.as_str(),
            new.author,
            new.author_kind.as_str(),
            new.message,
            new.created,
            new.tool_version,
            new.commands,
            new.skipped,
            new.report,
            snapshot.as_str(),
        ],
    )?;
    if let Some(moving) = moving {
        point(connection, moving, &id)?;
    }
    Ok(id)
}

/// Records a decision inside a transaction. Only a changeset still waiting
/// for review can be decided, so two decisions can't both land.
fn review_in(connection: &Connection, reviewing: &Reviewing<'_>) -> Result<()> {
    let review = &reviewing.review;
    let state: Option<String> = connection
        .query_row(
            "SELECT state FROM changesets WHERE id = ?1",
            [review.changeset.as_str()],
            |row| row.get(0),
        )
        .optional()?;
    match state.as_deref() {
        None => return Err(StoreError::NotFound(review.changeset.to_string())),
        Some(state) if State::parse(state)? != State::Proposed => {
            return Err(StoreError::Decided(review.changeset.short()));
        }
        Some(_) => {}
    }
    if let Some(moving) = &reviewing.advance {
        check_move(connection, moving)?;
        point(connection, moving, &review.changeset)?;
    }
    if let Some(name) = reviewing.drop_ref {
        connection.execute("DELETE FROM refs WHERE name = ?1", [name])?;
    }
    connection.execute(
        "UPDATE changesets SET state = ?1 WHERE id = ?2",
        params![review.state.as_str(), review.changeset.as_str()],
    )?;
    connection.execute(
        "INSERT INTO reviews (changeset, seq, state, author, author_kind, message, created)
         VALUES (?1, (SELECT COALESCE(MAX(seq), 0) + 1 FROM reviews), ?2, ?3, ?4, ?5, ?6)",
        params![
            review.changeset.as_str(),
            review.state.as_str(),
            review.author,
            review.author_kind.as_str(),
            review.message,
            review.created,
        ],
    )?;
    Ok(())
}

/// The range of IDs starting with a lowercase hex prefix, for an index scan:
/// from the prefix to the prefix followed by a letter after `f`.
fn prefix_range(prefix: &str) -> (String, String) {
    (prefix.to_owned(), format!("{prefix}g"))
}

impl Store {
    fn connect(path: &Path, flags: OpenFlags) -> Result<Connection> {
        let connection = Connection::open_with_flags(path, flags)?;
        // Another writer holds the lock for one short transaction at most.
        connection.busy_timeout(std::time::Duration::from_secs(10))?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        Ok(connection)
    }

    /// Makes a new, empty store at `path`, which mustn't exist. The file is
    /// claimed first, so two callers can't both make it, and it's removed
    /// again if the store can't be set up.
    ///
    /// # Errors
    ///
    /// [`StoreError::Exists`], or a database error.
    pub fn create(path: &Path) -> Result<Self> {
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
        {
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                return Err(StoreError::Exists(path.to_owned()));
            }
            Err(error) => return Err(StoreError::Io(format!("{}: {error}", path.display()))),
        }
        let made = (|| {
            let mut connection = Self::connect(path, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
            let transaction = connection.transaction()?;
            transaction.execute_batch(SCHEMA)?;
            transaction.execute_batch(REVIEWS)?;
            transaction.execute(
                "INSERT INTO meta (key, value) VALUES ('format', ?1)",
                [FORMAT],
            )?;
            transaction.commit()?;
            Ok(Self { connection })
        })();
        if made.is_err() {
            let _ = std::fs::remove_file(path);
        }
        made
    }

    /// Opens the store at `path`.
    ///
    /// # Errors
    ///
    /// [`StoreError::NotAStore`] for any other file, [`StoreError::UnknownFormat`]
    /// for a store this build doesn't read, or a database error.
    pub fn open(path: &Path) -> Result<Self> {
        if !path.is_file() {
            return Err(StoreError::Io(format!("{} doesn't exist", path.display())));
        }
        if !looks_like_store(path) {
            return Err(StoreError::NotAStore(path.to_owned()));
        }
        let connection = Self::connect(path, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
        let has_meta: bool = connection.query_row(
            "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'meta')",
            [],
            |row| row.get(0),
        )?;
        if !has_meta {
            return Err(StoreError::NotAStore(path.to_owned()));
        }
        let format: Option<String> = connection
            .query_row("SELECT value FROM meta WHERE key = 'format'", [], |row| {
                row.get(0)
            })
            .optional()?;
        match format {
            Some(format) if format == FORMAT => {
                // Stores made before reviews were kept gain the table.
                connection.execute_batch(REVIEWS)?;
                Ok(Self { connection })
            }
            Some(format) => Err(StoreError::UnknownFormat(format)),
            None => Err(StoreError::NotAStore(path.to_owned())),
        }
    }

    /// The checked-out branch, if one has been set.
    ///
    /// # Errors
    ///
    /// A database error.
    pub fn current(&self) -> Result<Option<String>> {
        Ok(self
            .connection
            .query_row("SELECT value FROM meta WHERE key = 'current'", [], |row| {
                row.get(0)
            })
            .optional()?)
    }

    /// Makes `name`, which must be a branch, the checked-out branch.
    ///
    /// # Errors
    ///
    /// [`StoreError::NotFound`] if there's no such ref,
    /// [`StoreError::WrongKind`] if it isn't a branch, or a database error.
    pub fn set_current(&self, name: &str) -> Result<()> {
        match self.get_ref(name)? {
            Some(found) if found.kind == RefKind::Branch => {}
            Some(found) => {
                return Err(StoreError::WrongKind {
                    name: name.to_owned(),
                    actual: found.kind,
                    wanted: RefKind::Branch,
                });
            }
            None => return Err(StoreError::NotFound(name.to_owned())),
        }
        self.connection.execute(
            "INSERT INTO meta (key, value) VALUES ('current', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [name],
        )?;
        Ok(())
    }

    /// Stores file bytes, once per distinct content, and gives their hash.
    ///
    /// # Errors
    ///
    /// A database error.
    pub fn put_blob(&self, bytes: &[u8]) -> Result<BlobHash> {
        blob::put(&self.connection, bytes)
    }

    /// The file bytes with this hash, checked against it.
    ///
    /// # Errors
    ///
    /// [`StoreError::NotFound`], [`StoreError::Damaged`], or a database error.
    pub fn blob(&self, hash: &BlobHash) -> Result<Vec<u8>> {
        blob::get(&self.connection, hash)
    }

    /// Records a changeset and, in the same transaction, moves a ref to it,
    /// but only if the ref is still where the caller last saw it.
    ///
    /// # Errors
    ///
    /// [`StoreError::Moved`] if the ref moved, [`StoreError::WrongKind`] for a
    /// ref of another kind or a baseline, [`StoreError::NotFound`] for a
    /// parent that isn't recorded, [`StoreError::Duplicate`] for contents
    /// already recorded, or a database error.
    pub fn record(&mut self, new: &NewChangeset, moving: Option<Moving<'_>>) -> Result<Changeset> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let id = record_in(&transaction, new, moving.as_ref())?;
        transaction.commit()?;
        self.changeset(&id)
    }

    /// Records a changeset unless its file is `previous` (the head it
    /// follows), hashing the file once for both. `None` when unchanged.
    ///
    /// # Errors
    ///
    /// As [`Store::record`].
    pub fn record_changed(
        &mut self,
        new: &NewChangeset,
        moving: Option<Moving<'_>>,
        previous: &BlobHash,
    ) -> Result<Option<Changeset>> {
        let hash = BlobHash::of(&new.snapshot);
        if hash == *previous {
            return Ok(None);
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let id = record_hashed(&transaction, new, moving.as_ref(), hash)?;
        transaction.commit()?;
        self.changeset(&id).map(Some)
    }

    /// The ID a changeset would get if recorded, so a ref can be named for it
    /// in the same transaction.
    #[must_use]
    pub fn id_for(new: &NewChangeset) -> ChangesetId {
        id::changeset_id(new, &BlobHash::of(&new.snapshot))
    }

    /// Records a changeset and a review together, such as a proposal replayed
    /// onto a branch and the decision that put it there, so neither lands
    /// without the other.
    ///
    /// # Errors
    ///
    /// As [`Store::record`] and [`Store::review`].
    pub fn record_reviewed(
        &mut self,
        new: &NewChangeset,
        moving: Moving<'_>,
        reviewing: &Reviewing<'_>,
    ) -> Result<Changeset> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let id = record_in(&transaction, new, Some(&moving))?;
        review_in(&transaction, reviewing)?;
        transaction.commit()?;
        self.changeset(&id)
    }

    /// The changeset with this ID.
    ///
    /// # Errors
    ///
    /// [`StoreError::NotFound`], or a database error.
    pub fn changeset(&self, id: &ChangesetId) -> Result<Changeset> {
        let sql = format!("SELECT {CHANGESET_COLUMNS} FROM changesets WHERE id = ?1");
        let row = self
            .connection
            .query_row(&sql, [id.as_str()], raw_changeset)
            .optional()?;
        row.ok_or_else(|| StoreError::NotFound(id.to_string()))?
            .into_changeset()
    }

    /// Records a decision on a changeset: sets its state, keeps who decided,
    /// why and when, and in the same transaction removes its proposal ref and
    /// moves a branch to it if asked.
    ///
    /// # Errors
    ///
    /// [`StoreError::NotFound`], [`StoreError::Moved`] if the branch moved,
    /// or a database error.
    pub fn review(&mut self, reviewing: &Reviewing<'_>) -> Result<()> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        review_in(&transaction, reviewing)?;
        transaction.commit()?;
        Ok(())
    }

    /// The most recently recorded changesets, newest first, at most `limit`.
    ///
    /// # Errors
    ///
    /// A database error, or a row that can't be read.
    pub fn recent(&self, limit: usize) -> Result<Vec<Changeset>> {
        let sql = format!("SELECT {CHANGESET_COLUMNS} FROM changesets ORDER BY seq DESC LIMIT ?1");
        let mut statement = self.connection.prepare(&sql)?;
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        let rows: Vec<RawChangeset> = statement
            .query_map([limit], raw_changeset)?
            .collect::<std::result::Result<_, _>>()?;
        rows.into_iter().map(RawChangeset::into_changeset).collect()
    }

    /// The highest `seq` recorded, or 0 in an empty store: a cheap way to
    /// tell whether anything has been recorded since a reader last looked.
    ///
    /// # Errors
    ///
    /// A database error.
    pub fn latest_seq(&self) -> Result<i64> {
        Ok(self.connection.query_row(
            "SELECT COALESCE(MAX(seq), 0) FROM changesets",
            [],
            |row| row.get(0),
        )?)
    }

    /// The changesets recorded after `seq`, in the order recorded.
    ///
    /// # Errors
    ///
    /// A database error, or a row that can't be read.
    pub fn since(&self, seq: i64) -> Result<Vec<Changeset>> {
        let sql = format!("SELECT {CHANGESET_COLUMNS} FROM changesets WHERE seq > ?1 ORDER BY seq");
        let mut statement = self.connection.prepare(&sql)?;
        let rows: Vec<RawChangeset> = statement
            .query_map([seq], raw_changeset)?
            .collect::<std::result::Result<_, _>>()?;
        rows.into_iter().map(RawChangeset::into_changeset).collect()
    }

    /// The decisions on a changeset, oldest first.
    ///
    /// # Errors
    ///
    /// A database error.
    pub fn reviews(&self, id: &ChangesetId) -> Result<Vec<Review>> {
        let mut statement = self.connection.prepare(
            "SELECT state, author, author_kind, message, created FROM reviews
             WHERE changeset = ?1 ORDER BY seq",
        )?;
        let rows: Vec<(String, String, String, String, String)> = statement
            .query_map([id.as_str()], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            })?
            .collect::<std::result::Result<_, _>>()?;
        rows.into_iter()
            .map(|(state, author, kind, message, created)| {
                Ok(Review {
                    changeset: id.clone(),
                    state: State::parse(&state)?,
                    author,
                    author_kind: AuthorKind::parse(&kind)?,
                    message,
                    created,
                })
            })
            .collect()
    }

    /// Changes a changeset's state, such as accepting a proposal. The ID
    /// doesn't depend on state, so it stays the same.
    ///
    /// # Errors
    ///
    /// [`StoreError::NotFound`], or a database error.
    pub fn set_state(&self, id: &ChangesetId, state: State) -> Result<()> {
        let changed = self.connection.execute(
            "UPDATE changesets SET state = ?1 WHERE id = ?2",
            params![state.as_str(), id.as_str()],
        )?;
        if changed == 0 {
            return Err(StoreError::NotFound(id.to_string()));
        }
        Ok(())
    }

    /// The changeset a ref name or an ID prefix names. Ref names are tried
    /// first.
    ///
    /// # Errors
    ///
    /// [`StoreError::NotFound`], [`StoreError::Ambiguous`],
    /// [`StoreError::TooShort`], or a database error.
    pub fn resolve(&self, name: &str) -> Result<ChangesetId> {
        if let Some(found) = self.get_ref(name)? {
            return Ok(found.target);
        }
        let lower = name.to_ascii_lowercase();
        if lower.is_empty() || !lower.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(StoreError::NotFound(name.to_owned()));
        }
        if lower.len() < MIN_PREFIX {
            return Err(StoreError::TooShort(name.to_owned()));
        }
        let (from, to) = prefix_range(&lower);
        let mut statement = self
            .connection
            .prepare("SELECT id FROM changesets WHERE id >= ?1 AND id < ?2 LIMIT 2")?;
        let found: Vec<String> = statement
            .query_map(params![from, to], |row| row.get(0))?
            .collect::<std::result::Result<_, _>>()?;
        match found.as_slice() {
            [one] => ChangesetId::from_stored(one.clone()),
            [] => Err(StoreError::NotFound(name.to_owned())),
            _ => {
                let count: i64 = self.connection.query_row(
                    "SELECT COUNT(*) FROM changesets WHERE id >= ?1 AND id < ?2",
                    params![from, to],
                    |row| row.get(0),
                )?;
                let count = usize::try_from(count).unwrap_or(usize::MAX);
                Err(StoreError::Ambiguous(name.to_owned(), count))
            }
        }
    }

    /// History back from `from` through first parents, newest first, at most
    /// `limit` changesets.
    ///
    /// # Errors
    ///
    /// [`StoreError::NotFound`], or a database error.
    pub fn log(&self, from: &ChangesetId, limit: usize) -> Result<Vec<Changeset>> {
        let mut out = Vec::new();
        let mut next = Some(from.clone());
        while let Some(id) = next.take() {
            if out.len() >= limit {
                break;
            }
            let changeset = self.changeset(&id)?;
            next.clone_from(&changeset.parent);
            out.push(changeset);
        }
        Ok(out)
    }

    /// Every changeset, in the order recorded.
    ///
    /// # Errors
    ///
    /// A database error, or [`StoreError::Damaged`] for a row that can't be
    /// read; `verify` reports those one by one.
    pub fn all(&self) -> Result<Vec<Changeset>> {
        self.raw_changesets()?
            .into_iter()
            .map(RawChangeset::into_changeset)
            .collect()
    }

    fn raw_changesets(&self) -> Result<Vec<RawChangeset>> {
        let sql = format!("SELECT {CHANGESET_COLUMNS} FROM changesets ORDER BY seq");
        let mut statement = self.connection.prepare(&sql)?;
        let rows = statement
            .query_map([], raw_changeset)?
            .collect::<std::result::Result<_, _>>()?;
        Ok(rows)
    }

    /// The ref with this name.
    ///
    /// # Errors
    ///
    /// A database error.
    pub fn get_ref(&self, name: &str) -> Result<Option<Ref>> {
        get_ref_in(&self.connection, name)
    }

    fn raw_refs(&self) -> Result<Vec<(String, String, String)>> {
        let mut statement = self
            .connection
            .prepare("SELECT name, kind, target FROM refs ORDER BY name")?;
        let rows = statement
            .query_map([], read_ref)?
            .collect::<std::result::Result<_, _>>()?;
        Ok(rows)
    }

    /// Every ref, by name.
    ///
    /// # Errors
    ///
    /// A database error.
    pub fn refs(&self) -> Result<Vec<Ref>> {
        self.raw_refs()?.into_iter().map(to_ref).collect()
    }

    /// Points a ref at a changeset, making it if needed, but only if it is
    /// still where the caller last saw it (`None`: it shouldn't exist). A
    /// baseline is made once and never moves.
    ///
    /// # Errors
    ///
    /// [`StoreError::Moved`], [`StoreError::WrongKind`],
    /// [`StoreError::NotFound`] for an unknown target, or a database error.
    pub fn set_ref(&mut self, moving: Moving<'_>, target: &ChangesetId) -> Result<()> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        check_move(&transaction, &moving)?;
        if !recorded(&transaction, target)? {
            return Err(StoreError::NotFound(target.to_string()));
        }
        point(&transaction, &moving, target)?;
        transaction.commit()?;
        Ok(())
    }

    /// Removes a ref. The changesets it named stay.
    ///
    /// # Errors
    ///
    /// [`StoreError::Current`] for the checked-out branch,
    /// [`StoreError::NotFound`], or a database error.
    pub fn delete_ref(&self, name: &str) -> Result<()> {
        if self.current()?.as_deref() == Some(name) {
            return Err(StoreError::Current(name.to_owned()));
        }
        let changed = self
            .connection
            .execute("DELETE FROM refs WHERE name = ?1", [name])?;
        if changed == 0 {
            return Err(StoreError::NotFound(name.to_owned()));
        }
        Ok(())
    }

    /// Every changeset `from` descends from, itself included, with its order
    /// recorded, in one query that reads only the links.
    fn ancestors(&self, from: &ChangesetId) -> Result<BTreeMap<ChangesetId, i64>> {
        if !recorded(&self.connection, from)? {
            return Err(StoreError::NotFound(from.to_string()));
        }
        let mut statement = self.connection.prepare(
            "WITH RECURSIVE up(id) AS (
                SELECT ?1
                UNION
                SELECT c.parent FROM changesets c JOIN up ON c.id = up.id
                    WHERE c.parent IS NOT NULL
                UNION
                SELECT c.second_parent FROM changesets c JOIN up ON c.id = up.id
                    WHERE c.second_parent IS NOT NULL
             )
             SELECT c.id, c.seq FROM changesets c JOIN up ON c.id = up.id",
        )?;
        let rows: Vec<(String, i64)> = statement
            .query_map([from.as_str()], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<std::result::Result<_, _>>()?;
        rows.into_iter()
            .map(|(id, seq)| Ok((ChangesetId::from_stored(id)?, seq)))
            .collect()
    }

    /// True if `ancestor` is `of` or one it descends from.
    ///
    /// # Errors
    ///
    /// [`StoreError::NotFound`], or a database error.
    pub fn is_ancestor(&self, ancestor: &ChangesetId, of: &ChangesetId) -> Result<bool> {
        Ok(self.ancestors(of)?.contains_key(ancestor))
    }

    /// The most recent changeset both descend from, if any.
    ///
    /// # Errors
    ///
    /// [`StoreError::NotFound`], or a database error.
    pub fn merge_base(&self, a: &ChangesetId, b: &ChangesetId) -> Result<Option<ChangesetId>> {
        let of_a = self.ancestors(a)?;
        let of_b = self.ancestors(b)?;
        Ok(of_a
            .into_iter()
            .filter(|(id, _)| of_b.contains_key(id))
            .max_by_key(|(_, seq)| *seq)
            .map(|(id, _)| id))
    }

    /// Checks everything stored: every file against its hash, every changeset
    /// against its ID, that every parent, file and ref target exists, and
    /// that the checked-out branch does. A row that can't be read is damage
    /// too, and the rest are still checked.
    ///
    /// # Errors
    ///
    /// A database error. What's damaged is returned, not an error.
    pub fn verify(&self) -> Result<Vec<Damage>> {
        let mut damage = BTreeSet::new();
        let mut found = |subject: &str, problem: String| {
            damage.insert(Damage {
                subject: subject.to_owned(),
                problem,
            });
        };
        let mut hashes = BTreeSet::new();
        for text in blob::stored_hashes(&self.connection)? {
            match BlobHash::from_stored(text.clone()) {
                Ok(hash) => {
                    if let Err(error) = blob::get_verified(&self.connection, &hash) {
                        found(&text, error.to_string());
                    }
                    hashes.insert(hash);
                }
                Err(error) => found(&text, error.to_string()),
            }
        }
        let mut changesets = Vec::new();
        for raw in self.raw_changesets()? {
            let subject = raw.id.clone();
            match raw.into_changeset() {
                Ok(changeset) => changesets.push(changeset),
                Err(error) => found(&subject, error.to_string()),
            }
        }
        let ids: BTreeSet<&ChangesetId> = changesets.iter().map(|c| &c.id).collect();
        for changeset in &changesets {
            let subject = changeset.id.as_str();
            for parent in changeset.parents() {
                if !ids.contains(parent) {
                    found(subject, format!("its parent {parent} is missing"));
                }
            }
            if !hashes.contains(&changeset.snapshot) {
                found(
                    subject,
                    format!("its file {} is missing", changeset.snapshot),
                );
            }
            if id::recorded_id(changeset) != changeset.id {
                found(subject, "its contents don't match its ID".to_owned());
            }
        }
        let mut branches = BTreeSet::new();
        for raw in self.raw_refs()? {
            let name = raw.0.clone();
            match to_ref(raw) {
                Ok(named) => {
                    if !ids.contains(&named.target) {
                        found(
                            &name,
                            format!("it names {}, which is missing", named.target),
                        );
                    }
                    if named.kind == RefKind::Branch {
                        branches.insert(name);
                    }
                }
                Err(error) => found(&name, error.to_string()),
            }
        }
        if let Some(current) = self.current()?
            && !branches.contains(&current)
        {
            found(&current, "it's checked out and isn't a branch".to_owned());
        }
        Ok(damage.into_iter().collect())
    }
}

/// A changeset row as read, before its words are checked.
struct RawChangeset {
    id: String,
    seq: i64,
    parent: Option<String>,
    second_parent: Option<String>,
    kind: String,
    state: String,
    author: String,
    author_kind: String,
    message: String,
    created: String,
    tool_version: String,
    commands: String,
    skipped: String,
    report: String,
    snapshot: String,
}

fn raw_changeset(row: &rusqlite::Row<'_>) -> rusqlite::Result<RawChangeset> {
    Ok(RawChangeset {
        id: row.get(0)?,
        seq: row.get(1)?,
        parent: row.get(2)?,
        second_parent: row.get(3)?,
        kind: row.get(4)?,
        state: row.get(5)?,
        author: row.get(6)?,
        author_kind: row.get(7)?,
        message: row.get(8)?,
        created: row.get(9)?,
        tool_version: row.get(10)?,
        commands: row.get(11)?,
        skipped: row.get(12)?,
        report: row.get(13)?,
        snapshot: row.get(14)?,
    })
}

impl RawChangeset {
    fn into_changeset(self) -> Result<Changeset> {
        Ok(Changeset {
            id: ChangesetId::from_stored(self.id)?,
            seq: self.seq,
            parent: self.parent.map(ChangesetId::from_stored).transpose()?,
            second_parent: self
                .second_parent
                .map(ChangesetId::from_stored)
                .transpose()?,
            kind: Kind::parse(&self.kind)?,
            state: State::parse(&self.state)?,
            author: self.author,
            author_kind: AuthorKind::parse(&self.author_kind)?,
            message: self.message,
            created: self.created,
            tool_version: self.tool_version,
            commands: self.commands,
            skipped: self.skipped,
            report: self.report,
            snapshot: BlobHash::from_stored(self.snapshot)?,
        })
    }
}

#[cfg(test)]
mod tests;
