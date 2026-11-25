//! `totalfloat portfolio`: the EPS and a portfolio of projects.
//!
//! An enterprise project structure (EPS) files projects under a tree of
//! nodes. XER files leave the tree out: each project's root WBS node keeps
//! only its EPS node's ID (`PROJWBS.parent_wbs_id` on the `proj_node_flag`
//! row), and that node isn't in the file. So the tree lives in a portfolio
//! file beside the schedules (`totalfloat.portfolio/1`, TOML), and the
//! schedules are never changed by it.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use toml::Spanned;
use totalfloat_model::{
    Activity, ActivityId, ActivityStatus, DateTime, Minutes, ProjectId, Schedule,
};

pub mod capacity;

use crate::error::{CliError, exit};
use crate::loaded::Loaded;
use crate::output::{create_new_text, print_line};

/// The schema a portfolio file names.
pub const SCHEMA: &str = "totalfloat.portfolio/1";

/// A portfolio file as written.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileIn {
    schema: Spanned<String>,
    #[serde(default)]
    eps: Vec<NodeIn>,
    #[serde(default)]
    project: Vec<EntryIn>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct NodeIn {
    id: Spanned<String>,
    name: String,
    parent: Option<Spanned<String>>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EntryIn {
    file: Spanned<String>,
    project: Option<u64>,
    eps: Spanned<String>,
}

/// An EPS node, in tree order.
#[derive(Debug, Clone)]
struct Node {
    id: String,
    name: String,
    parent: Option<String>,
    depth: usize,
}

/// A project placed in the EPS.
struct Placed {
    /// The file as the portfolio names it.
    file: String,
    /// The file's [`identity`].
    source: String,
    eps: String,
    id: u64,
    code: String,
    name: Option<String>,
    data_date: Option<DateTime>,
    must_finish_by: Option<DateTime>,
    /// Whether its figures are calculated or as its file stores them.
    results: Results,
    totals: Totals,
}

/// Where a project's figures come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Results {
    /// Calculated by the engine, as `serve` shows the schedule.
    Calculated,
    /// The results the file stores, as it was last calculated: `portfolio
    /// show` reads these, and the app falls back on them for a file the
    /// engine can't calculate.
    Stored,
}

/// A portfolio read and checked: its EPS in tree order and its projects.
pub struct Portfolio {
    nodes: Vec<Node>,
    projects: Vec<Placed>,
    /// The schedule files read, by [`identity`], when kept for capacity.
    files: BTreeMap<String, Loaded>,
}

fn refused(problems: &[String], path: &Path) -> CliError {
    CliError::new(
        "portfolio-refused",
        format!("{}: {}", path.display(), problems.join("; ")),
        "docs/guide/portfolio.md describes the file",
    )
}

/// The 1-based line of a byte offset in `text`.
fn line_of(text: &str, offset: usize) -> usize {
    text.get(..offset)
        .map_or(1, |before| before.matches('\n').count() + 1)
}

/// Reads and checks a portfolio file and the schedules it names, listing
/// every problem with its line.
///
/// # Errors
///
/// `portfolio-refused` with every problem; the file reader's error if it
/// can't be read.
pub fn read(path: &Path) -> Result<Portfolio, CliError> {
    read_with(path, false, false)
}

/// [`read`], with each file calculated as `serve` shows it, so the figures
/// agree with the app's grid. A file the engine can't calculate keeps
/// the results it stores, and its projects say so.
///
/// # Errors
///
/// As [`read`].
pub fn read_calculated(path: &Path) -> Result<Portfolio, CliError> {
    read_with(path, false, true)
}

/// [`read`], keeping the schedules read, for what needs more than the
/// figures, such as capacity.
///
/// # Errors
///
/// As [`read`].
pub fn read_keeping(path: &Path) -> Result<Portfolio, CliError> {
    read_with(path, true, false)
}

