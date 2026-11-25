//! Recalculating only what an edit reaches gives the full calculation's
//! result, activity for activity: seeded networks with progress,
//! constraints of every kind, spans, milestones and expected finishes, under
//! many schedule options, each edited every way an edit can change what the
//! engine reads.
#![forbid(unsafe_code)]
#![allow(clippy::unwrap_used, reason = "test helpers")]

#[path = "../benches/support/network.rs"]
mod network;

use totalfloat_engine::{
    Calculated, EngineError, ScheduleResult, schedule_open_together, schedule_reusing,
};
use totalfloat_model::{
    Activity, ActivityId, ActivityStatus, ActivityType, CalendarId, Constraint, ConstraintType,
    CriticalDefinition, DateTime, FloatPathMode, FloatType, LagCalendar, Minutes,
    OutOfSequenceMode, Relationship, RelationshipId, RelationshipType, Schedule, ScheduleOptions,
    WbsId,
};

/// A small deterministic generator, so the cases don't depend on a crate's version.
struct Random(u64);

impl Random {
    fn below(&mut self, n: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        usize::try_from((self.0 >> 33) % n as u64).unwrap()
    }
    fn id(&mut self, s: &Schedule) -> ActivityId {
        let ids: Vec<_> = s.activities.keys().copied().collect();
        ids[self.below(ids.len())]
    }
}

fn at(text: &str) -> DateTime {
    text.parse().unwrap()
}

fn day(n: i64) -> Minutes {
    Minutes(480 * n)
}

fn constraint(kind: ConstraintType, date: &str) -> Constraint {
    Constraint {
        kind,
        date: (kind != ConstraintType::AsLateAsPossible).then(|| at(date)),
    }
}

/// A generated network with something of everything the passes treat apart.
fn varied(seed: u64) -> Schedule {
    let size = 60 + usize::try_from(seed % 4).unwrap() * 50;
    let mut s = network::generate(size, seed + 1);
    let mut random = Random(seed);
    s.project.data_date = Some(at("2026-01-19 08:00"));
    // The first few activities have started or finished.
    for n in 1..=6 {
        let a = s.activities.get_mut(&ActivityId(n)).unwrap();
        a.actual_start = Some(at("2026-01-06 08:00"));
        if n % 3 == 0 {
            a.status = ActivityStatus::InProgress;
            a.remaining_duration = day(2);
            if n == 6 {
                a.expected_finish = Some(at("2026-01-27 17:00"));
            }
        } else {
            a.status = ActivityStatus::Completed;
            a.actual_finish = Some(at("2026-01-09 17:00"));
            a.remaining_duration = Minutes::ZERO;
        }
    }
    let kinds = [
        (ConstraintType::StartOnOrAfter, "2026-02-16 08:00"),
        (ConstraintType::FinishOnOrBefore, "2026-03-02 17:00"),
        (ConstraintType::AsLateAsPossible, ""),
        (ConstraintType::MandatoryStart, "2026-02-02 08:00"),
        (ConstraintType::MandatoryFinish, "2026-04-01 17:00"),
        (ConstraintType::StartOn, "2026-02-23 08:00"),
        (ConstraintType::FinishOn, "2026-03-20 17:00"),
        (ConstraintType::StartOnOrBefore, "2026-03-09 08:00"),
        (ConstraintType::FinishOnOrAfter, "2026-02-27 17:00"),
    ];
    for (kind, date) in kinds {
        let id = ActivityId(u64::try_from(10 + random.below(size - 10)).unwrap());
        let a = s.activities.get_mut(&id).unwrap();
        if a.status == ActivityStatus::NotStarted {
            a.primary_constraint = Some(constraint(kind, date));
        }
    }
    for (n, kind) in [
        (12, ActivityType::StartMilestone),
        (25, ActivityType::FinishMilestone),
        (31, ActivityType::LevelOfEffort),
        (44, ActivityType::WbsSummary),
    ] {
        s.activities.get_mut(&ActivityId(n)).unwrap().activity_type = kind;
    }
    s.activities
        .get_mut(&ActivityId(20))
        .unwrap()
        .external_early_start = Some(at("2026-02-09 08:00"));
    s.activities
        .get_mut(&ActivityId(40))
        .unwrap()
        .external_late_finish = Some(at("2026-05-01 17:00"));
    s
}

