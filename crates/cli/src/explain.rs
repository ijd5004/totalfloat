//! `totalfloat explain`: why a date is what it is, back to its origin.

use std::fmt::Write as _;
use std::path::Path;

use clap::ValueEnum;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use totalfloat_engine::{
    DateField, DateProvenance, ExplainError, ExplainStep, Explanation, explain,
    schedule_with_projects,
};
use totalfloat_model::{Activity, ActivityId, FloatType, Minutes, RelationshipType, Schedule};

use crate::error::{CliError, exit};
use crate::loaded::Loaded;
use crate::output::{print_line, rounded};
use crate::query::find;

/// What to explain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Deserialize, JsonSchema)]
#[value(rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum Field {
    /// `TASK.early_start_date`.
    EarlyStart,
    /// `TASK.early_end_date`.
    EarlyFinish,
    /// `TASK.late_start_date`.
    LateStart,
    /// `TASK.late_end_date`.
    LateFinish,
    /// `TASK.total_float_hr_cnt`: the gap between an early and a late date.
    TotalFloat,
}

/// `totalfloat.explain/1`.
#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct Report {
    #[schemars(extend("const" = "totalfloat.explain/1"))]
    schema: &'static str,
    file: String,
    project: u64,
    activity: ActivityReport,
    field: &'static str,
    /// Calculated total float, for `total_float`.
    total_float_hours: Option<f64>,
    /// True float where a calendar gap makes it differ from `total_float_hours`
    /// (TF-032).
    #[serde(skip_serializing_if = "Option::is_none")]
    true_total_float_hours: Option<f64>,
    /// One chain, or for total float one for each date it compares.
    chains: Vec<Chain>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct ActivityReport {
    id: u64,
    code: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct Chain {
    /// The date this chain explains.
    field: &'static str,
    date: Option<String>,
    /// What started the chain, such as `data-date`. Null for a chain that
    /// leaves this project.
    origin: Option<&'static str>,
    /// Steps in the whole chain, which can be more than `steps` holds.
    total_steps: usize,
    steps: Vec<Step>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct Step {
    activity: u64,
    code: Option<String>,
    /// Which of the activity's dates this step is, such as `early_finish`.
    field: &'static str,
    /// The working hours in a day of the activity's calendar, for putting
    /// spans and lags in days. Absent for an activity outside the schedule.
    #[serde(skip_serializing_if = "Option::is_none")]
    hours_per_day: Option<f64>,
    date: Option<String>,
    text: String,
    /// The relationship that drove this date (`TASKPRED.task_pred_id`), when
    /// one did.
    #[serde(skip_serializing_if = "Option::is_none")]
    relationship: Option<u64>,
    /// What set this date, for an app to put in its own words; `text` says
    /// the same for a person reading the report.
    #[serde(skip_serializing_if = "Option::is_none")]
    cause: Option<Cause>,
}

/// What set a date: `kind` names it (`relationship`, `constraint`,
/// `duration`, `data-date`, `actual`, ...), and the fields that apply to that
/// kind are given.
#[derive(Debug, Serialize, schemars::JsonSchema)]
struct Cause {
    kind: &'static str,
    /// The time it gave wasn't working time, so the date moved to the next.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    moved_to_working_time: bool,
    /// An as-late-as-possible constraint pushed the date as late as `kind` allowed.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    as_late_as_possible: bool,
    /// For a relationship: the activity at its other end (`task_id`) and which of its dates drove this.
    #[serde(skip_serializing_if = "Option::is_none")]
    from: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    from_field: Option<&'static str>,
    /// For a relationship: whether the activity at its other end is its
    /// `predecessor`, as for early dates, or its `successor`, as for late
    /// dates and dates pushed as late as possible.
    #[serde(skip_serializing_if = "Option::is_none")]
    from_role: Option<&'static str>,
    /// For a relationship: `FS`, `SS`, `FF` or `SF`, and the lag applied, in working hours.
    #[serde(skip_serializing_if = "Option::is_none")]
    link: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    lag_hours: Option<f64>,
    /// The lag in days of the calendar the project measures lags on, when
    /// that calendar's day is known.
    #[serde(skip_serializing_if = "Option::is_none")]
    lag_days: Option<f64>,
    /// For a constraint: its kind and date.
    #[serde(skip_serializing_if = "Option::is_none")]
    constraint: Option<&'static str>,
    /// For a constraint or an external date: the date it gave.
    #[serde(skip_serializing_if = "Option::is_none")]
    date: Option<String>,
}

#[allow(
    clippy::cast_precision_loss,
    reason = "lags are far below 2^52 minutes"
)]
fn cause_of(provenance: &DateProvenance, schedule: &Schedule, step: &ExplainStep) -> Cause {
    let driver = provenance.driver();
    let mut cause = Cause {
        kind: driver.origin_code(),
        moved_to_working_time: matches!(provenance, DateProvenance::CalendarAdjustment { .. }),
        as_late_as_possible: matches!(provenance, DateProvenance::AsLateAsPossible { .. })
            || matches!(provenance, DateProvenance::CalendarAdjustment { cause, .. } if matches!(**cause, DateProvenance::AsLateAsPossible { .. })),
        from: None,
        from_field: None,
        from_role: None,
        link: None,
        lag_hours: None,
        lag_days: None,
        constraint: None,
        date: None,
    };
    match driver {
        DateProvenance::Relationship {
            from,
            from_date,
            kind,
            lag,
            ..
        } => {
            cause.from = Some(from.0);
            cause.from_field = Some(field_label(*from_date));
            cause.link = Some(link_label(*kind));
            cause.lag_hours = Some(lag.0 as f64 / 60.0);
            // Forward, `from` is the predecessor; backward, and for a date
            // pushed as late as possible, the successor.
            let backward = matches!(step.field, DateField::LateStart | DateField::LateFinish)
                || cause.as_late_as_possible;
            cause.from_role = Some(if backward { "successor" } else { "predecessor" });
            let (pred, succ) = if backward {
                (step.activity, *from)
            } else {
                (*from, step.activity)
            };
            if let (Some(pred), Some(succ)) = (
                schedule.activities.get(&pred),
                schedule.activities.get(&succ),
            ) && let Some(day) = totalfloat_edit::lag_day_length(schedule, pred, succ)
                && day.0 > 0
            {
                cause.lag_days = Some(lag.0 as f64 / day.0 as f64);
            }
        }
        DateProvenance::Constraint { kind, date } => {
            cause.constraint = Some(crate::query::constraint_label(*kind));
            cause.date = Some(date.to_string());
        }
        DateProvenance::ExternalDate { date } => cause.date = Some(date.to_string()),
        DateProvenance::Duration { from_date } => cause.from_field = Some(field_label(*from_date)),
        _ => {}
    }
    cause
}

