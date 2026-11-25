//! Why a date is what it is: provenance walked back to its origin.
use std::fmt::Write as _;

use totalfloat_model::{
    ActivityId, ConstraintType, DateTime, LagCalendar, Minutes, RelationshipType, Schedule,
};

use crate::{DateField, DateProvenance, ScheduleResult};

/// Steps shown at each end of a rendered chain before the middle is summarized.
pub const RENDERED_ENDS: usize = 4;

/// One date in an explanation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExplainStep {
    /// The activity.
    pub activity: ActivityId,
    /// Its code, if it's in this project.
    pub code: Option<String>,
    /// Which of its dates.
    pub field: DateField,
    /// The date's value.
    pub date: Option<DateTime>,
    /// What controlled it. `None` for a date outside this project.
    pub provenance: Option<DateProvenance>,
}

/// A date and the chain of causes behind it, newest first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Explanation {
    /// Every step, from the date asked about back to its origin.
    pub steps: Vec<ExplainStep>,
}

/// Why a date can't be explained.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ExplainError {
    /// The activity isn't in the result.
    #[error("no such activity: {0:?}")]
    NoSuchActivity(ActivityId),
    /// The activity has no value for that date, as with the remaining dates of
    /// completed work.
    #[error("{0:?} has no {1:?}")]
    NoSuchDate(ActivityId, DateField),
    /// The date has a value but no recorded cause.
    #[error("no provenance for {1:?} on {0:?}")]
    NoProvenance(ActivityId, DateField),
}

/// Walks a date back to its origin.
///
/// # Errors
///
/// Fails if the activity isn't in the result, or the date is absent or has no
/// provenance. Engine results always carry provenance for every date they set.
pub fn explain(
    schedule: &Schedule,
    result: &ScheduleResult,
    activity: ActivityId,
    field: DateField,
) -> Result<Explanation, ExplainError> {
    let entry = result
        .activities
        .get(&activity)
        .ok_or(ExplainError::NoSuchActivity(activity))?;
    if entry.date(field).is_none() {
        return Err(ExplainError::NoSuchDate(activity, field));
    }
    if !entry.provenance.contains_key(&field) {
        return Err(ExplainError::NoProvenance(activity, field));
    }
    let steps = result
        .driving_chain(activity, field)
        .into_iter()
        .map(|link| ExplainStep {
            activity: link.activity,
            code: schedule
                .activities
                .get(&link.activity)
                .map(|a| a.code.clone()),
            field: link.field,
            date: link.date,
            provenance: link.provenance.cloned(),
        })
        .collect();
    Ok(Explanation { steps })
}

impl Explanation {
    /// What started the chain, looking through calendar moves.
    #[must_use]
    pub fn origin(&self) -> Option<&DateProvenance> {
        self.steps
            .last()?
            .provenance
            .as_ref()
            .map(DateProvenance::driver)
    }

    /// One line for every step, however long the chain is.
    #[must_use]
    pub fn lines(&self) -> Vec<String> {
        self.steps
            .iter()
            .enumerate()
            .map(|(index, step)| line(step, self.steps.get(index + 1)))
            .collect()
    }

    /// A concise account, one line per step. Long chains show both ends and say
    /// how many steps were left out of the middle.
    #[must_use]
    pub fn render(&self) -> String {
        let mut out = String::new();
        let count = self.steps.len();
        for (index, step) in self.steps.iter().enumerate() {
            if count > 2 * RENDERED_ENDS && index == RENDERED_ENDS {
                let _ = writeln!(out, "  ... {} more steps ...", count - 2 * RENDERED_ENDS);
            }
            if count <= 2 * RENDERED_ENDS || index < RENDERED_ENDS || index >= count - RENDERED_ENDS
            {
                let _ = writeln!(out, "{}", line(step, self.steps.get(index + 1)));
            }
        }
        out
    }
}

/// An activity's code, or a plain statement that it isn't in this project.
fn name(step: &ExplainStep) -> String {
    step.code.as_deref().map_or_else(
        || format!("activity {} (outside this project)", step.activity.0),
        str::to_owned,
    )
}

fn line(step: &ExplainStep, next: Option<&ExplainStep>) -> String {
    let who = name(step);
    let when = step
        .date
        .map_or_else(|| "no date".to_owned(), |date| date.to_string());
    let why = step
        .provenance
        .as_ref()
        .map_or_else(|| "not recorded here".to_owned(), |why| reason(why, next));
    format!("{who} {} {when}: {why}", field_name(step.field))
}

