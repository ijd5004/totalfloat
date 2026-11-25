//! The 50,000-activity schedule `cargo xtask history-cost` and
//! `cargo xtask ui-bench` use, built the same way: chains of 100
//! finish-to-start links on one five-day calendar, each chain under its own
//! WBS node, and each chain linked to the one before except every tenth, so
//! the paths are ten chains long. No real schedule data.
#![allow(
    clippy::expect_used,
    reason = "benchmark input from fixed values; a failure is a broken generator"
)]

use std::collections::BTreeMap;

use totalfloat_model::{
    Activity, ActivityId, Calendar, CalendarId, CalendarType, EarnedValue, HoursPerPeriod, Minutes,
    Project, ProjectId, Relationship, RelationshipId, RelationshipType, Schedule, TimeOfDay, Wbs,
    WbsId, WorkInterval, WorkWeek,
};

fn shift(start: u8, finish: u8) -> WorkInterval {
    WorkInterval::new(
        TimeOfDay::from_hm(start, 0).expect("valid time"),
        TimeOfDay::from_hm(finish, 0).expect("valid time"),
    )
    .expect("valid shift")
}

fn wbs(id: u64, parent: Option<WbsId>) -> Wbs {
    Wbs {
        id: WbsId(id),
        project: ProjectId(1),
        parent,
        code: format!("C{id}"),
        name: format!("C{id}"),
        sequence: 0,
        is_project_node: parent.is_none(),
        earned_value: EarnedValue::default(),
        obs: None,
        notes: BTreeMap::new(),
        udfs: BTreeMap::new(),
        source: None,
    }
}

/// `activities` activities in linked chains of 100.
///
/// # Panics
///
/// Only if the fixed calendar and date values above stop being valid.
pub fn generate(activities: u64) -> Schedule {
    let mut project = Project::new(ProjectId(1), "COST");
    project.data_date = Some("2026-01-05 08:00".parse().expect("valid date"));
    project.planned_start = project.data_date;
    project.default_calendar = Some(CalendarId(1));
    let mut schedule = Schedule::new(project);
    let calendar = Calendar {
        id: CalendarId(1),
        name: "Five-day".to_owned(),
        kind: CalendarType::Global,
        project: None,
        base: None,
        is_default: true,
        hours_per_period: HoursPerPeriod::default(),
        work_week: WorkWeek(std::array::from_fn(|day| {
            if (1..=5).contains(&day) {
                vec![shift(8, 12), shift(13, 17)]
            } else {
                vec![]
            }
        })),
        exceptions: BTreeMap::new(),
        source: None,
    };
    schedule.calendars.insert(calendar.id, calendar);
    schedule.wbs.insert(WbsId(1), wbs(1, None));
    let mut links = 0;
    let mut link = |schedule: &mut Schedule, from: u64, to: u64| {
        links += 1;
        let id = RelationshipId(links);
        schedule.relationships.insert(
            id,
            Relationship {
                id,
                predecessor: ActivityId(from),
                successor: ActivityId(to),
                predecessor_project: ProjectId(1),
                successor_project: ProjectId(1),
                kind: RelationshipType::FinishToStart,
                lag: Minutes::ZERO,
                source: None,
            },
        );
    };
    for n in 1..=activities {
        let chain = (n - 1) / 100;
        let node = 10 + chain;
        if (n - 1) % 100 == 0 {
            schedule.wbs.insert(WbsId(node), wbs(node, Some(WbsId(1))));
        }
        let mut activity = Activity::new(
            ActivityId(n),
            ProjectId(1),
            WbsId(node),
            CalendarId(1),
            format!("A{n}"),
            "Work",
        );
        activity.original_duration = Minutes(60 * 8 * (1 + i64::try_from(n % 5).expect("small")));
        activity.remaining_duration = activity.original_duration;
        schedule.activities.insert(activity.id, activity);
        if (n - 1) % 100 != 0 || chain % 10 != 0 {
            link(&mut schedule, n - 1, n);
        }
    }
    schedule
}