/// An activity's calendar's hours in a day, as a span in days counts them:
/// the calendar's stated hours, or the length of its working days when they're
/// all the same.
#[allow(clippy::cast_precision_loss, reason = "minutes in a day are tiny")]
fn hours_per_day(schedule: &Schedule, activity: ActivityId) -> Option<f64> {
    let calendar = schedule.activities.get(&activity)?.calendar;
    let day = totalfloat_edit::day_length(schedule, calendar)?;
    Some(day.0 as f64 / 60.0)
}

const fn link_label(kind: RelationshipType) -> &'static str {
    match kind {
        RelationshipType::FinishToStart => "FS",
        RelationshipType::StartToStart => "SS",
        RelationshipType::FinishToFinish => "FF",
        RelationshipType::StartToFinish => "SF",
    }
}

impl Report {
    /// Keeps this many steps from each end of every chain, so a long chain
    /// can't flood a model's context. `total_steps` still says how long it was.
    pub fn keep_ends(&mut self, ends: usize) {
        for chain in &mut self.chains {
            let excess = chain.steps.len().saturating_sub(2 * ends);
            if excess > 0 {
                chain.steps.drain(ends..ends + excess);
            }
        }
    }
}

const fn field_label(field: DateField) -> &'static str {
    match field {
        DateField::EarlyStart => "early_start",
        DateField::EarlyFinish => "early_finish",
        DateField::LateStart => "late_start",
        DateField::LateFinish => "late_finish",
        _ => "other",
    }
}

