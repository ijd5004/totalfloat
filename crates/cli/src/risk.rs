//! `totalfloat risk`: Monte Carlo schedule risk. Durations are drawn
//! from three-point estimates and every draw is calculated by the engine, so
//! no date here comes from anywhere else (ARCHITECTURE.md, Deterministic engine).

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use totalfloat_edit::Span;
use totalfloat_model::{Activity, ActivityId, DateTime, Minutes, Schedule, UdfSubject, UdfValue};
use totalfloat_risk::{
    Correlation, Distribution, Estimate, Inputs, Range, RiskError, RiskEvent, Settings, Simulation,
    is_sampled, simulate,
};

use crate::error::{CliError, exit, parse_date};
use crate::loaded::Loaded;
use crate::output::print_line;

/// The schema an inputs file may name.
pub const INPUTS_SCHEMA: &str = "totalfloat.risk-inputs/1";

/// Iterations the MCP tool and the app may ask for at most, so one call
/// can't hold the machine for long.
pub const MAX_TOOL_ITERATIONS: u32 = 10_000;

/// Items listed in each ranking.
const TOP: usize = 20;

/// Items the text report shows from each list.
const SHOWN: usize = 10;

/// What to sample: `totalfloat.risk-inputs/1`, from a file or the MCP tool.
#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InputsFile {
    /// `totalfloat.risk-inputs/1`, optional.
    schema: Option<String>,
    /// A range for unfinished activities with no estimate of their own.
    default: Option<RangeIn>,
    /// Three-point estimates by Activity ID.
    #[serde(default)]
    estimates: BTreeMap<String, EstimateIn>,
    /// Activity user-defined fields holding estimates, in days.
    udfs: Option<UdfsIn>,
    /// Groups of activities whose durations move together.
    #[serde(default)]
    correlations: Vec<CorrelationIn>,
    /// Risks that may happen and delay activities if they do.
    #[serde(default)]
    risks: Vec<RiskIn>,
}

/// A risk that may happen, such as `{"code": "R1", "probability": 0.3,
/// "impacts": {"A1": {"min": "2d", "likely": "5d", "max": "10d"}}}`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct RiskIn {
    /// Its code: any name.
    code: String,
    /// The chance it happens, from 0 to 1.
    probability: f64,
    /// The delay it adds to each activity it affects, by Activity ID.
    impacts: BTreeMap<String, DelayIn>,
}

/// A delay in spans. Without `likely`, it's uniform between `min` and `max`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct DelayIn {
    /// Least.
    min: Span,
    /// Most likely.
    likely: Option<Span>,
    /// Most.
    max: Span,
    /// `triangle` (the default), `pert` or `uniform`; uniform without `likely`.
    distribution: Option<Shape>,
}

/// A range in percent of remaining duration, such as `{"min": -10, "max": 25}`.
#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct RangeIn {
    /// Optimistic, 0 or below, such as -10.
    min: f64,
    /// Pessimistic, 0 or above, such as 25.
    max: f64,
    /// `triangle` (the default), `pert` or `uniform`.
    distribution: Option<Shape>,
}

/// A three-point estimate of remaining duration, in spans such as `8d` or `60h`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct EstimateIn {
    /// Optimistic.
    min: Span,
    /// Most likely. Default: the remaining duration.
    likely: Option<Span>,
    /// Pessimistic.
    max: Span,
    /// `triangle` (the default), `pert` or `uniform`.
    distribution: Option<Shape>,
}

/// Activity number fields holding estimates in days of the activity's calendar.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct UdfsIn {
    /// The field with the optimistic duration.
    min: String,
    /// The field with the most likely duration. Default: the remaining duration.
    likely: Option<String>,
    /// The field with the pessimistic duration.
    max: String,
    /// `triangle` (the default), `pert` or `uniform`.
    distribution: Option<Shape>,
}

/// Activities whose durations move together.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct CorrelationIn {
    /// Activity IDs.
    activities: Vec<String>,
    /// From 0 (independent) to 1 (in step).
    coefficient: f64,
}

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
enum Shape {
    Triangle,
    Pert,
    Uniform,
}

/// The distribution a shape names, triangular when none is given.
fn shape(shape: Option<Shape>) -> Distribution {
    match shape {
        None | Some(Shape::Triangle) => Distribution::Triangle,
        Some(Shape::Pert) => Distribution::Pert,
        Some(Shape::Uniform) => Distribution::Uniform,
    }
}

