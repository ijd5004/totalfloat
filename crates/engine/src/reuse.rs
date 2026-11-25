//! Recalculating only what an edit reaches.
//!
//! A calculation keeps what it read and what it worked out ([`Kept`]). The
//! next calculation of the same project compares its input with that, marks
//! the activities whose own inputs or links changed, and recalculates an
//! activity in each pass only where something it reads changed: in the
//! forward pass its predecessors' early dates, in the backward pass its
//! successors' late dates. Everything else is taken from the kept
//! calculation, which worked it out from the same inputs, so the result is
//! the one a full calculation gives.
//!
//! Anything the engine reads for the whole project (the options, the
//! project's dates and calendars, another project's finish or dates) must be
//! unchanged, or nothing is reused. Activities whose dates are moved after
//! the passes (as late as possible, spans) are always recalculated.
use super::{Context, Dates, spans_impl::is_span};
use crate::{ActivityResult, DateField};
use std::collections::BTreeMap;
use totalfloat_model::{
    Activity, ActivityId, ActivityStatus, ActivityType, Calendar, CalendarId, Constraint,
    ConstraintType, DateTime, Minutes, ProjectId, RelationshipId, RelationshipType, Schedule,
    ScheduleOptions, WbsId,
};

/// Everything about one activity that the passes read from it.
#[derive(Clone, PartialEq, Eq)]
struct Inputs {
    project: ProjectId,
    wbs: WbsId,
    /// The calendar its dates run on: its own, or its resources'.
    works_on: CalendarId,
    activity_type: ActivityType,
    status: ActivityStatus,
    remaining_duration: Minutes,
    actual_start: Option<DateTime>,
    actual_finish: Option<DateTime>,
    suspend: Option<DateTime>,
    resume: Option<DateTime>,
    expected_finish: Option<DateTime>,
    primary_constraint: Option<Constraint>,
    secondary_constraint: Option<Constraint>,
    external_early_start: Option<DateTime>,
    external_late_finish: Option<DateTime>,
}

impl Inputs {
    fn of(a: &Activity, works_on: CalendarId) -> Self {
        Self {
            project: a.project,
            wbs: a.wbs,
            works_on,
            activity_type: a.activity_type,
            status: a.status,
            remaining_duration: a.remaining_duration,
            actual_start: a.actual_start,
            actual_finish: a.actual_finish,
            suspend: a.suspend,
            resume: a.resume,
            expected_finish: a.expected_finish,
            primary_constraint: a.primary_constraint,
            secondary_constraint: a.secondary_constraint,
            external_early_start: a.external_early_start,
            external_late_finish: a.external_late_finish,
        }
    }
}

/// A link as the passes read it.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Link {
    id: RelationshipId,
    predecessor: ActivityId,
    successor: ActivityId,
    kind: RelationshipType,
    lag: Minutes,
}

/// What the engine reads for the whole project.
#[derive(Clone, PartialEq, Eq)]
struct Whole {
    options: ScheduleOptions,
    opened_finish: Option<DateTime>,
    project: ProjectId,
    planned_start: Option<DateTime>,
    data_date: Option<DateTime>,
    must_finish_by: Option<DateTime>,
    default_calendar: Option<CalendarId>,
}

impl Whole {
    fn of(input: &Schedule, options: &ScheduleOptions, opened_finish: Option<DateTime>) -> Self {
        let project = &input.project;
        Self {
            options: options.clone(),
            opened_finish,
            project: project.id,
            planned_start: project.planned_start,
            data_date: project.data_date,
            must_finish_by: project.must_finish_by,
            default_calendar: project.default_calendar,
        }
    }
}

/// What one calculation read and worked out, in activity ID order.
#[derive(Clone)]
pub(crate) struct Kept {
    whole: Whole,
    calendars: BTreeMap<CalendarId, Calendar>,
    ids: Vec<ActivityId>,
    inputs: Vec<Inputs>,
    links: Vec<Link>,
    /// Remaining durations once the forward pass has applied expected
    /// finishes. The early and late dates are the result's.
    durations: Vec<Minutes>,
    /// Late dates with every link inverted exactly, where worked out, which
    /// the result doesn't hold.
    exact: Option<Vec<Dates>>,
    finish: DateTime,
    /// Activities whose results this calculation worked out rather than took.
    pub worked_out: usize,
}

