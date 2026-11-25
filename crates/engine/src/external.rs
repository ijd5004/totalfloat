//! Other projects and external dates (TF-030).
//!
//! A project is calculated alone. Other projects' dates are fixed inputs that
//! bound this one (TF-030).
use super::{
    Activity, CalendarId, Context, DateField, DateProvenance, DateTime, Dates, EngineError, Graph,
    WorkCalendar, ends, later, negate, progress_impl, relationship, resource_impl, spans_impl,
};
use std::collections::BTreeMap;
use totalfloat_model::{
    ActivityId, ActivityStatus, ConstraintType, Minutes, OutOfSequenceMode, ProjectId,
    Relationship, Schedule, ScheduleOptions,
};

/// Whether a link's other end stands in with a date, or the link is one the
/// engine can't honor. These have no evidence, so they're refused, not guessed.
fn check(
    edge: &Relationship,
    local: &Activity,
    inbound: bool,
    options: &ScheduleOptions,
) -> Result<(), EngineError> {
    if spans_impl::is_span(local) {
        return Err(EngineError::Unsupported("span linked to another project"));
    }
    // Constraints and expected finishes no longer apply to finished work.
    if local.status == ActivityStatus::Completed {
        return Ok(());
    }
    let alap = [local.primary_constraint, local.secondary_constraint]
        .into_iter()
        .flatten()
        .any(|c| c.kind == ConstraintType::AsLateAsPossible);
    if !inbound && alap {
        return Err(EngineError::Unsupported(
            "as-late-as-possible activity linked to another project",
        ));
    }
    if inbound
        && ends(edge.kind).1
        && options.use_expected_finish
        && local.expected_finish.is_some()
    {
        return Err(EngineError::Unsupported(
            "expected finish with a finish link from another project",
        ));
    }
    Ok(())
}

/// Finds the other end of each link. A link whose other end isn't loaded is
/// dropped if an external date stands in for it, and refused otherwise.
pub(super) fn other_ends<'a>(
    input: &Schedule,
    options: &ScheduleOptions,
    graph: &mut Graph<'a>,
    projects: &'a [Schedule],
    calendars: &mut BTreeMap<CalendarId, WorkCalendar>,
    resource_calendars: &mut BTreeMap<ActivityId, CalendarId>,
) -> Result<BTreeMap<ActivityId, &'a Activity>, EngineError> {
    let mut others = BTreeMap::new();
    let mut kept = Vec::new();
    for edge in std::mem::take(&mut graph.external) {
        let inbound = edge.successor_project == input.project.id;
        let (local, other, project): (_, _, ProjectId) = if inbound {
            (edge.successor, edge.predecessor, edge.predecessor_project)
        } else {
            (edge.predecessor, edge.successor, edge.successor_project)
        };
        let local = input
            .activities
            .get(&local)
            .ok_or(EngineError::InvalidInput("missing relationship endpoint"))?;
        check(edge, local, inbound, options)?;
        let found = projects
            .iter()
            .find(|p| p.project.id == project)
            .and_then(|p| Some((p, p.activities.get(&other)?)));
        let Some((schedule, activity)) = found else {
            let stand_in = if inbound {
                local.external_early_start
            } else {
                local.external_late_finish
            };
            // Finished work reads no external date, so the link can't matter.
            if stand_in.is_some() || local.status == ActivityStatus::Completed {
                continue;
            }
            return Err(EngineError::Unsupported("external relationships"));
        };
        others.insert(other, activity);
        kept.push(edge);
        let works_on = resource_impl::calendar_of(schedule, activity)?;
        resource_calendars.insert(other, works_on);
        if let std::collections::btree_map::Entry::Vacant(entry) = calendars.entry(works_on) {
            let calendar = schedule
                .calendars
                .get(&works_on)
                .ok_or(EngineError::InvalidInput("missing calendar"))?;
            entry.insert(WorkCalendar::new(calendar)?);
        }
    }
    for edge in &kept {
        let (from, to) = (edge.predecessor, edge.successor);
        graph.external_from.entry(from).or_default().push(edge);
        graph.external_to.entry(to).or_default().push(edge);
    }
    graph.external = kept;
    Ok(others)
}