/// How to run, from the command line or the MCP tool.
#[derive(Debug, Clone)]
pub struct Run {
    /// Only this project (`proj_id`); needed if the file has several.
    pub project: Option<u64>,
    /// Iterations.
    pub iterations: u32,
    /// The seed.
    pub seed: u64,
    /// A date to give the chance of finishing by.
    pub target: Option<String>,
}

/// `totalfloat.risk-analysis/1`, documented in
/// docs/schemas/risk-analysis-1.schema.json.
#[derive(Debug, Serialize, JsonSchema)]
pub struct Report {
    schema: &'static str,
    file: String,
    project: String,
    project_id: u64,
    iterations: u32,
    seed: u64,
    sampled_activities: usize,
    deterministic_finish: String,
    /// The share of iterations finishing at or before the deterministic finish.
    deterministic_probability: f64,
    target: Option<Target>,
    /// P5 to P95 in steps of 5.
    percentiles: Vec<Percentile>,
    mean_finish: String,
    /// In calendar days.
    standard_deviation_days: f64,
    earliest_finish: String,
    latest_finish: String,
    /// For each day from the earliest finish to the latest, the share of
    /// iterations finished by its end.
    cumulative: Vec<Day>,
    milestones: Vec<Milestone>,
    /// The activities most often critical, most first.
    criticality: Vec<Ranked>,
    /// Unfinished activities critical in at least one iteration.
    criticality_total: usize,
    /// The durations that move the finish most: Spearman rank correlation
    /// with the finish, largest magnitude first.
    sensitivity: Vec<Ranked>,
    /// Activities with an estimate, or under the default range, that weren't
    /// drawn because their expected finish sets their duration.
    set_by_expected_finish: Vec<String>,
    /// Risk events, largest effect on P80 first.
    risks: Vec<RiskReport>,
}

#[derive(Debug, Serialize, JsonSchema)]
struct RiskReport {
    code: String,
    probability: f64,
    /// The share of iterations it happened in.
    occurred: f64,
    /// P50 of the finish less P50 without this risk, same seed, in calendar days.
    p50_effect_days: f64,
    /// P80 of the finish less P80 without this risk, same seed, in calendar days.
    p80_effect_days: f64,
}

#[derive(Debug, Serialize, JsonSchema)]
struct Target {
    date: String,
    probability: f64,
}

#[derive(Debug, Serialize, JsonSchema)]
struct Percentile {
    p: u8,
    finish: String,
}

#[derive(Debug, Serialize, JsonSchema)]
struct Day {
    date: String,
    probability: f64,
}

#[derive(Debug, Serialize, JsonSchema)]
struct Milestone {
    code: String,
    name: String,
    deterministic: Option<String>,
    p50: String,
    p80: String,
}

#[derive(Debug, Serialize, JsonSchema)]
struct Ranked {
    code: String,
    name: String,
    value: f64,
}

fn refused(message: impl Into<String>, hint: &str) -> CliError {
    CliError::new("risk-refused", message, hint)
}

/// Reads an inputs file.
///
/// # Errors
///
/// `risk-inputs` naming the path within the file that's wrong.
pub fn read_inputs(path: &Path) -> Result<InputsFile, CliError> {
    let text = std::fs::read_to_string(path).map_err(|e| {
        CliError::new(
            "risk-inputs",
            format!("can't read {}: {e}", path.display()),
            "give a JSON file of estimates (docs/guide/risk.md)",
        )
    })?;
    let mut de = serde_json::Deserializer::from_str(&text);
    let inputs: InputsFile = serde_path_to_error::deserialize(&mut de).map_err(|e| {
        CliError::new(
            "risk-inputs",
            format!("{}: {} at {}", path.display(), e.inner(), e.path()),
            "docs/guide/risk.md describes the file",
        )
    })?;
    Ok(inputs)
}

/// The one project to simulate.
fn one_project(loaded: &Loaded, project: Option<u64>) -> Result<&Schedule, CliError> {
    let reach: Vec<&Schedule> = loaded
        .schedules
        .iter()
        .filter(|s| project.is_none_or(|id| s.project.id.0 == id))
        .collect();
    match reach.as_slice() {
        [one] => Ok(one),
        [] => Err(CliError::new(
            "no-such-project",
            "there's no such project in the file",
            "`inspect` lists the projects in the file",
        )),
        many => Err(CliError::new(
            "needs-project",
            format!("the file has {} projects; risk runs on one", many.len()),
            "name the project: --project, or `project` over MCP",
        )),
    }
}