/// The project's options, and variants of every option the passes read.
fn variants(s: &Schedule) -> Vec<ScheduleOptions> {
    let base = s.project.options.clone();
    let with = |change: &dyn Fn(&mut ScheduleOptions)| {
        let mut options = base.clone();
        change(&mut options);
        options
    };
    vec![
        base.clone(),
        with(&|o| o.float_type = FloatType::StartFloat),
        with(&|o| o.float_type = FloatType::SmallestFloat),
        with(&|o| o.lag_calendar = LagCalendar::Successor),
        with(&|o| o.lag_calendar = LagCalendar::TwentyFourHour),
        with(&|o| o.lag_calendar = LagCalendar::ProjectDefault),
        with(&|o| o.out_of_sequence = OutOfSequenceMode::ProgressOverride),
        with(&|o| o.critical = CriticalDefinition::LongestPath),
        with(&|o| o.critical = CriticalDefinition::TotalFloatAtMost(day(2))),
        with(&|o| o.make_open_ended_critical = true),
        with(&|o| o.ss_lag_from_early_start = false),
        with(&|o| o.use_expected_finish = false),
        with(&|o| {
            o.float_paths.enabled = true;
            o.float_paths.max_paths = Some(4);
        }),
        with(&|o| {
            o.float_paths.enabled = true;
            o.float_paths.mode = FloatPathMode::FreeFloat;
        }),
    ]
}

type Edit = fn(&mut Schedule, &mut Random);

fn link(s: &mut Schedule, from: ActivityId, to: ActivityId, kind: RelationshipType, lag: i64) {
    let id = RelationshipId(s.relationships.keys().last().map_or(1, |id| id.0 + 1));
    let project = s.project.id;
    s.relationships.insert(
        id,
        Relationship {
            id,
            predecessor: from,
            successor: to,
            predecessor_project: project,
            successor_project: project,
            kind,
            lag: Minutes(lag),
            source: None,
        },
    );
}

