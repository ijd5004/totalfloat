//! A seeded synthetic network with realistic structure: WBS clusters of chained
//! work, mixed relationship types and lags, links between clusters, and several
//! calendars with holidays. No real schedule data.
#![allow(
    clippy::expect_used,
    reason = "benchmark input from fixed values; a failure is a broken generator"
)]

use std::collections::BTreeMap;

use totalfloat_model::{
    Activity, ActivityId, Calendar, CalendarId, CalendarType, Date, DateTime, HoursPerPeriod,
    Minutes, Project, ProjectId, Relationship, RelationshipId, RelationshipType, Schedule,
    TimeOfDay, Wbs, WbsId, WorkInterval, WorkWeek,
};

const CLUSTER: usize = 50;

/// A small deterministic generator, so results don't depend on a crate's version.
struct Random(u64);

impl Random {
    fn next(&mut self, below: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        usize::try_from((self.0 >> 33) % below as u64).unwrap_or(0)
    }
}

fn shift(start: u16, finish: u16) -> WorkInterval {
    WorkInterval::new(
        TimeOfDay::from_minutes(start).expect("valid time"),
        TimeOfDay::from_minutes(finish).expect("valid time"),
    )
    .expect("valid shift")
}

fn calendar(id: u64, days: &[usize], holidays: u64) -> Calendar {
    let day = vec![shift(480, 720), shift(780, 1020)];
    let first: Date = "2026-01-01".parse().expect("valid date");
    let exceptions = (0..holidays)
        .map(|i| {
            let offset = i32::try_from(i * 37).expect("small offset");
            (first.checked_add_days(offset).expect("in range"), vec![])
        })
        .collect();
    Calendar {
        id: CalendarId(id),
        name: format!("Generated calendar {id}"),
        kind: CalendarType::Global,
        project: None,
        base: None,
        is_default: id == 1,
        hours_per_period: HoursPerPeriod::default(),
        work_week: WorkWeek(std::array::from_fn(|i| {
            if days.contains(&i) {
                day.clone()
            } else {
                vec![]
            }
        })),
        exceptions,
        source: None,
    }
}

fn wbs(id: u64) -> Wbs {
    Wbs {
        id: WbsId(id),
        project: ProjectId(1),
        parent: (id > 1).then_some(WbsId(1)),
        code: format!("W{id}"),
        name: "Generated".to_owned(),
        sequence: i64::try_from(id).expect("small id"),
        is_project_node: id == 1,
        earned_value: totalfloat_model::EarnedValue::default(),
        obs: None,
        notes: std::collections::BTreeMap::new(),
        udfs: BTreeMap::new(),
        source: None,
    }
}

fn link(schedule: &mut Schedule, from: usize, to: usize, random: &mut Random) {
    let id = u64::try_from(schedule.relationships.len() + 1).expect("small id");
    let (kind, lag) = match random.next(10) {
        0 => (RelationshipType::StartToStart, 240),
        1 => (RelationshipType::FinishToFinish, 480),
        2 => (RelationshipType::FinishToStart, -120),
        3 => (RelationshipType::FinishToStart, 480),
        _ => (RelationshipType::FinishToStart, 0),
    };
    let relationship = Relationship {
        id: RelationshipId(id),
        predecessor: ActivityId(u64::try_from(from + 1).expect("small id")),
        successor: ActivityId(u64::try_from(to + 1).expect("small id")),
        predecessor_project: ProjectId(1),
        successor_project: ProjectId(1),
        kind,
        lag: Minutes(lag),
        source: None,
    };
    schedule.relationships.insert(relationship.id, relationship);
}

/// `activities` activities and about 1.5 relationships each, always from a
/// lower to a higher activity, so the network has no cycles.
///
/// # Panics
///
/// Only if the fixed calendar and shift values above stop being valid.
pub fn generate(activities: usize, seed: u64) -> Schedule {
    let start: DateTime = "2026-01-05 08:00".parse().expect("valid date");
    let mut project = Project::new(ProjectId(1), "GENERATED");
    project.planned_start = Some(start);
    project.data_date = Some(start);
    project.default_calendar = Some(CalendarId(1));
    let mut schedule = Schedule::new(project);
    for calendar in [
        calendar(1, &[1, 2, 3, 4, 5], 20),
        calendar(2, &[1, 2, 3, 4, 5, 6], 12),
        calendar(3, &[0, 1, 2, 3, 4, 5, 6], 4),
    ] {
        schedule.calendars.insert(calendar.id, calendar);
    }
    schedule.wbs.insert(WbsId(1), wbs(1));
    let mut random = Random(seed);
    for i in 0..activities {
        let cluster = u64::try_from(i / CLUSTER).expect("small id") + 2;
        schedule
            .wbs
            .entry(WbsId(cluster))
            .or_insert_with(|| wbs(cluster));
        let id = ActivityId(u64::try_from(i + 1).expect("small id"));
        let calendar = CalendarId(1 + u64::try_from(random.next(3)).expect("small id"));
        let mut activity = Activity::new(
            id,
            ProjectId(1),
            WbsId(cluster),
            calendar,
            format!("A{}", id.0),
            "Generated task",
        );
        let duration = Minutes(480 * i64::try_from(1 + random.next(10)).expect("small"));
        activity.original_duration = duration;
        activity.remaining_duration = duration;
        schedule.activities.insert(id, activity);
        if i % CLUSTER > 0 {
            link(
                &mut schedule,
                i - 1 - random.next((i % CLUSTER).min(5)),
                i,
                &mut random,
            );
        }
        if i >= CLUSTER && random.next(2) == 0 {
            let from = i - CLUSTER + random.next(CLUSTER / 2);
            link(&mut schedule, from, i, &mut random);
        }
        if i >= CLUSTER && i % CLUSTER == 0 {
            link(
                &mut schedule,
                i - 1 - random.next(CLUSTER / 2),
                i,
                &mut random,
            );
        }
    }
    schedule
}
