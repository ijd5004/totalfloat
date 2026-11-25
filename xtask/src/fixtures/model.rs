use super::spec;
use std::{collections::BTreeMap, error::Error};
use totalfloat_model as m;
type Result<T> = std::result::Result<T, Box<dyn Error>>;
fn choose<T: Copy>(value: &str, entries: &[(&str, T)]) -> Result<T> {
    entries
        .iter()
        .find(|(key, _)| *key == value)
        .map(|(_, value)| *value)
        .ok_or_else(|| format!("unsupported spec token: {value}").into())
}
fn intervals(values: &[String]) -> Result<Vec<m::WorkInterval>> {
    values
        .iter()
        .map(|value| {
            let (start, end) = value
                .split_once('-')
                .ok_or("interval must be HH:MM-HH:MM")?;
            let parse = |value: &str| -> Result<m::TimeOfDay> {
                let (hour, minute) = value.split_once(':').ok_or("time must be HH:MM")?;
                m::TimeOfDay::from_hm(hour.parse()?, minute.parse()?)
                    .ok_or_else(|| "invalid time".into())
            };
            m::WorkInterval::new(parse(start)?, parse(end)?)
                .ok_or_else(|| "reversed interval".into())
        })
        .collect()
}
fn constraint(value: &spec::Constraint) -> Result<m::Constraint> {
    use m::ConstraintType as C;
    let kind = choose(
        &value.kind,
        &[
            ("start_on", C::StartOn),
            ("start_on_or_after", C::StartOnOrAfter),
            ("start_on_or_before", C::StartOnOrBefore),
            ("finish_on", C::FinishOn),
            ("finish_on_or_after", C::FinishOnOrAfter),
            ("finish_on_or_before", C::FinishOnOrBefore),
            ("as_late_as_possible", C::AsLateAsPossible),
            ("mandatory_start", C::MandatoryStart),
            ("mandatory_finish", C::MandatoryFinish),
        ],
    )?;
    if kind.requires_date() != value.date.is_some() {
        return Err("constraint date presence does not match its type".into());
    }
    Ok(m::Constraint {
        kind,
        date: value.date.as_deref().map(str::parse).transpose()?,
    })
}
fn calendar(value: &spec::Calendar) -> Result<m::Calendar> {
    if value.weekdays.iter().any(|day| *day > 6) {
        return Err("weekday must be 0..6".into());
    }
    let shifts = intervals(&value.intervals)?;
    let mut exceptions = BTreeMap::new();
    for (date, shifts) in &value.exceptions {
        exceptions.insert(date.parse()?, intervals(shifts)?);
    }
    Ok(m::Calendar {
        id: m::CalendarId(value.id),
        name: value.name.clone(),
        kind: m::CalendarType::Global,
        project: None,
        base: None,
        is_default: false,
        hours_per_period: m::HoursPerPeriod::default(),
        work_week: m::WorkWeek(std::array::from_fn(|day| {
            if value.weekdays.contains(&day) {
                shifts.clone()
            } else {
                vec![]
            }
        })),
        exceptions,
        source: None,
    })
}
fn activity(value: &spec::Activity) -> Result<m::Activity> {
    use m::ActivityType as A;
    let mut activity = m::Activity::new(
        m::ActivityId(value.id),
        m::ProjectId(1),
        m::WbsId(1),
        m::CalendarId(value.calendar),
        format!("A{}", value.id),
        &value.name,
    );
    activity.activity_type = choose(
        &value.kind,
        &[
            ("task", A::TaskDependent),
            ("loe", A::LevelOfEffort),
            ("wbs_summary", A::WbsSummary),
            ("start_milestone", A::StartMilestone),
            ("finish_milestone", A::FinishMilestone),
        ],
    )?;
    activity.original_duration = m::Minutes(value.duration);
    // Finished work has nothing left, so that's the default for it.
    let left = if value.actual_finish.is_some() {
        0
    } else {
        value.duration
    };
    activity.remaining_duration = m::Minutes(value.remaining.unwrap_or(left));
    activity.actual_start = value.actual_start.as_deref().map(str::parse).transpose()?;
    activity.actual_finish = value.actual_finish.as_deref().map(str::parse).transpose()?;
    activity.status = if activity.actual_finish.is_some() {
        m::ActivityStatus::Completed
    } else if activity.actual_start.is_some() {
        m::ActivityStatus::InProgress
    } else {
        m::ActivityStatus::NotStarted
    };
    activity.suspend = value.suspend.as_deref().map(str::parse).transpose()?;
    activity.resume = value.resume.as_deref().map(str::parse).transpose()?;
    activity.expected_finish = value
        .expected_finish
        .as_deref()
        .map(str::parse)
        .transpose()?;
    activity.primary_constraint = value.constraint.as_ref().map(constraint).transpose()?;
    activity.secondary_constraint = value
        .secondary_constraint
        .as_ref()
        .map(constraint)
        .transpose()?;
    Ok(activity)
}
pub(super) fn build(spec: &spec::Spec, name: &str) -> Result<m::Schedule> {
    if spec.schema != 1
        || spec.description.trim().is_empty()
        || spec.calendars.is_empty()
        || spec.activities.is_empty()
    {
        return Err("schema must be 1; description, calendars and activities are required".into());
    }
    let mut project = m::Project::new(m::ProjectId(1), name);
    project.name = Some(name.to_owned());
    project.data_date = Some(spec.data_date.parse()?);
    project.planned_start = project.data_date;
    project.default_calendar = Some(m::CalendarId(spec.calendars[0].id));
    let options = &mut project.options;
    if let Some(v) = &spec.options.float_type {
        options.float_type = choose(
            v,
            &[
                ("start", m::FloatType::StartFloat),
                ("finish", m::FloatType::FinishFloat),
                ("smallest", m::FloatType::SmallestFloat),
            ],
        )?;
    }
    if let Some(v) = &spec.options.lag_calendar {
        options.lag_calendar = choose(
            v,
            &[
                ("predecessor", m::LagCalendar::Predecessor),
                ("successor", m::LagCalendar::Successor),
                ("project", m::LagCalendar::ProjectDefault),
                ("24_hour", m::LagCalendar::TwentyFourHour),
            ],
        )?;
    }
    if let Some(v) = &spec.options.out_of_sequence {
        options.out_of_sequence = choose(
            v,
            &[
                ("retained_logic", m::OutOfSequenceMode::RetainedLogic),
                ("progress_override", m::OutOfSequenceMode::ProgressOverride),
                ("actual_dates", m::OutOfSequenceMode::ActualDates),
            ],
        )?;
    }
    macro_rules! flags { ($($field:ident),*) => { $( if let Some(v)=spec.options.$field { options.$field=v; } )* }; }
    flags!(
        make_open_ended_critical,
        float_from_project_finish,
        use_expected_finish,
        ss_lag_from_early_start,
        set_planned_to_forecast
    );
    let mut schedule = m::Schedule::new(project);
    root(&mut schedule, name);
    for value in &spec.calendars {
        let calendar = calendar(value)?;
        if schedule.calendars.insert(calendar.id, calendar).is_some() {
            return Err("duplicate calendar ID".into());
        }
    }
    for value in &spec.activities {
        let activity = activity(value)?;
        if schedule.activities.insert(activity.id, activity).is_some() {
            return Err("duplicate activity ID".into());
        }
    }
    for (index, value) in spec.relationships.iter().enumerate() {
        use m::RelationshipType as R;
        let id = m::RelationshipId(u64::try_from(index)? + 1);
        schedule.relationships.insert(
            id,
            m::Relationship {
                id,
                predecessor: m::ActivityId(value.predecessor),
                successor: m::ActivityId(value.successor),
                predecessor_project: m::ProjectId(1),
                successor_project: m::ProjectId(1),
                kind: choose(
                    &value.kind,
                    &[
                        ("fs", R::FinishToStart),
                        ("ss", R::StartToStart),
                        ("ff", R::FinishToFinish),
                        ("sf", R::StartToFinish),
                    ],
                )?,
                lag: m::Minutes(value.lag),
                source: None,
            },
        );
    }
    Ok(schedule)
}

fn root(schedule: &mut m::Schedule, name: &str) {
    schedule.wbs.insert(
        m::WbsId(1),
        m::Wbs {
            id: m::WbsId(1),
            project: m::ProjectId(1),
            parent: None,
            code: name.to_owned(),
            name: name.to_owned(),
            sequence: 0,
            is_project_node: true,
            earned_value: m::EarnedValue::default(),
            obs: None,
            notes: std::collections::BTreeMap::new(),
            udfs: BTreeMap::new(),
            source: None,
        },
    );
}

#[test]
fn finished_work_has_nothing_left_by_default() {
    let finished: spec::Activity = toml::from_str(
        r#"
        id = 1
        name = "Done"
        calendar = 1
        kind = "task"
        duration = 480
        actual_start = "2026-01-05 08:00"
        actual_finish = "2026-01-05 17:00"
        "#,
    )
    .unwrap();
    assert_eq!(
        activity(&finished).unwrap().remaining_duration,
        m::Minutes(0)
    );
}
