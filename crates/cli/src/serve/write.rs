//! `serve`'s write side and its history: each change goes through the
//! history store as a changeset authored by the person using the app
//! (ARCHITECTURE.md, Local app), and each history endpoint runs the same command as its MCP tool.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::rejection::QueryRejection;
use axum::extract::{Path as UrlPath, Query, State};
use axum::http::StatusCode;
use axum::http::header::{CONTENT_DISPOSITION, CONTENT_TYPE, HeaderName, HeaderValue};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};
use totalfloat_edit::Edit;

use super::view::{After, Source, View};
use super::{App, Params, checked, json, refusal};
use crate::edit;
use crate::error::{CliError, exit};
use crate::history::{
    Action, Asked, Pin, as_person, branch_head, open, pinned, snapshot_at, store_error,
};
use crate::mcp::stores::{
    DiffParams, LogParams, MergeParams, RefAction, RefParams, RevertParams, ReviewParams,
    ShowParams, StoreParams, answer_as_person,
};

/// The header that carries the head the view shows after a change.
pub const HEAD_HEADER: &str = "x-totalfloat-head";

/// `totalfloat.edit-request/1`, the body of `POST /api/1/edit`: a
/// `totalfloat.command/1` batch and how to record it.
#[allow(
    clippy::struct_excessive_bools,
    reason = "independent request flags, as in MCP's edit tool"
)]
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EditBody {
    /// The edits.
    edits: Vec<Edit>,
    /// The changeset the page last saw. If the branch has moved past it, the
    /// edit is refused with `branch-moved`.
    head: Option<String>,
    /// Why.
    message: Option<String>,
    /// Report what would happen, and record nothing.
    #[serde(default)]
    dry_run: bool,
    /// Apply the edits that can be applied, and skip the rest.
    #[serde(default)]
    skip_failed: bool,
    /// Record it as a proposal for someone to accept or reject.
    #[serde(default)]
    propose: bool,
    /// Compare quality checks before and after; refuse if one newly fails.
    #[serde(default)]
    check: bool,
    /// Only this project (`proj_id`).
    project: Option<u64>,
    /// Move the data date too, as `YYYY-MM-DD HH:MM`.
    data_date: Option<String>,
    /// With `data_date`, record the progress planned before it first.
    #[serde(default)]
    as_planned: bool,
}

/// What a request about the store shown needs: the store, and the branch and
/// head shown.
struct Shown {
    store: PathBuf,
    branch: Option<String>,
    head: String,
}

/// The store a view comes from, or why a request needing one can't be done.
/// A change needs a branch: a plain file, a baseline or a changeset is
/// read-only.
fn shown(view: &View, changing: bool) -> Result<Shown, CliError> {
    match view.source() {
        Source::Store { path, branch, head } => {
            // A store shown from a portfolio is read-only even on a branch
            //: only `writable` views take changes.
            if changing && branch.is_some() && !view.writable() {
                return Err(CliError::new(
                    "read-only",
                    format!("{} is shown read-only", view.label()),
                    "serve the store on its own to change it",
                ));
            }
            if changing && branch.is_none() {
                return Err(CliError::new(
                    "read-only",
                    format!(
                        "{} isn't a branch, so nothing is changed from it",
                        view.label()
                    ),
                    "serve a branch, such as plant.tfdb@main",
                ));
            }
            Ok(Shown {
                store: path.clone(),
                branch: branch.clone(),
                head: head.clone(),
            })
        }
        Source::File(file) if changing => Err(CliError::new(
            "read-only",
            format!(
                "{} is a plain file, so changes can't be recorded",
                file.display()
            ),
            "start a history with `totalfloat history init`, then serve the store",
        )),
        Source::File(file) => Err(CliError::new(
            "no-history",
            format!("{} is a file, which has no history", file.display()),
            "start one with `totalfloat history init`, then serve the store",
        )),
    }
}

fn bad_body(message: String) -> CliError {
    CliError::new(
        "bad-body",
        message,
        "send a JSON object with the arguments of the matching MCP tool, without `store`",
    )
}

/// A JSON object body; an empty body is an empty object.
fn object_of(body: &[u8]) -> Result<Map<String, Value>, CliError> {
    if body.is_empty() {
        return Ok(Map::new());
    }
    match serde_json::from_slice(body) {
        Ok(Value::Object(object)) => Ok(object),
        Ok(_) => Err(bad_body("the body isn't a JSON object".to_owned())),
        Err(e) => Err(bad_body(format!("the body isn't JSON: {e}"))),
    }
}

