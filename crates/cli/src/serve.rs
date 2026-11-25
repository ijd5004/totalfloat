//! `totalfloat serve`: the local app (ARCHITECTURE.md, Local app). The `server` crate is the HTTP
//! shell; this adds the API, built on the same reports as the CLI and MCP.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use axum::Router;
use axum::extract::rejection::QueryRejection;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::http::header::CONTENT_TYPE;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use serde::Serialize;
use totalfloat_check::Thresholds;

use crate::error::{CliError, exit};
use crate::output::print_line;
use crate::{check, explain};

mod assets;
mod feed;
mod filters;
mod find;
mod layouts;
mod shelf;
mod view;
mod write;

use view::{Encoded, Source, View};

/// The query parameters a request gives, by name.
type Params = BTreeMap<String, String>;

/// What every handler shares: the location served, the view of it shown now,
/// and a turn for changes, which are made one at a time.
pub struct App {
    location: PathBuf,
    /// The portfolio file served, if `serve` was given one.
    portfolio: Option<PathBuf>,
    view: RwLock<Arc<View>>,
    writing: tokio::sync::Mutex<()>,
    /// Set while a risk analysis runs: each uses every core, so a second
    /// waits its turn rather than splitting them.
    risk_running: Arc<std::sync::atomic::AtomicBool>,
}

