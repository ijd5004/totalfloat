//! `history merge`: bring another branch's edits onto the checked-out one by
//! replaying them (ARCHITECTURE.md, History and changesets). Refused, listing every conflict at once, when a
//! changeset can't be replayed or both sides changed the same thing to
//! different values.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::{Value, json};
use totalfloat_diff::compare;
use totalfloat_model::Schedule;
use totalfloat_store::{Changeset, ChangesetId, Kind, Moving, RefKind, Store};
use totalfloat_xer::RawXer;

use super::replay::{cancel_reverts, conflict_error, replay};
use super::{Asked, read_head, record_change, store_error};
use crate::error::{CliError, Problem, exit, read_xer_bytes};
use crate::loaded::Loaded;
use crate::output::print_line;

/// The other branch's changesets after the base, oldest first, along its
/// first parents, or `None` if the base isn't on that line.
fn since(
    store: &Store,
    base: &ChangesetId,
    tip: &ChangesetId,
) -> Result<Option<Vec<Changeset>>, CliError> {
    let mut out = Vec::new();
    let mut next = Some(tip.clone());
    while let Some(id) = next.take() {
        if id == *base {
            out.reverse();
            return Ok(Some(out));
        }
        let changeset = store.changeset(&id).map_err(store_error)?;
        next.clone_from(&changeset.parent);
        out.push(changeset);
    }
    Ok(None)
}

