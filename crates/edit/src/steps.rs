//! Activity steps: the work broken into weighted parts, each with its own
//! percent complete (TF-042).

use totalfloat_model::{PercentCompleteType, Schedule, Step, StepId};

use crate::{EditError, NextIds, find_mut, refuse};

/// What a step edit changes.
pub(crate) struct Change<'a> {
    pub(crate) name: Option<&'a str>,
    pub(crate) weight: Option<f64>,
    pub(crate) percent: Option<f64>,
}

fn check(change: &Change<'_>) -> Result<(), EditError> {
    if let Some(name) = change.name
        && (name.trim().is_empty() || name != name.trim() || name.contains(['\t', '\n', '\r']))
    {
        return refuse(
            "invalid-step",
            format!("{name:?} can't name a step"),
            "write it on one line, with no spaces at the ends",
        );
    }
    if change.weight.is_some_and(|w| !w.is_finite() || w < 0.0) {
        return refuse(
            "invalid-step",
            "a step's weight is a number, 0 or more",
            "give a weight such as 1 or 2.5",
        );
    }
    if change
        .percent
        .is_some_and(|p| !p.is_finite() || !(0.0..=100.0).contains(&p))
    {
        return refuse(
            "invalid-step",
            "a step's percent complete is 0 to 100",
            "give a percent such as 50",
        );
    }
    Ok(())
}

/// The step a name or `proc_id` means on this activity.
fn find_step(activity: &totalfloat_model::Activity, wanted: &str) -> Result<StepId, EditError> {
    let lower = wanted.to_lowercase();
    let named: Vec<&Step> = activity
        .steps
        .values()
        .filter(|s| s.name.to_lowercase() == lower)
        .collect();
    match named.as_slice() {
        [one] => Ok(one.id),
        [] => match wanted.parse().map(StepId) {
            Ok(id) if activity.steps.contains_key(&id) => Ok(id),
            _ => {
                let mut held: Vec<&Step> = activity.steps.values().collect();
                held.sort_by_key(|s| (s.sequence, s.id));
                let listed: Vec<&str> = held.iter().map(|s| s.name.as_str()).collect();
                let hint = if listed.is_empty() {
                    "it has no steps. add_step adds one".to_owned()
                } else {
                    format!("its steps: {}", listed.join(", "))
                };
                refuse(
                    "no-such-step",
                    format!("{} has no step {wanted}", activity.code),
                    hint,
                )
            }
        },
        _ => {
            let ids: Vec<String> = named.iter().map(|s| s.id.0.to_string()).collect();
            refuse(
                "ambiguous-step",
                format!(
                    "{} steps of {} are named {wanted}",
                    named.len(),
                    activity.code
                ),
                format!("use one's `proc_id`: {}", ids.join(", ")),
            )
        }
    }
}

/// Physical percent follows the steps where the project says so and the
/// activity measures physical percent (TF-042). Says what moved, if anything.
fn follow(schedule: &mut Schedule, code: &str) -> Result<Option<String>, EditError> {
    let from_steps = schedule.project.percent_from_steps;
    let activity = find_mut(schedule, code)?;
    if !from_steps || activity.percent_complete_type != PercentCompleteType::Physical {
        return Ok(None);
    }
    let Some(percent) = activity.steps_percent() else {
        return Ok(None);
    };
    let percent = (percent * 100.0).round() / 100.0;
    if activity.physical_percent_complete == Some(percent) {
        return Ok(None);
    }
    activity.physical_percent_complete = Some(percent);
    Ok(Some(format!("physical percent now {percent}")))
}

fn said(line: String, followed: Option<String>) -> String {
    match followed {
        Some(moved) => format!("{line}; {moved}"),
        None => line,
    }
}

pub(crate) fn add_step(
    schedule: &mut Schedule,
    ids: &mut NextIds,
    code: &str,
    change: &Change<'_>,
) -> Result<String, EditError> {
    check(change)?;
    let Some(name) = change.name else {
        return refuse("invalid-step", "a new step needs a name", "give `step`");
    };
    let free = schedule
        .activities
        .values()
        .flat_map(|a| a.steps.keys())
        .map(|id| id.0 + 1)
        .max()
        .unwrap_or(0);
    let id = StepId(ids.step.max(free));
    let activity = find_mut(schedule, code)?;
    let lower = name.to_lowercase();
    if activity
        .steps
        .values()
        .any(|s| s.name.to_lowercase() == lower)
    {
        return refuse(
            "duplicate-step",
            format!("{code} already has a step {name}"),
            "set_step changes it",
        );
    }
    let sequence = activity
        .steps
        .values()
        .map(|s| s.sequence)
        .max()
        .map_or(0, |last| last + 1);
    let percent = change.percent.unwrap_or(0.0);
    activity.steps.insert(
        id,
        Step {
            id,
            name: name.to_owned(),
            sequence,
            weight: Some(change.weight.unwrap_or(1.0)),
            percent: Some(percent),
            completed: percent >= 100.0,
            description: String::new(),
            source: None,
        },
    );
    ids.step = id.0 + 1;
    let followed = follow(schedule, code)?;
    Ok(said(format!("{code} gained the step {name}"), followed))
}

pub(crate) fn set_step(
    schedule: &mut Schedule,
    code: &str,
    wanted: &str,
    change: &Change<'_>,
) -> Result<String, EditError> {
    check(change)?;
    if change.name.is_none() && change.weight.is_none() && change.percent.is_none() {
        return refuse(
            "nothing-to-change",
            format!("the edit changes nothing on step {wanted}"),
            "give `name`, `weight` or `percent`",
        );
    }
    let activity = find_mut(schedule, code)?;
    let id = find_step(activity, wanted)?;
    if let Some(name) = change.name {
        let lower = name.to_lowercase();
        if activity
            .steps
            .values()
            .any(|s| s.id != id && s.name.to_lowercase() == lower)
        {
            return refuse(
                "duplicate-step",
                format!("{code} already has a step {name}"),
                "choose another name",
            );
        }
    }
    if let Some(step) = activity.steps.get_mut(&id) {
        if let Some(name) = change.name {
            name.clone_into(&mut step.name);
        }
        if let Some(weight) = change.weight {
            step.weight = Some(weight);
        }
        if let Some(percent) = change.percent {
            step.percent = Some(percent);
            step.completed = percent >= 100.0;
        }
    }
    let followed = follow(schedule, code)?;
    Ok(said(format!("{code} step {wanted} changed"), followed))
}

pub(crate) fn complete_step(
    schedule: &mut Schedule,
    code: &str,
    wanted: &str,
) -> Result<String, EditError> {
    let activity = find_mut(schedule, code)?;
    let id = find_step(activity, wanted)?;
    if let Some(step) = activity.steps.get_mut(&id) {
        step.completed = true;
        step.percent = Some(100.0);
    }
    let followed = follow(schedule, code)?;
    Ok(said(format!("{code} step {wanted} complete"), followed))
}

pub(crate) fn delete_step(
    schedule: &mut Schedule,
    code: &str,
    wanted: &str,
) -> Result<String, EditError> {
    let activity = find_mut(schedule, code)?;
    let id = find_step(activity, wanted)?;
    activity.steps.remove(&id);
    let followed = follow(schedule, code)?;
    Ok(said(format!("{code} step {wanted} deleted"), followed))
}