impl App {
    /// An app showing `view`, read from `location`.
    pub fn new(location: PathBuf, view: View) -> Self {
        let view = Arc::new(view);
        assess_aside(&view);
        App {
            location,
            portfolio: None,
            view: RwLock::new(view),
            writing: tokio::sync::Mutex::new(()),
            risk_running: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    /// Shows `view` from now on; requests already running keep theirs.
    fn show(&self, view: View) -> Arc<View> {
        let view = Arc::new(view);
        assess_aside(&view);
        match self.view.write() {
            Ok(mut shown) => *shown = Arc::clone(&view),
            Err(poisoned) => *poisoned.into_inner() = Arc::clone(&view),
        }
        view
    }

    /// The view now. A request keeps the one it started with.
    fn current(&self) -> Arc<View> {
        match self.view.read() {
            Ok(view) => Arc::clone(&view),
            Err(poisoned) => Arc::clone(&poisoned.into_inner()),
        }
    }
}

/// Assesses a held view's quality checks on a thread of their own, so the
/// app's first edit preview at this head, which asks for them, needn't wait
/// for the "before" side. A preview that comes sooner waits for this
/// one assessment rather than making its own.
fn assess_aside(view: &Arc<View>) {
    if view.held().is_none() {
        return;
    }
    let view = Arc::clone(view);
    let spawned = std::thread::Builder::new()
        .name("assess-held".to_owned())
        .spawn(move || view.assess_held());
    // Without the thread, the first preview assesses it instead.
    drop(spawned);
}

/// The view of `location` as it is now, if it points somewhere other than
/// `shown` does: `None` when nothing moved, so nothing is calculated. After a
/// recorded edit, `after` stands for calculating its head again.
///
/// # Errors
///
/// What reading the store refuses, or a view that can't be calculated.
fn moved(
    location: &Path,
    shown: &View,
    after: Option<view::After>,
) -> Result<Option<View>, CliError> {
    if View::point_of(location)? == shown.point() {
        return Ok(None);
    }
    View::open_after(location, after).map(Some)
}

/// The API's routes, under `/api/1/`.
pub fn api(app: Arc<App>) -> Router {
    Router::new()
        .route("/api/1/summary", get(summary))
        .route("/api/1/rows", get(rows))
        .route("/api/1/links", get(links))
        .route("/api/1/wbs", get(wbs))
        .route("/api/1/explain", get(explain))
        .route("/api/1/check", get(check))
        .route("/api/1/usage", get(usage))
        .route("/api/1/risk", post(risk))
        .route("/api/1/portfolio", get(portfolio_show))
        .route("/api/1/portfolio/capacity", get(portfolio_capacity))
        .route("/api/1/portfolio/open", post(portfolio_open))
        .route("/api/1/query", get(find::query))
        .route("/api/1/fields", get(fields))
        .route("/api/1/edit", post(write::post_edit))
        .route("/api/1/history/log", get(write::log))
        .route("/api/1/history/show/{*at}", get(write::show))
        .route("/api/1/history/diff", get(write::diff))
        .route("/api/1/history/proposals", get(write::proposals))
        .route("/api/1/history/export", get(write::export))
        .route("/api/1/history/revert", post(write::revert))
        .route("/api/1/history/branch", post(write::branch))
        .route("/api/1/history/baseline", post(write::baseline))
        .route("/api/1/history/merge", post(write::merge))
        .route("/api/1/history/accept", post(write::accept))
        .route("/api/1/history/reject", post(write::reject))
        .route("/api/1/layouts", get(layouts::get_list))
        .route(
            "/api/1/layouts/{name}",
            get(layouts::get_one)
                .put(layouts::put_one)
                .delete(layouts::delete_one),
        )
        .route("/api/1/filters", get(filters::get_list))
        .route(
            "/api/1/filters/{name}",
            get(filters::get_one)
                .put(filters::put_one)
                .delete(filters::delete_one),
        )
        .with_state(app)
}

/// Runs the command: calculates the schedule, prints the URL, and serves until
/// Ctrl-C.
pub fn run(location: &Path, port: Option<u16>, open: bool) -> Result<u8, CliError> {
    let failed = |hint: &str, error: &dyn std::fmt::Display| {
        CliError::new("serve-failed", format!("the server stopped: {error}"), hint)
    };
    let portfolio = is_portfolio(location);
    let view = if portfolio {
        first_of_portfolio(location)?
    } else {
        View::open(location)?
    };
    if !view.writable() {
        print_line(&format!("{} is shown read-only.", view.label()));
    }
    let mut app = App::new(location.to_owned(), view);
    if portfolio {
        app.portfolio = Some(location.to_owned());
    }
    let app = Arc::new(app);
    // A portfolio's schedules are shown read-only, so nothing changes to follow.
    let feed = if portfolio {
        None
    } else {
        feed::Feed::new(&app)?
    };
    let events = totalfloat_server::Events::default();
    let bound = assets::with_ui(totalfloat_server::Server::new(api(Arc::clone(&app))))
        .with_events(events.clone())
        .bind(port)
        .map_err(|e| {
            failed(
                "check that nothing else is using the port, or leave --port out",
                &e,
            )
        })?;
    let url = bound.url();
    print_line(&format!("TotalFloat is serving {} at", location.display()));
    print_line(&format!("  {url}"));
    print_line("The link signs a browser in; treat it as a password. Press Ctrl-C to stop.");
    if open && let Err(error) = ::open::that_detached(&url) {
        eprintln!("note: couldn't open a browser ({error}); open the link above instead");
    }
    // The feed looks at the store on its own thread until the server stops.
    let (stop, stopping) = std::sync::mpsc::channel::<()>();
    let watcher = feed.map(|feed| std::thread::spawn(move || feed.run(&app, &events, &stopping)));
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| failed("the system refused to start the server's threads", &e))?;
    runtime
        .block_on(bound.serve(async {
            // Without a Ctrl-C handler, serve until the process is stopped.
            if tokio::signal::ctrl_c().await.is_err() {
                std::future::pending::<()>().await;
            }
        }))
        .map_err(|e| failed("the listener failed; start `serve` again", &e))?;
    drop(stop);
    if let Some(watcher) = watcher {
        let _ = watcher.join();
    }
    print_line("Stopped.");
    Ok(exit::OK)
}

/// A JSON answer, or a 500 if it couldn't be serialized.
fn json_bytes(encoded: Encoded) -> Response {
    match encoded {
        Ok(bytes) => ([(CONTENT_TYPE, "application/json")], bytes).into_response(),
        Err(error) => refusal(&CliError::new(
            "serve-failed",
            format!("the answer couldn't be written as JSON: {error}"),
            "this is a bug; the schedule shown is unaffected",
        )),
    }
}

fn json(value: &impl Serialize) -> Response {
    json_bytes(
        serde_json::to_vec(value)
            .map(Into::into)
            .map_err(|error| error.to_string()),
    )
}

/// Request errors that are the caller's to fix.
const BAD_REQUEST: [&str; 17] = [
    "bad-layout",
    "bad-layout-name",
    "bad-filter",
    "bad-filter-name",
    "bad-query",
    "bad-body",
    "unknown-argument",
    "missing-argument",
    "bad-field",
    "bad-project",
    "bad-limit",
    "bad-all",
    "no-history",
    "ambiguous-id",
    "name-needed",
    "reason-needed",
    "wrong-ref-kind",
];

/// Failures of the server or its files, not of the request.
const SERVER_FAULT: [&str; 6] = [
    "serve-failed",
    "tool-failed",
    "store-error",
    "store-damaged",
    "file-not-readable",
    "not-an-xer",
];

/// A refusal, as `totalfloat.error/1`: 404 for a name that matches nothing,
/// 409 for an edit on a branch that moved since the page saw it, 403 for a
/// change to something read-only, 400 for a request to fix, 500 for the
/// server's own failure, and 422 for a schedule that can't give the answer,
/// such as one that can't be calculated or a batch of refused edits.
fn refusal(error: &CliError) -> Response {
    let status = if SERVER_FAULT.contains(&error.code) {
        // The server's own failure, even one that lists what it found.
        StatusCode::INTERNAL_SERVER_ERROR
    } else if !error.problems.is_empty() {
        // A refused batch, whatever its one problem was.
        StatusCode::UNPROCESSABLE_ENTITY
    } else if error.code.starts_with("no-such-") {
        StatusCode::NOT_FOUND
    } else if ["branch-moved", "not-checked-out", "branch-shown"].contains(&error.code) {
        StatusCode::CONFLICT
    } else if error.code == "read-only" {
        StatusCode::FORBIDDEN
    } else if BAD_REQUEST.contains(&error.code) {
        StatusCode::BAD_REQUEST
    } else {
        StatusCode::UNPROCESSABLE_ENTITY
    };
    let document = serde_json::json!({ "schema": "totalfloat.error/1", "error": error });
    (
        status,
        [(CONTENT_TYPE, "application/json")],
        document.to_string(),
    )
        .into_response()
}

/// The query parameters, refusing any not in `allowed`.
fn checked(
    params: Result<Query<Params>, QueryRejection>,
    allowed: &[&str],
) -> Result<Params, CliError> {
    let Query(params) = params.map_err(|rejection| {
        CliError::new(
            "bad-query",
            format!("the query couldn't be read: {rejection}"),
            "parameters are name=value pairs, percent-encoded",
        )
    })?;
    if let Some(name) = params.keys().find(|name| !allowed.contains(&name.as_str())) {
        return Err(CliError::new(
            "unknown-argument",
            format!("this endpoint has no parameter {name}"),
            if allowed.is_empty() {
                "it takes none".to_owned()
            } else {
                format!("it takes: {}", allowed.join(", "))
            },
        ));
    }
    Ok(params)
}

/// Runs `work` on the view a request reads (`?at=` or the current one), off
/// the async runtime, after checking the parameters against `allowed`.
async fn on_view(
    app: Arc<App>,
    params: Result<Query<Params>, QueryRejection>,
    allowed: &'static [&'static str],
    work: impl FnOnce(&View, &Params) -> Result<Response, CliError> + Send + 'static,
) -> Response {
    let mut takes = allowed.to_vec();
    takes.push("at");
    let params = match checked(params, &takes) {
        Ok(params) => params,
        Err(error) => return refusal(&error),
    };
    let view = app.current();
    let answer = tokio::task::spawn_blocking(move || match params.get("at") {
        Some(at) => work(&elsewhere(&view, at)?, &params),
        None => work(&view, &params),
    })
    .await;
    match answer {
        Ok(Ok(response)) => response,
        Ok(Err(error)) => refusal(&error),
        Err(_) => refusal(&CliError::new(
            "serve-failed",
            "the request stopped unexpectedly",
            "this is a bug; the schedule shown is unaffected",
        )),
    }
}

