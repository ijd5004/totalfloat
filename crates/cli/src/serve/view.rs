//! What `serve` shows: a schedule at one point, calculated, and the read
//! endpoints' answers built from it.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use axum::body::Bytes;

use schemars::JsonSchema;
use serde::Serialize;
use totalfloat_model::{ActivityId, Constraint, RelationshipId, Schedule, WbsId};
use totalfloat_store::RefKind;
use totalfloat_xer::RawXer;

use crate::edit::Calculation;
use crate::error::{CliError, read_xer, read_xer_bytes};
use crate::history::{branch_head, open, resolve_point, store_error};
use crate::loaded::Loaded;
use crate::location::{Located, locate};
use crate::output::hours_of;
use crate::query::{constraint_label, critical as stored_critical, status_label, type_label};
use crate::{inspect, schedule};

/// A branch, if the point is one, and the changeset ID it stands at.
pub type Point = (Option<String>, String);

/// The branch `at` names, if it is one, and the changeset it stands at; with
/// no `at`, the checked-out branch.
fn resolve(
    store: &totalfloat_store::Store,
    path: &Path,
    at: Option<&str>,
) -> Result<(Option<String>, totalfloat_store::ChangesetId), CliError> {
    let Some(name) = at else {
        let (branch, id) = branch_head(store, None).map_err(|error| {
            CliError::new(
                error.code,
                error.message,
                format!("serve a branch by name: {}@BRANCH", path.display()),
            )
        })?;
        return Ok((Some(branch), id));
    };
    let branch = store
        .get_ref(name)
        .map_err(store_error)?
        .filter(|found| found.kind == RefKind::Branch)
        .map(|found| found.name);
    Ok((branch, resolve_point(store, path, Some(name))?))
}

/// Where the schedule shown comes from.
#[derive(Debug, Clone)]
pub enum Source {
    /// A plain file, which is read-only.
    File(PathBuf),
    /// A point in a history store.
    Store {
        /// The store file.
        path: PathBuf,
        /// The branch shown, if the point is one; a baseline or changeset is
        /// read-only.
        branch: Option<String>,
        /// The changeset shown.
        head: String,
    },
}

/// A schedule at one point, calculated by `totalfloat schedule`'s rules, with
/// the read endpoints' answers built once on first use. Never changed once made.
pub struct View {
    /// The location as given, for reports.
    label: String,
    source: Source,
    bytes: usize,
    /// The schedule with results from the engine, or as stored if it couldn't
    /// be calculated.
    pub loaded: Loaded,
    /// What an edit here is measured against without reading and calculating
    /// the file again: the snapshot, the file as read, and critical
    /// activities by project. Only for a branch's snapshot that could be
    /// calculated. No file as read means it is `loaded`: the head is a
    /// recorded edit's output, which calculating again leaves as it is.
    held: Option<Held>,
    /// The held view's quality checks, assessed for the first edit preview
    /// that asks for them.
    assessed: crate::edit::Assessed,
    /// Why it couldn't be calculated, if it couldn't.
    refused: Option<CliError>,
    /// Relationships that drive their successor's dates.
    driving: BTreeSet<RelationshipId>,
    /// Activities the engine found critical; `None` when it couldn't
    /// calculate, so the stored float decides.
    critical: Option<BTreeSet<ActivityId>>,
    /// Whether this is the view the server shows, which writes go to, rather
    /// than one read `?at=` another point.
    shown: bool,
    rows: OnceLock<Encoded>,
    links: OnceLock<Encoded>,
    wbs: OnceLock<Encoded>,
}

impl Source {
    /// Whether this is a store's branch, which takes edits.
    fn is_branch(&self) -> bool {
        matches!(
            self,
            Source::Store {
                branch: Some(_),
                ..
            }
        )
    }
}

/// What a branch's view holds for an edit measured against it: the
/// snapshot, the file as read, critical activities by project, and each
/// project's calculation for an edit's to take from.
type Held = (
    totalfloat_store::BlobHash,
    Option<Loaded>,
    BTreeMap<u64, usize>,
    schedule::Kept,
);