/// Every way an edit can change what the engine reads, local and global.
const EDITS: &[(&str, Edit)] = &[
    ("longer", |s, r| {
        let id = r.id(s);
        let a = s.activities.get_mut(&id).unwrap();
        a.remaining_duration = Minutes(a.remaining_duration.get() + 480 * 3);
    }),
    ("shorter", |s, r| {
        let id = r.id(s);
        let a = s.activities.get_mut(&id).unwrap();
        a.remaining_duration = Minutes(a.remaining_duration.get() / 2);
    }),
    ("no duration", |s, r| {
        let id = r.id(s);
        s.activities.get_mut(&id).unwrap().remaining_duration = Minutes::ZERO;
    }),
    ("last activity shorter", |s, _| {
        let id = *s.activities.keys().last().unwrap();
        let a = s.activities.get_mut(&id).unwrap();
        a.remaining_duration = Minutes(a.remaining_duration.get() / 2);
    }),
    ("constraint", |s, r| {
        let id = r.id(s);
        let kinds = [
            (ConstraintType::StartOnOrAfter, "2026-03-02 08:00"),
            (ConstraintType::FinishOnOrBefore, "2026-02-20 17:00"),
            (ConstraintType::AsLateAsPossible, ""),
            (ConstraintType::MandatoryStart, "2026-02-04 08:00"),
            (ConstraintType::MandatoryFinish, "2026-03-31 17:00"),
        ];
        let (kind, date) = kinds[r.below(kinds.len())];
        let a = s.activities.get_mut(&id).unwrap();
        if a.status == ActivityStatus::NotStarted && !a.activity_type.is_span() {
            a.primary_constraint = Some(constraint(kind, date));
            a.secondary_constraint = None;
        }
    }),
    ("no constraints", |s, _| {
        for a in s.activities.values_mut() {
            a.primary_constraint = None;
            a.secondary_constraint = None;
        }
    }),
    ("started", |s, r| {
        let id = r.id(s);
        let a = s.activities.get_mut(&id).unwrap();
        if a.status == ActivityStatus::NotStarted && !a.activity_type.is_span() {
            a.status = ActivityStatus::InProgress;
            a.actual_start = Some(at("2026-01-12 08:00"));
        }
    }),
    ("finished", |s, r| {
        let id = r.id(s);
        let a = s.activities.get_mut(&id).unwrap();
        if a.status != ActivityStatus::Completed && !a.activity_type.is_span() {
            a.status = ActivityStatus::Completed;
            a.actual_start = a.actual_start.or(Some(at("2026-01-12 08:00")));
            a.actual_finish = Some(at("2026-01-16 17:00"));
            a.remaining_duration = Minutes::ZERO;
            a.expected_finish = None;
        }
    }),
    ("expected finish", |s, r| {
        let id = r.id(s);
        let a = s.activities.get_mut(&id).unwrap();
        a.expected_finish = Some(at("2026-03-13 17:00"));
    }),
    ("link changed", |s, r| {
        let ids: Vec<_> = s.relationships.keys().copied().collect();
        let id = ids[r.below(ids.len())];
        let link = s.relationships.get_mut(&id).unwrap();
        link.kind = [
            RelationshipType::FinishToStart,
            RelationshipType::StartToStart,
            RelationshipType::FinishToFinish,
            RelationshipType::StartToFinish,
        ][r.below(4)];
        link.lag = Minutes(480 * i64::try_from(r.below(5)).unwrap() - 480);
    }),
    ("link removed", |s, r| {
        let ids: Vec<_> = s.relationships.keys().copied().collect();
        let id = ids[r.below(ids.len())];
        s.relationships.remove(&id);
    }),
    ("link added", |s, r| {
        let (a, b) = (r.id(s), r.id(s));
        // From the lower ID to the higher, as every generated link runs.
        if a != b {
            link(s, a.min(b), a.max(b), RelationshipType::FinishToStart, 0);
        }
    }),
    ("activity added", |s, r| {
        let (from, to) = (r.id(s), r.id(s));
        let id = ActivityId(s.activities.keys().last().unwrap().0 + 1);
        let mut a = Activity::new(id, s.project.id, WbsId(2), CalendarId(2), "NEW", "New");
        a.remaining_duration = day(4);
        a.original_duration = day(4);
        s.activities.insert(id, a);
        // A new ID is the highest, so links run into it from anywhere.
        link(s, from, id, RelationshipType::FinishToStart, 0);
        link(s, to, id, RelationshipType::StartToStart, 480);
    }),
    ("activity deleted", |s, r| {
        let id = r.id(s);
        s.activities.remove(&id);
        s.relationships
            .retain(|_, link| link.predecessor != id && link.successor != id);
    }),
    ("calendar", |s, r| {
        let id = r.id(s);
        let a = s.activities.get_mut(&id).unwrap();
        a.calendar = CalendarId(1 + (a.calendar.0 % 3));
    }),
    ("type", |s, r| {
        let id = r.id(s);
        let a = s.activities.get_mut(&id).unwrap();
        if a.status == ActivityStatus::NotStarted {
            a.activity_type = [
                ActivityType::TaskDependent,
                ActivityType::FinishMilestone,
                ActivityType::StartMilestone,
                ActivityType::LevelOfEffort,
            ][r.below(4)];
        }
    }),
    ("moved to another WBS", |s, r| {
        let id = r.id(s);
        s.activities.get_mut(&id).unwrap().wbs = WbsId(3);
    }),
    ("external dates", |s, r| {
        let id = r.id(s);
        let a = s.activities.get_mut(&id).unwrap();
        a.external_early_start = Some(at("2026-03-02 08:00"));
        a.external_late_finish = Some(at("2026-03-30 17:00"));
    }),
    ("several at once", |s, r| {
        for _ in 0..4 {
            let id = r.id(s);
            let a = s.activities.get_mut(&id).unwrap();
            a.remaining_duration = Minutes(a.remaining_duration.get() + 240);
        }
        let ids: Vec<_> = s.relationships.keys().copied().collect();
        s.relationships.remove(&ids[r.below(ids.len())]);
    }),
    ("nothing", |_, _| {}),
    ("data date", |s, _| {
        s.project.data_date = Some(at("2026-01-20 08:00"));
    }),
    ("must finish by", |s, _| {
        s.project.must_finish_by = Some(at("2026-05-29 17:00"));
    }),
    ("calendar edited", |s, _| {
        let calendar = s.calendars.get_mut(&CalendarId(2)).unwrap();
        calendar.exceptions.pop_first();
    }),
    ("options", |s, _| {
        s.project.options.float_type = FloatType::StartFloat;
    }),
];

/// Calculates `after` reusing `before`, and in full, and says how many
/// activities were worked out, if it calculated and kept anything.
fn compare(
    before: &Calculated,
    after: &Schedule,
    options: &ScheduleOptions,
    case: &str,
) -> Option<usize> {
    let options = if case == "options" {
        &after.project.options
    } else {
        options
    };
    let reused = schedule_reusing(after, options, (&[], None), Some(before));
    let full = schedule_open_together(after, options, &[], None);
    same(reused.as_ref().map(Calculated::result), full.as_ref(), case);
    reused.ok().and_then(|r| r.worked_out())
}