/// The view of another point in the same store, for `?at=`.
fn elsewhere(view: &View, at: &str) -> Result<View, CliError> {
    match view.source() {
        Source::Store { path, .. } => {
            View::at(format!("{}@{at}", path.display()), path, Some(at)).map(View::elsewhere)
        }
        Source::File(file) => Err(CliError::new(
            "no-history",
            format!(
                "{} is a file, which has no history to read `at`",
                file.display()
            ),
            "serve a history store (`totalfloat history init`) to read other points",
        )),
    }
}

async fn summary(
    State(app): State<Arc<App>>,
    params: Result<Query<Params>, QueryRejection>,
) -> Response {
    let portfolio = app.portfolio.clone();
    on_view(app, params, &[], move |view, _| {
        Ok(json(&view.summary().in_portfolio(portfolio.as_deref())))
    })
    .await
}

/// `GET /api/1/fields`: every activity code type, value and user field.
async fn fields(
    State(app): State<Arc<App>>,
    params: Result<Query<Params>, QueryRejection>,
) -> Response {
    on_view(app, params, &[], |view, _| {
        Ok(json(&crate::inspect::fields(&view.loaded.schedules)))
    })
    .await
}

async fn rows(
    State(app): State<Arc<App>>,
    params: Result<Query<Params>, QueryRejection>,
) -> Response {
    on_view(app, params, &[], |view, _| Ok(json_bytes(view.rows()))).await
}