fn reason(provenance: &DateProvenance, next: Option<&ExplainStep>) -> String {
    match provenance {
        DateProvenance::Relationship {
            kind,
            lag,
            lag_calendar,
            from,
            from_date,
            ..
        } => {
            let lag = if lag.get() == 0 {
                String::new()
            } else {
                format!(
                    " with {} lag on the {}",
                    hours(*lag),
                    calendar_name(*lag_calendar)
                )
            };
            let from = next.map_or_else(|| format!("activity {}", from.0), name);
            format!(
                "{}{lag} from {from} {}",
                kind_name(*kind),
                field_name(*from_date)
            )
        }
        DateProvenance::Constraint { kind, date } => {
            format!("{} constraint at {date}", constraint_name(*kind))
        }
        DateProvenance::ExternalDate { date } => format!("external date {date}"),
        DateProvenance::ExpectedFinish => "expected finish".to_owned(),
        DateProvenance::AsLateAsPossible { cause } => {
            format!("as late as possible, up to: {}", reason(cause, next))
        }
        DateProvenance::DataDate => "data date".to_owned(),
        DateProvenance::Resume => "resume date".to_owned(),
        DateProvenance::Actual => "actual date".to_owned(),
        DateProvenance::ProjectStart => "project start".to_owned(),
        DateProvenance::MustFinishBy => "project must-finish-by date".to_owned(),
        DateProvenance::ProjectFinish => "project finish".to_owned(),
        DateProvenance::OpenEnd => "open end".to_owned(),
        DateProvenance::Duration { from_date } => {
            format!("its {} and its duration", field_name(*from_date))
        }
        DateProvenance::LoeSpan => "level-of-effort span".to_owned(),
        DateProvenance::WbsSummarySpan => "WBS summary span".to_owned(),
        DateProvenance::CalendarAdjustment {
            unadjusted,
            calendar,
            cause,
        } => format!(
            "{}, moved from {unadjusted} to working time on calendar {}",
            reason(cause, next),
            calendar.0
        ),
    }
}

fn hours(lag: Minutes) -> String {
    let sign = if lag.get() < 0 { "-" } else { "" };
    let total = lag.get().unsigned_abs();
    match total % 60 {
        0 => format!("{sign}{}h", total / 60),
        minutes => format!("{sign}{}h{minutes:02}m", total / 60),
    }
}

fn field_name(field: DateField) -> &'static str {
    match field {
        DateField::EarlyStart => "early start",
        DateField::EarlyFinish => "early finish",
        DateField::LateStart => "late start",
        DateField::LateFinish => "late finish",
        DateField::RemainingEarlyStart => "remaining early start",
        DateField::RemainingEarlyFinish => "remaining early finish",
        DateField::RemainingLateStart => "remaining late start",
        DateField::RemainingLateFinish => "remaining late finish",
        DateField::ActualStart => "actual start",
        DateField::ActualFinish => "actual finish",
    }
}

fn kind_name(kind: RelationshipType) -> &'static str {
    match kind {
        RelationshipType::FinishToStart => "FS",
        RelationshipType::StartToStart => "SS",
        RelationshipType::FinishToFinish => "FF",
        RelationshipType::StartToFinish => "SF",
    }
}

fn calendar_name(calendar: LagCalendar) -> &'static str {
    match calendar {
        LagCalendar::Predecessor => "predecessor calendar",
        LagCalendar::Successor => "successor calendar",
        LagCalendar::TwentyFourHour => "24-hour calendar",
        LagCalendar::ProjectDefault => "project default calendar",
    }
}

fn constraint_name(kind: ConstraintType) -> &'static str {
    match kind {
        ConstraintType::StartOn => "start on",
        ConstraintType::StartOnOrAfter => "start on or after",
        ConstraintType::StartOnOrBefore => "start on or before",
        ConstraintType::FinishOn => "finish on",
        ConstraintType::FinishOnOrAfter => "finish on or after",
        ConstraintType::FinishOnOrBefore => "finish on or before",
        ConstraintType::MandatoryStart => "mandatory start",
        ConstraintType::MandatoryFinish => "mandatory finish",
        ConstraintType::AsLateAsPossible => "as late as possible",
    }
}

#[cfg(test)]
mod tests;