/// Turns the file's names and spans into the engine's IDs and minutes,
/// listing every problem.
fn resolve(schedule: &Schedule, file: &InputsFile) -> Result<Inputs, CliError> {
    let mut problems: Vec<String> = Vec::new();
    if let Some(schema) = &file.schema
        && schema != INPUTS_SCHEMA
    {
        problems.push(format!("schema {schema}: this reads {INPUTS_SCHEMA}"));
    }
    let by_code: BTreeMap<&str, &Activity> = schedule
        .activities
        .values()
        .map(|a| (a.code.as_str(), a))
        .collect();
    let mut inputs = Inputs {
        default: file.default.map(|r| Range {
            min_percent: r.min,
            max_percent: r.max,
            distribution: shape(r.distribution),
        }),
        ..Inputs::default()
    };
    if let Some(udfs) = &file.udfs {
        udf_estimates(
            schedule,
            (udfs, &file.estimates),
            &mut inputs,
            &mut problems,
        );
    }
    for (code, e) in &file.estimates {
        let Some(a) = by_code.get(code.as_str()) else {
            problems.push(format!("{code}: no such activity"));
            continue;
        };
        let lengths = totalfloat_edit::duration_lengths(schedule, a);
        let minutes = |span: Span| span.minutes_in(lengths.as_ref());
        let likely = match e.likely {
            Some(span) => minutes(span),
            None => Some(a.remaining_duration),
        };
        // The first span its calendar can't measure, to say which period is missing.
        let unread = [Some(e.min), e.likely, Some(e.max)]
            .into_iter()
            .flatten()
            .find(|span| minutes(*span).is_none());
        match (minutes(e.min), likely, minutes(e.max)) {
            (Some(min), Some(likely), Some(max)) => {
                inputs.estimates.insert(
                    a.id,
                    Estimate {
                        min,
                        likely,
                        max,
                        distribution: shape(e.distribution),
                    },
                );
            }
            _ => match unread {
                Some(span) => {
                    let why = totalfloat_edit::no_length(code, span);
                    problems.push(format!("{code}: {}; {}", why.message, why.hint));
                }
                None => problems.push(format!("{code}: a span can't be read on its calendar")),
            },
        }
    }
    for group in &file.correlations {
        let mut activities = Vec::new();
        for code in &group.activities {
            match by_code.get(code.as_str()) {
                Some(a) => activities.push(a.id),
                None => problems.push(format!("{code}: no such activity")),
            }
        }
        inputs.correlations.push(Correlation {
            activities,
            coefficient: group.coefficient,
        });
    }
    inputs.risks = risk_events(schedule, &by_code, &file.risks, &mut problems);
    if inputs.estimates.is_empty() && inputs.default.is_none() && inputs.risks.is_empty() {
        if file.udfs.is_some() {
            problems.push(
                "nothing to sample: no activity has values in the named fields; give estimates, a default range or risks"
                    .to_owned(),
            );
        } else {
            problems.push(
                "nothing to sample: give estimates, udfs, a default range or risks".to_owned(),
            );
        }
    }
    if problems.is_empty() {
        Ok(inputs)
    } else {
        Err(refused(
            problems.join("; "),
            "docs/guide/risk.md describes the inputs",
        ))
    }
}