fn read_with(path: &Path, keep: bool, calculate: bool) -> Result<Portfolio, CliError> {
    let text = std::fs::read_to_string(path).map_err(|e| {
        CliError::new(
            "portfolio-refused",
            format!("can't read {}: {e}", path.display()),
            "give a portfolio file; `portfolio new` writes one",
        )
    })?;
    let file: FileIn = toml::from_str(&text).map_err(|e| {
        let at = e
            .span()
            .map(|span| format!("line {}: ", line_of(&text, span.start)))
            .unwrap_or_default();
        refused(&[format!("{at}{}", e.message())], path)
    })?;
    let mut problems = Vec::new();
    let at = |span: std::ops::Range<usize>| line_of(&text, span.start);
    if file.schema.get_ref() != SCHEMA {
        problems.push(format!(
            "line {}: schema {} isn't {SCHEMA}",
            at(file.schema.span()),
            file.schema.get_ref()
        ));
    }
    let mut ids = BTreeMap::new();
    for node in &file.eps {
        if ids.insert(node.id.get_ref().clone(), node).is_some() {
            problems.push(format!(
                "line {}: two EPS nodes are {}",
                at(node.id.span()),
                node.id.get_ref()
            ));
        }
    }
    for node in &file.eps {
        if let Some(parent) = &node.parent
            && !ids.contains_key(parent.get_ref())
        {
            problems.push(format!(
                "line {}: {} has no EPS node {} to sit under",
                at(parent.span()),
                node.id.get_ref(),
                parent.get_ref()
            ));
        }
    }
    let nodes = tree(&file.eps, &ids, &text, &mut problems);
    let base = path.parent().unwrap_or(Path::new(""));
    let (projects, loaded) = place(&file.project, (base, &ids, calculate), &text, &mut problems);
    let files = if keep {
        loaded
            .into_iter()
            .filter_map(|(id, read)| Some((id, read.ok()?)))
            .collect()
    } else {
        BTreeMap::new()
    };
    if problems.is_empty() {
        Ok(Portfolio {
            nodes,
            projects,
            files,
        })
    } else {
        Err(refused(&problems, path))
    }
}

/// A file's projects calculated and the activities the engine found
/// critical, or `None` where it couldn't calculate them.
type Calculation = Option<(Vec<Schedule>, BTreeSet<ActivityId>)>;

/// Each project entry's projects, read from their files and checked, and
/// the files read, by [`identity`]. With `calculate`, each file is
/// calculated once and its projects' figures are the engine's.
fn place(
    entries: &[EntryIn],
    (base, ids, calculate): (&Path, &BTreeMap<String, &NodeIn>, bool),
    text: &str,
    problems: &mut Vec<String>,
) -> (Vec<Placed>, BTreeMap<String, Result<Loaded, CliError>>) {
    let at = |span: std::ops::Range<usize>| line_of(text, span.start);
    let mut loaded: BTreeMap<String, Result<Loaded, CliError>> = BTreeMap::new();
    let mut calculations: BTreeMap<String, Calculation> = BTreeMap::new();
    let mut seen = BTreeSet::new();
    let mut projects = Vec::new();
    for entry in entries {
        let line = at(entry.file.span());
        let name = entry.file.get_ref();
        if !ids.contains_key(entry.eps.get_ref()) {
            problems.push(format!(
                "line {}: there's no EPS node {}",
                at(entry.eps.span()),
                entry.eps.get_ref()
            ));
        }
        // One file, however its path is written, is read once and its
        // projects placed once.
        let resolved = identity(&base.join(name));
        let read = loaded
            .entry(resolved.clone())
            .or_insert_with(|| crate::location::read(&base.join(name)).map(Loaded::new));
        let read = match read {
            Ok(read) => &*read,
            Err(e) => {
                problems.push(format!("line {line}: {name}: {}; {}", e.message, e.hint));
                continue;
            }
        };
        let calculation = if calculate {
            calculations
                .entry(resolved.clone())
                .or_insert_with(|| crate::schedule::calculated(read).ok())
                .as_ref()
        } else {
            None
        };
        let chosen: Vec<&Schedule> = read
            .schedules
            .iter()
            .filter(|s| entry.project.is_none_or(|id| s.project.id.0 == id))
            .collect();
        if chosen.is_empty() {
            problems.push(format!(
                "line {line}: {name} has no project {}",
                entry.project.map_or_else(String::new, |id| id.to_string())
            ));
        }
        for schedule in chosen {
            if !seen.insert((resolved.clone(), schedule.project.id)) {
                problems.push(format!(
                    "line {line}: project {} of {name} is placed twice",
                    schedule.project.id.0
                ));
                continue;
            }
            // A project short of rows would have short figures, so it's
            // refused. Lost relationships change no figure here.
            let lost = lost_rows(read, schedule.project.id);
            if lost > 0 {
                problems.push(format!(
                    "line {line}: {name}: project {} lost {lost} activities, assignments or expenses in mapping, so its figures would be short; `inspect` lists them",
                    schedule.project.id.0
                ));
                continue;
            }
            let project = &schedule.project;
            let calculated = calculation.and_then(|(schedules, critical)| {
                schedules
                    .iter()
                    .find(|s| s.project.id == project.id)
                    .map(|s| (s, critical))
            });
            let (results, totals) = match calculated {
                Some((s, critical)) => (
                    Results::Calculated,
                    Totals::of(s, |a| critical.contains(&a.id)),
                ),
                None => (
                    Results::Stored,
                    Totals {
                        stored_results: 1,
                        ..Totals::of(schedule, |a| crate::query::critical(schedule, a))
                    },
                ),
            };
            projects.push(Placed {
                file: name.clone(),
                source: resolved.clone(),
                eps: entry.eps.get_ref().clone(),
                id: project.id.0,
                code: project.short_name.clone(),
                name: project.name.clone(),
                data_date: project.data_date,
                must_finish_by: project.must_finish_by,
                results,
                totals,
            });
        }
    }
    (projects, loaded)
}

