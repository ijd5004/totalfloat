//! What must hold for any network: seeded random schedules with mixed calendars,
//! all link types, leads and lags. TF-032 explains why true float is the one
//! that can't go negative here.
#![forbid(unsafe_code)]
#![allow(clippy::unwrap_used, reason = "helpers shared by tests")]

#[path = "../benches/support/network.rs"]
mod network;

use std::collections::BTreeMap;

use totalfloat_calendar::WorkCalendar;
use totalfloat_engine::{ActivityResult, ScheduleResult, schedule};
use totalfloat_model::{ActivityId, DateTime, Minutes, RelationshipType, Schedule, StoredResults};

const NETWORKS: u64 = 60;

fn networks() -> impl Iterator<Item = Schedule> {
    (0..NETWORKS).map(|seed| {
        let size = 20 + usize::try_from(seed % 5).unwrap() * 40;
        network::generate(size, seed + 1)
    })
}

fn run(s: &Schedule) -> ScheduleResult {
    schedule(s, &s.project.options).unwrap()
}

fn calendars(s: &Schedule) -> BTreeMap<ActivityId, WorkCalendar> {
    s.activities
        .iter()
        .map(|(id, a)| {
            let calendar = &s.calendars[&a.calendar];
            (*id, WorkCalendar::new(calendar).unwrap())
        })
        .collect()
}

fn dates(values: &StoredResults) -> [DateTime; 4] {
    [
        values.early_start.unwrap(),
        values.early_finish.unwrap(),
        values.late_start.unwrap(),
        values.late_finish.unwrap(),
    ]
}

#[test]
fn every_link_holds_in_the_early_dates() {
    for s in networks() {
        let r = run(&s);
        let on = calendars(&s);
        for link in s.relationships.values() {
            let pred = dates(&r.activities[&link.predecessor].values);
            let succ = dates(&r.activities[&link.successor].values);
            let (from, to) = match link.kind {
                RelationshipType::FinishToStart => (pred[1], succ[0]),
                RelationshipType::StartToStart => (pred[0], succ[0]),
                RelationshipType::FinishToFinish => (pred[1], succ[1]),
                RelationshipType::StartToFinish => (pred[0], succ[1]),
            };
            let bound = on[&link.predecessor].add_work(from, link.lag).unwrap();
            let late = on[&link.successor].work_between(to, bound);
            assert!(late <= Minutes::ZERO, "{:?}", link.id);
        }
    }
}

#[test]
fn early_and_late_spans_are_the_duration() {
    for s in networks() {
        let r = run(&s);
        let on = calendars(&s);
        for (id, a) in &s.activities {
            let [early_start, early_finish, late_start, late_finish] =
                dates(&r.activities[id].values);
            assert_eq!(
                on[id].work_between(early_start, early_finish),
                a.remaining_duration
            );
            assert_eq!(
                on[id].work_between(late_start, late_finish),
                a.remaining_duration
            );
        }
    }
}

#[test]
fn nothing_finishes_after_the_project() {
    for s in networks() {
        let r = run(&s);
        let finish = r.project.scheduled_finish.unwrap();
        for result in r.activities.values() {
            assert!(result.values.early_finish.unwrap() <= finish);
        }
    }
}

fn true_float(result: &ActivityResult) -> Minutes {
    result
        .true_total_float
        .or(result.values.total_float)
        .unwrap()
}

#[test]
fn true_float_is_never_negative_or_below_total_float() {
    for s in networks() {
        let r = run(&s);
        for result in r.activities.values() {
            assert!(true_float(result) >= Minutes::ZERO);
            assert!(true_float(result) >= result.values.total_float.unwrap());
        }
    }
}

#[test]
fn some_activity_has_no_float_to_spare() {
    for s in networks() {
        let r = run(&s);
        assert!(
            r.activities
                .values()
                .any(|a| true_float(a) == Minutes::ZERO)
        );
    }
}

// Not a property: longer work can bring dates forward. When a finish-to-finish
// link drives an activity's finish, more work moves its start earlier, and a
// start-to-start successor follows.

fn renumbered(s: &Schedule) -> (Schedule, u64) {
    let last = s.activities.keys().map(|id| id.0).max().unwrap() + 1;
    let flip = |id: ActivityId| ActivityId(last - id.0);
    let mut out = s.clone();
    out.activities = s
        .activities
        .values()
        .map(|a| {
            let mut a = a.clone();
            a.id = flip(a.id);
            (a.id, a)
        })
        .collect();
    for link in out.relationships.values_mut() {
        link.predecessor = flip(link.predecessor);
        link.successor = flip(link.successor);
    }
    (out, last)
}

#[test]
fn renumbering_activities_changes_nothing() {
    for s in networks() {
        let (flipped, last) = renumbered(&s);
        let (r, f) = (run(&s), run(&flipped));
        assert_eq!(r.project.scheduled_finish, f.project.scheduled_finish);
        for (id, result) in &r.activities {
            let other = &f.activities[&ActivityId(last - id.0)];
            assert_eq!(result.values, other.values, "{id:?}");
        }
    }
}

#[test]
fn the_same_input_gives_the_same_result() {
    for s in networks() {
        assert_eq!(run(&s), run(&s));
    }
}
