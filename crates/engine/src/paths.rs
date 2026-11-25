//! Driving relationships, the longest path and critical activities (TF-026).
use super::{ActivityStatus, Context, DateTime, Dates, EngineError, ends, spans_impl::is_span};
use std::cmp::Reverse;
use std::collections::BTreeSet;
use totalfloat_model::{CriticalDefinition, FloatPathMode, Minutes, RelationshipId};

/// Which links drive each activity, and who is on the longest path.
pub(super) struct Paths {
    pub driving: Vec<Vec<RelationshipId>>,
    pub longest: Vec<bool>,
}

impl Context<'_> {
    /// How far a link's predecessor could slip before moving its successor. Slack
    /// is the predecessor's to use, so it's counted on its calendar, whichever
    /// calendar the lag runs on. It's negative where a mandatory date has broken
    /// the link: the successor is earlier than the link allows.
    fn link_slack(&self, e: usize, early: &[Dates]) -> Result<Minutes, EngineError> {
        let (a, b) = self.graph.ends[e];
        let edge = self.graph.edges[e];
        let (pred_finish, succ_finish) = ends(edge.kind);
        let (bound, ..) = self.link_bound(a, b, edge, &early[a], pred_finish)?;
        let reached = if succ_finish {
            early[b].finish
        } else {
            early[b].start
        };
        Ok(self.calendar_at(a)?.work_between(bound, reached))
    }

    /// A link drives when it has no slack: its bound reaches the same work
    /// position as the date it controls. A broken link drives nothing, because
    /// what set the date is the mandatory date.
    fn drives(&self, e: usize, early: &[Dates]) -> Result<bool, EngineError> {
        Ok(self.link_applies(e) && self.link_slack(e, early)? == Minutes::ZERO)
    }

    /// The longest path starts from whatever reaches the project finish and runs
    /// back through every driving link, so ties all count.
    /// `kept` gives an activity's driving links where a kept calculation's
    /// stand for them.
    pub(super) fn paths<'k>(
        &self,
        early: &[Dates],
        finish: DateTime,
        kept: impl Fn(usize) -> Option<&'k [RelationshipId]>,
    ) -> Result<Paths, EngineError> {
        let count = self.graph.activities.len();
        let mut driving: Vec<Vec<usize>> = vec![Vec::new(); count];
        for (i, links) in driving.iter_mut().enumerate() {
            if let Some(ids) = kept(i) {
                // The graph holds its links in ID order.
                let edges = &self.graph.edges;
                links.extend(
                    ids.iter()
                        .filter_map(|id| edges.binary_search_by_key(id, |edge| edge.id).ok()),
                );
                continue;
            }
            for &e in &self.graph.incoming[i] {
                if self.drives(e, early)? {
                    links.push(e);
                }
            }
        }
        let mut longest = vec![false; count];
        let mut pending = Vec::new();
        for i in 0..count {
            let activity = self.graph.activities[i];
            if early[i].finish == finish && !is_span(activity) && self.can_end_a_path(i, early)? {
                longest[i] = true;
                pending.push(i);
            }
        }
        while let Some(i) = pending.pop() {
            for &e in &driving[i] {
                let (a, _) = self.graph.ends[e];
                if !longest[a] {
                    longest[a] = true;
                    pending.push(a);
                }
            }
        }
        let driving = driving
            .into_iter()
            .map(|links| {
                let mut ids: Vec<_> = links.iter().map(|&e| self.graph.edges[e].id).collect();
                ids.sort_unstable();
                ids
            })
            .collect();
        Ok(Paths { driving, longest })
    }

    /// Whether this activity is on the longest path. No link reaches a span, so
    /// the trace can't find it: an unfinished span is on the path when its float
    /// is the path's (TF-027).
    pub(super) fn on_path(
        &self,
        i: usize,
        total_float: Minutes,
        path_float: Option<Minutes>,
        longest: bool,
    ) -> bool {
        let activity = self.graph.activities[i];
        if is_span(activity) {
            activity.status != ActivityStatus::Completed && path_float == Some(total_float)
        } else {
            longest
        }
    }

    /// Unfinished work, or finished work logic pushed past the data date. Only
    /// being moved to working time doesn't count.
    fn can_end_a_path(&self, i: usize, early: &[Dates]) -> Result<bool, EngineError> {
        let activity = self.graph.activities[i];
        if activity.status != ActivityStatus::Completed {
            return Ok(true);
        }
        let Some(date) = self.input.project.data_date else {
            return Ok(false);
        };
        Ok(early[i].start > self.calendar_at(i)?.next_work_instant(date)?)
    }

    /// Whether the option treats this activity as an open end: nothing limits it
    /// in the backward pass, so it has nowhere later to go.
    pub(super) fn open_ended(&self, i: usize) -> bool {
        let a = self.graph.activities[i];
        self.options.make_open_ended_critical
            && !self.graph.outgoing[i]
                .iter()
                .any(|&e| self.limits_predecessor(e))
            && !self.graph.external_from.contains_key(&a.id)
            && self.external(a.external_late_finish, a).is_none()
    }

    /// Critical under the project's definition, or as an open end. Finished work
    /// has no float, so it is critical only by being on the longest path.
    pub(super) fn critical(&self, i: usize, total_float: Minutes, longest: bool) -> bool {
        let finished = self.graph.activities[i].status == ActivityStatus::Completed;
        let by_definition = match self.options.critical {
            CriticalDefinition::LongestPath => longest,
            CriticalDefinition::TotalFloatAtMost(limit) => !finished && total_float <= limit,
        };
        by_definition || (!finished && self.open_ended(i))
    }

    fn path_end(&self, early: &[Dates]) -> Option<usize> {
        let activities = &self.graph.activities;
        if let Some(end) = self.options.float_paths.end_activity {
            return activities.iter().position(|a| a.id == end);
        }
        (0..activities.len())
            .filter(|&i| {
                !is_span(activities[i]) && activities[i].status != ActivityStatus::Completed
            })
            .max_by_key(|&i| (early[i].finish, std::cmp::Reverse(i)))
    }

    /// Multiple float paths (docs/rules/TF-002-multiple-float-paths.md): each
    /// activity's relative float is the least slack on any route to the end
    /// activity. Each path is a chain with no slack between its members. By
    /// total float, each path starts from the lowest relative float left; by
    /// free float, from the least slack to work already on a path.
    pub(super) fn float_paths(
        &self,
        early: &[Dates],
    ) -> Result<Vec<Option<(u32, u32)>>, EngineError> {
        let count = self.graph.activities.len();
        let mut found = vec![None; count];
        let options = &self.options.float_paths;
        if !options.enabled {
            return Ok(found);
        }
        let Some(end) = self.path_end(early) else {
            return Ok(found);
        };
        let relative = self.relative_float(end, early)?;
        let mut heads = Heads::new(options.mode, end, &relative, early);
        let mut path = 0;
        // Everything on a finished chain is in `found`, so this never clears.
        let mut on_chain = vec![false; count];
        while let Some(head) = heads.next(&found) {
            if options.max_paths.is_some_and(|max| path >= max) {
                break;
            }
            path += 1;
            let mut chain = vec![head];
            let mut current = head;
            on_chain[head] = true;
            while let Some(next) =
                self.next_on_path(current, early, &relative, &found, &on_chain)?
            {
                chain.push(next);
                on_chain[next] = true;
                current = next;
            }
            for (order, &i) in chain.iter().rev().enumerate() {
                found[i] = Some((path, u32::try_from(order + 1).unwrap_or(u32::MAX)));
            }
            if options.mode == FloatPathMode::FreeFloat {
                for &i in &chain {
                    for &e in self.graph.incoming[i].iter().filter(|&&e| self.usable(e)) {
                        let (a, _) = self.graph.ends[e];
                        if found[a].is_none() && relative[a].is_some() {
                            heads.offer(a, self.link_slack(e, early)?.max(Minutes::ZERO));
                        }
                    }
                }
            }
        }
        Ok(found)
    }

    /// A link a float path can run along: it applies, and its predecessor
    /// isn't finished.
    fn usable(&self, e: usize) -> bool {
        let (a, _) = self.graph.ends[e];
        self.link_applies(e) && self.graph.activities[a].status != ActivityStatus::Completed
    }

    /// Each activity's least slack summed along any route to `end`, or `None`
    /// where no route reaches it.
    fn relative_float(
        &self,
        end: usize,
        early: &[Dates],
    ) -> Result<Vec<Option<Minutes>>, EngineError> {
        let mut relative: Vec<Option<Minutes>> = vec![None; self.graph.activities.len()];
        relative[end] = Some(Minutes::ZERO);
        let mut pending = BTreeSet::from([(Minutes::ZERO, end)]);
        while let Some((float, i)) = pending.pop_first() {
            if relative[i] != Some(float) {
                continue;
            }
            for &e in self.graph.incoming[i].iter().filter(|&&e| self.usable(e)) {
                let (a, _) = self.graph.ends[e];
                let total = float
                    .checked_add(self.link_slack(e, early)?.max(Minutes::ZERO))
                    .ok_or(EngineError::InvalidInput("float path overflow"))?;
                if relative[a].is_none_or(|old| total < old) {
                    relative[a] = Some(total);
                    pending.insert((total, a));
                }
            }
        }
        Ok(relative)
    }

    /// The unassigned predecessor with no slack to `i`, latest finish first.
    fn next_on_path(
        &self,
        i: usize,
        early: &[Dates],
        relative: &[Option<Minutes>],
        found: &[Option<(u32, u32)>],
        on_chain: &[bool],
    ) -> Result<Option<usize>, EngineError> {
        let mut best = None;
        for &e in &self.graph.incoming[i] {
            let (a, _) = self.graph.ends[e];
            if !self.link_applies(e)
                || found[a].is_some()
                || on_chain[a]
                || relative[a] != relative[i]
                || self.link_slack(e, early)? != Minutes::ZERO
            {
                continue;
            }
            let key = (early[a].finish, std::cmp::Reverse(a));
            if best.is_none_or(|(old, _)| key > old) {
                best = Some((key, a));
            }
        }
        Ok(best.map(|(_, a)| a))
    }
}