/// Risk events from the file, with their impacts' Activity IDs and spans
/// resolved, listing every problem.
fn risk_events(
    schedule: &Schedule,
    by_code: &BTreeMap<&str, &Activity>,
    risks: &[RiskIn],
    problems: &mut Vec<String>,
) -> Vec<RiskEvent> {
    let mut events = Vec::with_capacity(risks.len());
    let mut seen = std::collections::BTreeSet::new();
    for risk in risks {
        if !seen.insert(risk.code.to_lowercase()) {
            problems.push(format!("{}: two risks have this code", risk.code));
        }
        if !(0.0..=1.0).contains(&risk.probability) {
            problems.push(format!(
                "{}: probability {} is outside 0 to 1",
                risk.code, risk.probability
            ));
        }
        let mut impacts = Vec::new();
        for (code, d) in &risk.impacts {
            let Some(a) = by_code.get(code.as_str()) else {
                problems.push(format!("{}: {code}: no such activity", risk.code));
                continue;
            };
            let lengths = totalfloat_edit::duration_lengths(schedule, a);
            let minutes = |span: Span| span.minutes_in(lengths.as_ref());
            let likely = match d.likely {
                Some(span) => minutes(span),
                None => minutes(d.min),
            };
            // Without a most likely value there's no peak to shape, whatever
            // distribution is named.
            let distribution = match d.likely {
                None => Distribution::Uniform,
                Some(_) => shape(d.distribution),
            };
            if let (Some(min), Some(likely), Some(max)) = (minutes(d.min), likely, minutes(d.max)) {
                impacts.push((
                    a.id,
                    Estimate {
                        min,
                        likely,
                        max,
                        distribution,
                    },
                ));
            } else {
                let span = [Some(d.min), d.likely, Some(d.max)]
                    .into_iter()
                    .flatten()
                    .find(|span| minutes(*span).is_none());
                let why = span.map_or_else(
                    || "a span can't be read on its calendar".to_owned(),
                    |span| {
                        let e = totalfloat_edit::no_length(code, span);
                        format!("{}; {}", e.message, e.hint)
                    },
                );
                problems.push(format!("{}: {code}: {why}", risk.code));
            }
        }
        events.push(RiskEvent {
            name: risk.code.clone(),
            probability: risk.probability,
            impacts,
        });
    }
    events
}

/// Estimates from activity number fields, in days of each activity's calendar.
/// An activity with neither field set has none; with one but not the other,
/// it's a problem. An activity with an estimate in the file takes that one,
/// so its fields aren't read.
fn udf_estimates(
    schedule: &Schedule,
    (udfs, given): (&UdfsIn, &BTreeMap<String, EstimateIn>),
    inputs: &mut Inputs,
    problems: &mut Vec<String>,
) {
    let field = |name: &str, problems: &mut Vec<String>| match totalfloat_edit::udf_named(
        schedule,
        name,
        &UdfSubject::Activity,
    ) {
        Ok(id) => Some(id),
        Err(e) => {
            problems.push(format!("udf {name}: {e}"));
            None
        }
    };
    let min = field(&udfs.min, problems);
    let max = field(&udfs.max, problems);
    let likely = udfs.likely.as_deref().map(|name| field(name, problems));
    let (Some(min), Some(max)) = (min, max) else {
        return;
    };
    for a in schedule
        .activities
        .values()
        .filter(|a| is_sampled(a) && !given.contains_key(&a.code))
    {
        let number = |id| match a.udfs.get(&id) {
            Some(UdfValue::Number(n)) => Some(*n),
            _ => None,
        };
        let (low, high) = (number(min), number(max));
        if low.is_none() && high.is_none() {
            continue;
        }
        let Some(day) = totalfloat_edit::duration_lengths(schedule, a).and_then(|l| l.day) else {
            problems.push(format!("{}: its calendar has no hours per day", a.code));
            continue;
        };
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_precision_loss,
            reason = "days to whole minutes"
        )]
        let minutes = |days: f64| Minutes::new((days * day.get() as f64).round() as i64);
        // An empty most-likely field means the remaining duration, as with none named.
        let most = match likely {
            Some(Some(id)) => Some(number(id).map_or(a.remaining_duration, minutes)),
            _ => Some(a.remaining_duration),
        };
        match (low, most, high) {
            (Some(low), Some(most), Some(high)) => {
                inputs.estimates.insert(
                    a.id,
                    Estimate {
                        min: minutes(low),
                        likely: most,
                        max: minutes(high),
                        distribution: shape(udfs.distribution),
                    },
                );
            }
            _ => problems.push(format!("{}: an estimate field is empty", a.code)),
        }
    }
}

fn text(date: DateTime) -> String {
    date.to_string()
}

