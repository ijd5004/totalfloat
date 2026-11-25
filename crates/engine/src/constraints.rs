//! Constraint pass rules and validation (TF-016).
use super::{
    Activity, Context, DateField, DateProvenance, DateTime, Dates, EngineError, Inverse,
    WorkCalendar, adjust, at_work_boundary, negate,
};
use totalfloat_model::{ActivityStatus, Constraint, ConstraintType, Minutes};

pub(super) fn validate(activity: &Activity) -> Result<(), EngineError> {
    for constraint in constraints(activity) {
        if constraint.kind.requires_date() && constraint.date.is_none() {
            return Err(EngineError::InvalidInput("constraint without date"));
        }
    }
    if let Some(secondary) = activity.secondary_constraint {
        let Some(primary) = activity.primary_constraint else {
            return Err(EngineError::InvalidInput(
                "secondary constraint without primary",
            ));
        };
        if !primary.kind.allows_secondary() || !secondary.kind.can_be_secondary() {
            return Err(EngineError::InvalidInput(
                "incompatible primary and secondary constraints",
            ));
        }
    }
    Ok(())
}
fn constraints(a: &Activity) -> impl Iterator<Item = Constraint> {
    a.primary_constraint
        .into_iter()
        .chain(a.secondary_constraint)
}
/// Constraints that still apply. Finished work has none, and a started activity
/// can't be held by a start date, so only its finish constraints remain.
fn active(a: &Activity) -> impl Iterator<Item = Constraint> {
    let status = a.status;
    constraints(a).filter(move |c| match status {
        ActivityStatus::NotStarted => true,
        ActivityStatus::InProgress => matches!(
            c.kind,
            ConstraintType::FinishOn
                | ConstraintType::FinishOnOrAfter
                | ConstraintType::FinishOnOrBefore
                | ConstraintType::MandatoryFinish
        ),
        ActivityStatus::Completed => false,
    })
}
fn cause(c: Constraint) -> Result<DateProvenance, EngineError> {
    Ok(DateProvenance::Constraint {
        kind: c.kind,
        date: c
            .date
            .ok_or(EngineError::InvalidInput("constraint without date"))?,
    })
}
/// Where a set finish lands: as written if work can finish there, otherwise the
/// next working instant.
fn finish_at(calendar: &WorkCalendar, date: DateTime) -> Result<DateTime, EngineError> {
    if at_work_boundary(calendar, date) {
        Ok(date)
    } else {
        Ok(calendar.next_work_instant(date)?)
    }
}