async fn links(
    State(app): State<Arc<App>>,
    params: Result<Query<Params>, QueryRejection>,
) -> Response {
    on_view(app, params, &[], |view, _| Ok(json_bytes(view.links()))).await
}

async fn wbs(
    State(app): State<Arc<App>>,
    params: Result<Query<Params>, QueryRejection>,
) -> Response {
    on_view(app, params, &[], |view, _| Ok(json_bytes(view.wbs()))).await
}

async fn explain(
    State(app): State<Arc<App>>,
    params: Result<Query<Params>, QueryRejection>,
) -> Response {
    on_view(app, params, &["activity", "field", "project"], |view, params| {
        let Some(activity) = params.get("activity") else {
            return Err(CliError::new(
                "missing-argument",
                "explain needs an activity",
                "add ?activity=CODE, with the Activity ID",
            ));
        };
        let field = match params.get("field") {
            None => explain::Field::EarlyStart,
            Some(text) => serde_json::from_value(serde_json::Value::from(text.as_str())).map_err(|_| {
                CliError::new(
                    "bad-field",
                    format!("explain has no field {text}"),
                    "field is early_start, early_finish, late_start, late_finish or total_float",
                )
            })?,
        };
        let project = project(params)?;
        let (report, _) = explain::report(view.label(), &view.loaded, activity, field, project)?;
        Ok(json(&report))
    })
    .await
}

async fn usage(
    State(app): State<Arc<App>>,
    params: Result<Query<Params>, QueryRejection>,
) -> Response {
    on_view(
        app,
        params,
        &["resource", "by", "project"],
        |view, params| {
            let Some(resource) = params.get("resource") else {
                return Err(CliError::new(
                    "missing-argument",
                    "usage needs a resource",
                    "add ?resource=CODE, with the resource's short name",
                ));
            };
            let by = match params.get("by") {
                None => crate::usage::By::Week,
                Some(text) => {
                    <crate::usage::By as clap::ValueEnum>::from_str(text, true).map_err(|_| {
                        CliError::new(
                            "bad-period",
                            format!("usage has no period {text}"),
                            "by is week or month",
                        )
                    })?
                }
            };
            let report =
                crate::usage::report(view.label(), &view.loaded, resource, by, project(params)?)?;
            Ok(json(&report))
        },
    )
    .await
}