/// Runs a simulation on a file that's been read.
///
/// # Errors
///
/// `risk-refused` with every problem in the inputs, or the engine's reason a
/// schedule doesn't calculate.
pub fn report(
    file: &str,
    loaded: &Loaded,
    inputs: &InputsFile,
    run: &Run,
) -> Result<Report, CliError> {
    let schedule = one_project(loaded, run.project)?;
    loaded.refuse_incomplete(schedule)?;
    let target = run.target.as_deref().map(parse_date).transpose()?;
    let inputs = resolve(schedule, inputs)?;
    let threads = std::thread::available_parallelism().map_or(1, std::num::NonZero::get);
    let settings = Settings {
        iterations: run.iterations,
        seed: run.seed,
        threads,
    };
    let sim = simulate(schedule, &loaded.schedules, &inputs, settings).map_err(|e| match &e {
        RiskError::Invalid(problems) => refused(
            problems
                .iter()
                .map(
                    |p| match p.activity.and_then(|id| schedule.activities.get(&id)) {
                        Some(a) => format!("{}: {}", a.code, p.reason),
                        None => p.reason.to_owned(),
                    },
                )
                .collect::<Vec<_>>()
                .join("; "),
            "docs/guide/risk.md describes the inputs",
        ),
        RiskError::Deterministic(_) => refused(
            e.to_string(),
            "`schedule` says why a schedule doesn't calculate",
        ),
        RiskError::NoIterations => refused(e.to_string(), "give --iterations of 1 or more"),
        other => refused(
            other.to_string(),
            "the drawn durations didn't calculate; narrow the estimates' ranges",
        ),
    })?;
    Ok(build(file, schedule, &sim, run, target))
}

#[allow(
    clippy::cast_precision_loss,
    reason = "minutes and counts far below 2^52"
)]
fn build(
    file: &str,
    schedule: &Schedule,
    sim: &Simulation,
    run: &Run,
    target: Option<DateTime>,
) -> Report {
    let n = sim.finishes.len();
    let earliest = sim
        .finishes
        .iter()
        .min()
        .copied()
        .unwrap_or(sim.deterministic_finish);
    let latest = sim
        .finishes
        .iter()
        .max()
        .copied()
        .unwrap_or(sim.deterministic_finish);
    let offsets: Vec<f64> = sim
        .finishes
        .iter()
        .map(|f| f.minutes_since(earliest).get() as f64)
        .collect();
    let mean = offsets.iter().sum::<f64>() / n.max(1) as f64;
    let variance = offsets.iter().map(|x| (x - mean) * (x - mean)).sum::<f64>() / n.max(1) as f64;
    #[allow(clippy::cast_possible_truncation, reason = "rounded minutes")]
    let mean_finish = earliest
        .checked_add(Minutes::new(mean.round() as i64))
        .unwrap_or(earliest);
    let mut cumulative = Vec::new();
    let day_of = day_ending;
    let mut day = day_of(earliest);
    while day <= day_of(latest) {
        let next = day.checked_add_days(1);
        let by = |f: &&DateTime| next.is_none_or(|n| day_of(**f) < n);
        cumulative.push(Day {
            date: day.to_string(),
            probability: round(sim.finishes.iter().filter(by).count() as f64 / n as f64),
        });
        match next {
            Some(next) => day = next,
            None => break,
        }
    }
    let named = |id: &ActivityId, value: f64| {
        let a = &schedule.activities[id];
        Ranked {
            code: a.code.clone(),
            name: a.name.clone(),
            value: round(value),
        }
    };
    let mut critical: Vec<(&ActivityId, &f64)> =
        sim.criticality.iter().filter(|(_, v)| **v > 0.0).collect();
    critical.sort_by(|(a, x), (b, y)| y.total_cmp(x).then(a.cmp(b)));
    let criticality_total = critical.len();
    Report {
        schema: "totalfloat.risk-analysis/1",
        file: file.to_owned(),
        project: schedule.project.short_name.clone(),
        project_id: schedule.project.id.0,
        iterations: u32::try_from(n).unwrap_or(u32::MAX),
        seed: run.seed,
        sampled_activities: sim.sampled,
        deterministic_finish: text(sim.deterministic_finish),
        deterministic_probability: round(sim.probability_by(sim.deterministic_finish)),
        target: target.map(|date| Target {
            date: text(date),
            probability: round(sim.probability_by(date)),
        }),
        percentiles: (1..=19)
            .map(|i| Percentile {
                p: i * 5,
                finish: text(sim.percentile(i * 5)),
            })
            .collect(),
        mean_finish: text(mean_finish),
        standard_deviation_days: round(variance.sqrt() / 1440.0),
        earliest_finish: text(earliest),
        latest_finish: text(latest),
        cumulative,
        milestones: milestones(schedule, sim),
        set_by_expected_finish: sim
            .set_by_expected_finish
            .iter()
            .filter_map(|id| Some(schedule.activities.get(id)?.code.clone()))
            .collect(),
        criticality: critical
            .into_iter()
            .take(TOP)
            .map(|(id, v)| named(id, *v))
            .collect(),
        criticality_total,
        sensitivity: sim
            .sensitivity
            .iter()
            .take(TOP)
            .map(|(id, v)| named(id, *v))
            .collect(),
        risks: risks(sim),
    }
}

