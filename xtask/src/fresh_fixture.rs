//! Authored synthetic fixture for the fresh writer; no stored results.

use std::error::Error;
use std::io::Write;
use std::path::Path;

use totalfloat_model as m;
use totalfloat_xer::fresh::{FreshOptions, write};

fn model() -> Result<m::Schedule, Box<dyn Error>> {
    let project_id = m::ProjectId(1);
    let calendar_id = m::CalendarId(100);
    let wbs_id = m::WbsId(10);
    let name = "TotalFloat synthetic fresh-writer fixture";
    let mut project = m::Project::new(project_id, "TF-FRESH");
    project.name = Some(name.to_owned());
    project.data_date = Some("2026-01-05 08:00".parse()?);
    project.planned_start = project.data_date;
    project.default_calendar = Some(calendar_id);
    let mut schedule = m::Schedule::new(project);
    let time = |hour| m::TimeOfDay::from_hm(hour, 0).ok_or("invalid fixture hour");
    let morning = m::WorkInterval::new(time(8)?, time(12)?).ok_or("invalid morning interval")?;
    let afternoon =
        m::WorkInterval::new(time(13)?, time(17)?).ok_or("invalid afternoon interval")?;
    schedule.calendars.insert(
        calendar_id,
        m::Calendar {
            id: calendar_id,
            name: "Synthetic five-day workweek".to_owned(),
            kind: m::CalendarType::Global,
            project: None,
            base: None,
            is_default: true,
            hours_per_period: m::HoursPerPeriod {
                day: Some(m::Minutes(480)),
                week: Some(m::Minutes(2400)),
                month: Some(m::Minutes(9600)),
                year: Some(m::Minutes(115_200)),
            },
            work_week: m::WorkWeek(std::array::from_fn(|day| {
                if (1..=5).contains(&day) {
                    vec![morning, afternoon]
                } else {
                    vec![]
                }
            })),
            exceptions: std::collections::BTreeMap::new(),
            source: None,
        },
    );
    schedule.wbs.insert(
        wbs_id,
        m::Wbs {
            id: wbs_id,
            project: project_id,
            parent: None,
            code: "TF-FRESH".to_owned(),
            name: name.to_owned(),
            sequence: 0,
            is_project_node: true,
            earned_value: m::EarnedValue::default(),
            obs: None,
            notes: std::collections::BTreeMap::new(),
            udfs: std::collections::BTreeMap::new(),
            source: None,
        },
    );
    let mut activity = m::Activity::new(
        m::ActivityId(1),
        project_id,
        wbs_id,
        calendar_id,
        "TF-001",
        "Synthetic work",
    );
    activity.original_duration = m::Minutes(480);
    activity.remaining_duration = m::Minutes(480);
    activity
        .codes
        .insert(m::ActivityCodeTypeId(20), m::ActivityCodeId(21));
    activity.udfs.insert(
        m::UdfTypeId(30),
        m::UdfValue::Text("Synthetic note".to_owned()),
    );
    schedule.activities.insert(activity.id, activity);
    let mut milestone = m::Activity::new(
        m::ActivityId(2),
        project_id,
        wbs_id,
        calendar_id,
        "TF-002",
        "Synthetic finish milestone",
    );
    milestone.activity_type = m::ActivityType::FinishMilestone;
    schedule.activities.insert(milestone.id, milestone);
    schedule.relationships.insert(
        m::RelationshipId(1),
        m::Relationship {
            id: m::RelationshipId(1),
            predecessor: m::ActivityId(1),
            successor: m::ActivityId(2),
            predecessor_project: project_id,
            successor_project: project_id,
            kind: m::RelationshipType::FinishToStart,
            lag: m::Minutes::ZERO,
            source: None,
        },
    );
    dictionaries(&mut schedule, project_id, calendar_id);
    Ok(schedule)
}

fn dictionaries(schedule: &mut m::Schedule, project_id: m::ProjectId, calendar_id: m::CalendarId) {
    schedule.resources.insert(
        m::ResourceId(1),
        m::Resource {
            id: m::ResourceId(1),
            short_name: "TF-LABOR".to_owned(),
            name: "Synthetic labor".to_owned(),
            kind: m::ResourceType::Labor,
            calendar: Some(calendar_id),
            parent: None,
            default_units_per_hour: None,
            default_cost_follows_units: false,
            rates: Vec::new(),
            roles: Vec::new(),
            udfs: std::collections::BTreeMap::new(),
            source: None,
        },
    );
    schedule.assignments.insert(
        m::AssignmentId(1),
        m::Assignment {
            id: m::AssignmentId(1),
            activity: m::ActivityId(1),
            resource: Some(m::ResourceId(1)),
            role: None,
            cost_account: None,
            project: project_id,
            curve: None,
            typed_spread: None,
            amounts: m::Amounts::default(),
            udfs: std::collections::BTreeMap::new(),
            source: None,
        },
    );
    schedule.activity_code_types.insert(
        m::ActivityCodeTypeId(20),
        m::ActivityCodeType {
            id: m::ActivityCodeTypeId(20),
            name: "Synthetic phase".to_owned(),
            scope: m::CodeScope::Project,
            project: Some(project_id),
            max_length: Some(10),
            sequence: 0,
            source: None,
        },
    );
    schedule.activity_codes.insert(
        m::ActivityCodeId(21),
        m::ActivityCode {
            id: m::ActivityCodeId(21),
            code_type: m::ActivityCodeTypeId(20),
            parent: None,
            value: "WORK".to_owned(),
            description: "Synthetic work code".to_owned(),
            sequence: 0,
            source: None,
        },
    );
    schedule.udf_types.insert(
        m::UdfTypeId(30),
        m::UdfType {
            id: m::UdfTypeId(30),
            subject: m::UdfSubject::Activity,
            label: "Synthetic note".to_owned(),
            data_type: m::UdfDataType::Text,
            source: None,
        },
    );
}

fn generate() -> Result<Vec<u8>, Box<dyn Error>> {
    let schedule = model()?;
    let mut options = FreshOptions::new("2026-01-01".parse()?);
    options.seed = 42;
    let xer = write(&schedule, options)?;
    let mut bytes = Vec::new();
    xer.write(&mut bytes)?;
    Ok(bytes)
}

pub(crate) fn run(output: &Path) -> Result<(), Box<dyn Error>> {
    let bytes = generate()?;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)?;
    file.write_all(&bytes)?;
    println!(
        "wrote {} bytes of authored synthetic fixture data (no stored results)",
        bytes.len()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_fixture_matches_the_committed_authored_bytes() {
        let expected = include_bytes!("../../fixtures/xer/fresh-writer.xer");
        assert_eq!(generate().unwrap(), expected.as_slice());
    }
}