/// The dates a request covers. Total float compares whichever pair the project's
/// float type names, and smallest float can be either.
fn dates(field: Field, schedule: &Schedule) -> Vec<DateField> {
    let starts = [DateField::EarlyStart, DateField::LateStart];
    let finishes = [DateField::EarlyFinish, DateField::LateFinish];
    match field {
        Field::EarlyStart => vec![DateField::EarlyStart],
        Field::EarlyFinish => vec![DateField::EarlyFinish],
        Field::LateStart => vec![DateField::LateStart],
        Field::LateFinish => vec![DateField::LateFinish],
        Field::TotalFloat => match schedule.project.options.float_type {
            FloatType::StartFloat => starts.to_vec(),
            FloatType::FinishFloat => finishes.to_vec(),
            FloatType::SmallestFloat => starts.into_iter().chain(finishes).collect(),
        },
    }
}

fn chain(explanation: &Explanation, field: DateField, schedule: &Schedule) -> Chain {
    let steps = explanation
        .steps
        .iter()
        .zip(explanation.lines())
        .map(|(step, text)| Step {
            activity: step.activity.0,
            code: step.code.clone(),
            field: field_label(step.field),
            hours_per_day: hours_per_day(schedule, step.activity),
            date: step.date.map(|date| date.to_string()),
            text,
            relationship: match step.provenance.as_ref().map(DateProvenance::driver) {
                Some(DateProvenance::Relationship { relationship, .. }) => Some(relationship.0),
                _ => None,
            },
            cause: step
                .provenance
                .as_ref()
                .map(|provenance| cause_of(provenance, schedule, step)),
        })
        .collect();
    Chain {
        field: field_label(field),
        date: explanation
            .steps
            .first()
            .and_then(|step| step.date)
            .map(|date| date.to_string()),
        origin: explanation.origin().map(DateProvenance::origin_code),
        total_steps: explanation.steps.len(),
        steps,
    }
}

fn refused(activity: &Activity, error: &ExplainError) -> CliError {
    match error {
        ExplainError::NoSuchDate(..) => CliError::new(
            "no-such-date",
            format!("activity {} has no value for that date", activity.code),
            "finished work has no remaining dates or total float",
        ),
        other => CliError::new(
            "not-explained",
            format!("activity {}: {other}", activity.code),
            "a driving-path query lists what drives the activity",
        ),
    }
}

fn human(report: &Report, explanations: &[Explanation]) -> String {
    let mut out = String::new();
    if let Some(hours) = report.total_float_hours {
        let _ = writeln!(out, "Total float {} hours, between:", rounded(hours));
    }
    for explanation in explanations {
        out.push_str(&explanation.render());
    }
    if let Some(hours) = report.true_total_float_hours {
        let _ = writeln!(
            out,
            "\nTrue float is {} hours. Total float above leaves out the time a calendar gap leaves between a link and the activity it leads to (TF-032).",
            rounded(hours)
        );
    }
    out.trim_end().to_owned()
}

