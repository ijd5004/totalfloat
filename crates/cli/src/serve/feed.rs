//! The live change feed: while `serve` shows a store, it looks at the store
//! twice a second and tells open pages what changed, whoever changed it: the
//! page itself, the command line, or an agent through MCP in another process.
//!
//! Each event's data is a `totalfloat.event/1` document
//! (docs/schemas/event-1.schema.json); its SSE event name is its `event`.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::Duration;

use serde_json::{Value, json};
use totalfloat_server::Events;
use totalfloat_store::{Changeset, ChangesetId, RefKind, Store};

use super::App;
use super::view::{Source, View};
use crate::error::CliError;
use crate::history::{log_entry, open, proposal_branch, store_error};

/// The `schema` of every event's data.
pub const SCHEMA: &str = "totalfloat.event/1";

/// How often the store is looked at.
pub const EVERY: Duration = Duration::from_millis(500);

/// The refs, by name: their kind and target.
type Refs = BTreeMap<String, (RefKind, ChangesetId)>;

/// What a look at the store finds: the highest changeset `seq`, the refs, and
/// the checked-out branch. Three small reads, however large the store.
#[derive(PartialEq, Eq)]
struct Look {
    seq: i64,
    refs: Refs,
    current: Option<String>,
}

fn look(store: &Store) -> Result<Look, CliError> {
    let refs = store
        .refs()
        .map_err(store_error)?
        .into_iter()
        .map(|r| (r.name, (r.kind, r.target)))
        .collect();
    Ok(Look {
        seq: store.latest_seq().map_err(store_error)?,
        refs,
        current: store.current().map_err(store_error)?,
    })
}

/// The feed of one store, and what it last told pages.
pub struct Feed {
    store: Store,
    seen: Look,
    /// The view last announced: its branch and head.
    shown: Option<(Option<String>, String)>,
    /// Whether the store changed since the view was last brought up to date.
    /// It starts set, since the store may have moved while the first view
    /// was calculated, and stays set until a refresh succeeds.
    stale: bool,
    /// The last failure reported, so a lasting one is said once.
    failing: Option<String>,
}

impl Feed {
    /// The feed of the store `app` shows, or `None` for a plain file, which
    /// has nothing to watch.
    ///
    /// # Errors
    ///
    /// What opening the store refuses.
    pub fn new(app: &App) -> Result<Option<Feed>, CliError> {
        let view = app.current();
        let Source::Store { path, .. } = view.source() else {
            return Ok(None);
        };
        let store = open(path)?;
        let seen = look(&store)?;
        Ok(Some(Feed {
            store,
            seen,
            shown: view.point(),
            stale: true,
            failing: None,
        }))
    }

    /// Looks every [`EVERY`] until `stop` is dropped or sent on. A failure is
    /// said once on stderr, and the feed carries on.
    pub fn run(mut self, app: &App, events: &Events, stop: &Receiver<()>) {
        while let Err(RecvTimeoutError::Timeout) = stop.recv_timeout(EVERY) {
            match self.tick(app, events).1 {
                Ok(()) => self.failing = None,
                Err(error) => {
                    if self.failing.as_deref() != Some(error.message.as_str()) {
                        eprintln!(
                            "note: couldn't follow the store ({}); pages may be behind",
                            error.message
                        );
                        self.failing = Some(error.message);
                    }
                }
            }
        }
    }

    /// Looks once, and returns what it published and whether the view could
    /// be brought up to date. What changed in the store is published first,
    /// whatever happens to the view. Then, if the store changed since the
    /// view was last brought up to date, the view is calculated again, but
    /// only if the point it shows moved. Last comes `view`, if the view shown
    /// isn't the one last announced. An unchanged store costs three small
    /// reads and nothing else.
    pub fn tick(&mut self, app: &App, events: &Events) -> (Vec<Value>, Result<(), CliError>) {
        let mut sent = match self.changes() {
            Ok(changes) => changes,
            Err(error) => return (Vec::new(), Err(error)),
        };
        publish(events, &sent);
        let refreshed = if self.stale { catch_up(app) } else { Ok(true) };
        if matches!(refreshed, Ok(true)) {
            self.stale = false;
        }
        let view = app.current();
        let point = view.point();
        if point != self.shown {
            let event = view_event(&view);
            publish(events, std::slice::from_ref(&event));
            sent.push(event);
            self.shown = point;
        }
        (sent, refreshed.map(|_| ()))
    }

    /// What changed in the store since the last look, as events. Nothing is
    /// taken as seen unless all of it could be read, so a failed read is
    /// tried again whole.
    fn changes(&mut self) -> Result<Vec<Value>, CliError> {
        let now = look(&self.store)?;
        if now == self.seen {
            return Ok(Vec::new());
        }
        let mut changes: Vec<Value> = recorded(&self.store, self.seen.seq, now.seq)?
            .iter()
            .map(|changeset| changeset_event(changeset, &now.refs))
            .collect();
        changes.extend(ref_events(&self.store, &self.seen.refs, &now.refs)?);
        self.seen = now;
        self.stale = true;
        Ok(changes)
    }
}

