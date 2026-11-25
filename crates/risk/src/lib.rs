//! Monte Carlo schedule risk: remaining durations drawn from
//! three-point estimates, each draw calculated by the deterministic engine.
//!
//! The engine sets every date (ARCHITECTURE.md, Deterministic engine); this crate only chooses durations
//! and counts what comes out. Pure functions, with threads for speed: the same
//! inputs and seed give the same result on any number of threads.
//!
//! Each iteration has its own random stream, from the seed and the iteration
//! number. It draws one standard normal per correlation group, then one
//! number per sampled activity in ID order: a uniform for an activity on its
//! own, or a normal mixed with its group's for one in a group (a one-factor
//! Gaussian copula). That number picks the duration through the estimate's
//! quantile function.

#![forbid(unsafe_code)]

mod random;
mod sample;

pub use sample::{Distribution, Estimate};

use std::collections::{BTreeMap, BTreeSet};

use sample::{Sampler, Tables};
use totalfloat_engine::{
    DateField, DateProvenance, EngineError, ScheduleResult, schedule_with_projects,
};
use totalfloat_model::{
    Activity, ActivityId, ActivityStatus, ActivityType, DateTime, Minutes, Schedule,
};

/// A range for activities without their own estimate, in percent of their
/// remaining duration: `-10` and `25` give 90%, 100% and 125%.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Range {
    /// Optimistic, at or below zero.
    pub min_percent: f64,
    /// Pessimistic, at or above zero.
    pub max_percent: f64,
    /// The shape between them.
    pub distribution: Distribution,
}

/// Activities whose durations move together.
#[derive(Debug, Clone, PartialEq)]
pub struct Correlation {
    /// The activities, each in at most one group.
    pub activities: Vec<ActivityId>,
    /// From 0 (independent) to 1 (in step).
    pub coefficient: f64,
}

/// What to sample.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Inputs {
    /// Estimates by activity.
    pub estimates: BTreeMap<ActivityId, Estimate>,
    /// For unfinished activities with no estimate. Without it they keep their
    /// remaining duration.
    pub default: Option<Range>,
    /// Groups of activities whose durations move together.
    pub correlations: Vec<Correlation>,
    /// Risks that may happen, and delay activities if they do.
    pub risks: Vec<RiskEvent>,
}

/// A risk that may happen in an iteration, with a chance, and if it does,
/// delays the activities it affects by a drawn amount.
#[derive(Debug, Clone, PartialEq)]
pub struct RiskEvent {
    /// How it's known, such as its code in the register.
    pub name: String,
    /// From 0 to 1.
    pub probability: f64,
    /// The activities it delays, and by how much: a three-point estimate of
    /// the delay, added to the activity's duration.
    pub impacts: Vec<(ActivityId, Estimate)>,
}

/// What one risk event did over a simulation.
#[derive(Debug, Clone, PartialEq)]
pub struct RiskEffect {
    /// As the inputs name it.
    pub name: String,
    /// Its chance, as given.
    pub probability: f64,
    /// The share of iterations it happened in.
    pub occurred: f64,
    /// P50 of the finish less P50 with this risk switched off, same seed.
    pub p50: Minutes,
    /// P80 of the finish less P80 with this risk switched off, same seed.
    pub p80: Minutes,
}

/// The largest pessimistic range, in percent: ten times the duration.
pub const MAX_PERCENT: f64 = 1_000.0;

/// How to run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Settings {
    /// Iterations, at least one.
    pub iterations: u32,
    /// The seed: the same seed gives the same result.
    pub seed: u64,
    /// Threads to run on, at least one.
    pub threads: usize,
}

/// Why a simulation couldn't run.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum RiskError {
    /// Inputs that can't be used, every one listed.
    #[error("{} problems with the inputs", .0.len())]
    Invalid(Vec<Invalid>),
    /// Iterations or threads of zero.
    #[error("a simulation needs at least one iteration and one thread")]
    NoIterations,
    /// The schedule doesn't calculate as it is.
    #[error("the schedule doesn't calculate: {0}")]
    Deterministic(EngineError),
    /// One iteration's durations didn't calculate.
    #[error("iteration {iteration} didn't calculate: {error}")]
    Iteration {
        /// Counting from zero.
        iteration: u32,
        /// What the engine said.
        error: EngineError,
    },
}