/// Asserts two results are equal, naming the first activity that differs
/// rather than printing both whole.
fn same(
    reused: Result<&ScheduleResult, &EngineError>,
    full: Result<&ScheduleResult, &EngineError>,
    case: &str,
) {
    if let (Ok(reused), Ok(full)) = (reused, full) {
        assert_eq!(reused.project, full.project, "{case}");
        for (id, activity) in &full.activities {
            assert_eq!(reused.activities.get(id), Some(activity), "{case}: {id:?}");
        }
    }
    assert!(reused == full, "{case}: {reused:?} against {full:?}");
}

#[test]
fn reusing_a_calculation_gives_the_full_result_for_every_edit() {
    let (mut cases, mut local, mut skipped) = (0, 0, 0);
    for seed in 0..12 {
        let base = varied(seed);
        for (variant, options) in variants(&base).iter().enumerate() {
            let mut s = base.clone();
            s.project.options = options.clone();
            let Ok(before) = schedule_reusing(&s, options, (&[], None), None) else {
                skipped += 1;
                continue;
            };
            let full = schedule_open_together(&s, options, &[], None);
            same(Ok(before.result()), full.as_ref(), "before");
            let mut random = Random(seed * 100 + u64::try_from(variant).unwrap());
            for (case, edit) in EDITS {
                let mut after = s.clone();
                edit(&mut after, &mut random);
                let worked = compare(&before, &after, options, case);
                cases += 1;
                if worked.is_some_and(|worked| worked < after.activities.len()) {
                    local += 1;
                }
                // Reusing a calculation that itself reused one.
                if let Ok(again) = schedule_reusing(&after, options, (&[], None), Some(&before)) {
                    let mut third = after.clone();
                    EDITS[random.below(EDITS.len())].1(&mut third, &mut random);
                    compare(&again, &third, options, "a second edit");
                }
            }
        }
    }
    assert!(cases > 2_000, "{cases} cases");
    // Most edits reach only part of the network.
    assert!(local * 2 > cases, "{local} of {cases} reused something");
    assert!(skipped < 20, "{skipped} variants refused");
}

#[test]
fn a_local_edit_works_out_only_what_it_reaches() {
    // Lengthening an activity by less than its free float moves its own
    // finish and float, and its predecessors' free float, and nothing else.
    let mut s = network::generate(2_000, 7);
    let options = s.project.options.clone();
    let before = schedule_reusing(&s, &options, (&[], None), None).unwrap();
    assert_eq!(before.worked_out(), Some(2_000));
    let (&id, _) = before
        .result()
        .activities
        .iter()
        .rev()
        .find(|(_, a)| a.values.free_float.is_some_and(|f| f.get() >= 60))
        .unwrap();
    let a = s.activities.get_mut(&id).unwrap();
    a.remaining_duration = Minutes(a.remaining_duration.get() + 30);
    let after = schedule_reusing(&s, &options, (&[], None), Some(&before)).unwrap();
    let full = schedule_open_together(&s, &options, &[], None);
    same(Ok(after.result()), full.as_ref(), "local");
    let worked = after.worked_out().unwrap();
    assert!(worked < 20, "{worked} worked out");
}

#[test]
fn nothing_is_kept_of_a_calculation_fed_by_another_project() {
    let mut s = network::generate(60, 3);
    let mut other = network::generate(10, 4);
    other.project.id = totalfloat_model::ProjectId(2);
    other.relationships.clear();
    other.activities = std::mem::take(&mut other.activities)
        .into_values()
        .map(|mut a| {
            a.id = ActivityId(a.id.0 + 1_000);
            a.project = other.project.id;
            a.stored.early_start = Some(at("2026-02-02 08:00"));
            a.stored.early_finish = Some(at("2026-02-06 17:00"));
            a.stored.late_start = Some(at("2026-02-02 08:00"));
            a.stored.late_finish = Some(at("2026-02-06 17:00"));
            (a.id, a)
        })
        .collect();
    let id = RelationshipId(10_000);
    s.relationships.insert(
        id,
        Relationship {
            id,
            predecessor: ActivityId(1_003),
            successor: ActivityId(20),
            predecessor_project: other.project.id,
            successor_project: s.project.id,
            kind: RelationshipType::FinishToStart,
            lag: Minutes::ZERO,
            source: None,
        },
    );
    let projects = [s.clone(), other];
    let options = s.project.options.clone();
    let calculated = schedule_reusing(&s, &options, (&projects, None), None).unwrap();
    assert!(!calculated.reusable());
    let full = schedule_open_together(&s, &options, &projects, None);
    same(Ok(calculated.result()), full.as_ref(), "linked");
}
