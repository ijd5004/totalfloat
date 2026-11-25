//! Task/milestone CPM. Endpoint and float policy: TF-015; progress: TF-020.
use crate::{
    ActivityResult, DateField, DateProvenance, EngineError, ProjectResult, ScheduleResult,
    graph::Graph,
};
use std::collections::BTreeMap;
use totalfloat_calendar::WorkCalendar;
use totalfloat_model::{
    Activity, ActivityStatus, ActivityType, CalendarId, DateTime, FloatType, LagCalendar, Minutes,
    OutOfSequenceMode, Relationship, RelationshipId, RelationshipType, Schedule, ScheduleOptions,
    StoredResults,
};

#[derive(Clone, PartialEq)]
struct Dates {
    start: DateTime,
    finish: DateTime,
    start_cause: DateProvenance,
    finish_cause: DateProvenance,
}
impl Dates {
    /// Whether the finish is a date someone set, rather than a work position
    /// the network reached. Lateness against a set date is real (TF-032).
    fn finish_is_a_set_date(&self) -> bool {
        matches!(
            self.finish_cause.driver(),
            DateProvenance::Constraint { .. }
                | DateProvenance::ExternalDate { .. }
                | DateProvenance::MustFinishBy
        )
    }
}
struct Context<'a> {
    input: &'a Schedule,
    options: &'a ScheduleOptions,
    graph: Graph<'a>,
    calendars: BTreeMap<CalendarId, WorkCalendar>,
    continuous: WorkCalendar,
    durations: Vec<Minutes>,
    /// Resource-dependent activities, on their resources' calendar.
    resource_calendars: BTreeMap<totalfloat_model::ActivityId, CalendarId>,
    /// The other ends of relationships with other projects.
    others: BTreeMap<totalfloat_model::ActivityId, &'a Activity>,
    wbs_index: std::cell::OnceCell<spans_impl::WbsIndex>,
    /// `works_on` for each activity in `graph`, by index.
    calendar_ids: Vec<CalendarId>,
}
impl Context<'_> {
    fn calendar(&self, id: CalendarId) -> Result<&WorkCalendar, EngineError> {
        self.calendars
            .get(&id)
            .ok_or(EngineError::InvalidInput("missing calendar"))
    }
    fn lag_calendar(&self, edge: &Relationship) -> Result<&WorkCalendar, EngineError> {
        let activity = match self.options.lag_calendar {
            LagCalendar::Predecessor => edge.predecessor,
            LagCalendar::Successor => edge.successor,
            LagCalendar::TwentyFourHour => return Ok(&self.continuous),
            LagCalendar::ProjectDefault => {
                return self.calendar(
                    self.input
                        .project
                        .default_calendar
                        .ok_or(EngineError::InvalidInput("missing project lag calendar"))?,
                );
            }
        };
        let a = self
            .input
            .activities
            .get(&activity)
            .or_else(|| self.others.get(&activity).copied())
            .ok_or(EngineError::InvalidInput("missing activity"))?;
        self.calendar(self.works_on(a))
    }
    /// The calendar an activity's dates run on.
    pub(super) fn works_on(&self, a: &Activity) -> CalendarId {
        self.resource_calendars
            .get(&a.id)
            .copied()
            .unwrap_or(a.calendar)
    }
    /// The calendar activity `i` of the graph works on: [`Self::works_on`]
    /// without looking it up.
    pub(super) fn calendar_at(&self, i: usize) -> Result<&WorkCalendar, EngineError> {
        self.calendar(self.calendar_ids[i])
    }
    /// [`Self::lag_calendar`] for a link between activities `pred` and `succ`
    /// of the graph, without looking them up by ID.
    fn link_calendar(
        &self,
        pred: usize,
        succ: usize,
        edge: &Relationship,
    ) -> Result<&WorkCalendar, EngineError> {
        match self.options.lag_calendar {
            LagCalendar::Predecessor => self.calendar_at(pred),
            LagCalendar::Successor => self.calendar_at(succ),
            LagCalendar::TwentyFourHour | LagCalendar::ProjectDefault => self.lag_calendar(edge),
        }
    }
    fn forward(
        &self,
        i: usize,
        bound: DateTime,
        finish_bound: bool,
        cause: DateProvenance,
    ) -> Result<Dates, EngineError> {
        let a = self.graph.activities[i];
        let c = self.calendar_at(i)?;
        let works_on = self.calendar_ids[i];
        let duration = self.durations[i];
        let shape = Shape::of(a, duration);
        let spot = c.spot(bound);
        let start = if shape.keeps_forward_bound() && at_work_boundary(c, bound) {
            bound
        } else if finish_bound {
            spot.plus(negate(duration)?)?.start()?
        } else {
            spot.start()?
        };
        let finish = shape.finish(c, start, duration)?;
        // The start comes from the bound's work position, so the finish is at that
        // position too and can only differ in spelling. It can't read before the bound.
        let finish = if finish_bound && finish < bound {
            c.next_work_instant(bound)?
        } else {
            finish
        };
        let (start_cause, finish_cause) = if finish_bound {
            (
                DateProvenance::Duration {
                    from_date: DateField::EarlyFinish,
                },
                adjust(cause, bound, finish, works_on),
            )
        } else {
            (
                adjust(cause, bound, start, works_on),
                DateProvenance::Duration {
                    from_date: DateField::EarlyStart,
                },
            )
        };
        let finish_cause = if !finish_bound && duration.get() == 0 && finish != start {
            adjust(finish_cause, start, finish, works_on)
        } else {
            finish_cause
        };
        Ok(Dates {
            start,
            finish,
            start_cause,
            finish_cause,
        })
    }
    /// [`Self::backward`]'s dates, without the provenance.
    fn backward_dates(
        &self,
        i: usize,
        bound: DateTime,
        finish_bound: bool,
    ) -> Result<(DateTime, DateTime), EngineError> {
        let a = self.graph.activities[i];
        let c = self.calendar_at(i)?;
        let d = self.durations[i];
        let shape = Shape::of(a, d);
        let spot = c.spot(bound);
        let start = match (shape, finish_bound) {
            (Shape::Finished, _) => bound,
            (Shape::ZeroTask, true) => spot.start()?,
            (Shape::StartMilestone, true) if at_work_boundary(c, bound) => bound,
            (_, true) if d.get() == 0 => spot.finish()?,
            (_, true) => spot.plus(negate(d)?)?.start()?,
            (_, false) if c.is_work(bound) => bound,
            (_, false) => spot.finish()?,
        };
        Ok((start, shape.finish(c, start, d)?))
    }
    fn backward(
        &self,
        i: usize,
        bound: DateTime,
        finish_bound: bool,
        cause: DateProvenance,
    ) -> Result<Dates, EngineError> {
        let (start, finish) = self.backward_dates(i, bound, finish_bound)?;
        let works_on = self.calendar_ids[i];
        let d = self.durations[i];
        let (start_cause, finish_cause) = if finish_bound {
            (
                DateProvenance::Duration {
                    from_date: DateField::LateFinish,
                },
                adjust(cause, bound, finish, works_on),
            )
        } else {
            (
                adjust(cause, bound, start, works_on),
                DateProvenance::Duration {
                    from_date: DateField::LateStart,
                },
            )
        };
        let start_cause = if finish_bound && d.get() == 0 && start != finish {
            adjust(start_cause, finish, start, works_on)
        } else {
            start_cause
        };
        Ok(Dates {
            start,
            finish,
            start_cause,
            finish_cause,
        })
    }
    /// Latest dates with nothing after them but the calculated project end. That
    /// end is a moment some activity reached, not a successor's start, so where
    /// it's a working instant here, work can finish exactly on it. Without a
    /// duration, the start is that same moment.
    pub(super) fn to_project_end(
        &self,
        i: usize,
        end: DateTime,
        cause: DateProvenance,
    ) -> Result<Dates, EngineError> {
        let c = self.calendar_at(i)?;
        let mut dates = self.backward(i, end, true, cause.clone())?;
        if dates.finish < end && c.is_work(end) && c.work_between(dates.finish, end).get() == 0 {
            dates.finish = end;
            dates.finish_cause = cause;
            if self.durations[i].get() == 0 {
                dates.start = end;
            }
        }
        Ok(dates)
    }
    /// Where a link holds its predecessor back. `inverse` picks the rule as
    /// spelled or the exact one (TF-032).
    fn late_link(
        &self,
        e: usize,
        successor: &Dates,
        early: bool,
        inverse: Inverse,
    ) -> Result<Dates, EngineError> {
        let (a, _) = self.graph.ends[e];
        let edge = self.graph.edges[e];
        let (pred_finish, succ_finish) = ends(edge.kind);
        let (bound, edge_lag) = self.late_link_bound(e, successor, inverse)?;
        let field = match (early, succ_finish) {
            (true, true) => DateField::EarlyFinish,
            (true, false) => DateField::EarlyStart,
            (false, true) => DateField::LateFinish,
            (false, false) => DateField::LateStart,
        };
        let cause = relationship(edge, edge_lag, false, field, self.options.lag_calendar);
        self.backward(a, bound, pred_finish, cause)
    }
    /// Where a link puts its predecessor's start or finish (whichever the link
    /// leaves from), and the lag it ran.
    fn late_link_bound(
        &self,
        e: usize,
        successor: &Dates,
        inverse: Inverse,
    ) -> Result<(DateTime, Minutes), EngineError> {
        let (a, b) = self.graph.ends[e];
        let edge = self.graph.edges[e];
        let succ_finish = ends(edge.kind).1;
        let lag = self.link_calendar(a, b, edge)?;
        let edge_lag = self.remaining_lag(a, b, edge)?;
        let source = if !succ_finish {
            successor.start
        } else if inverse == Inverse::Exact && !successor.finish_is_a_set_date() {
            self.successor_calendar(e)?
                .next_work_instant(successor.finish)?
        } else {
            successor.finish
        };
        let inverse = lag.add_work(source, negate(edge_lag)?)?;
        // Nonzero lag depends on work coordinate, so the entire nonworking
        // plateau is allowed. Zero lag is a civil endpoint identity.
        let bound = if edge_lag.get() != 0 {
            lag.next_work_instant(inverse)?
        } else {
            inverse
        };
        Ok((bound, edge_lag))
    }
}
/// How a late link undoes the forward pass (TF-032).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Inverse {
    /// From the late finish as spelled, which can lose time (TF-032).
    AsSpelled,
    /// From that finish's work position, keeping the time.
    Exact,
}
/// Each activity's dates from one pass, and whether each differs from what a
/// kept calculation found.
type Pass = (Vec<Dates>, Vec<bool>);
/// Whether dates are later: by start, then by finish, because two finish bounds
/// at one work position can still be different clock moments.
fn later(candidate: &Dates, best: &Dates) -> bool {
    (candidate.start, candidate.finish) > (best.start, best.finish)
}
/// Whether work could start or finish at `instant` as it is written.
pub(super) fn at_work_boundary(c: &WorkCalendar, instant: DateTime) -> bool {
    c.is_work(instant) || c.prev_work_instant(instant) == Ok(instant)
}
/// What an activity is to the passes, worked out once so both directions share
/// one reading of it.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Shape {
    /// Finished work: one point, wherever it's put.
    Finished,
    /// A finish milestone, which keeps a working finish boundary.
    FinishMilestone,
    /// A start milestone.
    StartMilestone,
    /// An unstarted task with no duration, which spans the nonworking time
    /// around it.
    ZeroTask,
    /// Anything else.
    Work,
}
impl Shape {
    fn of(a: &Activity, duration: Minutes) -> Self {
        if a.status == ActivityStatus::Completed {
            Self::Finished
        } else if a.activity_type == ActivityType::FinishMilestone {
            Self::FinishMilestone
        } else if a.activity_type == ActivityType::StartMilestone {
            Self::StartMilestone
        } else if duration.get() == 0
            && a.activity_type == ActivityType::TaskDependent
            && a.status == ActivityStatus::NotStarted
        {
            Self::ZeroTask
        } else {
            Self::Work
        }
    }
    const fn keeps_forward_bound(self) -> bool {
        matches!(self, Self::Finished | Self::FinishMilestone)
    }
    fn finish(
        self,
        c: &WorkCalendar,
        start: DateTime,
        duration: Minutes,
    ) -> Result<DateTime, EngineError> {
        if self == Self::ZeroTask {
            Ok(c.prev_work_instant(start)?)
        } else {
            Ok(c.add_work(start, duration)?)
        }
    }
}
fn negate(value: Minutes) -> Result<Minutes, EngineError> {
    value.checked_neg().ok_or(EngineError::InvalidInput(
        "unrepresentable signed lag or duration",
    ))
}
fn ends(kind: RelationshipType) -> (bool, bool) {
    match kind {
        RelationshipType::FinishToStart => (true, false),
        RelationshipType::StartToStart => (false, false),
        RelationshipType::FinishToFinish => (true, true),
        RelationshipType::StartToFinish => (false, true),
    }
}
fn relationship(
    edge: &Relationship,
    lag: Minutes,
    forward: bool,
    from_date: DateField,
    lag_calendar: LagCalendar,
) -> DateProvenance {
    DateProvenance::Relationship {
        relationship: edge.id,
        from: if forward {
            edge.predecessor
        } else {
            edge.successor
        },
        from_date,
        kind: edge.kind,
        lag,
        lag_calendar,
    }
}
fn adjust(
    cause: DateProvenance,
    unadjusted: DateTime,
    adjusted: DateTime,
    calendar: CalendarId,
) -> DateProvenance {
    if adjusted == unadjusted {
        cause
    } else {
        DateProvenance::CalendarAdjustment {
            unadjusted,
            calendar,
            cause: Box::new(cause),
        }
    }
}
fn validate(a: &Activity) -> Result<(), EngineError> {
    progress_impl::validate(a)?;
    constraints_impl::validate(a)?;
    let timed = a.status != ActivityStatus::Completed && !a.activity_type.is_milestone();
    if timed && a.remaining_duration.get() < 0 {
        return Err(EngineError::InvalidInput("negative duration"));
    }
    Ok(())
}