/// One problem with the inputs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invalid {
    /// The activity it's about, if any.
    pub activity: Option<ActivityId>,
    /// What's wrong.
    pub reason: &'static str,
}

/// What a simulation found. Lists are in iteration order.
#[derive(Debug, Clone, PartialEq)]
pub struct Simulation {
    /// The finish with every duration as it is in the schedule.
    pub deterministic_finish: DateTime,
    /// Each iteration's finish: the project's own finish, as the engine's
    /// `network_finish` gives it, before as-late-as-possible work moves.
    pub finishes: Vec<DateTime>,
    /// Each unfinished finish milestone's early finish in each iteration.
    pub milestones: BTreeMap<ActivityId, Vec<DateTime>>,
    /// The share of iterations in which each unfinished activity was critical.
    pub criticality: BTreeMap<ActivityId, f64>,
    /// Spearman rank correlation of each sampled activity's duration with the
    /// finish, largest magnitude first (ties by ID). Activities whose duration
    /// or whose finish never varied are left out.
    pub sensitivity: Vec<(ActivityId, f64)>,
    /// Activities sampled.
    pub sampled: usize,
    /// Activities that would have been drawn, by an estimate or the default
    /// range, but whose expected finish sets their duration (TF-018), so
    /// they weren't.
    pub set_by_expected_finish: Vec<ActivityId>,
    /// Each unfinished finish milestone's early finish with durations as they
    /// are.
    pub deterministic_milestones: BTreeMap<ActivityId, DateTime>,
    /// Each risk event's effect, largest effect on P80 first (then P50, then
    /// name).
    pub risks: Vec<RiskEffect>,
}

impl Simulation {
    /// The finish at percentile `p` (0 to 100), by nearest rank.
    #[must_use]
    pub fn percentile(&self, p: u8) -> DateTime {
        // A simulation has at least one iteration.
        percentile(&self.finishes, p).unwrap_or(self.deterministic_finish)
    }

    /// The share of iterations finishing at or before `date`.
    #[must_use]
    pub fn probability_by(&self, date: DateTime) -> f64 {
        share(
            self.finishes.iter().filter(|f| **f <= date).count(),
            self.finishes.len(),
        )
    }
}

/// The value at percentile `p` (0 to 100) of `values`, by nearest rank, or
/// `None` for no values. Ranks are whole numbers, so they're worked out
/// without floating point.
#[must_use]
pub fn percentile(values: &[DateTime], p: u8) -> Option<DateTime> {
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let rank = (usize::from(p) * sorted.len()).div_ceil(100);
    sorted.get(rank.clamp(1, sorted.len().max(1)) - 1).copied()
}

#[allow(clippy::cast_precision_loss, reason = "counts far below 2^52")]
fn share(count: usize, total: usize) -> f64 {
    count as f64 / total.max(1) as f64
}

/// Whether an activity's duration is drawn: unfinished work that takes time.
/// Milestones take none, and spans take theirs from other work.
#[must_use]
pub fn is_sampled(a: &Activity) -> bool {
    a.status != ActivityStatus::Completed
        && !a.activity_type.is_milestone()
        && !a.activity_type.is_span()
}

/// Activities whose finish the engine set from their expected finish in the
/// deterministic run, so a drawn duration can't change it (TF-016). Where the
/// engine ignores an expected finish (a mandatory constraint, or no work left
/// before it), the duration counts and is drawn.
fn set_by_expected_finish(result: &ScheduleResult) -> BTreeSet<ActivityId> {
    result
        .activities
        .iter()
        .filter(|(_, r)| {
            r.provenance
                .get(&DateField::EarlyFinish)
                .is_some_and(|why| matches!(why.driver(), DateProvenance::ExpectedFinish))
        })
        .map(|(id, _)| *id)
        .collect()
}

/// One sampled activity: its index in the copy and how it's drawn.
struct Draw {
    id: ActivityId,
    sampler: Sampler,
    /// Its correlation group and coefficient's square roots, if any.
    group: Option<(usize, f64, f64)>,
}

/// One risk event as it's drawn.
struct Event {
    probability: f64,
    impacts: Vec<(ActivityId, Sampler)>,
}

/// How each iteration is drawn.
struct Plan {
    draws: Vec<Draw>,
    events: Vec<Event>,
    /// Remaining durations to put back before each iteration on activities a
    /// risk can delay, as a risk adds to them.
    bases: Vec<(ActivityId, Minutes)>,
    /// Correlation groups.
    groups: usize,
}

