//! The OBS: who is responsible for work. Each WBS node names one OBS node as
//! its responsible manager.

use totalfloat_model::{Obs, ObsId, Schedule};

use crate::{EditError, NextIds, clean, find_wbs, refuse};

/// The OBS node a name or `obs_id` means here. Names compare ignoring case.
///
/// # Errors
///
/// [`EditError`] naming the OBS nodes there are, if none or two match.
pub fn obs_named(schedule: &Schedule, wanted: &str) -> Result<ObsId, EditError> {
    let lower = wanted.to_lowercase();
    let same: Vec<&Obs> = schedule
        .obs
        .values()
        .filter(|node| node.name.to_lowercase() == lower)
        .collect();
    match same.as_slice() {
        [one] => Ok(one.id),
        [] => match wanted.parse().map(ObsId) {
            Ok(id) if schedule.obs.contains_key(&id) => Ok(id),
            _ => {
                let known: Vec<&str> = schedule.obs.values().map(|n| n.name.as_str()).collect();
                let hint = if known.is_empty() {
                    "the file has no OBS. add_obs adds a node".to_owned()
                } else {
                    format!("OBS nodes: {}. add_obs adds one", known.join(", "))
                };
                refuse(
                    "no-such-obs",
                    format!("no OBS node is named {wanted}"),
                    hint,
                )
            }
        },
        _ => {
            let ids: Vec<String> = same.iter().map(|node| node.id.0.to_string()).collect();
            refuse(
                "ambiguous-obs",
                format!("{} OBS nodes are named {wanted}", same.len()),
                format!("use one's `obs_id`: {}", ids.join(", ")),
            )
        }
    }
}

pub(crate) fn add_obs(
    schedule: &mut Schedule,
    ids: &mut NextIds,
    name: &str,
    parent: Option<&str>,
    description: Option<&str>,
) -> Result<String, EditError> {
    if !clean(name) {
        return refuse(
            "invalid-obs",
            format!("{name:?} can't name an OBS node"),
            "write it on one line, with no spaces at the ends",
        );
    }
    let lower = name.to_lowercase();
    if schedule
        .obs
        .values()
        .any(|node| node.name.to_lowercase() == lower)
    {
        return refuse(
            "duplicate-obs",
            format!("there's already an OBS node named {name}"),
            "use set_wbs_obs to make it responsible for a WBS node",
        );
    }
    let parent = parent.map(|p| obs_named(schedule, p)).transpose()?;
    let sequence = schedule
        .obs
        .values()
        .filter(|node| node.parent == parent)
        .map(|node| node.sequence)
        .max()
        .map_or(0, |last| last + 1);
    let free = schedule.obs.keys().next_back().map_or(0, |last| last.0 + 1);
    let id = ObsId(ids.obs.max(free));
    ids.obs = id.0 + 1;
    schedule.obs.insert(
        id,
        Obs {
            id,
            parent,
            name: name.to_owned(),
            description: description.unwrap_or_default().to_owned(),
            sequence,
            source: None,
        },
    );
    Ok(match parent {
        Some(parent) => format!(
            "Added OBS node {name}, under {}",
            schedule.obs[&parent].name
        ),
        None => format!("Added OBS node {name}"),
    })
}

pub(crate) fn set_wbs_obs(
    schedule: &mut Schedule,
    wanted: &str,
    obs: &str,
) -> Result<String, EditError> {
    let node = find_wbs(schedule, wanted)?;
    let obs = obs_named(schedule, obs)?;
    let name = schedule.obs[&obs].name.clone();
    if let Some(wbs) = schedule.wbs.get_mut(&node) {
        wbs.obs = Some(obs);
    }
    Ok(format!("WBS {wanted} is now {name}'s"))
}