/// One file however its path is written: the canonical path, keeping a
/// store's `@POINT`.
fn identity(path: &Path) -> String {
    let text = path.display().to_string();
    let (file, point) = match text.rsplit_once('@') {
        Some((file, point)) if !file.is_empty() && !point.contains(['/', '\\']) => {
            (PathBuf::from(file), Some(point))
        }
        _ => (path.to_path_buf(), None),
    };
    let file = std::fs::canonicalize(&file).unwrap_or(file);
    match point {
        Some(point) => format!("{}@{point}", file.display()),
        None => file.display().to_string(),
    }
}

/// Rows of a project's activities, assignments and expenses the model left
/// out, which its figures would miss. A row is a duplicate, not a loss, when
/// its ID was kept. Its project is its `proj_id`, or for an assignment or
/// expense its activity's, from the raw TASK table; a row naming neither
/// could be any project's, so it counts against each.
fn lost_rows(loaded: &Loaded, project: ProjectId) -> usize {
    let tables = loaded.raw.tables();
    let tasks: BTreeMap<u64, u64> = tables
        .iter()
        .find(|t| t.name == "TASK")
        .and_then(|t| Some((t, t.field_index("task_id")?, t.field_index("proj_id")?)))
        .map(|(t, task, owner)| {
            t.rows
                .iter()
                .filter_map(|r| Some((r.get(task)?.parse().ok()?, r.get(owner)?.parse().ok()?)))
                .collect()
        })
        .unwrap_or_default();
    let kept = |table: &str, id: u64| {
        loaded.schedules.iter().any(|s| match table {
            "TASK" => s.activities.contains_key(&totalfloat_model::ActivityId(id)),
            "TASKRSRC" => s
                .assignments
                .contains_key(&totalfloat_model::AssignmentId(id)),
            _ => s.expenses.contains_key(&totalfloat_model::ExpenseId(id)),
        })
    };
    loaded
        .diagnostics
        .omitted
        .iter()
        .filter(|row| {
            let table = &tables[row.table];
            let key = match table.name.as_str() {
                "TASK" => "task_id",
                "TASKRSRC" => "taskrsrc_id",
                "PROJCOST" => "cost_item_id",
                _ => return false,
            };
            let cell = |field: &str| {
                table
                    .field_index(field)
                    .and_then(|column| table.rows[row.row].get(column))
                    .and_then(|cell| cell.parse::<u64>().ok())
            };
            if cell(key).is_some_and(|id| kept(&table.name, id)) {
                return false;
            }
            let owner = cell("proj_id").or_else(|| {
                (table.name != "TASK")
                    .then(|| cell("task_id").and_then(|t| tasks.get(&t).copied()))
                    .flatten()
            });
            owner.is_none_or(|owner| owner == project.0)
        })
        .count()
}