/// Runs `settings.iterations` calculations of `schedule` with durations drawn
/// from `inputs`. `projects` are the other projects loaded with it, read as
/// `engine::schedule_with_projects` reads them.
///
/// # Errors
///
/// [`RiskError::Invalid`] with every problem in the inputs, and the engine's
/// error if the schedule, or any iteration, doesn't calculate.
pub fn simulate(
    schedule: &Schedule,
    projects: &[Schedule],
    inputs: &Inputs,
    settings: Settings,
) -> Result<Simulation, RiskError> {
    if settings.iterations == 0 || settings.threads == 0 {
        return Err(RiskError::NoIterations);
    }
    let options = &schedule.project.options;
    let deterministic =
        schedule_with_projects(schedule, options, projects).map_err(RiskError::Deterministic)?;
    let fixed = set_by_expected_finish(&deterministic);
    // One set of quantile tables for estimates and delays alike, and every
    // problem with either listed at once.
    let mut tables = Tables::default();
    let (draws, events) = match (
        plan(schedule, inputs, &fixed, &mut tables),
        events(schedule, inputs, &fixed, &mut tables),
    ) {
        (Ok(draws), Ok(events)) => (draws, events),
        (draws, events) => {
            let mut problems = Vec::new();
            for result in [draws.err(), events.err()].into_iter().flatten() {
                match result {
                    RiskError::Invalid(found) => problems.extend(found),
                    other => return Err(other),
                }
            }
            return Err(RiskError::Invalid(problems));
        }
    };
    let bases = events
        .iter()
        .flat_map(|e| &e.impacts)
        .filter_map(|(id, _)| Some((*id, schedule.activities.get(id)?.remaining_duration)))
        .collect();
    let deterministic_finish =
        deterministic
            .project
            .network_finish
            .ok_or(RiskError::Deterministic(EngineError::InvalidInput(
                "no activities",
            )))?;
    let plan = Plan {
        draws,
        events,
        bases,
        groups: inputs.correlations.len(),
    };
    let every: Vec<u64> = (0..u64::from(settings.iterations)).collect();
    let all = run_all(schedule, projects, &plan, settings, (None, &every))?;
    let with: Vec<DateTime> = all.iter().map(|r| r.finish).collect();
    let mut effects = Vec::with_capacity(plan.events.len());
    for (k, risk) in inputs.risks.iter().enumerate() {
        // Where the risk didn't happen, switching it off changes nothing, so
        // only the iterations it happened in run again.
        let happened: Vec<u64> = every
            .iter()
            .zip(&all)
            .filter(|(_, r)| r.occurred[k])
            .map(|(i, _)| *i)
            .collect();
        let mut off = with.clone();
        for (i, r) in happened.iter().zip(run_all(
            schedule,
            projects,
            &plan,
            settings,
            (Some(k), &happened),
        )?) {
            if let Some(slot) = usize::try_from(*i).ok().and_then(|i| off.get_mut(i)) {
                *slot = r.finish;
            }
        }
        let effect = |p| match (percentile(&with, p), percentile(&off, p)) {
            (Some(a), Some(b)) => a.minutes_since(b),
            _ => Minutes::ZERO,
        };
        effects.push(RiskEffect {
            name: risk.name.clone(),
            probability: risk.probability,
            occurred: share(happened.len(), all.len()),
            p50: effect(50),
            p80: effect(80),
        });
    }
    effects.sort_by(|a, b| {
        b.p80
            .cmp(&a.p80)
            .then(b.p50.cmp(&a.p50))
            .then(a.name.cmp(&b.name))
    });
    let mut simulation = summarize(schedule, &plan.draws, all, deterministic_finish);
    simulation.risks = effects;
    simulation.set_by_expected_finish = schedule
        .activities
        .values()
        .filter(|a| {
            fixed.contains(&a.id)
                && is_sampled(a)
                && (inputs.estimates.contains_key(&a.id) || inputs.default.is_some())
        })
        .map(|a| a.id)
        .collect();
    simulation.deterministic_milestones = milestones_of(schedule, &deterministic);
    Ok(simulation)
}