/// A recorded edit's changeset and what calculating it found.
pub struct After {
    /// The changeset the edit recorded.
    pub head: String,
    /// Its calculation.
    pub calculation: Calculation,
}

/// An answer serialized once, or why it couldn't be.
pub type Encoded = Result<Bytes, String>;

fn encode(value: &impl Serialize) -> Encoded {
    serde_json::to_vec(value)
        .map(Bytes::from)
        .map_err(|error| error.to_string())
}

impl View {
    /// The view of `location`: a file, a store's checked-out branch, or a
    /// point in a store (`plant.tfdb@what-if`).
    ///
    /// # Errors
    ///
    /// What reading the file or store refuses.
    pub fn open(location: &Path) -> Result<View, CliError> {
        View::open_after(location, None)
    }

    /// [`View::open`] once an edit has been recorded, as [`View::at_after`].
    ///
    /// # Errors
    ///
    /// As [`View::open`].
    pub fn open_after(location: &Path, after: Option<After>) -> Result<View, CliError> {
        let label = location.display().to_string();
        match locate(location) {
            Located::File(file) => Ok(View::new(
                label,
                Source::File(file.clone()),
                (read_xer(&file)?, None),
            )),
            Located::Store { path, at } => View::at_after(label, &path, at.as_deref(), after),
        }
    }

    /// The view of `at` in the store at `path`, or its checked-out branch.
    ///
    /// # Errors
    ///
    /// `no-such-ref`, or what the store refuses.
    pub fn at(label: String, path: &Path, at: Option<&str>) -> Result<View, CliError> {
        View::at_after(label, path, at, None)
    }

    /// [`View::at`] once an edit has been recorded: when the point is still
    /// the changeset `after` names, its calculation stands for calculating
    /// the head again.
    ///
    /// # Errors
    ///
    /// As [`View::at`].
    pub fn at_after(
        label: String,
        path: &Path,
        at: Option<&str>,
        after: Option<After>,
    ) -> Result<View, CliError> {
        let store = open(path)?;
        let (branch, id) = resolve(&store, path, at)?;
        let calculation = after
            .filter(|after| after.head == id.to_string())
            .map(|after| after.calculation);
        let changeset = store.changeset(&id).map_err(store_error)?;
        let bytes = store.blob(&changeset.snapshot).map_err(store_error)?;
        let source = Source::Store {
            path: path.to_owned(),
            branch,
            head: id.to_string(),
        };
        let raw = read_xer_bytes(bytes, path)?;
        Ok(match calculation {
            Some(calculation) => {
                View::calculated(label, source, (raw, changeset.snapshot), calculation)
            }
            None => View::new(label, source, (raw, Some(changeset.snapshot))),
        })
    }

    /// Where `location` points now, without reading the schedule: its branch,
    /// if it is one, and its head. A plain file points nowhere.
    ///
    /// # Errors
    ///
    /// `no-such-ref`, or what the store refuses.
    pub fn point_of(location: &Path) -> Result<Option<Point>, CliError> {
        match locate(location) {
            Located::File(_) => Ok(None),
            Located::Store { path, at } => {
                let store = open(&path)?;
                let (branch, id) = resolve(&store, &path, at.as_deref())?;
                Ok(Some((branch, id.to_string())))
            }
        }
    }

    /// Where this view points: its branch, if it is one, and its head.
    pub fn point(&self) -> Option<Point> {
        match &self.source {
            Source::File(_) => None,
            Source::Store { branch, head, .. } => Some((branch.clone(), head.clone())),
        }
    }

    /// Maps and calculates `raw`. A file that can't be calculated is still
    /// shown, with its stored results and the reason.
    fn new(
        label: String,
        source: Source,
        (raw, snapshot): (RawXer, Option<totalfloat_store::BlobHash>),
    ) -> View {
        let bytes = raw.source().len();
        // Only a branch takes edits, so only a branch's view keeps what
        // they're measured against.
        let editable = source.is_branch();
        let read = Loaded::new(raw);
        let (loaded, driving, critical, refused, held) = match schedule::fresh(&read, None, None) {
            Ok(fresh) => {
                let counts = fresh
                    .projects
                    .iter()
                    .map(|project| (project.id(), project.critical()))
                    .collect();
                (
                    Loaded::new(fresh.raw),
                    fresh.driving,
                    Some(fresh.critical),
                    None,
                    snapshot
                        .filter(|_| editable)
                        .map(|snapshot| (snapshot, Some(read), counts, fresh.kept)),
                )
            }
            Err(error) => (read, BTreeSet::new(), None, Some(error), None),
        };
        View {
            label,
            source,
            bytes,
            loaded,
            held,
            assessed: crate::edit::Assessed::new(),
            refused,
            driving,
            critical,
            shown: true,
            rows: OnceLock::new(),
            links: OnceLock::new(),
            wbs: OnceLock::new(),
        }
    }