/// What can be taken from a kept calculation, by activity in this one.
pub(super) struct Reuse<'k> {
    kept: &'k Kept,
    /// Each activity's place in the kept calculation, if it was there.
    old: Vec<Option<usize>>,
    /// Activities whose own inputs or links changed, or that are new.
    dirty: Vec<bool>,
    /// Activities moved after the passes, whose kept dates aren't the passes'.
    moved_later: Vec<bool>,
    /// Kept results, by place in the kept calculation.
    results: Vec<&'k ActivityResult>,
    /// Whether the project finish is the kept one.
    pub same_finish: bool,
}

fn links_of(ctx: &Context<'_>) -> Vec<Link> {
    ctx.graph
        .edges
        .iter()
        .map(|edge| Link {
            id: edge.id,
            predecessor: edge.predecessor,
            successor: edge.successor,
            kind: edge.kind,
            lag: edge.lag,
        })
        .collect()
}

fn inputs_of(ctx: &Context<'_>) -> Vec<Inputs> {
    ctx.graph
        .activities
        .iter()
        .zip(&ctx.calendar_ids)
        .map(|(a, works_on)| Inputs::of(a, *works_on))
        .collect()
}

impl Context<'_> {
    /// Whether nothing outside this project feeds the calculation, so a later
    /// one can be measured against it.
    pub(super) fn keepable(&self) -> bool {
        self.graph.external.is_empty() && self.others.is_empty()
    }

    /// Whether the activity's early dates move after the forward pass.
    pub(super) fn moved_later(&self, i: usize) -> bool {
        let a = self.graph.activities[i];
        is_span(a)
            || [a.primary_constraint, a.secondary_constraint]
                .into_iter()
                .flatten()
                .any(|c| c.kind == ConstraintType::AsLateAsPossible)
    }

    /// What to keep of this calculation for the next one.
    pub(super) fn kept(
        &self,
        opened_finish: Option<DateTime>,
        exact: Option<Vec<Dates>>,
        finish: DateTime,
        worked_out: usize,
    ) -> Kept {
        Kept {
            whole: Whole::of(self.input, self.options, opened_finish),
            calendars: self.input.calendars.clone(),
            ids: self.graph.activities.iter().map(|a| a.id).collect(),
            inputs: inputs_of(self),
            links: links_of(self),
            durations: self.durations.clone(),
            exact,
            finish,
            worked_out,
        }
    }

    /// What this calculation can take from `kept`, whose result is
    /// `result`, or `None` if something read for the whole project changed.
    pub(super) fn reuse<'k>(
        &self,
        kept: &'k Kept,
        result: &'k crate::ScheduleResult,
        opened_finish: Option<DateTime>,
    ) -> Option<Reuse<'k>> {
        if !self.keepable()
            || kept.whole != Whole::of(self.input, self.options, opened_finish)
            || kept.calendars != self.input.calendars
            || result.activities.len() != kept.ids.len()
        {
            return None;
        }
        let ids: Vec<ActivityId> = self.graph.activities.iter().map(|a| a.id).collect();
        let place = |id: &ActivityId| ids.binary_search(id).ok();
        let old: Vec<Option<usize>> = ids
            .iter()
            .map(|id| kept.ids.binary_search(id).ok())
            .collect();
        let inputs = inputs_of(self);
        let mut dirty: Vec<bool> = old
            .iter()
            .zip(&inputs)
            .map(|(old, now)| old.is_none_or(|j| kept.inputs[j] != *now))
            .collect();
        // Both lists are in ID order, as the graph takes them from the map.
        let links = links_of(self);
        let (mut a, mut b) = (0, 0);
        let mark = |link: &Link, dirty: &mut Vec<bool>| {
            for end in [link.predecessor, link.successor] {
                if let Some(i) = place(&end) {
                    dirty[i] = true;
                }
            }
        };
        while a < kept.links.len() || b < links.len() {
            match (kept.links.get(a), links.get(b)) {
                (Some(was), Some(now)) if was.id == now.id => {
                    if was != now {
                        mark(was, &mut dirty);
                        mark(now, &mut dirty);
                    }
                    a += 1;
                    b += 1;
                }
                (Some(was), Some(now)) if was.id < now.id => {
                    mark(was, &mut dirty);
                    a += 1;
                }
                (Some(was), None) => {
                    mark(was, &mut dirty);
                    a += 1;
                }
                (_, Some(now)) => {
                    mark(now, &mut dirty);
                    b += 1;
                }
                (None, None) => break,
            }
        }
        let moved_later = (0..ids.len()).map(|i| self.moved_later(i)).collect();
        Some(Reuse {
            kept,
            old,
            dirty,
            moved_later,
            results: result.activities.values().collect(),
            same_finish: false,
        })
    }
}

