//! Notebook topics and notes in a synthetic file,
//! `fixtures/xer/notebooks.xer`, written fresh from `model-mapping.xer`'s
//! first project with topics and notes added. The file is read, edited and
//! written back, and only what changed differs.
#![allow(clippy::unwrap_used, reason = "test code")]

use std::path::Path;

use totalfloat_model as m;
use totalfloat_xer::fresh::{FreshOptions, write};
use totalfloat_xer::raw::{self, RawXer};
use totalfloat_xer::rawdiff::{changed_cells, diff_lines};
use totalfloat_xer::{patch, to_model};

fn root() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

const BASIS: m::NotebookTopicId = m::NotebookTopicId(1);
const RISKS: m::NotebookTopicId = m::NotebookTopicId(2);
const SCOPE: m::NotebookTopicId = m::NotebookTopicId(3);
const A101: m::ActivityId = m::ActivityId(101);
const PACKAGE: m::WbsId = m::WbsId(12);

/// A note as P6 writes one: a full HTML page with quotes and line breaks.
const STORED: &str = "<!DOCTYPE HTML PUBLIC \"-//W3C//DTD HTML 4.0 Transitional//EN\">\n<HTML><HEAD><TITLE>Basis</TITLE>\n<META content=\"text/html; charset=utf-8\" http-equiv=Content-Type></HEAD>\n<BODY>\n<P>Two crews &amp; one crane.</P>\n<P>Durations&nbsp;from the 2025 job.</P></BODY></HTML>";

/// The model the fixture is written from.
fn authored() -> m::Schedule {
    let input =
        raw::read(std::fs::read(root().join("fixtures/xer/model-mapping.xer")).unwrap()).unwrap();
    let mut schedule = to_model(&input).0.swap_remove(0);
    let project = schedule.project.id;
    schedule
        .relationships
        .retain(|_, r| r.predecessor_project == project && r.successor_project == project);
    for (id, name, sequence, wbs, activities) in [
        (BASIS, "Basis of schedule", 0, true, true),
        (RISKS, "Risks", 1, false, true),
        (SCOPE, "Scope", 2, true, false),
    ] {
        schedule.notebook_topics.insert(
            id,
            m::NotebookTopic {
                id,
                name: name.to_owned(),
                sequence,
                for_eps: false,
                for_projects: true,
                for_wbs: wbs,
                for_activities: activities,
                source: None,
            },
        );
    }
    let note = |id, html: &str| m::Note {
        id,
        html: html.to_owned(),
        source: None,
    };
    let a = schedule.activities.get_mut(&A101).unwrap();
    a.notes.insert(BASIS, note(1, STORED));
    a.notes
        .insert(RISKS, note(2, &m::Note::html_of("Crane hire may slip")));
    let w = schedule.wbs.get_mut(&PACKAGE).unwrap();
    w.notes
        .insert(SCOPE, note(1, &m::Note::html_of("Civil works only")));
    schedule
}

fn bytes(xer: &RawXer) -> Vec<u8> {
    let mut out = Vec::new();
    xer.write(&mut out).unwrap();
    out
}

fn fixture() -> RawXer {
    raw::read(std::fs::read(root().join("fixtures/xer/notebooks.xer")).unwrap()).unwrap()
}

/// Set `TOTALFLOAT_UPDATE_CONTRACT=1` to rewrite the fixture after a
/// deliberate change, and record why in fixtures/xer/MANIFEST.md.
#[test]
fn the_fixture_is_the_authored_model_written_fresh() {
    let mut options = FreshOptions::new("2026-01-01".parse().unwrap());
    options.seed = 42;
    let written = bytes(&write(&authored(), options).unwrap());
    let path = root().join("fixtures/xer/notebooks.xer");
    if std::env::var_os("TOTALFLOAT_UPDATE_CONTRACT").is_some() {
        std::fs::write(&path, &written).unwrap();
        return;
    }
    assert!(
        std::fs::read(&path).unwrap() == written,
        "fixtures/xer/notebooks.xer is out of date; rerun with TOTALFLOAT_UPDATE_CONTRACT=1"
    );
}