    /// A branch's head that a recorded edit just wrote, with that edit's
    /// [`Calculation`]: mapped once, and shown as read, since calculating it
    /// again would leave it as it is.
    fn calculated(
        label: String,
        source: Source,
        (raw, snapshot): (RawXer, totalfloat_store::BlobHash),
        calculation: Calculation,
    ) -> View {
        let bytes = raw.source().len();
        let Calculation {
            driving,
            critical,
            counts,
            kept,
        } = calculation;
        let held = source.is_branch().then_some((snapshot, None, counts, kept));
        View {
            label,
            source,
            bytes,
            loaded: Loaded::new(raw),
            held,
            assessed: crate::edit::Assessed::new(),
            refused: None,
            driving,
            critical: Some(critical),
            shown: true,
            rows: OnceLock::new(),
            links: OnceLock::new(),
            wbs: OnceLock::new(),
        }
    }

    /// The schedule shown, read and calculated, for an edit measured against
    /// it: only a store's snapshot that could be calculated.
    pub fn held(&self) -> Option<crate::edit::Held<'_>> {
        self.held
            .as_ref()
            .map(|(snapshot, read, critical, kept)| crate::edit::Held {
                snapshot,
                read: read.as_ref().unwrap_or(&self.loaded),
                calculated: &self.loaded,
                critical,
                assessed: &self.assessed,
                kept: Some(kept),
            })
    }

    /// Assesses the held view's quality checks now, if it holds one, so the
    /// first edit preview that asks for them finds them kept.
    pub fn assess_held(&self) {
        if self.held.is_some() {
            self.assessed
                .get_or_init(|| crate::edit::assess_each(&self.loaded.schedules));
        }
    }

    /// Whether this view stands on a recorded edit's calculation.
    #[cfg(test)]
    pub(super) fn stands_on_an_edit(&self) -> bool {
        matches!(self.held, Some((_, None, _, _)))
    }

    /// This view as one read `?at=` another point: never written to.
    #[must_use]
    pub fn elsewhere(mut self) -> View {
        self.shown = false;
        self.held = None;
        self
    }

    /// The location as given.
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Where it comes from.
    pub fn source(&self) -> &Source {
        &self.source
    }

    /// Whether edits could be recorded here: only on a store's branch, in
    /// the view the server shows.
    pub fn writable(&self) -> bool {
        self.shown
            && matches!(
                &self.source,
                Source::Store {
                    branch: Some(_),
                    ..
                }
            )
    }

    /// `GET /api/1/summary`: `inspect`'s report, and where the view stands.
    pub fn summary(&self) -> Summary<'_> {
        let (branch, head) = match &self.source {
            Source::File(_) => (None, None),
            Source::Store { branch, head, .. } => (branch.clone(), Some(head.clone())),
        };
        Summary {
            schema: SUMMARY_SCHEMA,
            location: self.label.clone(),
            branch,
            head,
            writable: self.writable(),
            calculated: self.refused.is_none(),
            calculation_refused: self.refused.as_ref(),
            inspect: inspect::report(&self.label, self.bytes, &self.loaded),
            portfolio: None,
        }
    }

    /// `GET /api/1/rows`, serialized once.
    pub fn rows(&self) -> Encoded {
        self.rows
            .get_or_init(|| {
                encode(&rows(
                    &self.label,
                    &self.loaded.schedules,
                    self.critical.as_ref(),
                ))
            })
            .clone()
    }

    /// `GET /api/1/links`, serialized once.
    pub fn links(&self) -> Encoded {
        self.links
            .get_or_init(|| encode(&links(&self.loaded.schedules, &self.driving)))
            .clone()
    }

    /// `GET /api/1/wbs`, serialized once.
    pub fn wbs(&self) -> Encoded {
        self.wbs
            .get_or_init(|| encode(&wbs(&self.loaded.schedules)))
            .clone()
    }
}