/// The nodes in tree order (each after its parent, siblings in file order),
/// noting any loop.
fn tree(
    nodes: &[NodeIn],
    ids: &BTreeMap<String, &NodeIn>,
    text: &str,
    problems: &mut Vec<String>,
) -> Vec<Node> {
    let mut children: BTreeMap<Option<&str>, Vec<&NodeIn>> = BTreeMap::new();
    for node in nodes {
        let parent = node
            .parent
            .as_ref()
            .map(|p| p.get_ref().as_str())
            .filter(|p| ids.contains_key(*p));
        children.entry(parent).or_default().push(node);
    }
    let mut out = Vec::new();
    let mut stack: Vec<(&NodeIn, usize)> = children
        .get(&None)
        .into_iter()
        .flatten()
        .rev()
        .map(|n| (*n, 0))
        .collect();
    let mut placed = BTreeSet::new();
    while let Some((node, depth)) = stack.pop() {
        if !placed.insert(node.id.get_ref().as_str()) {
            continue;
        }
        out.push(Node {
            id: node.id.get_ref().clone(),
            name: node.name.clone(),
            parent: node.parent.as_ref().map(|p| p.get_ref().clone()),
            depth,
        });
        for child in children
            .get(&Some(node.id.get_ref().as_str()))
            .into_iter()
            .flatten()
            .rev()
        {
            stack.push((child, depth + 1));
        }
    }
    for node in nodes {
        if !placed.contains(node.id.get_ref().as_str()) {
            problems.push(format!(
                "line {}: {} is in a loop of parents",
                line_of(text, node.id.span().start),
                node.id.get_ref()
            ));
        }
    }
    out
}

/// What a project or node adds up to.
#[derive(Debug, Clone, Default)]
struct Totals {
    projects: usize,
    /// Projects whose figures are the results their file stores.
    stored_results: usize,
    not_started: usize,
    in_progress: usize,
    finished: usize,
    start: Option<DateTime>,
    finish: Option<DateTime>,
    first_data_date: Option<DateTime>,
    last_data_date: Option<DateTime>,
    critical: usize,
    negative_float: usize,
    budgeted_cost: f64,
    actual_cost: f64,
    remaining_cost: f64,
}

impl Totals {
    /// One project's figures, from the results `s` holds: stored, or the
    /// engine's. `critical` says whether an unfinished activity is critical.
    fn of(s: &Schedule, critical: impl Fn(&Activity) -> bool) -> Self {
        let mut t = Totals {
            projects: 1,
            first_data_date: s.project.data_date,
            last_data_date: s.project.data_date,
            ..Totals::default()
        };
        for a in s.activities.values() {
            match a.status {
                ActivityStatus::NotStarted => t.not_started += 1,
                ActivityStatus::InProgress => t.in_progress += 1,
                ActivityStatus::Completed => t.finished += 1,
            }
            let start = a.actual_start.or(a.stored.early_start);
            let finish = if a.status == ActivityStatus::Completed {
                a.actual_finish
            } else {
                a.stored.early_finish
            };
            t.start = earliest(t.start, start);
            t.finish = latest(t.finish, finish);
            if a.status == ActivityStatus::Completed {
                continue;
            }
            let float = a.stored.total_float;
            if float.is_some_and(Minutes::is_negative) {
                t.negative_float += 1;
            }
            if critical(a) {
                t.critical += 1;
            }
        }
        let own = |p: ProjectId| p == s.project.id;
        for amounts in s
            .assignments
            .values()
            .filter(|a| own(a.project))
            .map(|a| &a.amounts)
        {
            t.budgeted_cost += amounts.budgeted_cost.unwrap_or(0.0);
            t.actual_cost +=
                amounts.actual_cost.unwrap_or(0.0) + amounts.actual_overtime_cost.unwrap_or(0.0);
            t.remaining_cost += amounts.remaining_cost.unwrap_or(0.0);
        }
        for e in s.expenses.values().filter(|e| own(e.project)) {
            t.budgeted_cost += e.budgeted_cost.unwrap_or(0.0);
            t.actual_cost += e.actual_cost.unwrap_or(0.0);
            t.remaining_cost += e.remaining_cost.unwrap_or(0.0);
        }
        t
    }