#[test]
fn notes_are_read_as_html_and_text() {
    let raw = fixture();
    // The cell doubles quotes and writes line breaks as two DEL characters.
    let t = raw.table_index("TASKMEMO").unwrap();
    let column = raw.tables()[t].field_index("task_memo").unwrap();
    let cell = raw.tables()[t].rows[0].get(column).unwrap();
    assert!(
        cell.contains("\"\"text/html") && cell.contains('\u{7f}'),
        "{cell}"
    );
    let schedule = to_model(&raw).0.swap_remove(0);
    let notes = &schedule.activities[&A101].notes;
    assert_eq!(notes[&BASIS].html, STORED);
    assert_eq!(
        notes[&BASIS].text(),
        "Two crews & one crane.\nDurations from the 2025 job."
    );
    assert_eq!(notes[&RISKS].text(), "Crane hire may slip");
    assert_eq!(
        schedule.wbs[&PACKAGE].notes[&SCOPE].text(),
        "Civil works only"
    );
    assert_eq!(schedule.notebook_topics[&SCOPE].name, "Scope");
    assert!(!schedule.notebook_topics[&SCOPE].for_activities);
    assert_eq!(schedule.validate(), []);
}

#[test]
fn an_unchanged_schedule_writes_back_byte_for_byte() {
    let raw = fixture();
    let schedule = to_model(&raw).0.swap_remove(0);
    assert_eq!(bytes(&patch(&raw, &schedule).unwrap()), raw.source());
}

#[test]
fn a_changed_note_changes_one_cell() {
    let raw = fixture();
    let mut schedule = to_model(&raw).0.swap_remove(0);
    let note = schedule
        .activities
        .get_mut(&A101)
        .unwrap()
        .notes
        .get_mut(&BASIS)
        .unwrap();
    note.html = m::Note::html_of("Three crews.\nSecond \"line\"");
    let out = bytes(&patch(&raw, &schedule).unwrap());
    let d = diff_lines(raw.source(), &out);
    assert_eq!((d.removed.len(), d.added.len()), (1, 1), "{d:?}");
    assert_eq!(changed_cells(&d.removed[0].1, &d.added[0].1).len(), 1);
    let back = to_model(&raw::read(out).unwrap()).0.swap_remove(0);
    assert_eq!(
        back.activities[&A101].notes[&BASIS].text(),
        "Three crews.\nSecond \"line\""
    );
}

#[test]
fn new_and_deleted_notes_are_rows_added_and_removed() {
    let raw = fixture();
    let mut schedule = to_model(&raw).0.swap_remove(0);
    let a = schedule.activities.get_mut(&A101).unwrap();
    a.notes.remove(&RISKS);
    let other = schedule.activities.get_mut(&m::ActivityId(102)).unwrap();
    other.notes.insert(
        RISKS,
        m::Note {
            id: 3,
            html: m::Note::html_of("Late steel"),
            source: None,
        },
    );
    let w = schedule.wbs.get_mut(&m::WbsId(11)).unwrap();
    w.notes.insert(
        BASIS,
        m::Note {
            id: 2,
            html: m::Note::html_of("Whole job"),
            source: None,
        },
    );
    let out = bytes(&patch(&raw, &schedule).unwrap());
    let d = diff_lines(raw.source(), &out);
    assert_eq!((d.removed.len(), d.added.len()), (1, 2), "{d:?}");
    let back = to_model(&raw::read(out).unwrap()).0.swap_remove(0);
    assert!(!back.activities[&A101].notes.contains_key(&RISKS));
    assert_eq!(
        back.activities[&m::ActivityId(102)].notes[&RISKS].text(),
        "Late steel"
    );
    assert_eq!(back.wbs[&m::WbsId(11)].notes[&BASIS].text(), "Whole job");
}

#[test]
fn changing_a_topic_is_refused() {
    let raw = fixture();
    let mut schedule = to_model(&raw).0.swap_remove(0);
    schedule.notebook_topics.get_mut(&RISKS).unwrap().name = "Threats".to_owned();
    assert!(patch(&raw, &schedule).is_err());
}

#[test]
fn a_note_under_a_topic_the_file_lacks_is_left_out_and_reported() {
    let mut raw = fixture();
    let t = raw.table_index("TASKMEMO").unwrap();
    let column = raw.tables()[t].field_index("memo_type_id").unwrap();
    raw.set_cell(t, 0, column, "99").unwrap();
    let (schedules, diagnostics) = to_model(&raw);
    assert_eq!(schedules[0].activities[&A101].notes.len(), 1);
    assert_eq!(diagnostics.omitted.len(), 1);
    assert_eq!(schedules[0].validate(), []);
}