/// Runs the given iterations of `plan` across the threads, giving their
/// results in the same order, with risk `off` switched off if given.
fn run_all(
    schedule: &Schedule,
    projects: &[Schedule],
    plan: &Plan,
    settings: Settings,
    (off, iterations): (Option<usize>, &[u64]),
) -> Result<Vec<Run>, RiskError> {
    if iterations.is_empty() {
        return Ok(Vec::new());
    }
    let chunk = iterations.len().div_ceil(settings.threads.max(1));
    let runs: Vec<Result<Vec<Run>, RiskError>> = std::thread::scope(|scope| {
        let handles: Vec<_> = iterations
            .chunks(chunk)
            .map(|part| {
                scope.spawn(move || {
                    let mut copy = schedule.clone();
                    part.iter()
                        .map(|&i| run(&mut copy, projects, plan, (settings.seed, i), off))
                        .collect::<Result<Vec<_>, _>>()
                })
            })
            .collect();
        handles
            .into_iter()
            // A panic in a worker is a bug: it goes on up as it was.
            .map(|h| {
                h.join()
                    .unwrap_or_else(|payload| std::panic::resume_unwind(payload))
            })
            .collect()
    });
    let mut all = Vec::with_capacity(iterations.len());
    for chunk in runs {
        all.extend(chunk?);
    }
    Ok(all)
}

/// Checks the risk events and works out how each is drawn.
fn events(
    schedule: &Schedule,
    inputs: &Inputs,
    fixed: &BTreeSet<ActivityId>,
    tables: &mut Tables,
) -> Result<Vec<Event>, RiskError> {
    let mut problems = Vec::new();
    let mut events = Vec::with_capacity(inputs.risks.len());
    for risk in &inputs.risks {
        if !(0.0..=1.0).contains(&risk.probability) {
            problems.push(Invalid {
                activity: None,
                reason: "a risk's probability outside 0 to 1",
            });
        }
        let mut impacts = Vec::with_capacity(risk.impacts.len());
        for (id, e) in &risk.impacts {
            let mut bad = |reason| {
                problems.push(Invalid {
                    activity: Some(*id),
                    reason,
                });
            };
            match schedule.activities.get(id) {
                None => bad("not an activity in this project"),
                Some(a) if !is_sampled(a) => {
                    bad("finished, a milestone or a span: a risk can't delay it");
                }
                Some(a) if fixed.contains(&a.id) => {
                    bad("its expected finish sets its duration, so a risk can't delay it");
                }
                Some(_) if e.min.get() < 0 || e.min > e.likely || e.likely > e.max => {
                    bad("a risk's delay: minimum, most likely and maximum out of order");
                }
                Some(_) => impacts.push((*id, Sampler::new(e, tables))),
            }
        }
        events.push(Event {
            probability: risk.probability,
            impacts,
        });
    }
    if problems.is_empty() {
        Ok(events)
    } else {
        Err(RiskError::Invalid(problems))
    }
}

/// An activity's estimate from the default range: its remaining duration
/// scaled by the range's percentages, most likely as it is.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "rounded minutes, bounded by MAX_PERCENT"
)]
fn ranged(a: &Activity, r: Range) -> Estimate {
    let scale = |percent: f64| {
        let m = (a.remaining_duration.get() as f64 * (1.0 + percent / 100.0)).round() as i64;
        Minutes::new(m.max(0))
    };
    Estimate {
        min: scale(r.min_percent),
        likely: a.remaining_duration,
        max: scale(r.max_percent),
        distribution: r.distribution,
    }
}