/// Reads a body's arguments as `T`, with the served store filled in as
/// `store`.
fn with_store<T: DeserializeOwned>(
    mut object: Map<String, Value>,
    store: &Path,
) -> Result<T, CliError> {
    if object.contains_key("store") {
        return Err(bad_body(
            "`store` is the one being served, so it isn't given".to_owned(),
        ));
    }
    let Some(store) = store.to_str() else {
        return Err(CliError::new(
            "store-error",
            format!(
                "{} isn't valid UTF-8, so history commands can't name it",
                store.display()
            ),
            "serve the store from a path that's valid UTF-8",
        ));
    };
    object.insert("store".to_owned(), Value::from(store));
    serde_json::from_value(Value::Object(object))
        .map_err(|e| bad_body(format!("the body doesn't fit: {e}")))
}

/// A history command's answer. A damaged store is a 500, with what's wrong.
fn answered((code, document): (u8, Value)) -> Response {
    let mut response = json(&document);
    if code == exit::ERROR {
        *response.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
    }
    response
}

fn stopped() -> CliError {
    CliError::new(
        "serve-failed",
        "the request stopped unexpectedly",
        "this is a bug; reload to see what the store holds",
    )
}

/// Runs a read of the store shown off the async runtime.
async fn read_history(
    app: Arc<App>,
    action: impl FnOnce(&Shown) -> Result<Action, CliError> + Send + 'static,
) -> Response {
    let view = app.current();
    let answer =
        tokio::task::spawn_blocking(move || answer_as_person(&action(&shown(&view, false)?)?))
            .await;
    match answer {
        Ok(Ok(answer)) => answered(answer),
        Ok(Err(error)) => refusal(&error),
        Err(_) => refusal(&stopped()),
    }
}

/// Makes a change, one at a time: runs `work` on the view shown now, then, if
/// the location served now points elsewhere, calculates and shows the new
/// point. The head shown comes back in [`HEAD_HEADER`].
///
/// It runs as its own task, so a page that goes away mid-change can't leave
/// it half done: the turn is held, and the new view shown, either way.
async fn change(
    app: Arc<App>,
    work: impl FnOnce(&View) -> Result<Response, CliError> + Send + 'static,
) -> Response {
    change_after(app, move |view| (work(view), None)).await
}

/// [`change`], where `work` may also give what a recorded edit's calculation
/// found, so the view after it needn't calculate the new head again.
async fn change_after(
    app: Arc<App>,
    work: impl FnOnce(&View) -> (Result<Response, CliError>, Option<After>) + Send + 'static,
) -> Response {
    let task = tokio::spawn(async move {
        let _turn = app.writing.lock().await;
        let view = app.current();
        let location = app.location.clone();
        // A portfolio's location is its file, not a schedule, and its views
        // never change, so there's nothing to follow.
        let follows = app.portfolio.is_none();
        let done = tokio::task::spawn_blocking(move || {
            let (answer, after) = work(&view);
            if !follows {
                return (answer, None);
            }
            // Refused or not, the branch may have moved, here or from the CLI
            // or MCP, so the page can reload and see it. What changed is
            // recorded either way; a view that can't be rebuilt stays as it
            // was until the next change.
            let fresh = super::moved(&location, &view, after).unwrap_or_else(|error| {
                eprintln!(
                    "note: couldn't show {} as it is now ({}); the page shows it as before",
                    location.display(),
                    error.message
                );
                None
            });
            (answer, fresh)
        })
        .await;
        match done {
            Ok((answer, fresh)) => {
                let view = fresh.map_or_else(|| app.current(), |fresh| app.show(fresh));
                let mut response = match answer {
                    Ok(response) => response,
                    Err(error) => return refusal(&error),
                };
                if let Some((_, head)) = view.point()
                    && let Ok(value) = HeaderValue::from_str(&head)
                {
                    response
                        .headers_mut()
                        .insert(HeaderName::from_static(HEAD_HEADER), value);
                }
                response
            }
            Err(_) => refusal(&stopped()),
        }
    });
    task.await.unwrap_or_else(|_| refusal(&stopped()))
}

/// The head the page last saw, if it sent one.
fn seen_head(object: &mut Map<String, Value>) -> Result<Option<String>, CliError> {
    match object.remove("head") {
        None => Ok(None),
        Some(Value::String(head)) => Ok(Some(head)),
        Some(_) => Err(bad_body("`head` is a changeset ID, as a string".to_owned())),
    }
}

