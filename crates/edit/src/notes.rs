//! Notes on activities and WBS nodes, each under a notebook topic.

use std::collections::BTreeMap;

use totalfloat_model::{Note, NotebookTopicId, Schedule};

use crate::{EditError, NextIds, find_mut, find_wbs, refuse};

/// What a note is on: an activity or a WBS node, by the names the edit gives.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Target<'a> {
    pub(crate) activity: Option<&'a str>,
    pub(crate) wbs: Option<&'a str>,
}

/// What the edit does to the note.
pub(crate) enum Change<'a> {
    Add(&'a str),
    Set(&'a str),
    Delete,
}

/// The topic a name or `memo_type_id` means here, if it takes notes on this
/// kind of thing.
fn topic(schedule: &Schedule, wanted: &str, on_wbs: bool) -> Result<NotebookTopicId, EditError> {
    let lower = wanted.to_lowercase();
    let fits = |t: &&totalfloat_model::NotebookTopic| {
        if on_wbs { t.for_wbs } else { t.for_activities }
    };
    let named: Vec<_> = schedule
        .notebook_topics
        .values()
        .filter(|t| t.name.to_lowercase() == lower || t.id.0.to_string() == wanted)
        .collect();
    // A name two topics share means the one for this kind, if only one is.
    let fitting: Vec<_> = named.iter().copied().filter(fits).collect();
    let found = if fitting.len() == 1 { fitting } else { named };
    let what = if on_wbs { "WBS nodes" } else { "activities" };
    match found.as_slice() {
        [one] if fits(one) => Ok(one.id),
        [one] => refuse(
            "wrong-topic",
            format!("the notebook topic {} isn't for {what}", one.name),
            topics_hint(schedule, on_wbs),
        ),
        [] => refuse(
            "no-such-topic",
            format!("no notebook topic is named {wanted}"),
            topics_hint(schedule, on_wbs),
        ),
        _ => refuse(
            "ambiguous-topic",
            format!("{} notebook topics are named {wanted}", found.len()),
            "use one's `memo_type_id`",
        ),
    }
}

fn topics_hint(schedule: &Schedule, on_wbs: bool) -> String {
    let names: Vec<&str> = schedule
        .notebook_topics
        .values()
        .filter(|t| if on_wbs { t.for_wbs } else { t.for_activities })
        .map(|t| t.name.as_str())
        .collect();
    if names.is_empty() {
        "the file has no notebook topics for these; add one in P6 first".to_owned()
    } else {
        format!("topics: {}", names.join(", "))
    }
}

/// The next free ID among the notes of one kind.
fn next_id<'a>(next: u64, held: impl Iterator<Item = &'a Note>) -> u64 {
    let free = held.map(|n| n.id + 1).max().unwrap_or(0);
    next.max(free)
}

const WBS_HINT: &str = "`query --columns code,wbs` shows WBS codes";

fn check_text(text: &str) -> Result<(), EditError> {
    if text.trim().is_empty() {
        return refuse(
            "empty-note",
            "a note needs some text",
            "delete_note takes a note away",
        );
    }
    Ok(())
}

pub(crate) fn change_note(
    schedule: &mut Schedule,
    ids: &mut NextIds,
    target: Target<'_>,
    wanted: &str,
    change: &Change<'_>,
) -> Result<String, EditError> {
    let (on, on_wbs) = match (target.activity, target.wbs) {
        (Some(activity), None) => (activity, false),
        (None, Some(wbs)) => (wbs, true),
        _ => {
            return refuse(
                "note-target",
                "a note is on an activity or a WBS node",
                "give `activity` or `wbs`, not both",
            );
        }
    };
    let kind = topic(schedule, wanted, on_wbs)?;
    let name = schedule.notebook_topics[&kind].name.clone();
    if let Change::Add(text) | Change::Set(text) = change {
        check_text(text)?;
    }
    // Only an added note takes an ID; `ids` moves once it's added.
    let new_id = matches!(change, Change::Add(_)).then(|| {
        if on_wbs {
            let held = schedule.wbs.values().flat_map(|w| w.notes.values());
            next_id(ids.wbs_note, held)
        } else {
            let held = schedule.activities.values().flat_map(|a| a.notes.values());
            next_id(ids.activity_note, held)
        }
    });
    let notes: &mut BTreeMap<NotebookTopicId, Note> = if on_wbs {
        let node = find_wbs(schedule, on)?;
        match schedule.wbs.get_mut(&node) {
            Some(node) => &mut node.notes,
            None => return refuse("no-such-wbs", format!("no WBS node is {on}"), WBS_HINT),
        }
    } else {
        &mut find_mut(schedule, on)?.notes
    };
    let said = match change {
        Change::Add(text) => {
            if notes.contains_key(&kind) {
                return refuse(
                    "duplicate-note",
                    format!("{on} already has a {name} note"),
                    "set_note replaces it",
                );
            }
            notes.insert(
                kind,
                Note {
                    id: new_id.unwrap_or_default(),
                    html: Note::html_of(text),
                    source: None,
                },
            );
            format!("{on} gained a {name} note")
        }
        Change::Set(text) => {
            let Some(note) = notes.get_mut(&kind) else {
                return refuse(
                    "no-note",
                    format!("{on} has no {name} note"),
                    "add_note adds one",
                );
            };
            note.html = Note::html_of(text);
            format!("{on}'s {name} note replaced")
        }
        Change::Delete => {
            if notes.remove(&kind).is_none() {
                return refuse(
                    "no-note",
                    format!("{on} has no {name} note"),
                    "`query --columns code,notes` shows notes",
                );
            }
            format!("{on}'s {name} note deleted")
        }
    };
    if let Some(id) = new_id {
        if on_wbs {
            ids.wbs_note = id + 1;
        } else {
            ids.activity_note = id + 1;
        }
    }
    Ok(said)
}