/// Where each float path starts, best first: the lowest relative float by
/// total float, or the least slack to a path already found by free float.
/// Ties go to the lower relative float, then the later early finish, then the
/// earlier position.
struct Heads {
    mode: FloatPathMode,
    /// The end activity, which starts path 1 in both modes.
    first: Option<usize>,
    /// By total float, every activity that reaches the end; by free float,
    /// those offered. Stale entries are skipped.
    queue: BTreeSet<(Minutes, Minutes, Reverse<DateTime>, usize)>,
    /// By free float, each activity's least slack offered so far.
    best: Vec<Option<Minutes>>,
    relative: Vec<Option<Minutes>>,
    finish: Vec<DateTime>,
}

impl Heads {
    fn new(mode: FloatPathMode, end: usize, relative: &[Option<Minutes>], early: &[Dates]) -> Self {
        let finish: Vec<DateTime> = early.iter().map(|d| d.finish).collect();
        let key = |i: usize, first: Minutes, float: Minutes| (first, float, Reverse(finish[i]), i);
        let queue = match mode {
            FloatPathMode::TotalFloat => relative
                .iter()
                .enumerate()
                .filter_map(|(i, r)| r.map(|r| key(i, r, r)))
                .collect(),
            // Offered as paths are found.
            FloatPathMode::FreeFloat => BTreeSet::new(),
        };
        Self {
            mode,
            first: Some(end),
            queue,
            best: vec![None; relative.len()],
            relative: relative.to_vec(),
            finish,
        }
    }

    /// By free float, `i` is `slack` from a path already found.
    fn offer(&mut self, i: usize, slack: Minutes) {
        if self.best[i].is_some_and(|old| old <= slack) {
            return;
        }
        self.best[i] = Some(slack);
        if let Some(r) = self.relative[i] {
            self.queue.insert((slack, r, Reverse(self.finish[i]), i));
        }
    }

    /// The next head not yet on a path.
    fn next(&mut self, found: &[Option<(u32, u32)>]) -> Option<usize> {
        if let Some(end) = self.first.take() {
            return Some(end);
        }
        while let Some((first, _, _, i)) = self.queue.pop_first() {
            let current = match self.mode {
                FloatPathMode::TotalFloat => true,
                FloatPathMode::FreeFloat => self.best[i].is_none_or(|best| best == first),
            };
            if found[i].is_none() && current {
                return Some(i);
            }
        }
        None
    }
}