/// Checks the inputs and works out how each activity is drawn.
fn plan(
    schedule: &Schedule,
    inputs: &Inputs,
    fixed: &BTreeSet<ActivityId>,
    tables: &mut Tables,
) -> Result<Vec<Draw>, RiskError> {
    let mut problems = Vec::new();
    let mut bad = |activity, reason| problems.push(Invalid { activity, reason });
    for (id, e) in &inputs.estimates {
        match schedule.activities.get(id) {
            None => bad(Some(*id), "not an activity in this project"),
            Some(a) if !is_sampled(a) => {
                bad(
                    Some(*id),
                    "finished, a milestone or a span: nothing to draw",
                );
            }
            Some(_) if e.min.get() < 0 => bad(Some(*id), "a negative minimum"),
            Some(_) if e.min > e.likely || e.likely > e.max => {
                bad(Some(*id), "minimum, most likely and maximum out of order");
            }
            Some(_) => {}
        }
    }
    if let Some(r) = inputs.default
        && !(r.min_percent > -100.0
            && r.min_percent <= 0.0
            && (0.0..=MAX_PERCENT).contains(&r.max_percent))
    {
        bad(
            None,
            "the default range must run from above -100% to 0% or less, and from 0% to 1,000%",
        );
    }
    let mut group_of = BTreeMap::new();
    for (g, c) in inputs.correlations.iter().enumerate() {
        if !(0.0..=1.0).contains(&c.coefficient) {
            bad(None, "a correlation coefficient outside 0 to 1");
        }
        for id in &c.activities {
            match schedule.activities.get(id) {
                None => bad(Some(*id), "not an activity in this project"),
                Some(a) if !is_sampled(a) => {
                    bad(
                        Some(*id),
                        "finished, a milestone or a span: nothing to correlate",
                    );
                }
                Some(_) => {}
            }
            if group_of.insert(*id, g).is_some() {
                bad(Some(*id), "in more than one correlation group");
            }
        }
    }
    let mut draws = Vec::new();
    for a in schedule
        .activities
        .values()
        .filter(|a| is_sampled(a) && !fixed.contains(&a.id))
    {
        let estimate = match (inputs.estimates.get(&a.id), inputs.default) {
            (Some(e), _) => *e,
            (None, Some(r)) => ranged(a, r),
            (None, None) if group_of.contains_key(&a.id) => {
                bad(
                    Some(a.id),
                    "correlated, but has no estimate and there's no default range",
                );
                continue;
            }
            (None, None) => continue,
        };
        let group = group_of.get(&a.id).map(|&g| {
            let r = inputs.correlations[g].coefficient.clamp(0.0, 1.0);
            (g, r.sqrt(), (1.0 - r).sqrt())
        });
        draws.push(Draw {
            id: a.id,
            sampler: Sampler::new(&estimate, tables),
            group,
        });
    }
    if problems.is_empty() {
        Ok(draws)
    } else {
        Err(RiskError::Invalid(problems))
    }
}

/// What one iteration found.
struct Run {
    finish: DateTime,
    milestones: Vec<(ActivityId, DateTime)>,
    critical: Vec<ActivityId>,
    durations: Vec<Minutes>,
    /// Whether each risk event happened.
    occurred: Vec<bool>,
}

fn run(
    copy: &mut Schedule,
    projects: &[Schedule],
    plan: &Plan,
    (seed, iteration): (u64, u64),
    off: Option<usize>,
) -> Result<Run, RiskError> {
    let mut rng = random::Rng::for_iteration(seed, iteration);
    for (id, base) in &plan.bases {
        if let Some(a) = copy.activities.get_mut(id) {
            a.remaining_duration = *base;
        }
    }
    let factors: Vec<f64> = (0..plan.groups).map(|_| rng.normal()).collect();
    let mut durations = Vec::with_capacity(plan.draws.len());
    for d in &plan.draws {
        let u = match d.group {
            Some((g, shared, own)) => random::normal_cdf(shared * factors[g] + own * rng.normal()),
            None => rng.uniform(),
        };
        let minutes = d.sampler.at(u);
        if let Some(a) = copy.activities.get_mut(&d.id) {
            a.remaining_duration = minutes;
        }
        durations.push(minutes);
    }
    // Every risk's numbers are drawn whether it happens or not, so switching
    // one off changes nothing else in the iteration.
    let mut occurred = Vec::with_capacity(plan.events.len());
    for (k, event) in plan.events.iter().enumerate() {
        let happens = rng.uniform() < event.probability && off != Some(k);
        for (id, sampler) in &event.impacts {
            let delay = sampler.at(rng.uniform());
            if happens && let Some(a) = copy.activities.get_mut(id) {
                a.remaining_duration = Minutes::new(a.remaining_duration.get() + delay.get());
            }
        }
        occurred.push(happens);
    }
    let failed = |error| RiskError::Iteration {
        iteration: u32::try_from(iteration).unwrap_or(u32::MAX),
        error,
    };
    let result = schedule_with_projects(copy, &copy.project.options, projects).map_err(failed)?;
    // The project's own finish, before as-late-as-possible work moves.
    let finish = result
        .project
        .network_finish
        .ok_or_else(|| failed(EngineError::InvalidInput("no activities")))?;
    let milestones = milestones_of(copy, &result).into_iter().collect();
    let critical = result
        .activities
        .iter()
        .filter(|(_, r)| r.critical == Some(true))
        .map(|(id, _)| *id)
        .collect();
    Ok(Run {
        finish,
        milestones,
        critical,
        durations,
        occurred,
    })
}