/// The early or late dates a result shows, with their causes.
const EARLY: (DateField, DateField) = (DateField::EarlyStart, DateField::EarlyFinish);
const LATE: (DateField, DateField) = (DateField::LateStart, DateField::LateFinish);

/// `fields` of `result` as the passes hold them. The results show each
/// activity's dates as the passes left them, with what set each one.
fn dates_of(result: &ActivityResult, (start, finish): (DateField, DateField)) -> Option<Dates> {
    Some(Dates {
        start: result.date(start)?,
        finish: result.date(finish)?,
        start_cause: result.provenance.get(&start)?.clone(),
        finish_cause: result.provenance.get(&finish)?.clone(),
    })
}

/// Whether `dates` are `fields` of `result`.
fn shows(result: &ActivityResult, (start, finish): (DateField, DateField), dates: &Dates) -> bool {
    result.date(start) == Some(dates.start)
        && result.date(finish) == Some(dates.finish)
        && result.provenance.get(&start) == Some(&dates.start_cause)
        && result.provenance.get(&finish) == Some(&dates.finish_cause)
}

impl<'k> Reuse<'k> {
    /// Activity `i`'s early dates and remaining duration from the kept
    /// calculation, if its own inputs and links are unchanged, its dates
    /// aren't moved after the pass, and none of `predecessors` changed.
    pub fn early(
        &self,
        i: usize,
        mut predecessors: impl Iterator<Item = usize>,
        changed: &[bool],
    ) -> Option<(Dates, Minutes)> {
        let j = self.old[i]?;
        if self.dirty[i] || self.moved_later[i] || predecessors.any(|a| changed[a] || self.dirty[a])
        {
            return None;
        }
        Some((dates_of(self.results[j], EARLY)?, self.kept.durations[j]))
    }

    /// Whether activity `i`'s recalculated early dates or remaining duration
    /// differ from the kept ones, or are moved after the pass.
    pub fn early_changed(&self, i: usize, dates: &Dates, duration: Minutes) -> bool {
        self.moved_later[i]
            || self.old[i].is_none_or(|j| {
                !shows(self.results[j], EARLY, dates) || self.kept.durations[j] != duration
            })
    }

    /// The kept late dates (exact ones if `exact`) of activity `i`, if the
    /// project finish is the same, its own inputs, links and early dates are
    /// unchanged, and none of `successors` changed.
    pub fn late(
        &self,
        i: usize,
        exact: bool,
        mut successors: impl Iterator<Item = usize>,
        (early_changed, changed): (&[bool], &[bool]),
    ) -> Option<Dates> {
        let j = self.old[i]?;
        if !self.same_finish
            || self.dirty[i]
            || early_changed[i]
            || successors.any(|b| changed[b] || self.dirty[b])
        {
            return None;
        }
        if exact {
            Some(self.kept.exact.as_ref()?[j].clone())
        } else {
            dates_of(self.results[j], LATE)
        }
    }