const SUMMARY_SCHEMA: &str = "totalfloat.summary/1";

/// `totalfloat.summary/1`, `GET /api/1/summary`: what's served, and
/// `inspect`'s report on it.
#[derive(Debug, Serialize, JsonSchema)]
pub struct Summary<'a> {
    #[schemars(extend("const" = "totalfloat.summary/1"))]
    schema: &'static str,
    /// The file or store served, as given.
    location: String,
    /// The branch shown, on a store; null for a file, a baseline or a
    /// changeset.
    branch: Option<String>,
    /// The changeset shown, on a store.
    head: Option<String>,
    /// Whether edits can be made: a branch of a store.
    writable: bool,
    /// Whether the schedule could be calculated.
    calculated: bool,
    /// Why it couldn't be, when it couldn't.
    #[serde(skip_serializing_if = "Option::is_none")]
    calculation_refused: Option<&'a CliError>,
    /// `inspect --json`'s report.
    inspect: inspect::Report,
    /// The portfolio file served, when `serve` was given one; its schedules
    /// are shown read-only, one at a time.
    #[serde(skip_serializing_if = "Option::is_none")]
    portfolio: Option<String>,
}

impl Summary<'_> {
    /// The summary, served as part of a portfolio.
    #[must_use]
    pub fn in_portfolio(mut self, portfolio: Option<&Path>) -> Self {
        self.portfolio = portfolio.map(|p| p.display().to_string());
        self
    }
}

/// The schema tag of [`Rows`].
#[derive(Debug, Clone, Copy, Default)]
pub struct RowsSchema;

impl RowsSchema {
    const ID: &'static str = "totalfloat.rows/1";
}

impl Serialize for RowsSchema {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(Self::ID)
    }
}

impl JsonSchema for RowsSchema {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "RowsSchema".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({ "const": RowsSchema::ID })
    }
}

/// `totalfloat.rows/1`: every activity, with the fields the grid shows, one
/// array per field so 50,000 rows stay compact. Entry `n` of each array is
/// activity `n`. Activities are in project order, then by `task_id`. Dates
/// are `YYYY-MM-DD HH:MM`; spans and floats are working hours.
#[derive(Debug, Default, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Rows {
    /// Always `totalfloat.rows/1`.
    pub schema: RowsSchema,
    /// The location shown.
    pub file: String,
    /// How many activities: every array has this many entries.
    pub count: usize,
    /// `PROJECT.proj_id`.
    pub project: Vec<u64>,
    /// `TASK.task_id`.
    pub id: Vec<u64>,
    /// Activity ID, `TASK.task_code`.
    pub code: Vec<String>,
    /// `TASK.task_name`.
    pub name: Vec<String>,
    /// WBS codes from the project's node down, joined with `.`: the WBS path.
    pub wbs_path: Vec<Option<String>>,
    /// `task`, `resource`, `loe`, `start-milestone`, `finish-milestone` or
    /// `wbs-summary`.
    #[serde(rename = "type")]
    pub kind: Vec<&'static str>,
    /// `not-started`, `in-progress` or `completed`.
    pub status: Vec<&'static str>,
    /// Original duration, in hours.
    pub original_duration_hours: Vec<f64>,
    /// Remaining duration, in hours.
    pub remaining_duration_hours: Vec<f64>,
    /// Calculated early start.
    pub early_start: Vec<Option<String>>,
    /// Calculated early finish.
    pub early_finish: Vec<Option<String>>,
    /// Calculated late start.
    pub late_start: Vec<Option<String>>,
    /// Calculated late finish.
    pub late_finish: Vec<Option<String>>,
    /// Actual start.
    pub actual_start: Vec<Option<String>>,
    /// Actual finish.
    pub actual_finish: Vec<Option<String>>,
    /// Total float, in hours.
    pub total_float_hours: Vec<Option<f64>>,
    /// Free float, in hours.
    pub free_float_hours: Vec<Option<f64>>,
    /// Critical by the project's own definition.
    pub critical: Vec<bool>,
    /// Calendar name.
    pub calendar: Vec<Option<String>>,
    /// Working hours in a day for the activity's durations given in days: its
    /// calendar's stated hours, or the length of its working days when
    /// they're all the same. Null where days can't be used, as for resource
    /// dependent work, so its spans and float are shown in hours.
    pub hours_per_day: Vec<Option<f64>>,
    /// Primary constraint, such as `start-on-or-after`.
    pub constraint: Vec<Option<&'static str>>,
    /// Primary constraint date.
    pub constraint_date: Vec<Option<String>>,
    /// Secondary constraint.
    pub secondary_constraint: Vec<Option<&'static str>>,
    /// Secondary constraint date.
    pub secondary_constraint_date: Vec<Option<String>>,
}

