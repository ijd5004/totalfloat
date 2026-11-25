//! Stable adjacency indexes and nonrecursive cycle detection.
use crate::EngineError;
use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap};
use totalfloat_model::{Activity, ActivityId, Relationship, Schedule, ScheduleOptions};

/// Each activity's links, by edge index in the order they were added. One
/// list for the whole graph rather than one per activity.
pub(crate) struct Adjacency {
    starts: Vec<usize>,
    edges: Vec<usize>,
}
impl Adjacency {
    /// Lists for `count` activities, from each edge's activity in edge order.
    fn new(count: usize, ends: impl Iterator<Item = usize> + Clone) -> Self {
        let mut starts = vec![0; count + 1];
        for end in ends.clone() {
            starts[end + 1] += 1;
        }
        for i in 0..count {
            starts[i + 1] += starts[i];
        }
        let mut next = starts.clone();
        let mut edges = vec![0; starts[count]];
        for (e, end) in ends.enumerate() {
            edges[next[end]] = e;
            next[end] += 1;
        }
        Self { starts, edges }
    }
}
impl std::ops::Index<usize> for Adjacency {
    type Output = [usize];
    fn index(&self, i: usize) -> &[usize] {
        &self.edges[self.starts[i]..self.starts[i + 1]]
    }
}

pub(crate) struct Graph<'a> {
    pub activities: Vec<&'a Activity>,
    pub edges: Vec<&'a Relationship>,
    pub incoming: Adjacency,
    pub outgoing: Adjacency,
    pub ends: Vec<(usize, usize)>,
    /// Relationships with an end in another project. They don't join the
    /// network: the other end's dates bound this one (TF-030).
    pub external: Vec<&'a Relationship>,
    /// `external` by predecessor and by successor, in the same order.
    pub external_from: BTreeMap<ActivityId, Vec<&'a Relationship>>,
    pub external_to: BTreeMap<ActivityId, Vec<&'a Relationship>>,
    pub order: Vec<usize>,
}
impl<'a> Graph<'a> {
    pub fn new(schedule: &'a Schedule, options: &ScheduleOptions) -> Result<Self, EngineError> {
        // In ID order, as the map holds them, so an ID's index is a search away.
        let activities: Vec<_> = schedule.activities.values().collect();
        let ids: Vec<_> = activities.iter().map(|a| a.id).collect();
        let index = |id: &ActivityId| ids.binary_search(id).ok();
        if schedule
            .activities
            .iter()
            .any(|(id, a)| *id != a.id || a.project != schedule.project.id)
        {
            return Err(EngineError::InvalidInput(
                "activity identity or project mismatch",
            ));
        }
        let (mut edges, mut ends, mut external) = (vec![], vec![], vec![]);
        for (id, edge) in &schedule.relationships {
            if *id != edge.id {
                return Err(EngineError::InvalidInput("relationship identity mismatch"));
            }
            if edge.predecessor_project != schedule.project.id
                && edge.successor_project != schedule.project.id
            {
                return Err(EngineError::InvalidInput(
                    "relationship has no local endpoint",
                ));
            }
            if schedule.is_external(edge) {
                if !options.ignore_external_relationships {
                    external.push(edge);
                }
                continue;
            }
            let (Some(a), Some(b)) = (index(&edge.predecessor), index(&edge.successor)) else {
                return Err(EngineError::InvalidInput("missing relationship endpoint"));
            };
            edges.push(edge);
            ends.push((a, b));
        }
        let count = activities.len();
        let mut graph = Self {
            incoming: Adjacency::new(count, ends.iter().map(|&(_, b)| b)),
            outgoing: Adjacency::new(count, ends.iter().map(|&(a, _)| a)),
            activities,
            edges,
            ends,
            external,
            external_from: BTreeMap::new(),
            external_to: BTreeMap::new(),
            order: vec![],
        };
        let mut degree: Vec<_> = (0..count)
            .map(|i| {
                graph.incoming[i]
                    .iter()
                    .filter(|&&e| graph.schedules(e))
                    .count()
            })
            .collect();
        // Lowest index first, so the order is the same however it's stored.
        let mut ready: BinaryHeap<_> = degree
            .iter()
            .enumerate()
            .filter_map(|(i, d)| (*d == 0).then_some(Reverse(i)))
            .collect();
        while let Some(Reverse(i)) = ready.pop() {
            graph.order.push(i);
            for &e in &graph.outgoing[i] {
                if !graph.schedules(e) {
                    continue;
                }
                let b = graph.ends[e].1;
                degree[b] -= 1;
                if degree[b] == 0 {
                    ready.push(Reverse(b));
                }
            }
        }
        if graph.order.len() != graph.activities.len() {
            return Err(EngineError::Cycle(graph.cycle()));
        }
        Ok(graph)
    }
    fn schedules(&self, e: usize) -> bool {
        let (a, b) = self.ends[e];
        [a, b].into_iter().all(|i| {
            !matches!(
                self.activities[i].activity_type,
                totalfloat_model::ActivityType::LevelOfEffort
                    | totalfloat_model::ActivityType::WbsSummary
            )
        })
    }
    fn cycle(&self) -> Vec<ActivityId> {
        let mut color = vec![0; self.activities.len()];
        for root in 0..color.len() {
            if color[root] != 0 {
                continue;
            }
            color[root] = 1;
            let mut stack = vec![(root, 0)];
            while let Some(&(node, next)) = stack.last() {
                if next == self.outgoing[node].len() {
                    color[node] = 2;
                    stack.pop();
                    continue;
                }
                if let Some(last) = stack.last_mut() {
                    last.1 += 1;
                }
                let edge = self.outgoing[node][next];
                if !self.schedules(edge) {
                    continue;
                }
                let successor = self.ends[edge].1;
                if color[successor] == 1 {
                    let mut loop_ids: Vec<_> = stack
                        .iter()
                        .skip_while(|(n, _)| *n != successor)
                        .map(|(n, _)| self.activities[*n].id)
                        .collect();
                    loop_ids.push(self.activities[successor].id);
                    return loop_ids;
                }
                if color[successor] == 0 {
                    color[successor] = 1;
                    stack.push((successor, 0));
                }
            }
        }
        vec![]
    }
}