/// Each unfinished finish milestone's early finish in a result.
fn milestones_of(schedule: &Schedule, result: &ScheduleResult) -> BTreeMap<ActivityId, DateTime> {
    schedule
        .activities
        .values()
        .filter(|a| {
            a.activity_type == ActivityType::FinishMilestone
                && a.status != ActivityStatus::Completed
        })
        .filter_map(|a| Some((a.id, result.activities.get(&a.id)?.values.early_finish?)))
        .collect()
}

fn summarize(
    schedule: &Schedule,
    draws: &[Draw],
    runs: Vec<Run>,
    deterministic_finish: DateTime,
) -> Simulation {
    let n = runs.len();
    let mut milestones: BTreeMap<ActivityId, Vec<DateTime>> = BTreeMap::new();
    let mut critical: BTreeMap<ActivityId, usize> = schedule
        .activities
        .values()
        .filter(|a| a.status != ActivityStatus::Completed)
        .map(|a| (a.id, 0))
        .collect();
    let mut finishes = Vec::with_capacity(n);
    let mut columns: Vec<Vec<Minutes>> = vec![Vec::with_capacity(n); draws.len()];
    for r in runs {
        finishes.push(r.finish);
        for (id, date) in r.milestones {
            milestones.entry(id).or_default().push(date);
        }
        for id in r.critical {
            if let Some(count) = critical.get_mut(&id) {
                *count += 1;
            }
        }
        for (column, d) in columns.iter_mut().zip(r.durations) {
            column.push(d);
        }
    }
    let base = finishes
        .iter()
        .min()
        .copied()
        .unwrap_or(deterministic_finish);
    let finish_ranks = ranks(
        &finishes
            .iter()
            .map(|f| f.minutes_since(base).get())
            .collect::<Vec<_>>(),
    );
    let mut sensitivity: Vec<(ActivityId, f64)> = draws
        .iter()
        .zip(&columns)
        .filter_map(|(d, column)| {
            let r = ranks(&column.iter().map(|m| m.get()).collect::<Vec<_>>());
            Some((d.id, correlation(&r, &finish_ranks)?))
        })
        .collect();
    sensitivity.sort_by(|(a, x), (b, y)| y.abs().total_cmp(&x.abs()).then(a.cmp(b)));
    Simulation {
        deterministic_finish,
        finishes,
        milestones,
        criticality: critical
            .into_iter()
            .map(|(id, count)| (id, share(count, n)))
            .collect(),
        sensitivity,
        sampled: draws.len(),
        set_by_expected_finish: Vec::new(),
        deterministic_milestones: BTreeMap::new(),
        risks: Vec::new(),
    }
}

/// Ranks from 1, ties sharing their average rank.
#[allow(clippy::cast_precision_loss, reason = "ranks far below 2^52")]
fn ranks(values: &[i64]) -> Vec<f64> {
    let mut order: Vec<usize> = (0..values.len()).collect();
    order.sort_by_key(|&i| values[i]);
    let mut out = vec![0.0; values.len()];
    let mut start = 0;
    while start < order.len() {
        let mut end = start;
        while end + 1 < order.len() && values[order[end + 1]] == values[order[start]] {
            end += 1;
        }
        let rank = (start + end) as f64 / 2.0 + 1.0;
        for &i in &order[start..=end] {
            out[i] = rank;
        }
        start = end + 1;
    }
    out
}

/// Pearson correlation, or `None` if either side never varies.
#[allow(clippy::cast_precision_loss, reason = "counts far below 2^52")]
fn correlation(x: &[f64], y: &[f64]) -> Option<f64> {
    let n = x.len() as f64;
    let (mx, my) = (x.iter().sum::<f64>() / n, y.iter().sum::<f64>() / n);
    let (mut sxy, mut sxx, mut syy) = (0.0, 0.0, 0.0);
    for (a, b) in x.iter().zip(y) {
        sxy += (a - mx) * (b - my);
        sxx += (a - mx) * (a - mx);
        syy += (b - my) * (b - my);
    }
    (sxx > 0.0 && syy > 0.0).then(|| sxy / (sxx * syy).sqrt())
}

#[cfg(test)]
mod tests;