/// Something one side changed from the base, by the source IDs replay uses,
/// so a rename doesn't hide a clash.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Thing {
    /// A field of an activity the base holds, by `task_id`.
    Field(u64, &'static str),
    /// An activity the base holds, deleted.
    Deleted(u64),
    /// A new activity, by its Activity ID.
    Added(String),
    /// A link between two activities, by their IDs (or new codes), and type.
    Link(String, String, &'static str),
}

/// One side's changes, by project ID, with how to name each in a message.
type Changes = BTreeMap<(u64, Thing), (String, String)>;

/// What one side changed from the base: each thing, with its name for a
/// message and its new value.
fn changes(base: &[Schedule], side: &[Schedule]) -> Changes {
    let mut out = BTreeMap::new();
    for before in base {
        let project = before.project.id.0;
        let Some(after) = side.iter().find(|s| s.project.id.0 == project) else {
            continue;
        };
        // Codes on this side, then in the base, to source IDs.
        let id_of = |code: &str| {
            after
                .activity_by_code(code)
                .or_else(|| before.activity_by_code(code))
                .filter(|a| before.activities.contains_key(&a.id))
                .map_or_else(|| format!("new {code}"), |a| a.id.0.to_string())
        };
        let comparison = compare(before, after);
        for changed in &comparison.changed {
            let Some(activity) = after.activity_by_code(&changed.code) else {
                continue;
            };
            for field in &changed.changes {
                let what = format!("{} {}", changed.code, field.field);
                out.insert(
                    (project, Thing::Field(activity.id.0, field.field)),
                    (what, field.to.clone()),
                );
            }
        }
        for gone in &comparison.deleted {
            if let Some(activity) = before.activity_by_code(&gone.code) {
                let what = format!("{} deleted", gone.code);
                out.insert(
                    (project, Thing::Deleted(activity.id.0)),
                    (what, String::new()),
                );
            }
        }
        for added in &comparison.added {
            let what = format!("{} added", added.code);
            out.insert(
                (project, Thing::Added(added.code.clone())),
                (what, added.name.clone()),
            );
        }
        for link in comparison
            .links_added
            .iter()
            .chain(&comparison.links_deleted)
            .chain(&comparison.links_changed)
        {
            let what = format!("link {} {} {}", link.predecessor, link.kind, link.successor);
            let to = link.to_lag.clone().unwrap_or_else(|| "deleted".to_owned());
            let thing = Thing::Link(id_of(&link.predecessor), id_of(&link.successor), link.kind);
            out.insert((project, thing), (what, to));
        }
    }
    out
}

fn clash(project: u64, what: &str, message: &str) -> Problem {
    Problem {
        edit: None,
        source: format!("project {project}"),
        code: "both-changed",
        message: format!("{what}: {message}"),
        hint: "change one side to match, or take the change out of one, then merge again"
            .to_owned(),
        depends_on: None,
        line: None,
        column: None,
    }
}

/// True if `thing` touches the activity with this source ID.
fn touches(thing: &Thing, id: u64) -> bool {
    let id_text = id.to_string();
    match thing {
        Thing::Field(of, _) => *of == id,
        Thing::Link(from, to, _) => *from == id_text || *to == id_text,
        Thing::Deleted(_) | Thing::Added(_) => false,
    }
}

/// Every clash between the two sides' changes: the same thing to different
/// values; the same add or delete on both, which replay can't do twice; and
/// an activity one side deleted while the other changed or linked it.
fn clashes(base: &[Schedule], ours: &[Schedule], theirs: &[Schedule]) -> Vec<Problem> {
    let (mine, other) = (changes(base, ours), changes(base, theirs));
    let shown = |v: &str| {
        if v.is_empty() {
            "it deleted".to_owned()
        } else {
            v.to_owned()
        }
    };
    let mut out = Vec::new();
    for (key, (what, value)) in &other {
        let Some((_, ours)) = mine.get(key) else {
            continue;
        };
        // A field set to the same value replays harmlessly; an add, delete
        // or link change can't be made twice.
        let structural = !matches!(key.1, Thing::Field(..));
        if ours != value {
            let message = format!(
                "this branch has {}, and the other has {}",
                shown(ours),
                shown(value)
            );
            out.push(clash(key.0, what, &message));
        } else if structural {
            let message = "both branches made this change, and it can't be made twice";
            out.push(clash(key.0, what, message));
        }
    }
    for (deleter, changer) in [(&mine, &other), (&other, &mine)] {
        for ((project, thing), (what, _)) in deleter {
            let Thing::Deleted(id) = thing else {
                continue;
            };
            let changed = changer.keys().any(|(p, t)| p == project && touches(t, *id));
            if changed {
                let message = "one branch deleted it, and the other changed it".to_owned();
                out.push(clash(*project, what, &message));
            }
        }
    }
    out.sort_by(|a, b| (&a.source, &a.message).cmp(&(&b.source, &b.message)));
    out.dedup_by(|a, b| a.source == b.source && a.message == b.message);
    out
}

/// The changesets that can't be replayed, each as a problem.
fn unreplayable(changesets: &[Changeset]) -> Vec<Problem> {
    changesets
        .iter()
        .filter(|c| c.kind != Kind::Edit)
        .map(|c| Problem {
            edit: None,
            source: format!("changeset {}", c.id.short()),
            code: "not-replayable",
            message: format!(
                "{} is {} {}, which holds a whole file rather than edits",
                c.id.short(),
                if c.kind == Kind::Import { "an" } else { "a" },
                c.kind
            ),
            hint:
                "only edits can be merged; bring the other branch's file in with `history import`"
                    .to_owned(),
            depends_on: None,
            line: None,
            column: None,
        })
        .collect()
}

fn refused(problems: Vec<Problem>, other: &str) -> CliError {
    let mut error = CliError::new(
        "merge-conflict",
        format!("{other} can't be merged as it stands, so nothing was recorded"),
        "each problem says what clashes",
    );
    error.problems = problems;
    error
}

fn file_of(store: &Store, id: &ChangesetId) -> Result<Vec<u8>, CliError> {
    let changeset = store.changeset(id).map_err(store_error)?;
    store.blob(&changeset.snapshot).map_err(store_error)
}

fn print(json: bool, doc: &Value, text: &str) {
    if json {
        print_line(&serde_json::to_string_pretty(doc).unwrap_or_default());
    } else {
        print_line(text);
    }
}

/// The other branch's changesets since the base, replayed onto ours, with
/// our file as read. Every conflict is listed at once.
fn replayed(
    store: &Store,
    path: &Path,
    (base, tip): (&ChangesetId, &ChangesetId),
    other: &str,
    current: RawXer,
) -> Result<(Vec<Changeset>, Loaded, RawXer), CliError> {
    let Some(theirs) = since(store, base, tip)? else {
        return Err(refused(
            vec![Problem {
                edit: None,
                source: format!("branch {other}"),
                code: "not-replayable",
                message: format!("{other} reaches the common point only through a merge"),
                hint: "merge in the other direction, or bring its file in with `history import`"
                    .to_owned(),
                depends_on: None,
                line: None,
                column: None,
            }],
            other,
        ));
    };
    let theirs = cancel_reverts(theirs);
    let mut problems = unreplayable(&theirs);
    let loaded = |id: &ChangesetId| -> Result<Loaded, CliError> {
        Ok(Loaded::new(read_xer_bytes(file_of(store, id)?, path)?))
    };
    let (base_file, their_file) = (loaded(base)?, loaded(tip)?);
    let ours = Loaded::new(current.clone());
    problems.extend(clashes(
        &base_file.schedules,
        &ours.schedules,
        &their_file.schedules,
    ));
    if !problems.is_empty() {
        return Err(refused(problems, other));
    }
    let what = format!("can't merge {other}");
    let merged =
        replay(current, &theirs).map_err(|c| conflict_error("merge-conflict", &what, c))?;
    Ok((theirs, ours, merged))
}

/// The merged file's impact on the branch, as a short `totalfloat.diff/1`.
fn impact(path: &Path, branch: &str, ours: &Loaded, merged: RawXer) -> Value {
    let label = path.file_name().unwrap_or_default().to_string_lossy();
    crate::diff::report(
        &format!("{label}@{branch}"),
        ours,
        &format!("{label}@merge"),
        &Loaded::new(merged),
    )
    .map_or(Value::Null, |mut r| {
        r.limit(25);
        serde_json::to_value(&r).unwrap_or(Value::Null)
    })
}

/// The head of the branch to merge in, refusing anything but a branch.
fn branch_tip(store: &Store, other: &str) -> Result<ChangesetId, CliError> {
    match store.get_ref(other).map_err(store_error)? {
        Some(named) if named.kind == RefKind::Branch => Ok(named.target),
        Some(_) => Err(CliError::new(
            "not-a-branch",
            format!("{other} isn't a branch, and only a branch can be merged"),
            "make a branch from it with `history branch`, then merge that",
        )),
        None => Err(store_error(totalfloat_store::StoreError::NotFound(
            other.to_owned(),
        ))),
    }
}

fn no_common_point(other: &str, branch: &str) -> CliError {
    CliError::new(
        "no-common-point",
        format!("{other} and {branch} share no history"),
        "compare them with `history diff` instead",
    )
}

/// What to merge, and how the merge is recorded.
#[derive(Debug, Clone, Copy)]
pub struct Merging<'a> {
    /// The other branch.
    pub other: &'a str,
    /// Look, and record nothing.
    pub dry_run: bool,
    /// Why, and who.
    pub asked: Asked<'a>,
}

