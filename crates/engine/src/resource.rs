//! Resource-dependent activities run on their resources' calendar (TF-028).
use super::EngineError;
use std::collections::{BTreeMap, BTreeSet};
use totalfloat_model::{Activity, ActivityId, ActivityType, CalendarId, Schedule};

/// The calendar one activity of `schedule` runs on, for an activity read from
/// another project.
pub(super) fn calendar_of(
    schedule: &Schedule,
    activity: &Activity,
) -> Result<CalendarId, EngineError> {
    if activity.activity_type != ActivityType::ResourceDependent {
        return Ok(activity.calendar);
    }
    let mut assigned = BTreeSet::new();
    for assignment in schedule
        .assignments
        .values()
        .filter(|a| a.activity == activity.id)
    {
        // A role with no resource has no calendar of its own.
        let Some(resource) = assignment.resource else {
            continue;
        };
        let resource = schedule
            .resources
            .get(&resource)
            .ok_or(EngineError::InvalidInput("missing assigned resource"))?;
        assigned.insert(resource.calendar.unwrap_or(activity.calendar));
    }
    match assigned.len() {
        0 => Ok(activity.calendar),
        1 => Ok(assigned.first().copied().unwrap_or(activity.calendar)),
        _ => Err(EngineError::Unsupported(
            "resource-dependent activity with several resource calendars",
        )),
    }
}

/// The calendar of each resource-dependent activity with assigned resources.
/// Resources on different calendars would need each assignment scheduled on its
/// own, so that's refused rather than approximated.
pub(super) fn calendars(input: &Schedule) -> Result<BTreeMap<ActivityId, CalendarId>, EngineError> {
    let mut found: BTreeMap<ActivityId, BTreeSet<CalendarId>> = BTreeMap::new();
    for assignment in input.assignments.values() {
        let Some(activity) = input
            .activities
            .get(&assignment.activity)
            .filter(|a| a.activity_type == ActivityType::ResourceDependent)
        else {
            continue;
        };
        // A role with no resource has no calendar of its own.
        let Some(resource) = assignment.resource else {
            continue;
        };
        let resource = input
            .resources
            .get(&resource)
            .ok_or(EngineError::InvalidInput("missing assigned resource"))?;
        let calendar = resource.calendar.unwrap_or(activity.calendar);
        found
            .entry(assignment.activity)
            .or_default()
            .insert(calendar);
    }
    found
        .into_iter()
        .map(|(activity, calendars)| match calendars.first() {
            Some(&only) if calendars.len() == 1 => Ok((activity, only)),
            _ => Err(EngineError::Unsupported(
                "resource-dependent activity with several resource calendars",
            )),
        })
        .collect()
}