/// The day a finish ends: a finish at midnight ends the day before, as XER
/// files write the end of a day (TF-011).
fn day_ending(finish: DateTime) -> totalfloat_model::Date {
    finish
        .checked_add(Minutes::new(-1))
        .unwrap_or(finish)
        .date()
}

/// Each risk event's effect, as the simulation ranked them.
#[allow(clippy::cast_precision_loss, reason = "minutes far below 2^52")]
fn risks(sim: &Simulation) -> Vec<RiskReport> {
    sim.risks
        .iter()
        .map(|effect| RiskReport {
            code: effect.name.clone(),
            probability: effect.probability,
            occurred: round(effect.occurred),
            p50_effect_days: round(effect.p50.get() as f64 / 1440.0),
            p80_effect_days: round(effect.p80.get() as f64 / 1440.0),
        })
        .collect()
}

/// Unfinished finish milestones, latest P50 first (ties by Activity ID).
fn milestones(schedule: &Schedule, sim: &Simulation) -> Vec<Milestone> {
    let mut found: Vec<(DateTime, &str, Milestone)> = sim
        .milestones
        .iter()
        .filter_map(|(id, finishes)| {
            let a = &schedule.activities[id];
            let p50 = totalfloat_risk::percentile(finishes, 50)?;
            let p80 = totalfloat_risk::percentile(finishes, 80)?;
            let m = Milestone {
                code: a.code.clone(),
                name: a.name.clone(),
                deterministic: sim.deterministic_milestones.get(id).copied().map(text),
                p50: text(p50),
                p80: text(p80),
            };
            Some((p50, a.code.as_str(), m))
        })
        .collect();
    found.sort_by(|(a, x, _), (b, y, _)| b.cmp(a).then(x.cmp(y)));
    found.into_iter().map(|(_, _, m)| m).collect()
}

/// Four decimal places, so the JSON doesn't carry float noise.
fn round(x: f64) -> f64 {
    (x * 10_000.0).round() / 10_000.0
}

/// A share as a whole percent.
#[allow(clippy::cast_possible_truncation, reason = "0 to 100")]
fn percent(x: f64) -> i64 {
    (x * 100.0).round() as i64
}

fn human(report: &Report) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "Project {} ({}): {} iterations, seed {}, {} activities sampled",
        report.project,
        report.project_id,
        report.iterations,
        report.seed,
        report.sampled_activities
    );
    let _ = writeln!(
        out,
        "  deterministic finish: {} ({}% of iterations finish by then)",
        report.deterministic_finish,
        percent(report.deterministic_probability)
    );
    let p = |n: u8| {
        report
            .percentiles
            .iter()
            .find(|x| x.p == n)
            .map_or("?", |x| x.finish.as_str())
    };
    let _ = writeln!(
        out,
        "  P10 {}, P50 {}, P80 {}, P90 {}",
        p(10),
        p(50),
        p(80),
        p(90)
    );
    let spread = if report.standard_deviation_days < 1.0 {
        format!("{:.1} hours", report.standard_deviation_days * 24.0)
    } else {
        format!("{:.1} days", report.standard_deviation_days)
    };
    let _ = writeln!(
        out,
        "  mean {}, standard deviation {spread}, range {} to {}",
        report.mean_finish, report.earliest_finish, report.latest_finish
    );
    if let Some(t) = &report.target {
        let _ = writeln!(out, "  finish by {}: {}%", t.date, percent(t.probability));
    }
    if !report.milestones.is_empty() {
        let _ = writeln!(out, "  milestones, latest first:");
        for m in report.milestones.iter().take(SHOWN) {
            let _ = writeln!(
                out,
                "    {} {}: P50 {}, P80 {}",
                m.code, m.name, m.p50, m.p80
            );
        }
        if report.milestones.len() > SHOWN {
            let _ = writeln!(out, "    and {} more", report.milestones.len() - SHOWN);
        }
    }
    if !report.criticality.is_empty() {
        let _ = writeln!(
            out,
            "  most often critical ({} critical at least once):",
            report.criticality_total
        );
        for r in report.criticality.iter().take(SHOWN) {
            let _ = writeln!(out, "    {} {}: {}%", r.code, r.name, percent(r.value));
        }
    }
    if !report.set_by_expected_finish.is_empty() {
        let _ = writeln!(
            out,
            "  not drawn, as their expected finish sets their duration: {}",
            report.set_by_expected_finish.join(", ")
        );
    }
    if !report.sensitivity.is_empty() {
        let _ = writeln!(out, "  durations that move the finish most:");
        for r in report.sensitivity.iter().take(SHOWN) {
            let _ = writeln!(out, "    {} {}: {:.2}", r.code, r.name, r.value);
        }
    }
    if !report.risks.is_empty() {
        let _ = writeln!(out, "  risk events, by their effect on P80:");
        for r in report.risks.iter().take(SHOWN) {
            let _ = writeln!(
                out,
                "    {}: happened in {}% of iterations, P80 {:+.1} days, P50 {:+.1} days",
                r.code,
                percent(r.occurred),
                r.p80_effect_days,
                r.p50_effect_days
            );
        }
    }
    out.trim_end().to_owned()
}