    fn add(&mut self, other: &Totals) {
        self.projects += other.projects;
        self.stored_results += other.stored_results;
        self.not_started += other.not_started;
        self.in_progress += other.in_progress;
        self.finished += other.finished;
        self.start = earliest(self.start, other.start);
        self.finish = latest(self.finish, other.finish);
        self.first_data_date = earliest(self.first_data_date, other.first_data_date);
        self.last_data_date = latest(self.last_data_date, other.last_data_date);
        self.critical += other.critical;
        self.negative_float += other.negative_float;
        self.budgeted_cost += other.budgeted_cost;
        self.actual_cost += other.actual_cost;
        self.remaining_cost += other.remaining_cost;
    }
}

fn earliest(a: Option<DateTime>, b: Option<DateTime>) -> Option<DateTime> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    }
}

fn latest(a: Option<DateTime>, b: Option<DateTime>) -> Option<DateTime> {
    a.max(b)
}

/// A node's or project's figures, as reported.
#[derive(Debug, Serialize, JsonSchema)]
struct Figures {
    projects: usize,
    /// Projects whose figures are the results their file stores, as it was
    /// last calculated; the others' are calculated.
    stored_results: usize,
    activities: usize,
    not_started: usize,
    in_progress: usize,
    finished: usize,
    /// The earliest start: actual, or early for work not started.
    start: Option<String>,
    /// The latest finish: actual for finished work, early for the rest.
    finish: Option<String>,
    /// The earliest data date among the projects.
    first_data_date: Option<String>,
    /// The latest data date among the projects.
    last_data_date: Option<String>,
    /// Unfinished activities critical by their project's own definition.
    critical: usize,
    /// Unfinished activities with negative total float.
    negative_float: usize,
    /// Assignments' and expenses' costs.
    budgeted_cost: f64,
    actual_cost: f64,
    remaining_cost: f64,
}

fn text(date: Option<DateTime>) -> Option<String> {
    date.map(|d| d.to_string())
}