/// Refuses a change against a head the page no longer sees, checked against
/// the branch as stored now, not the view cached, so a change recorded by the
/// CLI or MCP since the page loaded is caught too. A change that records
/// checks again as it does, under [`pinned`].
fn guard_head(seen: Option<&str>, shown: &Shown) -> Result<(), CliError> {
    let Some(seen) = seen else {
        return Ok(());
    };
    let store = open(&shown.store)?;
    let (_, live) = branch_head(&store, shown.branch.as_deref())?;
    if seen == live.to_string() {
        return Ok(());
    }
    Err(CliError::new(
        "branch-moved",
        format!(
            "{} has moved on since the page saw it",
            shown.branch.as_deref().unwrap_or("the branch")
        ),
        "reload to see what changed, then try again",
    ))
}

/// Refuses a change the CLI makes on the checked-out branch, such as a revert
/// or merge, unless that's the branch shown.
fn guard_checked_out(shown: &Shown) -> Result<(), CliError> {
    let store = open(&shown.store)?;
    let current = store.current().map_err(store_error)?;
    if current.is_some() && current == shown.branch {
        return Ok(());
    }
    Err(CliError::new(
        "not-checked-out",
        format!(
            "this acts on the checked-out branch, {}, not {}, the one shown",
            current.as_deref().unwrap_or("none"),
            shown.branch.as_deref().unwrap_or("this point")
        ),
        "switch to the branch shown first: POST /api/1/history/branch {\"action\": \"switch\", \"name\": ...}",
    ))
}

/// Makes a history change through the command `action` builds from the body.
/// The body may carry `head`, refused with `branch-moved` if the page is
/// behind, before the command runs and again as it records. With
/// `checked_out`, the command acts on the checked-out branch, so that must be
/// the one shown, from the check until it records.
async fn change_history(
    app: Arc<App>,
    body: Bytes,
    checked_out: bool,
    action: impl FnOnce(Map<String, Value>, &Shown) -> Result<Action, CliError> + Send + 'static,
) -> Response {
    change(app, move |view| {
        let shown = shown(view, true)?;
        let mut object = object_of(&body)?;
        let seen = seen_head(&mut object)?;
        guard_head(seen.as_deref(), &shown)?;
        if checked_out {
            guard_checked_out(&shown)?;
        }
        let pin = Pin {
            branch: shown.branch.clone().unwrap_or_default(),
            head: seen,
            only: checked_out,
        };
        let action = action(object, &shown)?;
        Ok(answered(pinned(pin, || answer_as_person(&action))?))
    })
    .await
}

/// `POST /api/1/edit`.
pub async fn post_edit(State(app): State<Arc<App>>, body: Bytes) -> Response {
    change_after(app, move |view| match edit_answer(view, &body) {
        Ok((response, after)) => (Ok(response), after),
        Err(error) => (Err(error), None),
    })
    .await
}

/// The answer to an edit request on `view`, with a recorded edit's
/// calculation, which the view after it can stand on.
fn edit_answer(view: &View, body: &Bytes) -> Result<(Response, Option<After>), CliError> {
    let shown = shown(view, true)?;
    let branch = shown.branch.unwrap_or_default();
    let body: EditBody = serde_json::from_slice(body).map_err(|e| {
        CliError::new(
            "bad-body",
            format!("the edit request doesn't fit: {e}"),
            "send {\"edits\": [...], \"head\": \"...\", \"message\": \"...\"}; docs/schemas/command-1.schema.json gives the edits",
        )
    })?;
    let Some(head) = body.head.as_deref() else {
        return Err(CliError::new(
            "missing-argument",
            "an edit needs `head`, the changeset the page last saw",
            "send the head from /api/1/summary, so an edit never lands on changes you haven't seen",
        ));
    };
    let options = edit::Options {
        project: body.project,
        data_date: body.data_date.as_deref(),
        as_planned: body.as_planned,
        check: body.check,
        skip_failed: body.skip_failed,
    };
    let asked = Asked {
        message: body.message.as_deref(),
        author: None,
        propose: body.propose,
    };
    let (report, _, calculation) = as_person(|| {
        edit::edit_store_calculated(
            (&shown.store, Some(branch.as_str()), Some(head)),
            &body.edits,
            (options, view.held()),
            (asked, body.dry_run),
        )
    })?;
    let after = calculation
        .zip(report.recorded())
        .map(|(calculation, recorded)| After {
            head: recorded.id.clone(),
            calculation,
        });
    let mut response = json(&report);
    // A check that newly fails refuses the batch: nothing is recorded.
    if report.blocked() {
        *response.status_mut() = StatusCode::UNPROCESSABLE_ENTITY;
    }
    Ok((response, after))
}