/// The changesets recorded after `after` up to `upto`: one recorded since the
/// look that found `upto` waits for the next look, so none is told twice.
fn recorded(store: &Store, after: i64, upto: i64) -> Result<Vec<Changeset>, CliError> {
    let mut since = store.since(after).map_err(store_error)?;
    since.retain(|changeset| changeset.seq <= upto);
    Ok(since)
}

fn publish(events: &Events, sent: &[Value]) {
    for event in sent {
        let kind = event["event"].as_str().unwrap_or("change");
        // None listening is fine, and the kinds are all well formed.
        let _ = events.send(kind, event.to_string());
    }
}

/// Shows the point served as it is now, if it moved: the one place, with
/// `write::change`, that calculates a view again. It needs the turn for
/// changes, so the two never race, but never waits for it: a change under
/// way shows its own result, and a waiting feed could hold up shutdown.
/// `Ok(false)` means the turn was taken; try again at the next look.
fn catch_up(app: &App) -> Result<bool, CliError> {
    let Ok(_turn) = app.writing.try_lock() else {
        return Ok(false);
    };
    if let Some(view) = super::moved(&app.location, &app.current(), None)? {
        app.show(view);
    }
    Ok(true)
}

/// The branch a changeset is the head of, or the branch it's proposed for.
fn branch_of(id: &ChangesetId, refs: &Refs) -> Option<String> {
    let named = |wanted: RefKind| {
        refs.iter()
            .find(|(_, (kind, target))| *kind == wanted && target == id)
            .map(|(name, _)| name)
    };
    named(RefKind::Branch).cloned().or_else(|| {
        named(RefKind::Proposal)
            .and_then(|name| proposal_branch(name))
            .map(str::to_owned)
    })
}

fn changeset_event(changeset: &Changeset, refs: &Refs) -> Value {
    json!({
        "schema": SCHEMA,
        "event": "changeset",
        "branch": branch_of(&changeset.id, refs),
        "changeset": log_entry(changeset),
    })
}

/// Branches made, moved and deleted, and proposals made and decided, by
/// name.
fn ref_events(store: &Store, before: &Refs, after: &Refs) -> Result<Vec<Value>, CliError> {
    let names: BTreeSet<&String> = before.keys().chain(after.keys()).collect();
    let mut sent = Vec::new();
    for name in names {
        let (was, is) = (before.get(name), after.get(name));
        let kind = is.or(was).map(|(kind, _)| *kind);
        match kind {
            Some(RefKind::Branch) => {
                let change = match (was, is) {
                    (None, Some(_)) => "created",
                    (Some(_), None) => "deleted",
                    (Some((_, from)), Some((_, to))) if from != to => "moved",
                    _ => continue,
                };
                sent.push(json!({
                    "schema": SCHEMA,
                    "event": "branch",
                    "change": change,
                    "name": name,
                    "head": is.map(|(_, id)| id.as_str()),
                    "from": was.map(|(_, id)| id.as_str()),
                }));
            }
            Some(RefKind::Proposal) => {
                let (change, id) = match (was, is) {
                    (None, Some((_, id))) => ("new", id),
                    (Some((_, id)), None) => ("decided", id),
                    _ => continue,
                };
                let changeset = store.changeset(id).map_err(store_error)?;
                sent.push(json!({
                    "schema": SCHEMA,
                    "event": "proposal",
                    "change": change,
                    "branch": proposal_branch(name),
                    "changeset": log_entry(&changeset),
                }));
            }
            Some(RefKind::Baseline) | None => {}
        }
    }
    Ok(sent)
}

fn view_event(view: &View) -> Value {
    let (branch, head) = view.point().unzip();
    json!({
        "schema": SCHEMA,
        "event": "view",
        "label": view.label(),
        "branch": branch.flatten(),
        "head": head,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A changeset recorded between a look and the read of what's new is left
    /// for the next look.
    #[test]
    fn a_changeset_newer_than_the_look_waits_for_the_next() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("plan.tfdb");
        let mut store = Store::create(&path).unwrap();
        let mut parent = None;
        for n in 1..=3u8 {
            let new = totalfloat_store::NewChangeset {
                parent: parent.clone(),
                second_parent: None,
                kind: totalfloat_store::Kind::Edit,
                state: totalfloat_store::State::Accepted,
                author: "Pat".into(),
                author_kind: totalfloat_store::AuthorKind::Human,
                message: format!("{n}"),
                created: "2026-11-02T09:00:00Z".into(),
                tool_version: "0".into(),
                commands: "{}".into(),
                skipped: "[]".into(),
                report: "{}".into(),
                snapshot: vec![n],
            };
            parent = Some(store.record(&new, None).unwrap().id);
        }
        let seqs = |found: Vec<Changeset>| found.iter().map(|c| c.seq).collect::<Vec<_>>();
        assert_eq!(seqs(recorded(&store, 0, 2).unwrap()), [1, 2]);
        assert_eq!(seqs(recorded(&store, 2, 3).unwrap()), [3]);
    }
}