/// Two decimal places, as money is written.
fn money(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

impl From<&Totals> for Figures {
    fn from(t: &Totals) -> Self {
        Figures {
            projects: t.projects,
            stored_results: t.stored_results,
            activities: t.not_started + t.in_progress + t.finished,
            not_started: t.not_started,
            in_progress: t.in_progress,
            finished: t.finished,
            start: text(t.start),
            finish: text(t.finish),
            first_data_date: text(t.first_data_date),
            last_data_date: text(t.last_data_date),
            critical: t.critical,
            negative_float: t.negative_float,
            budgeted_cost: money(t.budgeted_cost),
            actual_cost: money(t.actual_cost),
            remaining_cost: money(t.remaining_cost),
        }
    }
}

/// `totalfloat.portfolio-show/1`, documented in
/// docs/schemas/portfolio-show-1.schema.json.
#[derive(Debug, Serialize, JsonSchema)]
pub struct Report {
    schema: &'static str,
    file: String,
    /// The EPS in tree order: each node after its parent.
    nodes: Vec<NodeReport>,
    /// Every project, in the order the file places them.
    projects: Vec<ProjectReport>,
}

#[derive(Debug, Serialize, JsonSchema)]
struct NodeReport {
    id: String,
    name: String,
    parent: Option<String>,
    /// 0 at the top.
    depth: usize,
    /// Every project at or under this node.
    totals: Figures,
}

#[derive(Debug, Serialize, JsonSchema)]
struct ProjectReport {
    /// The schedule file, as the portfolio names it.
    file: String,
    /// `PROJECT.proj_id`.
    id: u64,
    code: String,
    name: Option<String>,
    /// The EPS node it sits under.
    eps: String,
    data_date: Option<String>,
    must_finish_by: Option<String>,
    /// Whether its figures are calculated or the results its file stores.
    results: Results,
    /// Must-finish-by less the finish, in calendar days; negative is late.
    finish_variance_days: Option<f64>,
    totals: Figures,
}

impl Portfolio {
    /// The schedule files it places projects from, as it names them, each
    /// once, in the order it first names them.
    #[must_use]
    pub fn files(&self) -> Vec<String> {
        let mut seen = BTreeSet::new();
        self.projects
            .iter()
            .filter(|p| seen.insert(p.file.clone()))
            .map(|p| p.file.clone())
            .collect()
    }
}

/// The portfolio's figures.
#[must_use]
pub fn show(file: &str, portfolio: &Portfolio) -> Report {
    let own: Vec<Totals> = portfolio
        .projects
        .iter()
        .map(|p| p.totals.clone())
        .collect();
    // Each node's own projects, then rolled up from the deepest node up.
    let mut sums: BTreeMap<&str, Totals> = BTreeMap::new();
    for (p, t) in portfolio.projects.iter().zip(&own) {
        sums.entry(p.eps.as_str()).or_default().add(t);
    }
    for node in portfolio.nodes.iter().rev() {
        let mine = sums.get(node.id.as_str()).cloned().unwrap_or_default();
        if let Some(parent) = &node.parent {
            sums.entry(parent.as_str()).or_default().add(&mine);
        }
    }
    #[allow(clippy::cast_precision_loss, reason = "minutes far below 2^52")]
    let days = |m: Minutes| (m.get() as f64 / 1440.0 * 100.0).round() / 100.0;
    Report {
        schema: "totalfloat.portfolio-show/1",
        file: file.to_owned(),
        nodes: portfolio
            .nodes
            .iter()
            .map(|n| NodeReport {
                id: n.id.clone(),
                name: n.name.clone(),
                parent: n.parent.clone(),
                depth: n.depth,
                totals: Figures::from(&sums.get(n.id.as_str()).cloned().unwrap_or_default()),
            })
            .collect(),
        projects: portfolio
            .projects
            .iter()
            .zip(&own)
            .map(|(p, t)| ProjectReport {
                file: p.file.clone(),
                id: p.id,
                code: p.code.clone(),
                name: p.name.clone(),
                eps: p.eps.clone(),
                data_date: text(p.data_date),
                must_finish_by: text(p.must_finish_by),
                results: p.results,
                finish_variance_days: p
                    .must_finish_by
                    .zip(t.finish)
                    .map(|(due, finish)| days(due.minutes_since(finish))),
                totals: Figures::from(t),
            })
            .collect(),
    }
}

fn human(report: &Report) -> String {
    let mut out = String::new();
    let span = |f: &Figures| match (&f.start, &f.finish) {
        (Some(s), Some(e)) => format!("{s} to {e}"),
        _ => "no dates".to_owned(),
    };
    let detail = |f: &Figures| {
        format!(
            "{} activit{} ({} not started, {} in progress, {} finished), {}, critical {}, negative float {}, cost {:.2} budgeted, {:.2} actual, {:.2} remaining",
            f.activities,
            if f.activities == 1 { "y" } else { "ies" },
            f.not_started,
            f.in_progress,
            f.finished,
            span(f),
            f.critical,
            f.negative_float,
            f.budgeted_cost,
            f.actual_cost,
            f.remaining_cost
        )
    };
    for node in &report.nodes {
        let indent = "  ".repeat(node.depth);
        let t = &node.totals;
        let _ = writeln!(
            out,
            "{indent}{} ({}): {} project{}, {}",
            node.name,
            node.id,
            t.projects,
            if t.projects == 1 { "" } else { "s" },
            detail(t)
        );
        for p in report.projects.iter().filter(|p| p.eps == node.id) {
            let late = p
                .finish_variance_days
                .map(|d| format!(", {d:+.1} days against must finish by"))
                .unwrap_or_default();
            let _ = writeln!(
                out,
                "{indent}  - {} ({}) in {}, data date {}: {}{late}",
                p.code,
                p.id,
                p.file,
                p.data_date.as_deref().unwrap_or("none"),
                detail(&p.totals)
            );
        }
    }
    out.trim_end().to_owned()
}

/// Runs `portfolio show`.
///
/// # Errors
///
/// As [`read`].
pub fn run_show(path: &Path, json: bool) -> Result<u8, CliError> {
    let portfolio = read(path)?;
    let report = show(&path.display().to_string(), &portfolio);
    if json {
        print_line(&serde_json::to_string_pretty(&report).unwrap_or_default());
    } else {
        print_line(&human(&report));
    }
    Ok(exit::OK)
}

/// The EPS parent ID each project's root node keeps, by project, as the file
/// writes it.
fn eps_parents(loaded: &Loaded) -> BTreeMap<u64, String> {
    let mut out = BTreeMap::new();
    let Some(table) = loaded.raw.tables().iter().find(|t| t.name == "PROJWBS") else {
        return out;
    };
    let (Some(project), Some(parent), Some(flag)) = (
        table.field_index("proj_id"),
        table.field_index("parent_wbs_id"),
        table.field_index("proj_node_flag"),
    ) else {
        return out;
    };
    for row in &table.rows {
        if row.get(flag) != Some("Y") {
            continue;
        }
        let (Some(id), Some(eps)) = (
            row.get(project).and_then(|p| p.parse().ok()),
            row.get(parent).filter(|p| !p.is_empty()),
        ) else {
            continue;
        };
        out.insert(id, eps.to_owned());
    }
    out
}

/// Text safe in a TOML comment: control characters, which TOML forbids
/// there, become spaces.
fn comment(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}

/// A TOML string.
fn quoted(text: &str) -> String {
    toml::Value::String(text.to_owned()).to_string()
}

/// Runs `portfolio capacity`.
///
/// # Errors
///
/// As [`read`], and for a resource or role no project has demand on.
pub fn run_capacity(
    path: &Path,
    by: crate::usage::By,
    asked: (Option<&str>, Option<&str>),
    json: bool,
) -> Result<u8, CliError> {
    let portfolio = read_keeping(path)?;
    let report = capacity::report(&path.display().to_string(), &portfolio, by, asked)?;
    if json {
        print_line(&serde_json::to_string_pretty(&report).unwrap_or_default());
    } else {
        print_line(&capacity::human(
            &report,
            asked.0.is_some() || asked.1.is_some(),
        ));
    }
    Ok(exit::OK)
}

/// Writes a portfolio file for `files`: one EPS node for each EPS parent
/// the files' projects share, under one root.
///
/// # Errors
///
/// When a file can't be read, has no projects, or `output` exists.
pub fn run_new(files: &[PathBuf], output: &Path) -> Result<u8, CliError> {
    crate::output::refuse_existing(output)?;
    let base = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map_or_else(|| PathBuf::from("."), Path::to_path_buf);
    let base = std::path::absolute(&base).unwrap_or(base);
    let mut nodes: BTreeSet<String> = BTreeSet::new();
    let mut placed = Vec::new();
    let mut read = BTreeSet::new();
    for file in files {
        // A file given twice, however it's written, is placed once.
        if !read.insert(identity(file)) {
            continue;
        }
        let loaded = Loaded::new(crate::location::read(file)?);
        if loaded.schedules.is_empty() {
            return Err(CliError::new(
                "no-projects",
                format!("{} has no project", file.display()),
                "`inspect` reports what the file holds",
            ));
        }
        let parents = eps_parents(&loaded);
        let full = std::path::absolute(file).unwrap_or_else(|_| file.clone());
        let named = full
            .strip_prefix(&base)
            .map_or_else(|_| full.display().to_string(), |p| p.display().to_string())
            .replace('\\', "/");
        for s in &loaded.schedules {
            let node = parents
                .get(&s.project.id.0)
                .map_or_else(|| "ENT".to_owned(), |p| format!("EPS-{p}"));
            if node != "ENT" {
                nodes.insert(node.clone());
            }
            placed.push((
                named.clone(),
                s.project.id.0,
                s.project.short_name.clone(),
                node,
            ));
        }
    }
    let mut out = format!(
        "# Written by `totalfloat portfolio new`. Rename and move nodes and projects here:\n# XER exports keep only each project's EPS node ID, not its name or place.\nschema = {}\n\n[[eps]]\nid = \"ENT\"\nname = \"Enterprise\"\n",
        quoted(SCHEMA)
    );
    for node in &nodes {
        let id = node.trim_start_matches("EPS-");
        let _ = write!(
            out,
            "\n[[eps]]\nid = {}\nname = {}\nparent = \"ENT\"\n",
            quoted(node),
            quoted(&format!("EPS {id}"))
        );
    }
    for (file, id, code, node) in &placed {
        let _ = write!(
            out,
            "\n[[project]]\n# {}\nfile = {}\nproject = {id}\neps = {}\n",
            comment(code),
            quoted(file),
            quoted(node)
        );
    }
    create_new_text(&out, output)?;
    print_line(&format!(
        "Wrote {}: {} projects under {} EPS nodes",
        output.display(),
        placed.len(),
        nodes.len() + 1
    ));
    Ok(exit::OK)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The published schema is the report type's: regenerate with
    /// `TOTALFLOAT_WRITE_SCHEMAS=1 cargo test -p totalfloat-cli portfolio::tests`.
    #[test]
    fn the_published_schema_matches_the_type() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../docs/schemas/portfolio-show-1.schema.json");
        let mut value = serde_json::to_value(schemars::schema_for!(Report)).unwrap();
        value["$id"] = "totalfloat.portfolio-show/1".into();
        value["title"] = "totalfloat.portfolio-show/1".into();
        let text = serde_json::to_string_pretty(&value).unwrap() + "\n";
        if std::env::var_os("TOTALFLOAT_WRITE_SCHEMAS").is_some() {
            std::fs::write(&path, &text).unwrap();
        }
        assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
    }

    /// The capacity schema is its report type's, as `show`'s is.
    #[test]
    fn the_published_capacity_schema_matches_the_type() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../docs/schemas/portfolio-capacity-1.schema.json");
        let mut value = serde_json::to_value(schemars::schema_for!(capacity::Report)).unwrap();
        value["$id"] = "totalfloat.portfolio-capacity/1".into();
        value["title"] = "totalfloat.portfolio-capacity/1".into();
        let text = serde_json::to_string_pretty(&value).unwrap() + "\n";
        if std::env::var_os("TOTALFLOAT_WRITE_SCHEMAS").is_some() {
            std::fs::write(&path, &text).unwrap();
        }
        assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
    }

    #[test]
    fn a_comment_holds_no_control_characters() {
        // TOML forbids them in comments, so a short name with one would break
        // the file `portfolio new` writes.
        let line = format!("# {}\n", comment("A\rB\tC\u{7}D"));
        assert_eq!(line, "# A B C D\n");
        assert!(toml::from_str::<toml::Table>(&line).is_ok());
    }

    #[test]
    fn a_store_point_is_one_file_however_its_path_is_written() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("plant.tfdb"), b"").unwrap();
        let plain = identity(&directory.path().join("plant.tfdb@v1"));
        let dotted = identity(&directory.path().join(".").join("plant.tfdb@v1"));
        assert_eq!(plain, dotted);
        assert!(plain.ends_with("plant.tfdb@v1"), "{plain}");
        assert_ne!(plain, identity(&directory.path().join("plant.tfdb@v2")));
    }

    #[test]
    fn lines_count_from_one() {
        assert_eq!(line_of("a\nb\nc", 0), 1);
        assert_eq!(line_of("a\nb\nc", 2), 2);
        assert_eq!(line_of("a\nb\nc", 4), 3);
    }
}