impl Context<'_> {
    pub(super) fn early_constraints(
        &mut self,
        i: usize,
        mut best: Dates,
    ) -> Result<Dates, EngineError> {
        let a = self.graph.activities[i];
        best = self.early_external(i, best)?;
        for constraint in active(a) {
            let finish = match constraint.kind {
                ConstraintType::StartOn
                | ConstraintType::StartOnOrAfter
                | ConstraintType::MandatoryStart => false,
                ConstraintType::FinishOn
                | ConstraintType::FinishOnOrAfter
                | ConstraintType::MandatoryFinish => true,
                _ => continue,
            };
            let date = constraint
                .date
                .ok_or(EngineError::InvalidInput("constraint without date"))?;
            let mut candidate = self.forward(i, date, finish, cause(constraint)?)?;
            if constraint.kind == ConstraintType::MandatoryFinish {
                let calendar = self.calendar(self.works_on(a))?;
                let endpoint = finish_at(calendar, date)?;
                candidate.start = calendar.add_work(endpoint, negate(self.durations[i])?)?;
                // Started work whose mandatory finish leaves too little time
                // keeps the early dates its links and the data date give; the
                // backward pass keeps the mandatory finish, so the activity and
                // the work driving it show negative float
                // (docs/rules/TF-005-no-forecast-before-the-data-date.md, item 7).
                // Compared in work, so a tie across non-working time isn't a miss.
                if a.status == ActivityStatus::InProgress
                    && calendar.work_between(candidate.start, best.start) > Minutes::ZERO
                {
                    continue;
                }
                candidate.finish = endpoint;
                candidate.finish_cause =
                    adjust(cause(constraint)?, date, endpoint, self.works_on(a));
            }
            if matches!(
                constraint.kind,
                ConstraintType::MandatoryStart | ConstraintType::MandatoryFinish
            ) {
                candidate = self.not_before_data_date(i, candidate)?;
            }
            if matches!(
                constraint.kind,
                ConstraintType::MandatoryStart | ConstraintType::MandatoryFinish
            ) || super::later(&candidate, &best)
            {
                best = candidate;
            }
        }
        if self.options.use_expected_finish
            && let Some(expected) = a.expected_finish
        {
            // Mandatory endpoint constraints take precedence over an estimate.
            if !active(a).any(|c| {
                matches!(
                    c.kind,
                    ConstraintType::MandatoryStart | ConstraintType::MandatoryFinish
                )
            }) {
                let calendar = self.calendar(self.works_on(a))?;
                let finish = finish_at(calendar, expected)?;
                let duration = calendar.work_between(best.start, finish);
                // An estimate leaving no work before it can't be met, so it's
                // ignored and the remaining duration stands (TF-018). Compared in work, because two
                // spellings of one position mustn't get opposite answers. A
                // milestone has no work for an estimate to stretch.
                let remaining = self.durations[i];
                if (duration <= Minutes::ZERO && remaining > Minutes::ZERO)
                    || duration < Minutes::ZERO
                    || a.activity_type.is_milestone()
                {
                    return Ok(best);
                }
                if active(a).any(|c| {
                    matches!(
                        c.kind,
                        ConstraintType::FinishOn | ConstraintType::FinishOnOrAfter
                    ) && c.date.is_some_and(|date| finish < date)
                }) {
                    return Err(EngineError::InvalidInput(
                        "expected finish conflicts with finish constraint",
                    ));
                }
                best.finish = finish;
                best.finish_cause = adjust(
                    DateProvenance::ExpectedFinish,
                    expected,
                    finish,
                    self.works_on(a),
                );
                self.durations[i] = duration;
            }
        }
        Ok(best)
    }
    /// Unstarted work isn't forecast before the data date: a mandatory date that
    /// would start it earlier gives way to the data date in the forward pass,
    /// and the backward pass keeps the mandatory date, so the miss shows as
    /// negative float (docs/rules/TF-005-no-forecast-before-the-data-date.md).
    fn not_before_data_date(&self, i: usize, candidate: Dates) -> Result<Dates, EngineError> {
        let a = self.graph.activities[i];
        let Some(data_date) = self.input.project.data_date else {
            return Ok(candidate);
        };
        if a.status != ActivityStatus::NotStarted {
            return Ok(candidate);
        }
        let floor = self.forward(i, data_date, false, DateProvenance::DataDate)?;
        Ok(if candidate.start < floor.start {
            floor
        } else {
            candidate
        })
    }
    pub(super) fn late_constraints(&self, i: usize, mut best: Dates) -> Result<Dates, EngineError> {
        for constraint in active(self.graph.activities[i]) {
            let finish = match constraint.kind {
                ConstraintType::StartOn
                | ConstraintType::StartOnOrBefore
                | ConstraintType::MandatoryStart => false,
                ConstraintType::FinishOn
                | ConstraintType::FinishOnOrBefore
                | ConstraintType::MandatoryFinish => true,
                _ => continue,
            };
            let date = constraint
                .date
                .ok_or(EngineError::InvalidInput("constraint without date"))?;
            let candidate = self.backward(i, date, finish, cause(constraint)?)?;
            if matches!(
                constraint.kind,
                ConstraintType::MandatoryStart | ConstraintType::MandatoryFinish
            ) || candidate.start < best.start
            {
                best = candidate;
            }
        }
        Ok(best)
    }
    pub(super) fn alap_dates(
        &self,
        early: &mut [Dates],
        finish: DateTime,
    ) -> Result<(), EngineError> {
        for &i in self.graph.order.iter().rev() {
            if !active(self.graph.activities[i]).any(|c| c.kind == ConstraintType::AsLateAsPossible)
            {
                continue;
            }
            // Only an estimate that was honored holds the activity where it is.
            if matches!(
                early[i].finish_cause.driver(),
                DateProvenance::ExpectedFinish
            ) {
                continue;
            }
            let mut best = self.to_project_end(i, finish, DateProvenance::ProjectFinish)?;
            for &edge in &self.graph.outgoing[i] {
                if !self.limits_predecessor(edge) {
                    continue;
                }
                let candidate = self.late_link(
                    edge,
                    &early[self.graph.ends[edge].1],
                    true,
                    Inverse::AsSpelled,
                )?;
                if candidate.start < best.start {
                    best = candidate;
                }
            }
            best = self.late_external(i, best)?;
            best = self.late_constraints(i, best)?;
            // Compared in work, not on the clock: moving across a weekend to the
            // same work position gains nothing, so that tie stays put (TF-016).
            if self
                .calendar(self.works_on(self.graph.activities[i]))?
                .work_between(early[i].start, best.start)
                .get()
                > 0
            {
                best.start_cause = as_early(best.start_cause);
                best.finish_cause = as_early(best.finish_cause);
                // Wrap the controlling endpoint without replacing its driver.
                let target = if matches!(best.start_cause.driver(), DateProvenance::Duration { .. })
                {
                    &mut best.finish_cause
                } else {
                    &mut best.start_cause
                };
                *target = DateProvenance::AsLateAsPossible {
                    cause: Box::new(target.clone()),
                };
                early[i] = best;
            }
        }
        Ok(())
    }
}
fn as_early(cause: DateProvenance) -> DateProvenance {
    match cause {
        DateProvenance::Duration {
            from_date: DateField::LateStart,
        } => DateProvenance::Duration {
            from_date: DateField::EarlyStart,
        },
        DateProvenance::Duration {
            from_date: DateField::LateFinish,
        } => DateProvenance::Duration {
            from_date: DateField::EarlyFinish,
        },
        DateProvenance::CalendarAdjustment {
            unadjusted,
            calendar,
            cause,
        } => DateProvenance::CalendarAdjustment {
            unadjusted,
            calendar,
            cause: Box::new(as_early(*cause)),
        },
        other => other,
    }
}