/// The `limit` parameter, if given.
fn limit_of(params: &Params) -> Result<Option<usize>, CliError> {
    params
        .get("limit")
        .map(|text| {
            text.parse().map_err(|_| {
                CliError::new(
                    "bad-limit",
                    format!("limit {text} isn't a count"),
                    "give a whole number",
                )
            })
        })
        .transpose()
}

/// `GET /api/1/history/log?branch=&limit=&all=`; the branch shown by default.
pub async fn log(
    State(app): State<Arc<App>>,
    params: Result<Query<Params>, QueryRejection>,
) -> Response {
    let params = match checked(params, &["branch", "limit", "all"]) {
        Ok(params) => params,
        Err(error) => return refusal(&error),
    };
    read_history(app, move |shown| {
        let limit = limit_of(&params)?;
        let all = match params.get("all").map(String::as_str) {
            None | Some("false") => false,
            Some("true") => true,
            Some(other) => {
                return Err(CliError::new(
                    "bad-all",
                    format!("all is true or false, not {other}"),
                    "give all=true to list every changeset",
                ));
            }
        };
        Ok(LogParams {
            store: shown.store.display().to_string(),
            branch: params
                .get("branch")
                .cloned()
                .or_else(|| shown.branch.clone()),
            limit,
            all,
        }
        .action())
    })
    .await
}

/// `GET /api/1/history/show/<point>`. A point may hold `/`, as proposals do.
pub async fn show(State(app): State<Arc<App>>, UrlPath(at): UrlPath<String>) -> Response {
    read_history(app, move |shown| {
        Ok(ShowParams {
            store: shown.store.display().to_string(),
            at,
        }
        .action())
    })
    .await
}

/// `GET /api/1/history/diff?from=&to=`; `to` is the head shown by default.
pub async fn diff(
    State(app): State<Arc<App>>,
    params: Result<Query<Params>, QueryRejection>,
) -> Response {
    let params = match checked(params, &["from", "to", "limit"]) {
        Ok(params) => params,
        Err(error) => return refusal(&error),
    };
    read_history(app, move |shown| {
        Ok(DiffParams {
            store: shown.store.display().to_string(),
            from: params.get("from").cloned(),
            to: params
                .get("to")
                .cloned()
                .or_else(|| Some(shown.head.clone())),
            limit: limit_of(&params)?,
        }
        .action())
    })
    .await
}

/// `GET /api/1/history/proposals`.
pub async fn proposals(
    State(app): State<Arc<App>>,
    params: Result<Query<Params>, QueryRejection>,
) -> Response {
    if let Err(error) = checked(params, &[]) {
        return refusal(&error);
    }
    read_history(app, |shown| {
        Ok(StoreParams {
            store: shown.store.display().to_string(),
        }
        .proposals_action())
    })
    .await
}

/// `GET /api/1/history/export?at=`: the file at a point, by default the head
/// shown, as a download of the same bytes `history export` writes.
pub async fn export(
    State(app): State<Arc<App>>,
    params: Result<Query<Params>, QueryRejection>,
) -> Response {
    let params = match checked(params, &["at"]) {
        Ok(params) => params,
        Err(error) => return refusal(&error),
    };
    let view = app.current();
    let answer = tokio::task::spawn_blocking(move || {
        let shown = shown(&view, false)?;
        let store = open(&shown.store)?;
        let at = params.get("at").map_or(shown.head.as_str(), String::as_str);
        let (changeset, bytes) = snapshot_at(&store, &shown.store, Some(at))?;
        let stem = shown.store.file_stem().map_or_else(
            || "schedule".to_owned(),
            |s| s.to_string_lossy().into_owned(),
        );
        let short = changeset.id.short();
        // A branch or baseline by its name, with the changeset it's at; a
        // changeset by its short ID alone. Names are tried first, as the
        // store resolves them.
        let point = params.get("at").or(shown.branch.as_ref());
        let named = match point {
            Some(point) => store.get_ref(point).map_err(store_error)?.is_some(),
            None => false,
        };
        let name = match point {
            Some(point) if named => format!("{stem}-{point}-{short}.xer"),
            _ => format!("{stem}-{short}.xer"),
        };
        Ok::<_, CliError>((file_name(&name), bytes))
    })
    .await;
    match answer {
        Ok(Ok((name, bytes))) => (
            [
                (CONTENT_TYPE, "application/octet-stream".to_owned()),
                (
                    CONTENT_DISPOSITION,
                    format!("attachment; filename=\"{name}\""),
                ),
            ],
            bytes,
        )
            .into_response(),
        Ok(Err(error)) => refusal(&error),
        Err(_) => refusal(&stopped()),
    }
}

