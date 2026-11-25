//! Proposals (ARCHITECTURE.md, History and changesets): changesets, usually an agent's, that wait with their
//! schedule impact until a person accepts or rejects them. Every decision is
//! kept: who, when and why.

use std::path::Path;

use serde_json::{Value, json};
use totalfloat_store::{
    Changeset, ChangesetId, Kind, Moving, NewChangeset, RefKind, Review, Reviewing, State, Store,
};

use super::replay::{conflict_error, replay};
use super::{
    Asked, author, branch_head, now, open, proposal_branch, read_head, reason, resolve_point,
    store_error, who,
};
use crate::error::{CliError, exit};
use crate::loaded::Loaded;
use crate::output::print_line;

/// The proposal ref naming a changeset, if it has one.
fn proposal_ref(store: &Store, id: &ChangesetId) -> Result<Option<String>, CliError> {
    Ok(store
        .refs()
        .map_err(store_error)?
        .into_iter()
        .find(|r| r.kind == RefKind::Proposal && r.target == *id)
        .map(|r| r.name))
}

/// A proposal's headline impact: how far each project's finish moved.
fn headline(changeset: &Changeset) -> String {
    let report: Value = serde_json::from_str(&changeset.report).unwrap_or(Value::Null);
    let moved: Vec<String> = report["projects"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|p| {
            let now = p["scheduled_finish"].as_str()?;
            let code = p["code"].as_str().unwrap_or_default();
            Some(match p["previous_finish"].as_str() {
                Some(was) if was == now => format!("{code} finish unchanged"),
                Some(was) => format!("{code} finish {was} to {now}"),
                None => format!("{code} finish {now}, with none stored before"),
            })
        })
        .collect();
    if moved.is_empty() {
        "no impact recorded".to_owned()
    } else {
        moved.join("; ")
    }
}

/// `history proposals`: open proposals, with who made them and their impact.
///
/// # Errors
///
/// What the store refuses.
pub fn list(path: &Path, json: bool) -> Result<u8, CliError> {
    let store = open(path)?;
    let mut rows = Vec::new();
    for named in store.refs().map_err(store_error)? {
        if named.kind != RefKind::Proposal {
            continue;
        }
        let proposal = store.changeset(&named.target).map_err(store_error)?;
        rows.push(json!({
            "name": named.name,
            "id": proposal.id.as_str(),
            "short_id": proposal.id.short(),
            "author": proposal.author,
            "author_kind": proposal.author_kind.as_str(),
            "created": proposal.created,
            "who": who(&proposal),
            "reason": reason(&proposal),
            "for_branch": proposal_branch(&named.name),
            "parent": proposal.parent.as_ref().map(ChangesetId::as_str),
            "impact": headline(&proposal),
        }));
    }
    if json {
        let doc = json!({
            "schema": "totalfloat.history-proposals/1",
            "store": path.display().to_string(),
            "proposals": rows,
        });
        print_line(&serde_json::to_string_pretty(&doc).unwrap_or_default());
        return Ok(exit::OK);
    }
    if rows.is_empty() {
        print_line("No proposals waiting");
        return Ok(exit::OK);
    }
    let lines: Vec<String> = rows
        .iter()
        .map(|row| {
            format!(
                "{}  for {}  by {}: {}\n    {}",
                row["short_id"].as_str().unwrap_or_default(),
                row["for_branch"].as_str().unwrap_or("?"),
                row["who"].as_str().unwrap_or_default(),
                row["reason"].as_str().unwrap_or_default(),
                row["impact"].as_str().unwrap_or_default(),
            )
        })
        .collect();
    print_line(&lines.join("\n"));
    Ok(exit::OK)
}

/// The proposal a name or ID points at, refusing anything already decided.
fn open_proposal(store: &Store, path: &Path, at: &str) -> Result<Changeset, CliError> {
    let id = resolve_point(store, path, Some(at))?;
    let changeset = store.changeset(&id).map_err(store_error)?;
    if changeset.state != State::Proposed {
        return Err(CliError::new(
            "not-a-proposal",
            format!(
                "{} is {}, not waiting for review",
                id.short(),
                changeset.state
            ),
            "`history proposals` lists those waiting",
        ));
    }
    Ok(changeset)
}

/// Refuses an agent accepting its own proposal: a proposal waits for
/// someone else to decide.
fn not_own(proposal: &Changeset, asked: Asked<'_>) -> Result<(), CliError> {
    let (reviewer, kind) = author(asked.author);
    if kind == totalfloat_store::AuthorKind::Agent && reviewer == proposal.author {
        return Err(CliError::new(
            "own-proposal",
            format!(
                "{reviewer} made {}, so it can't accept it",
                proposal.id.short()
            ),
            "a proposal waits for someone else to decide; ask the user",
        ));
    }
    Ok(())
}

fn review_of(changeset: &Changeset, state: State, asked: Asked<'_>) -> Result<Review, CliError> {
    let (author, author_kind) = author(asked.author);
    Ok(Review {
        changeset: changeset.id.clone(),
        state,
        author,
        author_kind,
        message: asked.message.unwrap_or_default().to_owned(),
        created: now()?,
    })
}