/// Runs the command.
///
/// # Errors
///
/// As [`report`], and when the file or the inputs can't be read.
pub fn run(
    file: &Path,
    inputs: Option<&Path>,
    range: Option<&str>,
    run: &Run,
    json: bool,
) -> Result<u8, CliError> {
    let loaded = Loaded::new(crate::location::read(file)?);
    let mut parsed = match inputs {
        Some(path) => read_inputs(path)?,
        None => InputsFile::default(),
    };
    if let Some(range) = range {
        parsed.default = Some(parse_range(range)?);
    }
    let report = report(&file.display().to_string(), &loaded, &parsed, run)?;
    if json {
        print_line(&serde_json::to_string_pretty(&report).unwrap_or_default());
    } else {
        print_line(&human(&report));
    }
    Ok(exit::OK)
}

/// `-10,25`: a triangular default range in percent.
fn parse_range(text: &str) -> Result<RangeIn, CliError> {
    let bad = || {
        CliError::new(
            "risk-inputs",
            format!("{text} isn't a range: write it as MIN,MAX in percent, such as -10,25"),
            "docs/guide/risk.md describes the default range",
        )
    };
    let (min, max) = text.split_once(',').ok_or_else(bad)?;
    let number = |s: &str| {
        s.trim()
            .trim_end_matches('%')
            .parse::<f64>()
            .map_err(|_| bad())
    };
    Ok(RangeIn {
        min: number(min)?,
        max: number(max)?,
        distribution: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_finish_at_midnight_ends_the_day_before() {
        let day = |text: &str| day_ending(text.parse().unwrap()).to_string();
        assert_eq!(day("2026-01-14 00:00"), "2026-01-13");
        assert_eq!(day("2026-01-14 00:01"), "2026-01-14");
        assert_eq!(day("2026-01-13 17:00"), "2026-01-13");
    }

    /// The published schemas are the types': regenerate with
    /// `TOTALFLOAT_WRITE_SCHEMAS=1 cargo test -p totalfloat-cli risk::tests`.
    #[test]
    fn the_published_schemas_match_the_types() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/schemas");
        for (name, id, schema) in [
            (
                "risk-analysis-1.schema.json",
                "totalfloat.risk-analysis/1",
                schemars::schema_for!(Report),
            ),
            (
                "risk-inputs-1.schema.json",
                INPUTS_SCHEMA,
                schemars::schema_for!(InputsFile),
            ),
        ] {
            // As the other published schemas: named by `$id` and title, keys sorted.
            let mut value = serde_json::to_value(&schema).unwrap();
            value["$id"] = id.into();
            value["title"] = id.into();
            let text = serde_json::to_string_pretty(&value).unwrap() + "\n";
            let path = root.join(name);
            if std::env::var_os("TOTALFLOAT_WRITE_SCHEMAS").is_some() {
                std::fs::write(&path, &text).unwrap();
            }
            assert_eq!(std::fs::read_to_string(&path).unwrap(), text, "{name}");
        }
    }
}