/// Every activity of `schedules` as columns. `critical` is the engine's
/// finding; without one, the stored float decides.
pub fn rows(file: &str, schedules: &[Schedule], critical: Option<&BTreeSet<ActivityId>>) -> Rows {
    let count = schedules.iter().map(|s| s.activities.len()).sum();
    let mut rows = Rows {
        file: file.to_owned(),
        count,
        ..Rows::default()
    };
    let text = |date: Option<totalfloat_model::DateTime>| date.map(|d| d.to_string());
    let kind = |c: Option<Constraint>| c.map(|c| constraint_label(c.kind));
    let date = |c: Option<Constraint>| text(c.and_then(|c| c.date));
    for schedule in schedules {
        let paths = wbs_paths(schedule);
        for a in schedule.activities.values() {
            let stored = &a.stored;
            rows.project.push(schedule.project.id.0);
            rows.id.push(a.id.0);
            rows.code.push(a.code.clone());
            rows.name.push(a.name.clone());
            rows.wbs_path.push(paths.get(&a.wbs).cloned());
            rows.kind.push(type_label(a.activity_type));
            rows.status.push(status_label(a.status));
            rows.original_duration_hours
                .push(hours_of(a.original_duration));
            rows.remaining_duration_hours
                .push(hours_of(a.remaining_duration));
            rows.early_start.push(text(stored.early_start));
            rows.early_finish.push(text(stored.early_finish));
            rows.late_start.push(text(stored.late_start));
            rows.late_finish.push(text(stored.late_finish));
            rows.actual_start.push(text(a.actual_start));
            rows.actual_finish.push(text(a.actual_finish));
            rows.total_float_hours
                .push(stored.total_float.map(hours_of));
            rows.free_float_hours.push(stored.free_float.map(hours_of));
            rows.critical.push(match critical {
                Some(found) => found.contains(&a.id),
                None => stored_critical(schedule, a),
            });
            rows.calendar
                .push(schedule.calendars.get(&a.calendar).map(|c| c.name.clone()));
            rows.hours_per_day
                .push(totalfloat_edit::duration_day_length(schedule, a).map(hours_of));
            rows.constraint.push(kind(a.primary_constraint));
            rows.constraint_date.push(date(a.primary_constraint));
            rows.secondary_constraint.push(kind(a.secondary_constraint));
            rows.secondary_constraint_date
                .push(date(a.secondary_constraint));
        }
    }
    rows
}

/// Each WBS node's path of codes from the top, joined with `.`. A node whose
/// parents loop gets the path as far as the loop.
fn wbs_paths(schedule: &Schedule) -> BTreeMap<WbsId, String> {
    let mut paths = BTreeMap::new();
    for &id in schedule.wbs.keys() {
        let mut codes = Vec::new();
        let mut seen = BTreeSet::new();
        let mut at = Some(id);
        while let Some(node) = at.and_then(|id| schedule.wbs.get(&id)) {
            if !seen.insert(node.id) {
                break;
            }
            codes.push(node.code.as_str());
            at = node.parent;
        }
        codes.reverse();
        paths.insert(id, codes.join("."));
    }
    paths
}

