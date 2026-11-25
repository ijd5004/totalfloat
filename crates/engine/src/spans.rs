//! Span activities: level of effort and WBS summary (TF-027).
//!
//! These measure other work instead of doing their own, so they have no links in
//! the network passes and can't drive anything. They're measured afterwards.
use super::{
    Activity, ActivityStatus, ActivityType, Context, DateProvenance, DateTime, Dates, EngineError,
    ends, negate, progress_impl,
};
use std::collections::{BTreeMap, BTreeSet};
use totalfloat_model::{Relationship, RelationshipType, WbsId};

pub(super) fn is_span(a: &Activity) -> bool {
    matches!(
        a.activity_type,
        ActivityType::LevelOfEffort | ActivityType::WbsSummary
    )
}

/// What WBS summaries read about the tree, built once if a schedule has any.
#[derive(Default)]
pub(super) struct WbsIndex {
    children: BTreeMap<WbsId, Vec<WbsId>>,
    activities: BTreeMap<WbsId, Vec<usize>>,
}

#[derive(Clone, Copy)]
enum Pass {
    /// No earlier than `floor`.
    Forward { floor: Option<DateTime> },
    /// No later than `end` unless a neighbor says so.
    Backward { end: DateTime },
}

fn later(old: Option<DateTime>, new: DateTime) -> DateTime {
    old.map_or(new, |old| old.max(new))
}