/// Runs `history merge`.
///
/// # Errors
///
/// `merge-conflict` listing every conflict, `no-such-ref`, `branch-moved`, or
/// what the store refuses.
pub fn run(path: &Path, merging: Merging<'_>, json: bool) -> Result<u8, CliError> {
    let Merging {
        other,
        dry_run,
        asked,
    } = merging;
    let (head, current) = read_head(path, None)?;
    let mut store = super::open(path)?;
    let tip = branch_tip(&store, other)?;
    let base = store.merge_base(&head.id, &tip).map_err(store_error)?;
    let base = base.ok_or_else(|| no_common_point(other, &head.branch))?;
    let mut doc = json!({
        "schema": "totalfloat.history-merge/1",
        "branch": head.branch,
        "from": other,
        "base": base.as_str(),
        "fast_forward": false,
        "replayed": [],
        "recorded": null,
        "impact": null,
    });
    if base == tip {
        print(
            json,
            &doc,
            &format!("{} already has everything on {other}", head.branch),
        );
        return Ok(exit::OK);
    }
    if base == head.id {
        doc["fast_forward"] = json!(true);
        if !dry_run {
            let moving = Moving {
                name: &head.branch,
                kind: RefKind::Branch,
                expected: Some(&head.id),
            };
            store.set_ref(moving, &tip).map_err(store_error)?;
        }
        let verb = if dry_run { "Would move" } else { "Moved" };
        let text = format!(
            "{verb} {} forward to {} from {other}; nothing to replay",
            head.branch,
            tip.short()
        );
        print(json, &doc, &text);
        return Ok(exit::OK);
    }
    let (theirs, ours, merged) = replayed(&store, path, (&base, &tip), other, current)?;
    let mut bytes = Vec::new();
    merged
        .write(&mut bytes)
        .map_err(|e| CliError::new("write-failed", e.to_string(), "try again"))?;
    let impact = impact(path, &head.branch, &ours, merged.clone());
    let ids: Vec<&str> = theirs.iter().map(|c| c.id.as_str()).collect();
    doc["replayed"] = json!(ids);
    doc["impact"] = impact.clone();
    if dry_run {
        let text = format!(
            "Would replay {} changeset(s) from {other}; nothing recorded",
            ids.len()
        );
        print(json, &doc, &text);
        return Ok(exit::OK);
    }
    let commands = json!({ "merges": tip.as_str(), "replayed": ids }).to_string();
    let parts = (commands, "[]".to_owned(), impact.to_string());
    let recorded = record_change(
        path,
        &head,
        (Kind::Merge, Some(tip.clone())),
        bytes,
        parts,
        asked,
    )?;
    let text = recorded.as_ref().map_or_else(
        || {
            format!(
                "{other}'s edits leave {} as it is, so nothing was recorded",
                head.branch
            )
        },
        |r| {
            format!(
                "Merged {other}, replaying {} changeset(s); recorded {} on {}",
                ids.len(),
                r.short_id,
                r.branch
            )
        },
    );
    doc["recorded"] = json!(recorded);
    print(json, &doc, &text);
    Ok(exit::OK)
}