async fn check(
    State(app): State<Arc<App>>,
    params: Result<Query<Params>, QueryRejection>,
) -> Response {
    on_view(app, params, &["project"], |view, params| {
        let report = check::report(
            view.label(),
            &view.loaded,
            project(params)?,
            (&Thresholds::default(), &check::Own::default()),
        )?;
        Ok(json(&report))
    })
    .await
}

/// What `POST /api/1/risk` takes: `totalfloat risk`'s inputs and settings.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RiskBody {
    #[serde(default)]
    inputs: crate::risk::InputsFile,
    project: Option<u64>,
    iterations: Option<u32>,
    seed: Option<u64>,
    target: Option<String>,
}

/// Clears [`App`]'s risk flag when the analysis ends, however it ends.
struct RiskTurn(Arc<std::sync::atomic::AtomicBool>);

impl Drop for RiskTurn {
    fn drop(&mut self) {
        self.0.store(false, std::sync::atomic::Ordering::Release);
    }
}

/// A schedule risk analysis of what's shown. It calculates, but
/// changes nothing, so it records no changeset.
async fn risk(State(app): State<Arc<App>>, body: axum::body::Bytes) -> Response {
    let view = app.current();
    if app
        .risk_running
        .swap(true, std::sync::atomic::Ordering::AcqRel)
    {
        return refusal(&CliError::new(
            "risk-busy",
            "another schedule risk analysis is running",
            "run this one when it has finished",
        ));
    }
    let turn = RiskTurn(Arc::clone(&app.risk_running));
    let answer = tokio::task::spawn_blocking(move || -> Result<Response, CliError> {
        let _turn = turn;
        let body: RiskBody = serde_json::from_slice(&body).map_err(|e| {
            CliError::new(
                "bad-body",
                format!("the risk request doesn't fit: {e}"),
                "send {\"inputs\": {...}, \"iterations\": 1000}; docs/schemas/risk-inputs-1.schema.json gives the inputs",
            )
        })?;
        let run = crate::risk::Run {
            project: body.project,
            iterations: body
                .iterations
                .unwrap_or(1_000)
                .clamp(1, crate::risk::MAX_TOOL_ITERATIONS),
            seed: body.seed.unwrap_or(1),
            target: body.target,
        };
        let report = crate::risk::report(view.label(), &view.loaded, &body.inputs, &run)?;
        Ok(json(&report))
    })
    .await;
    match answer {
        Ok(Ok(response)) => response,
        Ok(Err(error)) => refusal(&error),
        Err(_) => refusal(&CliError::new(
            "serve-failed",
            "the request stopped unexpectedly",
            "this is a bug; the schedule shown is unaffected",
        )),
    }
}

/// Whether `location` is a portfolio file rather than a schedule.
fn is_portfolio(location: &Path) -> bool {
    location
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("toml"))
}

/// The portfolio's first schedule, read-only, to show first.
fn first_of_portfolio(location: &Path) -> Result<View, CliError> {
    let read = crate::portfolio::read(location)?;
    let file = read.files().into_iter().next().ok_or_else(|| {
        CliError::new(
            "portfolio-empty",
            format!("{} places no project", location.display()),
            "add a [[project]] to it; docs/guide/portfolio.md shows how",
        )
    })?;
    open_in_portfolio(location, &file)
}

/// A schedule of the portfolio at `location`, as the portfolio names it,
/// shown read-only.
fn open_in_portfolio(location: &Path, file: &str) -> Result<View, CliError> {
    let base = location.parent().unwrap_or(Path::new(""));
    Ok(View::open(&base.join(file))?.elsewhere())
}

/// The portfolio served, or why there isn't one.
fn served_portfolio(app: &App) -> Result<PathBuf, CliError> {
    app.portfolio.clone().ok_or_else(|| {
        CliError::new(
            "not-a-portfolio",
            "this server shows one schedule, not a portfolio",
            "serve a portfolio file to see its projects: totalfloat serve plan.portfolio.toml",
        )
    })
}