    /// Whether activity `i`'s recalculated late dates differ from the kept
    /// ones. Spans' kept dates are measured after the pass, so they always do.
    pub fn late_changed(&self, i: usize, exact: bool, dates: &Dates) -> bool {
        self.moved_later[i]
            || self.old[i].is_none_or(|j| {
                if exact {
                    self.kept
                        .exact
                        .as_ref()
                        .is_none_or(|kept| kept[j] != *dates)
                } else {
                    !shows(self.results[j], LATE, dates)
                }
            })
    }

    /// Records whether the project finish is the kept one.
    pub fn finish_is(&mut self, finish: DateTime) {
        self.same_finish = self.kept.finish == finish;
    }

    /// Activity `i`'s kept result.
    pub fn result(&self, i: usize) -> Option<&'k ActivityResult> {
        self.old[i].map(|j| self.results[j])
    }
}

/// Which activities' dates differ from the kept calculation's, by pass.
pub(super) struct Changed<'a> {
    pub early: &'a [bool],
    pub late: &'a [bool],
    /// With exact late dates worked out.
    pub exact: Option<&'a [bool]>,
}

/// What each activity's result can take from the kept calculation.
pub(super) struct Taken<'r, 'k> {
    reuse: &'r Reuse<'k>,
    /// Its driving links, which read its early dates and its predecessors'.
    driving: Vec<bool>,
    /// Its total float, which reads its early and late dates.
    total_float: Vec<bool>,
    /// The rest of it: free float reads its successors' early dates, true
    /// total float its exact late dates, and free float with no link out the
    /// project finish.
    result: Vec<bool>,
}

impl<'k> Reuse<'k> {
    /// What each activity's result can take from the kept calculation, once
    /// the passes have found which dates changed.
    pub fn taken<'r>(&'r self, ctx: &Context<'_>, changed: &Changed<'_>) -> Taken<'r, 'k> {
        let count = self.old.len();
        // Activities whose own inputs, links or early dates changed.
        let near: Vec<bool> = (0..count)
            .map(|i| self.old[i].is_none() || self.dirty[i] || changed.early[i])
            .collect();
        let graph = &ctx.graph;
        let driving: Vec<bool> = (0..count)
            .map(|i| near[i] || graph.incoming[i].iter().any(|&e| near[graph.ends[e].0]))
            .collect();
        let total_float = (0..count).map(|i| near[i] || changed.late[i]).collect();
        let exact_kept = self.kept.exact.is_some();
        let result = (0..count)
            .map(|i| {
                !self.same_finish
                    || driving[i]
                    || changed.late[i]
                    || changed.exact.map_or(exact_kept, |exact| exact[i])
                    || graph.outgoing[i].iter().any(|&e| near[graph.ends[e].1])
            })
            .collect();
        Taken {
            reuse: self,
            driving,
            total_float,
            result,
        }
    }
}

impl<'k> Taken<'_, 'k> {
    /// Activity `i`'s kept driving links, if nothing they read changed.
    pub fn driving(&self, i: usize) -> Option<&'k [RelationshipId]> {
        let result = self.reuse.result(i).filter(|_| !self.driving[i])?;
        Some(result.driving_relationships.as_slice())
    }

    /// Activity `i`'s kept total float, if nothing it reads changed and it
    /// has one.
    pub fn total_float(&self, i: usize) -> Option<Minutes> {
        let result = self.reuse.result(i).filter(|_| !self.total_float[i])?;
        result.values.total_float
    }

    /// Activity `i`'s kept result, whole, if nothing it reads changed and it
    /// is on the same paths: the longest path and its float path.
    pub fn whole(
        &self,
        i: usize,
        (longest, float_path): (bool, Option<(u32, u32)>),
    ) -> Option<&'k ActivityResult> {
        let result = self.reuse.result(i).filter(|_| !self.result[i])?;
        let values = &result.values;
        let same = values.on_longest_path == Some(longest)
            && values.float_path == float_path.map(|(path, _)| path)
            && values.float_path_order == float_path.map(|(_, order)| order);
        same.then_some(result)
    }
}
