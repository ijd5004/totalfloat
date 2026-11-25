//! Project codes: a database-wide dictionary of values that classify projects,
//! such as region or phase. Each project holds at most one value of each type.

use totalfloat_model::{ProjectCode, ProjectCodeId, ProjectCodeType, ProjectCodeTypeId, Schedule};

use crate::{EditError, NextIds, clean, refuse};

/// A new project code type's longest value: the length most project code types
/// in real files have.
pub const DEFAULT_PROJECT_CODE_LENGTH: u32 = 20;

/// The project code type a name means here.
pub(crate) fn project_code_type(
    schedule: &Schedule,
    wanted: &str,
) -> Result<ProjectCodeTypeId, EditError> {
    let lower = wanted.to_lowercase();
    let mut found = schedule
        .project_code_types
        .values()
        .filter(|t| t.name.to_lowercase() == lower);
    match (found.next(), found.next()) {
        (Some(one), None) => Ok(one.id),
        (Some(_), Some(_)) => refuse(
            "ambiguous-project-code",
            format!("two project code types are named {wanted}"),
            "rename one in P6 first",
        ),
        (None, _) => {
            let known: Vec<&str> = schedule
                .project_code_types
                .values()
                .map(|t| t.name.as_str())
                .collect();
            let hint = if known.is_empty() {
                "the file has no project code types. add_project_code_type adds one".to_owned()
            } else {
                format!(
                    "project code types: {}. add_project_code_type adds one",
                    known.join(", ")
                )
            };
            refuse(
                "no-such-project-code",
                format!("no project code type is named {wanted}"),
                hint,
            )
        }
    }
}

pub(crate) fn set_project_code(
    schedule: &mut Schedule,
    wanted: &str,
    value: &str,
) -> Result<String, EditError> {
    let kind = project_code_type(schedule, wanted)?;
    let lower = value.to_lowercase();
    let values: Vec<&ProjectCode> = schedule
        .project_codes
        .values()
        .filter(|c| c.code_type == kind)
        .collect();
    // A value is exact. Its description is only a fallback, as for activity codes.
    let exact: Vec<ProjectCodeId> = values
        .iter()
        .filter(|c| c.value.to_lowercase() == lower)
        .map(|c| c.id)
        .collect();
    let named: Vec<ProjectCodeId> = values
        .iter()
        .filter(|c| c.description.to_lowercase() == lower)
        .map(|c| c.id)
        .collect();
    let matches = if exact.is_empty() { named } else { exact };
    let id = match matches.as_slice() {
        [one] => *one,
        [] => {
            let known: Vec<&str> = values.iter().map(|c| c.value.as_str()).collect();
            let hint = if known.is_empty() {
                format!("{wanted} has no values yet. add_project_code_value adds one")
            } else {
                format!("{wanted} holds: {}", known.join(", "))
            };
            return refuse(
                "no-such-project-code",
                format!("{wanted} has no value {value}"),
                hint,
            );
        }
        _ => {
            return refuse(
                "ambiguous-project-code",
                format!("{} values of {wanted} match {value}", matches.len()),
                "rename one in P6, or use a value that names one",
            );
        }
    };
    let shown = schedule.project_codes[&id].value.clone();
    schedule.project.codes.insert(kind, id);
    Ok(format!(
        "project {} set to {wanted}={shown}",
        schedule.project.short_name
    ))
}

pub(crate) fn clear_project_code(
    schedule: &mut Schedule,
    wanted: &str,
) -> Result<String, EditError> {
    let kind = project_code_type(schedule, wanted)?;
    let project = schedule.project.short_name.clone();
    if schedule.project.codes.remove(&kind).is_none() {
        return refuse(
            "no-project-code",
            format!("project {project} has no {wanted} code"),
            "`inspect` lists the project's codes",
        );
    }
    Ok(format!("project {project} no longer has a {wanted} code"))
}

pub(crate) fn add_project_code_value(
    schedule: &mut Schedule,
    ids: &mut NextIds,
    wanted: &str,
    value: &str,
    description: Option<&str>,
) -> Result<String, EditError> {
    let kind = project_code_type(schedule, wanted)?;
    let description = description.unwrap_or(value);
    for (text, what) in [(value, "a value"), (description, "a description")] {
        if !clean(text) {
            return refuse(
                "invalid-project-code-value",
                format!("{text:?} can't be {what} of {wanted}"),
                "write it on one line, with no spaces at the ends",
            );
        }
    }
    let limit = schedule.project_code_types[&kind]
        .max_length
        .and_then(|limit| usize::try_from(limit).ok());
    if let Some(limit) = limit.filter(|limit| value.chars().count() > *limit) {
        return refuse(
            "project-code-too-long",
            format!("{wanted} values hold {limit} characters"),
            "shorten it",
        );
    }
    let lower = value.to_lowercase();
    let values = || {
        schedule
            .project_codes
            .values()
            .filter(|c| c.code_type == kind)
    };
    if values().any(|c| c.value.to_lowercase() == lower) {
        return refuse(
            "duplicate-project-code",
            format!("{wanted} already has the value {value}"),
            "use set_project_code to give it to the project",
        );
    }
    let sequence = values()
        .map(|c| c.sequence)
        .max()
        .map_or(0, |last| last + 1);
    let free = schedule
        .project_codes
        .keys()
        .next_back()
        .map_or(0, |last| last.0 + 1);
    let id = ProjectCodeId(ids.project_code.max(free));
    ids.project_code = id.0 + 1;
    schedule.project_codes.insert(
        id,
        ProjectCode {
            id,
            code_type: kind,
            parent: None,
            value: value.to_owned(),
            description: description.to_owned(),
            sequence,
            source: None,
        },
    );
    Ok(format!("{wanted} gained the value {value}"))
}

pub(crate) fn add_project_code_type(
    schedule: &mut Schedule,
    ids: &mut NextIds,
    name: &str,
    max_length: Option<u32>,
) -> Result<String, EditError> {
    if !clean(name) {
        return refuse(
            "invalid-project-code-type",
            format!("{name:?} can't name a project code type"),
            "write it on one line, with no spaces at the ends",
        );
    }
    if max_length == Some(0) {
        return refuse(
            "invalid-project-code-type",
            "a project code type's values need at least one character",
            "give a max_length of 1 or more",
        );
    }
    let lower = name.to_lowercase();
    if schedule
        .project_code_types
        .values()
        .any(|t| t.name.to_lowercase() == lower)
    {
        return refuse(
            "duplicate-project-code-type",
            format!("there's already a project code type named {name}"),
            "use add_project_code_value to give it values",
        );
    }
    let sequence = schedule
        .project_code_types
        .values()
        .map(|t| t.sequence)
        .max()
        .map_or(0, |last| last + 1);
    let free = schedule
        .project_code_types
        .keys()
        .next_back()
        .map_or(0, |last| last.0 + 1);
    let id = ProjectCodeTypeId(ids.project_code_type.max(free));
    ids.project_code_type = id.0 + 1;
    schedule.project_code_types.insert(
        id,
        ProjectCodeType {
            id,
            name: name.to_owned(),
            max_length: Some(max_length.unwrap_or(DEFAULT_PROJECT_CODE_LENGTH)),
            sequence,
            source: None,
        },
    );
    Ok(format!("Added project code type {name}"))
}