fn say(json: bool, doc: &Value, text: &str) {
    if json {
        print_line(&serde_json::to_string_pretty(doc).unwrap_or_default());
    } else {
        print_line(text);
    }
}

/// `history accept`: put a proposal on its branch. If the branch is where it
/// was proposed from, the branch moves to it; otherwise it's replayed on the
/// branch's head and recorded there, still as its proposer's work, with a
/// fresh impact. The decision lands with the change, in one transaction.
///
/// # Errors
///
/// `not-a-proposal`, `accept-conflict` naming what no longer applies,
/// `branch-moved`, or what the store refuses.
pub fn accept(path: &Path, at: &str, asked: Asked<'_>, json: bool) -> Result<u8, CliError> {
    let mut store = open(path)?;
    let proposal = open_proposal(&store, path, at)?;
    not_own(&proposal, asked)?;
    let name = proposal_ref(&store, &proposal.id)?;
    let branch = match name.as_deref().and_then(proposal_branch) {
        Some(branch) => branch.to_owned(),
        None => branch_head(&store, None)?.0,
    };
    let (head, current) = read_head(path, Some(&branch))?;
    let review = review_of(&proposal, State::Accepted, asked)?;
    let mut doc = json!({
        "schema": "totalfloat.history-review/1",
        "proposal": proposal.id.as_str(),
        "state": "accepted",
        "branch": branch,
        "replayed": false,
        "recorded": proposal.id.as_str(),
    });
    let onto = Moving {
        name: &branch,
        kind: RefKind::Branch,
        expected: Some(&head.id),
    };
    if proposal.parent.as_ref() == Some(&head.id) {
        let reviewing = Reviewing {
            review,
            drop_ref: name.as_deref(),
            advance: Some(onto),
        };
        store.review(&reviewing).map_err(store_error)?;
        say(
            json,
            &doc,
            &format!("Accepted {} onto {branch}", proposal.id.short()),
        );
        return Ok(exit::OK);
    }
    // The branch moved on: replay the proposal's edits on its head.
    let what = format!("can't accept {}", proposal.id.short());
    let merged = replay(current.clone(), std::slice::from_ref(&proposal))
        .map_err(|c| conflict_error("accept-conflict", &what, c))?;
    let mut bytes = Vec::new();
    merged
        .write(&mut bytes)
        .map_err(|e| CliError::new("write-failed", e.to_string(), "try again"))?;
    let label = path.file_name().unwrap_or_default().to_string_lossy();
    let impact = crate::diff::report(
        &format!("{label}@{branch}"),
        &Loaded::new(current),
        &format!("{label}@accepted"),
        &Loaded::new(merged),
    )
    .map_or(Value::Null, |mut r| {
        r.limit(25);
        serde_json::to_value(&r).unwrap_or(Value::Null)
    });
    let new = NewChangeset {
        parent: Some(head.id.clone()),
        second_parent: None,
        kind: Kind::Edit,
        state: State::Accepted,
        author: proposal.author.clone(),
        author_kind: proposal.author_kind,
        message: format!(
            "{} (proposal {}, accepted by {})",
            proposal.message,
            proposal.id.short(),
            review.author
        ),
        created: review.created.clone(),
        tool_version: env!("CARGO_PKG_VERSION").to_owned(),
        commands: proposal.commands.clone(),
        skipped: proposal.skipped.clone(),
        report: impact.to_string(),
        snapshot: bytes,
    };
    let reviewing = Reviewing {
        review,
        drop_ref: name.as_deref(),
        advance: None,
    };
    let recorded = store
        .record_reviewed(&new, onto, &reviewing)
        .map_err(store_error)?;
    doc["replayed"] = json!(true);
    doc["recorded"] = json!(recorded.id.as_str());
    let text = format!(
        "Accepted {}, replayed on {branch}'s newer head as {}",
        proposal.id.short(),
        recorded.id.short()
    );
    say(json, &doc, &text);
    Ok(exit::OK)
}

/// `history reject`: turn a proposal down, with a reason. It stays in the
/// store, out of the way.
///
/// # Errors
///
/// `reason-needed`, `not-a-proposal`, or what the store refuses.
pub fn reject(path: &Path, at: &str, asked: Asked<'_>, json: bool) -> Result<u8, CliError> {
    if asked.message.is_none_or(|m| m.trim().is_empty()) {
        return Err(CliError::new(
            "reason-needed",
            "a rejection needs a reason",
            "give one with -m, so whoever proposed it knows why",
        ));
    }
    let mut store = open(path)?;
    let proposal = open_proposal(&store, path, at)?;
    let name = proposal_ref(&store, &proposal.id)?;
    let reviewing = Reviewing {
        review: review_of(&proposal, State::Rejected, asked)?,
        drop_ref: name.as_deref(),
        advance: None,
    };
    store.review(&reviewing).map_err(store_error)?;
    let doc = json!({
        "schema": "totalfloat.history-review/1",
        "proposal": proposal.id.as_str(),
        "state": "rejected",
        "branch": name.as_deref().and_then(proposal_branch),
        "replayed": false,
        "recorded": null,
    });
    say(
        json,
        &doc,
        &format!("Rejected {}; it stays in the store", proposal.id.short()),
    );
    Ok(exit::OK)
}