impl Context<'_> {
    /// An external date, unless the option ignores other projects.
    pub(super) fn external(&self, date: Option<DateTime>, a: &Activity) -> Option<DateTime> {
        let finished = a.status == ActivityStatus::Completed;
        date.filter(|_| !finished && !self.options.ignore_external_relationships)
    }

    fn other(&self, id: ActivityId) -> Result<&Activity, EngineError> {
        self.others
            .get(&id)
            .copied()
            .ok_or(EngineError::InvalidInput("missing relationship endpoint"))
    }

    /// Progress Override drops a link that recorded progress has broken.
    fn link_broken(&self, edge: &Relationship, pred: &Activity, succ: &Activity) -> bool {
        self.options.out_of_sequence == OutOfSequenceMode::ProgressOverride
            && progress_impl::out_of_sequence(edge.kind, pred, succ)
    }

    /// The date on the other end of a link into this project, and which date it
    /// is. Work that has happened counts from its actual date.
    fn other_early(pred: &Activity, finish: bool) -> Result<(DateTime, DateField), EngineError> {
        let (date, field) = if let Some(actual) = pred.actual_finish.filter(|_| finish) {
            (Some(actual), DateField::ActualFinish)
        } else if finish {
            (pred.stored.early_finish, DateField::EarlyFinish)
        } else if let Some(actual) = pred.actual_start {
            (Some(actual), DateField::ActualStart)
        } else {
            (pred.stored.early_start, DateField::EarlyStart)
        };
        let date = date.ok_or(EngineError::Unsupported(
            "linked activity in another project has no stored dates",
        ))?;
        Ok((date, field))
    }

    /// The date on the other end of a link out of this project. Started work
    /// stores what's left of it in its remaining late dates.
    fn other_late(succ: &Activity, finish: bool) -> Result<(DateTime, DateField), EngineError> {
        let stored = &succ.stored;
        let (date, field) = if finish {
            (
                stored.remaining_late_finish.or(stored.late_finish),
                DateField::LateFinish,
            )
        } else {
            (
                stored.remaining_late_start.or(stored.late_start),
                DateField::LateStart,
            )
        };
        let date = date.ok_or(EngineError::Unsupported(
            "linked activity in another project has no stored dates",
        ))?;
        Ok((date, field))
    }

    /// Holds back work that hasn't started.
    pub(super) fn early_external(&self, i: usize, mut best: Dates) -> Result<Dates, EngineError> {
        let a = self.graph.activities[i];
        if a.status != ActivityStatus::NotStarted {
            return Ok(best);
        }
        if let Some(date) = self.external(a.external_early_start, a) {
            let cause = DateProvenance::ExternalDate { date };
            let candidate = self.forward(i, date, false, cause)?;
            if later(&candidate, &best) {
                best = candidate;
            }
        }
        for &edge in self.graph.external_to.get(&a.id).into_iter().flatten() {
            let pred = self.other(edge.predecessor)?;
            if self.link_broken(edge, pred, a) {
                continue;
            }
            let (date, from_date) = Self::other_early(pred, ends(edge.kind).0)?;
            let bound = self.lag_calendar(edge)?.add_work(date, edge.lag)?;
            let lag_calendar = self.options.lag_calendar;
            let cause = relationship(edge, edge.lag, true, from_date, lag_calendar);
            let candidate = self.forward(i, bound, ends(edge.kind).1, cause)?;
            if later(&candidate, &best) {
                best = candidate;
            }
        }
        Ok(best)
    }

    /// The least slack before a link out of activity `i` to another project
    /// would move the other end's early date, or `None` with no such link.
    /// Read from the other end's stored early dates, which are its latest
    /// results when projects are scheduled together.
    pub(super) fn external_free_float(
        &self,
        i: usize,
        early: &Dates,
    ) -> Result<Option<Minutes>, EngineError> {
        let a = self.graph.activities[i];
        // Finished work has no free float to report.
        if a.status == ActivityStatus::Completed {
            return Ok(None);
        }
        let mut free_float = None;
        for &edge in self.graph.external_from.get(&a.id).into_iter().flatten() {
            let succ = self.other(edge.successor)?;
            let (pred_finish, succ_finish) = ends(edge.kind);
            if succ.status == ActivityStatus::Completed
                || self.link_broken(edge, a, succ)
                || self.measures_from_actual_start(i, pred_finish)
            {
                continue;
            }
            let stored = &succ.stored;
            let date = if succ_finish {
                stored.remaining_early_finish.or(stored.early_finish)
            } else {
                stored.remaining_early_start.or(stored.early_start)
            };
            // Free float is reported, not used: an end with no date leaves
            // the link out rather than refusing the project.
            let Some(date) = date else {
                continue;
            };
            let lag = self.lag_left(i, edge)?;
            let calendar = self.lag_calendar(edge)?;
            let bound = calendar.add_work(date, negate(lag)?)?;
            let bound = if lag.get() == 0 {
                bound
            } else {
                calendar.next_work_instant(bound)?
            };
            let (start, _) = self.backward_dates(i, bound, pred_finish)?;
            let slack = self.calendar_at(i)?.work_between(early.start, start);
            free_float = Some(free_float.map_or(slack, |old: Minutes| old.min(slack)));
        }
        Ok(free_float)
    }

    /// Limits late dates on work that hasn't finished.
    pub(super) fn late_external(&self, i: usize, mut best: Dates) -> Result<Dates, EngineError> {
        let a = self.graph.activities[i];
        if a.status == ActivityStatus::Completed {
            return Ok(best);
        }
        if let Some(date) = self.external(a.external_late_finish, a) {
            let cause = DateProvenance::ExternalDate { date };
            let candidate = self.backward(i, date, true, cause)?;
            if candidate.start < best.start {
                best = candidate;
            }
        }
        for &edge in self.graph.external_from.get(&a.id).into_iter().flatten() {
            let succ = self.other(edge.successor)?;
            let (pred_finish, succ_finish) = ends(edge.kind);
            if succ.status == ActivityStatus::Completed
                || self.link_broken(edge, a, succ)
                || self.measures_from_actual_start(i, pred_finish)
            {
                continue;
            }
            let (date, from_date) = Self::other_late(succ, succ_finish)?;
            let lag = self.lag_left(i, edge)?;
            let calendar = self.lag_calendar(edge)?;
            let bound = calendar.add_work(date, negate(lag)?)?;
            // As in `late_link`: a nonzero lag allows the whole nonworking plateau.
            let bound = if lag.get() == 0 {
                bound
            } else {
                calendar.next_work_instant(bound)?
            };
            let lag_calendar = self.options.lag_calendar;
            let cause = relationship(edge, lag, false, from_date, lag_calendar);
            let candidate = self.backward(i, bound, pred_finish, cause)?;
            if candidate.start < best.start {
                best = candidate;
            }
        }
        Ok(best)
    }
}