/// `totalfloat.links/1`, `GET /api/1/links`: every relationship, one array
/// per field.
#[derive(Debug, Default, Serialize, schemars::JsonSchema)]
pub struct Links {
    #[schemars(extend("const" = "totalfloat.links/1"))]
    schema: &'static str,
    /// How many links: every array has this many entries.
    pub count: usize,
    /// `TASKPRED.task_pred_id`.
    pub id: Vec<u64>,
    /// The predecessor's `task_id`.
    pub predecessor: Vec<u64>,
    /// The successor's `task_id`.
    pub successor: Vec<u64>,
    /// `FS`, `SS`, `FF` or `SF`.
    #[serde(rename = "type")]
    pub kind: Vec<&'static str>,
    /// Lag, in hours.
    pub lag_hours: Vec<f64>,
    /// Whether it drives the successor's dates.
    pub driving: Vec<bool>,
}

fn links(schedules: &[Schedule], driving: &BTreeSet<RelationshipId>) -> Links {
    // A link between projects is held by both, so it's listed once.
    let all: BTreeMap<RelationshipId, &totalfloat_model::Relationship> = schedules
        .iter()
        .flat_map(|s| s.relationships.iter().map(|(id, link)| (*id, link)))
        .collect();
    let mut links = Links {
        schema: "totalfloat.links/1",
        count: all.len(),
        ..Links::default()
    };
    for (id, link) in all {
        links.id.push(id.0);
        links.predecessor.push(link.predecessor.0);
        links.successor.push(link.successor.0);
        links.kind.push(crate::query::link_label(link.kind));
        links.lag_hours.push(hours_of(link.lag));
        links.driving.push(driving.contains(&id));
    }
    links
}

/// A WBS node and the nodes under it.
#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct Node {
    /// `PROJWBS.wbs_id`.
    pub id: u64,
    /// `PROJWBS.wbs_short_name`.
    pub code: String,
    /// `PROJWBS.wbs_name`.
    pub name: String,
    /// In `seq_num` order, then by code.
    pub children: Vec<Node>,
}

/// `totalfloat.wbs/1`, `GET /api/1/wbs`: each project's WBS tree.
#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct Wbs {
    #[schemars(extend("const" = "totalfloat.wbs/1"))]
    schema: &'static str,
    /// In the order the file lists the projects.
    pub projects: Vec<ProjectWbs>,
}

/// One project's WBS.
#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct ProjectWbs {
    /// `PROJECT.proj_id`.
    pub project: u64,
    /// `PROJECT.proj_short_name`.
    pub code: String,
    /// The top nodes, usually the project's own.
    pub nodes: Vec<Node>,
}

fn wbs(schedules: &[Schedule]) -> Wbs {
    let projects = schedules
        .iter()
        .map(|schedule| {
            let mut children: BTreeMap<Option<WbsId>, Vec<&totalfloat_model::Wbs>> =
                BTreeMap::new();
            for node in schedule.wbs.values() {
                // A parent outside the file makes the node a top one.
                let parent = node.parent.filter(|p| schedule.wbs.contains_key(p));
                children.entry(parent).or_default().push(node);
            }
            for list in children.values_mut() {
                list.sort_by(|a, b| (a.sequence, &a.code, a.id).cmp(&(b.sequence, &b.code, b.id)));
            }
            ProjectWbs {
                project: schedule.project.id.0,
                code: schedule.project.short_name.clone(),
                nodes: tree(&children, None),
            }
        })
        .collect();
    Wbs {
        schema: "totalfloat.wbs/1",
        projects,
    }
}

/// The nodes under `parent`. Each node has one parent, so a loop of parents
/// can't be reached from the top and is left out rather than followed.
fn tree(
    children: &BTreeMap<Option<WbsId>, Vec<&totalfloat_model::Wbs>>,
    parent: Option<WbsId>,
) -> Vec<Node> {
    children.get(&parent).map_or_else(Vec::new, |nodes| {
        nodes
            .iter()
            .map(|node| Node {
                id: node.id.0,
                code: node.code.clone(),
                name: node.name.clone(),
                children: tree(children, Some(node.id)),
            })
            .collect()
    })
}