/// Runs `work` on a blocking thread, answering its JSON or its refusal.
async fn blocking(work: impl FnOnce() -> Result<Response, CliError> + Send + 'static) -> Response {
    match tokio::task::spawn_blocking(work).await {
        Ok(Ok(response)) => response,
        Ok(Err(error)) => refusal(&error),
        Err(_) => refusal(&CliError::new(
            "serve-failed",
            "the request stopped unexpectedly",
            "this is a bug; the schedule shown is unaffected",
        )),
    }
}

/// `GET /api/1/portfolio`: `portfolio show --json` of the portfolio served,
/// with each file calculated as the grid shows it.
async fn portfolio_show(State(app): State<Arc<App>>) -> Response {
    let path = match served_portfolio(&app) {
        Ok(path) => path,
        Err(error) => return refusal(&error),
    };
    blocking(move || {
        let read = crate::portfolio::read_calculated(&path)?;
        Ok(json(&crate::portfolio::show(
            &path.display().to_string(),
            &read,
        )))
    })
    .await
}

/// `GET /api/1/portfolio/capacity?by=week&resource=CREW`: `portfolio
/// capacity --json` of the portfolio served.
async fn portfolio_capacity(
    State(app): State<Arc<App>>,
    params: Result<Query<Params>, QueryRejection>,
) -> Response {
    let params = match checked(params, &["by", "resource", "role"]) {
        Ok(params) => params,
        Err(error) => return refusal(&error),
    };
    let path = match served_portfolio(&app) {
        Ok(path) => path,
        Err(error) => return refusal(&error),
    };
    blocking(move || {
        let by = match params.get("by") {
            None => crate::usage::By::Week,
            Some(text) => {
                <crate::usage::By as clap::ValueEnum>::from_str(text, true).map_err(|_| {
                    CliError::new(
                        "bad-period",
                        format!("capacity has no period {text}"),
                        "by is week or month",
                    )
                })?
            }
        };
        let read = crate::portfolio::read_keeping(&path)?;
        let report = crate::portfolio::capacity::report(
            &path.display().to_string(),
            &read,
            by,
            (
                params.get("resource").map(String::as_str),
                params.get("role").map(String::as_str),
            ),
        )?;
        Ok(json(&report))
    })
    .await
}

/// What `POST /api/1/portfolio/open` takes: a schedule file as the
/// portfolio names it.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct OpenBody {
    file: String,
}

/// Shows another of the portfolio's schedules, read-only, and answers its
/// summary. Nothing is changed or recorded.
async fn portfolio_open(State(app): State<Arc<App>>, body: axum::body::Bytes) -> Response {
    let path = match served_portfolio(&app) {
        Ok(path) => path,
        Err(error) => return refusal(&error),
    };
    let shown = Arc::clone(&app);
    blocking(move || {
        let body: OpenBody = serde_json::from_slice(&body).map_err(|e| {
            CliError::new(
                "bad-body",
                format!("the open request doesn't fit: {e}"),
                "send {\"file\": \"plant.xer\"}, a file the portfolio names",
            )
        })?;
        let read = crate::portfolio::read(&path)?;
        if !read.files().contains(&body.file) {
            return Err(CliError::new(
                "not-in-portfolio",
                format!("{} isn't one of the portfolio's files", body.file),
                "`GET /api/1/portfolio` lists its projects and their files",
            ));
        }
        let view = shown.show(open_in_portfolio(&path, &body.file)?);
        Ok(json(&view.summary().in_portfolio(Some(&path))))
    })
    .await
}

/// The `project` parameter, a `proj_id`.
fn project(params: &Params) -> Result<Option<u64>, CliError> {
    params
        .get("project")
        .map(|text| {
            text.parse().map_err(|_| {
                CliError::new(
                    "bad-project",
                    format!("project {text} isn't a proj_id"),
                    "`/api/1/summary` lists each project's id",
                )
            })
        })
        .transpose()
}

#[cfg(test)]
mod tests;