/// Explains a date in a file that's already been read.
pub fn report(
    file: &str,
    loaded: &Loaded,
    activity: &str,
    field: Field,
    project: Option<u64>,
) -> Result<(Report, Vec<Explanation>), CliError> {
    let schedules = &loaded.schedules;
    let selected: Vec<&Schedule> = schedules
        .iter()
        .filter(|s| project.is_none_or(|id| s.project.id.0 == id))
        .collect();
    let (schedule, target) = find(&selected, activity)?;
    loaded.refuse_incomplete(schedule)?;
    let options = &schedule.project.options;
    let result = schedule_with_projects(schedule, options, schedules).map_err(|e| {
        CliError::new(
            "schedule-refused",
            format!("project {}: {e}", schedule.project.id.0),
            "an explanation needs a calculated schedule",
        )
    })?;
    let fields = dates(field, schedule);
    let explanations = fields
        .iter()
        .map(|&date| explain(schedule, &result, target.id, date).map_err(|e| refused(target, &e)))
        .collect::<Result<Vec<_>, _>>()?;
    let calculated = result.activities.get(&target.id);
    let total_float = calculated.and_then(|a| a.values.total_float);
    if field == Field::TotalFloat && total_float.is_none() {
        return Err(refused(
            target,
            &ExplainError::NoSuchDate(target.id, DateField::EarlyStart),
        ));
    }
    #[allow(
        clippy::cast_precision_loss,
        reason = "display hours; minutes are far below 2^52"
    )]
    let hours = |float: Option<Minutes>| {
        float
            .filter(|_| field == Field::TotalFloat)
            .map(|float| float.get() as f64 / 60.0)
    };
    let report = Report {
        schema: "totalfloat.explain/1",
        file: file.to_owned(),
        project: schedule.project.id.0,
        activity: ActivityReport {
            id: target.id.0,
            code: target.code.clone(),
        },
        field: match field {
            Field::TotalFloat => "total_float",
            _ => field_label(fields[0]),
        },
        total_float_hours: hours(total_float),
        true_total_float_hours: hours(calculated.and_then(|a| a.true_total_float)),
        chains: explanations
            .iter()
            .zip(&fields)
            .map(|(explanation, &date)| chain(explanation, date, schedule))
            .collect(),
    };
    Ok((report, explanations))
}

/// Runs the command.
pub fn run(
    file: &Path,
    activity: &str,
    field: Field,
    project: Option<u64>,
    json: bool,
) -> Result<u8, CliError> {
    let raw = crate::location::read(file)?;
    let label = file.display().to_string();
    let (report, explanations) = report(&label, &Loaded::new(raw), activity, field, project)?;
    if json {
        print_line(&serde_json::to_string_pretty(&report).unwrap_or_default());
    } else {
        print_line(&human(&report, &explanations));
    }
    Ok(exit::OK)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_long_chain_keeps_its_ends_and_its_length() {
        let step = |activity| Step {
            activity,
            code: None,
            field: "early_start",
            hours_per_day: None,
            date: None,
            text: String::new(),
            relationship: None,
            cause: None,
        };
        let chain = |length: u64| Chain {
            field: "early_start",
            date: None,
            origin: None,
            total_steps: usize::try_from(length).unwrap(),
            steps: (0..length).map(step).collect(),
        };
        let mut report = Report {
            schema: "totalfloat.explain/1",
            file: String::new(),
            project: 1,
            activity: ActivityReport {
                id: 1,
                code: String::new(),
            },
            field: "early_start",
            total_float_hours: None,
            true_total_float_hours: None,
            chains: vec![chain(100), chain(5), chain(2)],
        };
        report.total_float_hours = Some(-8.0);
        report.true_total_float_hours = Some(8.0);
        let text = human(&report, &[]);
        assert!(text.contains("Total float -8 hours"));
        assert!(text.contains("True float is 8 hours."));
        let json = serde_json::to_value(&report).unwrap();
        assert_eq!(json["true_total_float_hours"], 8.0);
        report.keep_ends(3);
        let kept: Vec<u64> = report.chains[0].steps.iter().map(|s| s.activity).collect();
        assert_eq!(kept, [0, 1, 2, 97, 98, 99]);
        assert_eq!(report.chains[0].total_steps, 100);
        assert_eq!(report.chains[1].steps.len(), 5);
        assert_eq!(report.chains[2].steps.len(), 2);
    }
}