/// A download's file name with only characters that are safe everywhere.
fn file_name(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// A person's reason is theirs to give or not, as on the command line.
fn optional_reason(object: &mut Map<String, Value>) -> Result<Option<String>, CliError> {
    match object.remove("reason") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(reason)) => Ok(Some(reason).filter(|reason| !reason.trim().is_empty())),
        Some(_) => Err(bad_body("`reason` is text".to_owned())),
    }
}

/// `POST /api/1/history/revert`: `history_revert`'s arguments, on the
/// checked-out branch, which must be the one shown.
pub async fn revert(State(app): State<Arc<App>>, body: Bytes) -> Response {
    change_history(app, body, true, |mut object, shown| {
        let message = optional_reason(&mut object)?;
        let params: RevertParams = with_store(object, &shown.store)?;
        let store = PathBuf::from(params.store);
        Ok(match params.at {
            Some(at) => Action::Revert {
                store,
                at,
                message,
                author: None,
                json: true,
            },
            None => Action::Undo {
                store,
                message,
                author: None,
                json: true,
            },
        })
    })
    .await
}

/// A new branch or baseline starts at the head shown unless `at` says
/// otherwise, not at the checked-out branch's.
fn from_shown(mut params: RefParams, shown: &Shown) -> RefParams {
    if matches!(params.action, RefAction::Create) && params.at.is_none() {
        params.at = Some(shown.head.clone());
    }
    params
}

/// `POST /api/1/history/branch`: `history_branch`'s arguments. The branch
/// shown can't be deleted.
pub async fn branch(State(app): State<Arc<App>>, body: Bytes) -> Response {
    change_history(app, body, false, |object, shown| {
        let params = from_shown(with_store(object, &shown.store)?, shown);
        if matches!(params.action, RefAction::Delete) && params.name == shown.branch {
            return Err(CliError::new(
                "branch-shown",
                "that's the branch being shown, so it isn't deleted from here",
                "serve another branch, then delete this one",
            ));
        }
        params.branch_action()
    })
    .await
}

/// `POST /api/1/history/baseline`: `history_baseline`'s arguments.
pub async fn baseline(State(app): State<Arc<App>>, body: Bytes) -> Response {
    change_history(app, body, false, |object, shown| {
        from_shown(with_store(object, &shown.store)?, shown).baseline_action()
    })
    .await
}

/// `POST /api/1/history/merge`: `history_merge`'s arguments, into the
/// checked-out branch, which must be the one shown.
pub async fn merge(State(app): State<Arc<App>>, body: Bytes) -> Response {
    change_history(app, body, true, |mut object, shown| {
        let message = optional_reason(&mut object)?;
        let params: MergeParams = with_store(object, &shown.store)?;
        Ok(Action::Merge {
            store: PathBuf::from(params.store),
            branch: params.branch,
            message,
            author: None,
            dry_run: params.dry_run,
            json: true,
        })
    })
    .await
}

/// `POST /api/1/history/accept`: `history_review`'s arguments, without
/// `decision`.
pub async fn accept(State(app): State<Arc<App>>, body: Bytes) -> Response {
    review(app, body, "accept").await
}

/// `POST /api/1/history/reject`: `history_review`'s arguments, without
/// `decision`.
pub async fn reject(State(app): State<Arc<App>>, body: Bytes) -> Response {
    review(app, body, "reject").await
}

async fn review(app: Arc<App>, body: Bytes, decision: &'static str) -> Response {
    change_history(app, body, false, move |mut object, shown| {
        if object.contains_key("decision") {
            return Err(bad_body(
                "the endpoint gives the decision, so leave `decision` out".to_owned(),
            ));
        }
        object.insert("decision".to_owned(), Value::from(decision));
        Ok(with_store::<ReviewParams>(object, &shown.store)?.action())
    })
    .await
}