impl<'a> Context<'a> {
    fn new(
        input: &'a Schedule,
        options: &'a ScheduleOptions,
        projects: &'a [Schedule],
    ) -> Result<Self, EngineError> {
        for a in input.activities.values() {
            validate(a)?;
        }
        let mut graph = Graph::new(input, options)?;
        progress_impl::check_network(input, &graph, options.out_of_sequence)?;
        let mut resource_calendars = resource_impl::calendars(input)?;
        let mut calendars = BTreeMap::new();
        for a in &graph.activities {
            let id = resource_calendars.get(&a.id).copied().unwrap_or(a.calendar);
            if let std::collections::btree_map::Entry::Vacant(entry) = calendars.entry(id) {
                let c = input
                    .calendars
                    .get(&id)
                    .ok_or(EngineError::InvalidInput("missing activity calendar"))?;
                entry.insert(WorkCalendar::new(c)?);
            }
        }
        if options.lag_calendar == LagCalendar::ProjectDefault
            && !(graph.edges.is_empty() && graph.external.is_empty())
        {
            let id = input
                .project
                .default_calendar
                .ok_or(EngineError::InvalidInput("missing project lag calendar"))?;
            if let std::collections::btree_map::Entry::Vacant(entry) = calendars.entry(id) {
                entry.insert(WorkCalendar::new(
                    input
                        .calendars
                        .get(&id)
                        .ok_or(EngineError::InvalidInput("missing project lag calendar"))?,
                )?);
            }
        }
        let others = external_impl::other_ends(
            input,
            options,
            &mut graph,
            projects,
            &mut calendars,
            &mut resource_calendars,
        )?;
        let calendar_ids = graph
            .activities
            .iter()
            .map(|a| resource_calendars.get(&a.id).copied().unwrap_or(a.calendar))
            .collect();
        Ok(Self {
            input,
            options,
            others,
            calendar_ids,
            wbs_index: std::cell::OnceCell::new(),
            durations: graph
                .activities
                .iter()
                .map(|a| {
                    // A milestone takes no time, whatever duration the file holds.
                    if a.status == ActivityStatus::Completed || a.activity_type.is_milestone() {
                        Minutes::ZERO
                    } else {
                        a.remaining_duration
                    }
                })
                .collect(),
            graph,
            calendars,
            resource_calendars,
            continuous: WorkCalendar::continuous()?,
        })
    }
    /// Where unstarted work may begin.
    pub(super) fn project_anchor(&self) -> Result<(DateTime, DateProvenance), EngineError> {
        match (
            self.input.project.planned_start,
            self.input.project.data_date,
        ) {
            (Some(p), Some(d)) if p > d => Ok((p, DateProvenance::ProjectStart)),
            (_, Some(d)) => Ok((d, DateProvenance::DataDate)),
            (Some(p), None) => Ok((p, DateProvenance::ProjectStart)),
            (None, None) => Err(EngineError::InvalidInput(
                "missing project start and data date",
            )),
        }
    }
    /// Early dates and, for each activity, whether they differ from what
    /// `reuse` kept (always, with nothing kept).
    fn early_dates(&mut self, reuse: Option<&reuse_impl::Reuse<'_>>) -> Result<Pass, EngineError> {
        let (anchor, cause) = self.project_anchor()?;
        let count = self.graph.activities.len();
        let mut early = vec![None; count];
        let mut changed = vec![true; count];
        for i in self.graph.order.clone() {
            let Some(reuse) = reuse else {
                early[i] = Some(self.early_at(i, &early, (anchor, &cause))?);
                continue;
            };
            let predecessors = self.graph.incoming[i].iter().map(|&e| self.graph.ends[e].0);
            if let Some((dates, duration)) = reuse.early(i, predecessors, &changed) {
                early[i] = Some(dates);
                self.durations[i] = duration;
                changed[i] = false;
                continue;
            }
            let dates = self.early_at(i, &early, (anchor, &cause))?;
            changed[i] = reuse.early_changed(i, &dates, self.durations[i]);
            early[i] = Some(dates);
        }
        let early = early
            .into_iter()
            .collect::<Option<_>>()
            .ok_or(EngineError::InvalidInput("incomplete forward pass"))?;
        Ok((early, changed))
    }
    /// Activity `i`'s early dates, from its predecessors' in `early`.
    fn early_at(
        &mut self,
        i: usize,
        early: &[Option<Dates>],
        (anchor, cause): (DateTime, &DateProvenance),
    ) -> Result<Dates, EngineError> {
        let data_date = self.input.project.data_date;
        let activity = self.graph.activities[i];
        let completed = activity.status == ActivityStatus::Completed;
        let (anchor, cause) = match (activity.status, data_date) {
            (ActivityStatus::Completed, Some(date)) => (date, DateProvenance::DataDate),
            (ActivityStatus::InProgress, Some(date)) => {
                progress_impl::remaining_anchor(activity, date)
            }
            _ => (anchor, cause.clone()),
        };
        // Finished work stays on the data date as it is unless something
        // still holds it, which puts it through the pass like other work.
        let held = self.graph.incoming[i].iter().any(|&e| {
            let (a, _) = self.graph.ends[e];
            self.link_applies(e)
                && (self.graph.activities[a].status != ActivityStatus::Completed
                    || early[a]
                        .as_ref()
                        .is_some_and(|p: &Dates| Self::pushed(data_date, p)))
        });
        let mut best = if completed && !held {
            progress_impl::completed_dates(anchor)
        } else {
            self.forward(i, anchor, false, cause)?
        };
        let mut winner = None;
        let mut finish_floor = None;
        for &e in &self.graph.incoming[i] {
            if !self.link_applies(e) {
                continue;
            }
            let (a, _) = self.graph.ends[e];
            let edge = self.graph.edges[e];
            let (pred_finish, succ_finish) = ends(edge.kind);
            let pred: &Dates = early[a]
                .as_ref()
                .ok_or(EngineError::InvalidInput("invalid topological order"))?;
            let (bound, field, lag) = self.link_bound(a, i, edge, pred, pred_finish)?;
            // Finished work that hasn't been pushed holds nothing back. Being
            // moved to working time doesn't count as being pushed, but lag
            // still running does.
            let finished = self.graph.activities[a];
            let unpushed = if lag.get() > 0 {
                anchor
            } else {
                self.calendar_at(a)?.next_work_instant(anchor)?
            };
            if finished.status == ActivityStatus::Completed && bound <= unpushed {
                continue;
            }
            if succ_finish {
                finish_floor = Some(finish_floor.map_or(bound, |old: DateTime| old.max(bound)));
            }
            let candidate = self.forward(
                i,
                bound,
                succ_finish,
                relationship(edge, lag, true, field, self.options.lag_calendar),
            )?;
            if later(&candidate, &best)
                || (candidate.start == best.start
                    && candidate.finish == best.finish
                    && winner.is_none())
            {
                best = candidate;
                winner = Some(edge.id);
            }
        }
        // A span measures other work, so its constraints bind nothing.
        if !completed && !spans_impl::is_span(activity) {
            best = self.early_constraints(i, best)?;
        }
        if self.options.use_expected_finish
            && self.graph.activities[i].expected_finish.is_some()
            && matches!(best.finish_cause.driver(), DateProvenance::ExpectedFinish)
            && let Some(floor) = finish_floor.filter(|floor| best.finish < *floor)
        {
            let calendar = self.calendar_at(i)?;
            if calendar.work_between(best.finish, floor).get() > 0 {
                return Err(EngineError::InvalidInput(
                    "expected finish conflicts with finish relationship",
                ));
            }
            let moved = calendar.next_work_instant(floor)?;
            let calendar = self.calendar_ids[i];
            best.finish_cause = adjust(best.finish_cause, best.finish, moved, calendar);
            best.finish = moved;
        }
        Ok(best)
    }
    /// Whether a link can limit its predecessor in the backward pass. A link that
    /// measures from the predecessor's fixed actual start can't.
    fn limits_predecessor(&self, e: usize) -> bool {
        let (a, _) = self.graph.ends[e];
        let (pred_finish, _) = ends(self.graph.edges[e].kind);
        self.link_applies(e) && !self.measures_from_actual_start(a, pred_finish)
    }
    /// Span activities have no links in the network passes, and Progress Override
    /// drops a link that recorded progress has broken.
    fn link_applies(&self, e: usize) -> bool {
        let (a, b) = self.graph.ends[e];
        if spans_impl::is_span(self.graph.activities[a])
            || spans_impl::is_span(self.graph.activities[b])
        {
            return false;
        }
        !self.dropped_by_progress(e)
    }
    /// Progress Override drops a link that recorded progress has broken.
    pub(super) fn dropped_by_progress(&self, e: usize) -> bool {
        let (a, b) = self.graph.ends[e];
        self.options.out_of_sequence == OutOfSequenceMode::ProgressOverride
            && progress_impl::out_of_sequence(
                self.graph.edges[e].kind,
                self.graph.activities[a],
                self.graph.activities[b],
            )
    }
    /// The actual date of the side of a link that has already happened, if any.
    fn happened(&self, i: usize, pred_finish: bool) -> Option<DateTime> {
        let a = self.graph.activities[i];
        match (a.status, pred_finish) {
            (ActivityStatus::Completed, true) => a.actual_finish,
            (ActivityStatus::Completed | ActivityStatus::InProgress, false) => a.actual_start,
            _ => None,
        }
    }
    /// Whether a completed point has left the data date, by logic or by moving
    /// to working time.
    fn pushed(data_date: Option<DateTime>, pred: &Dates) -> bool {
        data_date.is_some_and(|date| pred.start > date)
    }
    /// Lag still to run on a link. Lag on the side that has happened counts down
    /// in work time from the actual date to the data date, and what's left runs
    /// from where the predecessor stands now. An actual after the data date adds
    /// no lag, and a lead on that side is used up: keeping it loses corpus matches
    /// in both passes (TF-022).
    fn remaining_lag(
        &self,
        i: usize,
        succ: usize,
        edge: &Relationship,
    ) -> Result<Minutes, EngineError> {
        let completed = self.graph.activities[i].status == ActivityStatus::Completed;
        // Finished work can't be held back by the history of other finished work.
        if completed && self.graph.activities[succ].status == ActivityStatus::Completed {
            return Ok(Minutes::ZERO);
        }
        self.lag_left_on(i, edge, || self.link_calendar(i, succ, edge))
    }
    /// [`Self::remaining_lag`] on a link from activity `i`, whatever its successor.
    pub(super) fn lag_left(&self, i: usize, edge: &Relationship) -> Result<Minutes, EngineError> {
        self.lag_left_on(i, edge, || self.lag_calendar(edge))
    }
    /// [`Self::lag_left`], with the lag calendar from `lag`, asked for only
    /// when the lag has started to run.
    fn lag_left_on<'c>(
        &'c self,
        i: usize,
        edge: &Relationship,
        lag: impl FnOnce() -> Result<&'c WorkCalendar, EngineError>,
    ) -> Result<Minutes, EngineError> {
        let (pred_finish, _) = ends(edge.kind);
        let completed = self.graph.activities[i].status == ActivityStatus::Completed;
        let Some(actual) = self.happened(i, pred_finish) else {
            if completed && edge.lag.get() > 0 {
                return Err(EngineError::InvalidInput(
                    "lag from a completed activity with no actual start",
                ));
            }
            return Ok(edge.lag);
        };
        let Some(data_date) = self.input.project.data_date else {
            return Ok(edge.lag);
        };
        let elapsed = lag()?.work_between(actual, data_date);
        Ok(Minutes(
            (edge.lag.get().max(0) - elapsed.get().max(0)).max(0),
        ))
    }
    /// Whether a start-side link measures from the predecessor's fixed actual
    /// start, with lag run out from there.
    fn measures_from_actual_start(&self, i: usize, pred_finish: bool) -> bool {
        self.graph.activities[i].status == ActivityStatus::InProgress
            && !pred_finish
            && !self.options.ss_lag_from_early_start
    }
    /// Where a relationship puts its successor.
    fn link_bound(
        &self,
        i: usize,
        succ: usize,
        edge: &Relationship,
        pred: &Dates,
        pred_finish: bool,
    ) -> Result<(DateTime, DateField, Minutes), EngineError> {
        let calendar = self.link_calendar(i, succ, edge)?;
        if self.measures_from_actual_start(i, pred_finish)
            && let Some(actual) = self.graph.activities[i].actual_start
        {
            let bound = calendar.add_work(actual, edge.lag)?;
            return Ok((bound, DateField::ActualStart, edge.lag));
        }
        let (position, field) = if pred_finish {
            (pred.finish, DateField::EarlyFinish)
        } else {
            (pred.start, DateField::EarlyStart)
        };
        let lag = self.remaining_lag(i, succ, edge)?;
        let bound = calendar.add_work(position, lag)?;
        Ok((bound, field, lag))
    }
    fn successor_calendar(&self, e: usize) -> Result<&WorkCalendar, EngineError> {
        self.calendar_at(self.graph.ends[e].1)
    }
    /// Whether a finish-side link reaches its successor from another calendar.
    /// Only then can the two ways of inverting a link disagree (TF-032).
    fn link_can_lose_time(&self, e: usize) -> Result<bool, EngineError> {
        let edge = self.graph.edges[e];
        if !ends(edge.kind).1 || !self.limits_predecessor(e) {
            return Ok(false);
        }
        let (a, b) = self.graph.ends[e];
        let successor = self.calendar_at(b)?;
        Ok(!std::ptr::eq(self.calendar_at(a)?, successor)
            || (edge.lag.get() != 0 && !std::ptr::eq(self.link_calendar(a, b, edge)?, successor)))
    }
    /// Late dates with every link inverted exactly, if that can change anything.
    fn exact_late_dates(
        &self,
        early: &[Dates],
        finish: DateTime,
        reuse: Option<(&reuse_impl::Reuse<'_>, &[bool])>,
    ) -> Result<Option<Pass>, EngineError> {
        for e in 0..self.graph.edges.len() {
            if self.link_can_lose_time(e)? {
                return self
                    .late_dates(early, finish, Inverse::Exact, reuse)
                    .map(Some);
            }
        }
        Ok(None)
    }
    /// Late dates and, for each activity, whether they differ from what
    /// `reuse` kept (always, with nothing kept). `reuse` comes with which
    /// activities' early dates differ from the kept ones.
    fn late_dates(
        &self,
        early: &[Dates],
        finish: DateTime,
        inverse: Inverse,
        reuse: Option<(&reuse_impl::Reuse<'_>, &[bool])>,
    ) -> Result<Pass, EngineError> {
        let count = self.graph.activities.len();
        let mut late = vec![None; count];
        let mut changed = vec![true; count];
        let exact = inverse == Inverse::Exact;
        for &i in self.graph.order.iter().rev() {
            let Some((reuse, early_changed)) = reuse else {
                late[i] = Some(self.late_at(i, (early, &late), finish, inverse)?);
                continue;
            };
            let successors = self.graph.outgoing[i].iter().map(|&e| self.graph.ends[e].1);
            if let Some(dates) = reuse.late(i, exact, successors, (early_changed, &changed)) {
                late[i] = Some(dates);
                changed[i] = false;
                continue;
            }
            let dates = self.late_at(i, (early, &late), finish, inverse)?;
            changed[i] = reuse.late_changed(i, exact, &dates);
            late[i] = Some(dates);
        }
        let late = late
            .into_iter()
            .collect::<Option<_>>()
            .ok_or(EngineError::InvalidInput("incomplete backward pass"))?;
        Ok((late, changed))
    }
    /// Activity `i`'s late dates, from its successors' in `late`.
    fn late_at(
        &self,
        i: usize,
        (early, late): (&[Dates], &[Option<Dates>]),
        finish: DateTime,
        inverse: Inverse,
    ) -> Result<Dates, EngineError> {
        let (bound, cause) = self
            .input
            .project
            .must_finish_by
            .map_or((finish, DateProvenance::ProjectFinish), |date| {
                (date, DateProvenance::MustFinishBy)
            });
        let mut best = if self.open_ended(i) {
            let end = self
                .input
                .project
                .must_finish_by
                .map_or(early[i].finish, |deadline| deadline.min(early[i].finish));
            self.to_project_end(i, end, DateProvenance::OpenEnd)?
        } else if self.input.project.must_finish_by.is_some() {
            self.backward(i, bound, true, cause)?
        } else {
            self.to_project_end(i, bound, DateProvenance::ProjectFinish)?
        };
        let mut winner = None;
        for &e in &self.graph.outgoing[i] {
            if !self.limits_predecessor(e) {
                continue;
            }
            let b = self.graph.ends[e].1;
            let successor = late[b]
                .as_ref()
                .ok_or(EngineError::InvalidInput("invalid reverse order"))?;
            let candidate = self.late_link(e, successor, false, inverse)?;
            if candidate.start < best.start || (candidate.start == best.start && winner.is_none()) {
                best = candidate;
                winner = Some(self.graph.edges[e].id);
            }
        }
        let best = self.late_external(i, best)?;
        self.late_constraints(i, best)
    }
    /// Total float between an activity's early and late dates.
    fn float_between(&self, i: usize, e: &Dates, l: &Dates) -> Result<Minutes, EngineError> {
        let c = self.calendar_at(i)?;
        let start_float = c.work_between(e.start, l.start);
        let finish_float = c.work_between(e.finish, l.finish);
        Ok(match self.options.float_type {
            FloatType::StartFloat => start_float,
            FloatType::FinishFloat => finish_float,
            FloatType::SmallestFloat => start_float.min(finish_float),
        })
    }
    fn results(
        &self,
        early: Vec<Dates>,
        late: Vec<Dates>,
        exact: Option<&[Dates]>,
        (finish, own_finish): (DateTime, DateTime),
        reuse: Option<(&reuse_impl::Reuse<'_>, &reuse_impl::Changed<'_>)>,
    ) -> Result<
        (
            BTreeMap<totalfloat_model::ActivityId, ActivityResult>,
            usize,
        ),
        EngineError,
    > {
        let count = self.graph.activities.len();
        // What a kept calculation can stand for.
        let taken = reuse.map(|(reuse, changed)| reuse.taken(self, changed));
        let paths_impl::Paths {
            mut driving,
            longest,
        } = self.paths(&early, own_finish, |i| {
            taken.as_ref().and_then(|taken| taken.driving(i))
        })?;
        let total_floats = (0..count)
            .map(
                |i| match taken.as_ref().and_then(|taken| taken.total_float(i)) {
                    Some(float) => Ok(float),
                    None => self.float_between(i, &early[i], &late[i]),
                },
            )
            .collect::<Result<Vec<_>, _>>()?;
        let mut path_float = None;
        for (i, a) in self.graph.activities.iter().enumerate() {
            if longest[i] && a.status != ActivityStatus::Completed {
                let float = total_floats[i];
                path_float = Some(path_float.map_or(float, |old: Minutes| old.min(float)));
            }
        }
        let float_paths = self.float_paths(&early)?;
        let whole: Vec<Option<&ActivityResult>> = (0..count)
            .map(|i| {
                let paths = (longest[i], float_paths[i]);
                taken.as_ref().and_then(|taken| taken.whole(i, paths))
            })
            .collect();
        let free_floats = (0..count)
            .map(|i| match whole[i] {
                Some(_) => Ok(None),
                None => self.free_float(i, &early),
            })
            .collect::<Result<Vec<_>, _>>()?;
        // Each activity's dates are its own from here, so their causes move
        // into its result rather than being copied.
        let mut output = Vec::with_capacity(count);
        for (i, (a, (e, l))) in self
            .graph
            .activities
            .iter()
            .zip(early.into_iter().zip(late))
            .enumerate()
        {
            if let Some(result) = whole[i] {
                output.push((a.id, result.clone()));
                continue;
            }
            let c = self.calendar_at(i)?;
            let total_float = total_floats[i];
            let free_float = free_floats[i];
            let completed = a.status == ActivityStatus::Completed;
            let remaining = |date: DateTime| (!completed).then_some(date);
            let on_path = self.on_path(i, total_float, path_float, longest[i]);
            let true_total_float = match exact {
                Some(exact) if !completed && !spans_impl::is_span(a) => {
                    Some(self.float_between(i, &e, &exact[i])?)
                        .filter(|&float| float != total_float)
                }
                _ => None,
            };
            let values = StoredResults {
                early_start: Some(e.start),
                early_finish: Some(e.finish),
                late_start: Some(l.start),
                late_finish: Some(l.finish),
                remaining_early_start: remaining(e.start),
                remaining_early_finish: remaining(e.finish),
                remaining_late_start: remaining(l.start),
                remaining_late_finish: remaining(l.finish),
                total_float: remaining(e.start).map(|_| total_float),
                free_float: remaining(e.start).map(|_| {
                    if a.activity_type == ActivityType::LevelOfEffort {
                        return Minutes::ZERO;
                    }
                    // Never below zero, even where a mandatory date has broken
                    // a link (docs/rules/TF-005-no-forecast-before-the-data-date.md).
                    free_float
                        .unwrap_or_else(|| {
                            let end = if self.open_ended(i) { l.finish } else { finish };
                            c.work_between(e.finish, end)
                        })
                        .max(Minutes::ZERO)
                }),
                on_longest_path: Some(on_path),
                float_path: float_paths[i].map(|(path, _)| path),
                float_path_order: float_paths[i].map(|(_, order)| order),
            };
            let provenance = provenance_of(a, e, l);
            let result = ActivityResult {
                actual_start: a.actual_start,
                actual_finish: a.actual_finish,
                values,
                critical: Some(self.critical(i, total_float, on_path)),
                driving_relationships: std::mem::take(&mut driving[i]),
                true_total_float,
                provenance,
            };
            output.push((a.id, result));
        }
        let worked_out = count - whole.iter().flatten().count();
        // In ID order already, so the map is built without searching.
        Ok((output.into_iter().collect(), worked_out))
    }
    /// The least slack before an activity's links out of it would move their
    /// successors' early dates, or `None` with no such link.
    fn free_float(&self, i: usize, early: &[Dates]) -> Result<Option<Minutes>, EngineError> {
        let c = self.calendar_at(i)?;
        let mut free_float = None;
        for &edge in &self.graph.outgoing[i] {
            if !self.limits_predecessor(edge) {
                continue;
            }
            let successor = &early[self.graph.ends[edge].1];
            let (bound, _) = self.late_link_bound(edge, successor, Inverse::AsSpelled)?;
            let pred_finish = ends(self.graph.edges[edge].kind).0;
            let (start, _) = self.backward_dates(i, bound, pred_finish)?;
            let slack = c.work_between(early[i].start, start);
            free_float = Some(free_float.map_or(slack, |old: Minutes| old.min(slack)));
        }
        if let Some(slack) = self.external_free_float(i, &early[i])? {
            free_float = Some(free_float.map_or(slack, |old: Minutes| old.min(slack)));
        }
        Ok(free_float)
    }
}
/// What set each of an activity's dates, moved out of its early and late
/// dates.
fn provenance_of(a: &Activity, e: Dates, l: Dates) -> BTreeMap<DateField, DateProvenance> {
    let completed = a.status == ActivityStatus::Completed;
    // Inserted in key order, so each one goes on the end.
    let remaining_causes = (!completed).then(|| {
        [
            (DateField::RemainingEarlyStart, e.start_cause.clone()),
            (DateField::RemainingEarlyFinish, e.finish_cause.clone()),
            (DateField::RemainingLateStart, l.start_cause.clone()),
            (DateField::RemainingLateFinish, l.finish_cause.clone()),
        ]
    });
    let mut provenance = BTreeMap::new();
    for (field, why) in [
        (DateField::EarlyStart, e.start_cause),
        (DateField::EarlyFinish, e.finish_cause),
        (DateField::LateStart, l.start_cause),
        (DateField::LateFinish, l.finish_cause),
    ]
    .into_iter()
    .chain(remaining_causes.into_iter().flatten())
    {
        provenance.insert(field, why);
    }
    if a.actual_start.is_some() {
        provenance.insert(DateField::ActualStart, DateProvenance::Actual);
    }
    provenance
}
pub(crate) fn broken_relationships(input: &Schedule, projects: &[Schedule]) -> Vec<RelationshipId> {
    let find = |project: totalfloat_model::ProjectId, id: totalfloat_model::ActivityId| {
        if project == input.project.id {
            return input.activities.get(&id);
        }
        projects
            .iter()
            .find(|p| p.project.id == project)
            .and_then(|p| p.activities.get(&id))
    };
    input
        .relationships
        .values()
        .filter(|edge| {
            let pred = find(edge.predecessor_project, edge.predecessor);
            let succ = find(edge.successor_project, edge.successor);
            pred.zip(succ)
                .is_some_and(|(a, b)| progress_impl::out_of_sequence(edge.kind, a, b))
        })
        .map(|edge| edge.id)
        .collect()
}
/// The latest early finish stored in the projects loaded with `input`, leaving
/// span activities out as the project's own finish does.
pub(crate) fn stored_finish(input: &Schedule, projects: &[Schedule]) -> Option<DateTime> {
    projects
        .iter()
        .filter(|p| p.project.id != input.project.id)
        .filter_map(|p| {
            let finishes = |spans: bool| {
                p.activities
                    .values()
                    .filter(move |a| spans || !spans_impl::is_span(a))
                    .filter_map(|a| a.stored.early_finish)
                    .max()
            };
            finishes(false).or_else(|| finishes(true))
        })
        .max()
}
pub(crate) fn calculate(
    input: &Schedule,
    options: &ScheduleOptions,
    projects: &[Schedule],
    opened_finish: Option<DateTime>,
) -> Result<ScheduleResult, EngineError> {
    calculate_reusing(input, options, (projects, opened_finish), None, false)
        .map(|(result, _)| result)
}

/// [`calculate`], taking from `previous` what it can, and keeping what
/// it read and worked out when asked to `keep` it and nothing outside the
/// project fed it.
pub(crate) fn calculate_reusing(
    input: &Schedule,
    options: &ScheduleOptions,
    (projects, opened_finish): (&[Schedule], Option<DateTime>),
    previous: Option<(&Kept, &ScheduleResult)>,
    keep: bool,
) -> Result<(ScheduleResult, Option<Kept>), EngineError> {
    let mut ctx = Context::new(input, options, projects)?;
    let mut output = ScheduleResult {
        project: ProjectResult {
            id: input.project.id,
            data_date: input.project.data_date,
            scheduled_finish: None,
            network_finish: None,
        },
        activities: BTreeMap::new(),
    };
    if ctx.graph.activities.is_empty() {
        return Ok((output, None));
    }
    let mut reuse = previous.and_then(|(kept, result)| ctx.reuse(kept, result, opened_finish));
    let (mut early, early_changed) = ctx.early_dates(reuse.as_ref())?;
    let finish = early
        .iter()
        .zip(&ctx.graph.activities)
        .filter(|(_, a)| !spans_impl::is_span(a))
        .map(|(d, _)| d.finish)
        .max()
        .or_else(|| early.iter().map(|d| d.finish).max())
        .ok_or(EngineError::InvalidInput("empty forward pass"))?;
    // Float from the opened projects' finish: late dates count back from the
    // latest finish of the projects open together (TF-031). The longest path
    // still runs back from this project's own finish.
    let own_finish = finish;
    let finish = if options.float_from_project_finish {
        finish
    } else {
        opened_finish.map_or(finish, |other| other.max(finish))
    };
    if let Some(reuse) = &mut reuse {
        reuse.finish_is(finish);
    }
    ctx.alap_dates(&mut early, finish)?;
    let passes = reuse
        .as_ref()
        .map(|reuse| (reuse, early_changed.as_slice()));
    let (mut late, late_changed) = ctx.late_dates(&early, finish, Inverse::AsSpelled, passes)?;
    let (exact, exact_changed) = ctx.exact_late_dates(&early, finish, passes)?.unzip();
    ctx.measure_spans(&mut early, &mut late, finish)?;
    output.project.scheduled_finish = Some(finish);
    output.project.network_finish = Some(own_finish);
    let kept = (keep && ctx.keepable()).then(|| exact.clone());
    let changed = reuse_impl::Changed {
        early: &early_changed,
        late: &late_changed,
        exact: exact_changed.as_deref(),
    };
    let (activities, worked_out) = ctx.results(
        early,
        late,
        exact.as_deref(),
        (finish, own_finish),
        reuse.as_ref().map(|reuse| (reuse, &changed)),
    )?;
    output.activities = activities;
    let kept = kept.map(|exact| ctx.kept(opened_finish, exact, finish, worked_out));
    Ok((output, kept))
}

#[path = "constraints.rs"]
mod constraints_impl;
#[path = "external.rs"]
mod external_impl;
#[path = "paths.rs"]
mod paths_impl;
#[path = "progress.rs"]
mod progress_impl;
#[path = "resource.rs"]
mod resource_impl;
#[path = "reuse.rs"]
mod reuse_impl;
pub(crate) use reuse_impl::Kept;
#[path = "spans.rs"]
mod spans_impl;