impl Context<'_> {
    /// Where the neighbors' dates put a level-of-effort activity: its start from
    /// links into its start, and its finish from links into or out of its finish.
    fn neighbors(
        &self,
        i: usize,
        dates: &[Option<Dates>],
    ) -> Result<(Option<DateTime>, Option<DateTime>), EngineError> {
        let (mut start, mut finish) = (None, None);
        for &e in &self.graph.incoming[i] {
            if self.dropped_by_progress(e) {
                continue;
            }
            let (a, _) = self.graph.ends[e];
            let edge = self.graph.edges[e];
            let Some(pred) = &dates[a] else { continue };
            let (pred_finish, succ_finish) = ends(edge.kind);
            let (bound, ..) = self.link_bound(a, i, edge, pred, pred_finish)?;
            if succ_finish {
                finish = Some(later(finish, bound));
            } else {
                // A span starts with the earliest of the work it measures.
                start = Some(start.map_or(bound, |old: DateTime| old.min(bound)));
            }
        }
        for &e in &self.graph.outgoing[i] {
            if self.dropped_by_progress(e) {
                continue;
            }
            let (_, b) = self.graph.ends[e];
            let edge = self.graph.edges[e];
            let Some(succ) = &dates[b] else { continue };
            if let Some(bound) = self.span_end(edge, succ)? {
                finish = Some(later(finish, bound));
            }
        }
        Ok((start, finish))
    }

    /// The finish a link out of a level-of-effort activity gives it.
    fn span_end(&self, edge: &Relationship, succ: &Dates) -> Result<Option<DateTime>, EngineError> {
        let source = match edge.kind {
            RelationshipType::FinishToStart => succ.start,
            RelationshipType::FinishToFinish => succ.finish,
            RelationshipType::StartToStart | RelationshipType::StartToFinish => return Ok(None),
        };
        Ok(Some(
            self.lag_calendar(edge)?
                .add_work(source, negate(edge.lag)?)?,
        ))
    }

    /// Each pass reaches one end of the span first and holds it: the forward
    /// pass its start and the backward pass its finish. The other end comes from
    /// neighbors, or from the duration if it has none, and yields if they cross.
    fn level_of_effort(
        &self,
        i: usize,
        dates: &[Option<Dates>],
        pass: Pass,
    ) -> Result<Dates, EngineError> {
        let c = self.calendar(self.works_on(self.graph.activities[i]))?;
        let (start, finish) = self.neighbors(i, dates)?;
        let (start, finish) = match pass {
            Pass::Forward { floor } => {
                let start = start
                    .into_iter()
                    .chain(floor)
                    .max()
                    .ok_or(EngineError::InvalidInput("level of effort with no span"))?;
                let start = c.next_work_instant(start)?;
                let finish = match finish {
                    Some(finish) => c.prev_work_instant(finish)?.max(start),
                    None => c.add_work(start, self.durations[i])?,
                };
                (start, finish)
            }
            Pass::Backward { end } => {
                let finish = c.prev_work_instant(finish.unwrap_or(end))?;
                let start = match start {
                    Some(start) => c.next_work_instant(start)?.min(finish),
                    None => c.add_work(finish, negate(self.durations[i])?)?,
                };
                (start, finish)
            }
        };
        Ok(span_dates(start, finish, DateProvenance::LoeSpan))
    }

    fn wbs_index(&self) -> &WbsIndex {
        self.wbs_index.get_or_init(|| {
            let mut index = WbsIndex::default();
            for wbs in self.input.wbs.values() {
                if let Some(parent) = wbs.parent {
                    index.children.entry(parent).or_default().push(wbs.id);
                }
            }
            for (i, a) in self.graph.activities.iter().enumerate() {
                index.activities.entry(a.wbs).or_default().push(i);
            }
            index
        })
    }

    fn subtree(&self, root: WbsId) -> BTreeSet<WbsId> {
        let children = &self.wbs_index().children;
        let mut ids = BTreeSet::from([root]);
        let mut pending = vec![root];
        while let Some(id) = pending.pop() {
            for &child in children.get(&id).into_iter().flatten() {
                if ids.insert(child) {
                    pending.push(child);
                }
            }
        }
        ids
    }

    /// The work a WBS summary spans: what's unfinished in its subtree, or all of
    /// it once everything has finished.
    fn members(&self, i: usize) -> Vec<usize> {
        let activities = &self.wbs_index().activities;
        let tree = self.subtree(self.graph.activities[i].wbs);
        let mut all: Vec<_> = tree
            .iter()
            .filter_map(|wbs| activities.get(wbs))
            .flatten()
            .copied()
            .filter(|&j| {
                j != i && self.graph.activities[j].activity_type != ActivityType::WbsSummary
            })
            .collect();
        all.sort_unstable();
        let unfinished: Vec<_> = all
            .iter()
            .copied()
            .filter(|&j| self.graph.activities[j].status != ActivityStatus::Completed)
            .collect();
        if unfinished.is_empty() {
            all
        } else {
            unfinished
        }
    }

    fn summary(&self, i: usize, dates: &[Option<Dates>]) -> Option<Dates> {
        let spans = self
            .members(i)
            .into_iter()
            .filter_map(|j| dates[j].as_ref());
        let (start, finish) = spans.fold(None, |range, d: &Dates| match range {
            None => Some((d.start, d.finish)),
            Some((start, finish)) => Some((d.start.min(start), d.finish.max(finish))),
        })?;
        Some(span_dates(start, finish, DateProvenance::WbsSummarySpan))
    }

    /// Measures every span activity from the finished network, level of effort
    /// first, because a WBS summary can span it. A summary with nothing to span
    /// keeps its own place in the passes.
    pub(super) fn measure_spans(
        &self,
        early: &mut [Dates],
        late: &mut [Dates],
        end: DateTime,
    ) -> Result<(), EngineError> {
        if !self.graph.activities.iter().any(|a| is_span(a)) {
            return Ok(());
        }
        let data_date = self.input.project.data_date;
        let network = |dates: &[Dates]| -> Vec<Option<Dates>> {
            self.graph
                .activities
                .iter()
                .zip(dates)
                .map(|(a, d)| (!is_span(a)).then(|| d.clone()))
                .collect()
        };
        let (mut known_early, mut known_late) = (network(early), network(late));
        for (i, a) in self.graph.activities.iter().enumerate() {
            if a.activity_type != ActivityType::LevelOfEffort {
                continue;
            }
            let deadline = self.input.project.must_finish_by.unwrap_or(end);
            let mut l = self.level_of_effort(i, &known_late, Pass::Backward { end: deadline })?;
            let e = if a.status == ActivityStatus::Completed {
                let point =
                    data_date.ok_or(EngineError::InvalidInput("completed without data date"))?;
                // Finished work is one point, as it is in the network passes.
                l.start = l.finish;
                l.start_cause = l.finish_cause.clone();
                progress_impl::completed_dates(point)
            } else {
                let floor = match (a.status, data_date) {
                    (ActivityStatus::InProgress, Some(date)) => {
                        Some(progress_impl::remaining_anchor(a, date).0)
                    }
                    _ => Some(self.project_anchor()?.0),
                };
                self.level_of_effort(i, &known_early, Pass::Forward { floor })?
            };
            early[i] = e.clone();
            late[i] = l.clone();
            known_early[i] = Some(e);
            known_late[i] = Some(l);
        }
        for (i, a) in self.graph.activities.iter().enumerate() {
            if a.activity_type != ActivityType::WbsSummary {
                continue;
            }
            if let Some(e) = self.summary(i, &known_early) {
                early[i] = e;
            }
            if let Some(l) = self.summary(i, &known_late) {
                late[i] = l;
            }
        }
        Ok(())
    }
}

fn span_dates(start: DateTime, finish: DateTime, cause: DateProvenance) -> Dates {
    Dates {
        start,
        finish,
        start_cause: cause.clone(),
        finish_cause: cause,
    }
}
