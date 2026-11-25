use std::path::{Path, PathBuf};

use serde_json::Value;

use super::view::Rows;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The published schema for an answer `T`, as the server writes it:
/// generated from the type, with its ID as `$id` and `title`.
fn generated<T: schemars::JsonSchema>(id: &str) -> String {
    contract::<T>(
        id,
        schemars::generate::SchemaSettings::default().for_serialize(),
    )
}

/// The published schema for a request body `T`, as the server reads it.
fn generated_request<T: schemars::JsonSchema>(id: &str) -> String {
    contract::<T>(
        id,
        schemars::generate::SchemaSettings::default().for_deserialize(),
    )
}

fn contract<T: schemars::JsonSchema>(
    id: &str,
    settings: schemars::generate::SchemaSettings,
) -> String {
    let mut schema =
        serde_json::to_value(settings.into_generator().into_root_schema_for::<T>()).unwrap();
    let object = schema.as_object_mut().unwrap();
    object.insert("$id".into(), Value::from(id));
    object.insert("title".into(), Value::from(id));
    serde_json::to_string_pretty(&schema).unwrap() + "\n"
}

/// `totalfloat.layouts/1`: the folder listed, or a layout deleted.
#[derive(schemars::JsonSchema)]
#[serde(untagged)]
#[expect(dead_code, reason = "only its schema is used")]
enum LayoutsAnswer {
    Listing(super::layouts::Listing),
    Deleted(super::layouts::Deleted),
}

/// `totalfloat.filters/1`: the named filters listed, or one deleted.
#[derive(schemars::JsonSchema)]
#[serde(untagged)]
#[expect(dead_code, reason = "only its schema is used")]
enum FiltersAnswer {
    Listing(super::filters::Listing),
    Deleted(super::filters::Deleted),
}

/// The API's schemas generated from the server's types, each with its file
/// in docs/schemas/.
fn published() -> Vec<(&'static str, String)> {
    vec![
        ("rows-1", generated::<Rows>("totalfloat.rows/1")),
        (
            "summary-1",
            generated::<super::view::Summary<'static>>("totalfloat.summary/1"),
        ),
        (
            "links-1",
            generated::<super::view::Links>("totalfloat.links/1"),
        ),
        ("wbs-1", generated::<super::view::Wbs>("totalfloat.wbs/1")),
        (
            "explain-1",
            generated::<crate::explain::Report>("totalfloat.explain/1"),
        ),
        (
            "check-1",
            generated::<crate::check::Report>("totalfloat.check/1"),
        ),
        (
            "usage-1",
            generated::<crate::usage::Report>("totalfloat.usage/1"),
        ),
        (
            "edit-1",
            generated::<crate::edit::Report>("totalfloat.edit/1"),
        ),
        (
            "edit-request-1",
            generated_request::<super::write::EditBody>("totalfloat.edit-request/1"),
        ),
        (
            "layouts-1",
            generated::<LayoutsAnswer>("totalfloat.layouts/1"),
        ),
        (
            "filters-1",
            generated::<FiltersAnswer>("totalfloat.filters/1"),
        ),
        (
            "fields-1",
            generated::<crate::inspect::Fields>("totalfloat.fields/1"),
        ),
    ]
}

/// Set `TOTALFLOAT_UPDATE_CONTRACT=1` to rewrite them after a deliberate,
/// compatible change, and say why in the PR.
#[test]
fn the_published_api_schemas_match_the_types() {
    for (name, generated) in published() {
        let path = root().join(format!("docs/schemas/{name}.schema.json"));
        if std::env::var_os("TOTALFLOAT_UPDATE_CONTRACT").is_some() {
            std::fs::write(&path, &generated).unwrap();
            continue;
        }
        let published = std::fs::read_to_string(&path).unwrap_or_default();
        assert_eq!(
            published, generated,
            "docs/schemas/{name}.schema.json is out of date; rerun with TOTALFLOAT_UPDATE_CONTRACT=1"
        );
    }
}

#[test]
fn every_column_has_one_entry_per_activity() {
    let loaded = crate::loaded::Loaded::new(
        crate::error::read_xer(&root().join("fixtures/xer/fresh-writer.xer")).unwrap(),
    );
    let rows = super::view::rows("fresh-writer.xer", &loaded.schedules, None);
    let value = serde_json::to_value(&rows).unwrap();
    let count = value["count"].as_u64().unwrap();
    assert!(count > 0);
    for (name, column) in value.as_object().unwrap() {
        if let Some(column) = column.as_array() {
            assert_eq!(column.len() as u64, count, "{name}");
        }
    }
}

/// The view an edit is measured against: the head as read, with what
/// `edit` would calculate for it, held only where edits can be recorded.
#[test]
fn the_shown_branch_holds_its_head_read_and_calculated() {
    let directory = tempfile::tempdir().unwrap();
    let store = against_mcp::started(directory.path());
    let view = super::view::View::open(&store).unwrap();
    let kept = view.held().unwrap();
    let (head, raw) = crate::history::read_head(&store, None).unwrap();
    assert_eq!(*kept.snapshot, head.snapshot);
    let read = crate::loaded::Loaded::new(raw);
    assert_eq!(kept.read.schedules, read.schedules);
    let fresh = crate::schedule::fresh(&read, None, None).unwrap();
    let counts: std::collections::BTreeMap<u64, usize> = fresh
        .projects
        .iter()
        .map(|project| (project.id(), project.critical()))
        .collect();
    assert_eq!(*kept.critical, counts);
    assert!(kept.critical.values().any(|count| *count > 0), "{counts:?}");
    assert_eq!(
        kept.calculated.schedules,
        crate::loaded::Loaded::new(fresh.raw).schedules
    );
    // Its checks are assessed once, aside, for the edits measured against it
    //.
    assert!(kept.assessed.get().is_none());
    view.assess_held();
    assert_eq!(
        kept.assessed.get(),
        Some(&crate::edit::assess_each(&kept.calculated.schedules))
    );
    assert!(view.elsewhere().held().is_none());
    let file = super::view::View::open(Path::new(against_mcp::FIXTURE)).unwrap();
    assert!(file.held().is_none());
}

/// Two projects, with the link from A1 into project 2 lagged so it, not the
/// data date, sets B1's start.
fn driven_across_projects(directory: &Path) -> PathBuf {
    let text = std::fs::read_to_string(root().join("fixtures/xer/two-projects.xer")).unwrap();
    let link = "%R\t1002\t2\t201\t1\t101\tPR_SS\t-1";
    assert!(text.contains(link));
    let path = directory.join("linked.xer");
    std::fs::write(
        &path,
        text.replace(link, "%R\t1002\t2\t201\t1\t101\tPR_SS\t80"),
    )
    .unwrap();
    path
}

#[test]
fn a_link_from_another_project_that_sets_a_date_is_driving() {
    let directory = tempfile::tempdir().unwrap();
    let view = super::view::View::open(&driven_across_projects(directory.path())).unwrap();
    let (explained, _) = crate::explain::report(
        "linked.xer",
        &view.loaded,
        "B1",
        crate::explain::Field::EarlyStart,
        None,
    )
    .unwrap();
    let explained = serde_json::to_value(&explained).unwrap();
    let first = explained["chains"][0]["steps"][0]["text"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(
        first.contains("SS with 80h lag") && first.contains("101"),
        "{first}"
    );
    let links: Value = serde_json::from_slice(&view.links().unwrap()).unwrap();
    let ids = links["id"].as_array().unwrap();
    let at = ids.iter().position(|id| id == 1002).unwrap();
    assert_eq!(links["driving"][at], true, "{links}");
}

#[test]
fn completed_work_is_never_critical_on_float() {
    let directory = tempfile::tempdir().unwrap();
    let view = super::view::View::open(&driven_across_projects(directory.path())).unwrap();
    let rows: Value = serde_json::from_slice(&view.rows().unwrap()).unwrap();
    for (n, status) in rows["status"].as_array().unwrap().iter().enumerate() {
        if status == "completed" {
            assert_eq!(rows["critical"][n], false, "{}", rows["code"][n]);
        }
    }
    // Stored float alone follows the same rule.
    let loaded = crate::loaded::Loaded::new(
        crate::error::read_xer(&root().join("fixtures/xer/fresh-writer.xer")).unwrap(),
    );
    let mut schedule = loaded.schedules[0].clone();
    let activity = schedule.activities.values_mut().next().unwrap();
    activity.status = totalfloat_model::ActivityStatus::Completed;
    activity.stored.total_float = Some(totalfloat_model::Minutes(-480));
    let activity = schedule.activities.values().next().unwrap();
    assert!(!crate::query::critical(&schedule, activity));
}

/// More than `inspect` lists: one code type with 150 values, in a sequence
/// that isn't ID order, and 120 activity user fields, written afresh.
fn many_fields(directory: &Path) -> PathBuf {
    use totalfloat_model as m;
    let loaded = crate::loaded::Loaded::new(
        crate::error::read_xer(Path::new(against_mcp::FIXTURE)).unwrap(),
    );
    let mut schedule = loaded.schedules[0].clone();
    let kind = m::ActivityCodeTypeId(9001);
    schedule.activity_code_types.insert(
        kind,
        m::ActivityCodeType {
            id: kind,
            name: "Many".to_owned(),
            scope: m::CodeScope::Global,
            project: None,
            max_length: None,
            sequence: 1,
            source: None,
        },
    );
    for k in 0..150_u64 {
        let id = m::ActivityCodeId(9100 + k);
        schedule.activity_codes.insert(
            id,
            m::ActivityCode {
                id,
                code_type: kind,
                parent: None,
                value: format!("V{k:03}"),
                description: format!("Value {k}"),
                // Last made, first in sequence.
                sequence: 1000 - i64::try_from(k).unwrap(),
                source: None,
            },
        );
    }
    for k in 0..120_u64 {
        let id = m::UdfTypeId(9500 + k);
        schedule.udf_types.insert(
            id,
            m::UdfType {
                id,
                subject: m::UdfSubject::Activity,
                label: format!("Field {k:03}"),
                data_type: m::UdfDataType::Text,
                source: None,
            },
        );
    }
    let options = totalfloat_xer::fresh::FreshOptions::new("2026-01-01".parse().unwrap());
    let raw = totalfloat_xer::fresh::write(&schedule, options).unwrap();
    let file = directory.join("many.xer");
    let mut bytes = Vec::new();
    raw.write(&mut bytes).unwrap();
    std::fs::write(&file, bytes).unwrap();
    file
}

/// `GET /api/1/fields` lists every code value in sequence and every user
/// field, where the summary's `inspect` stops at 100.
#[tokio::test]
async fn fields_lists_every_code_value_and_user_field() {
    let directory = tempfile::tempdir().unwrap();
    let file = many_fields(directory.path());
    let (status, fields) = against_mcp::call(&file, "GET", "/api/1/fields", None).await;
    assert_eq!(status, axum::http::StatusCode::OK, "{fields}");
    assert_eq!(fields["schema"], "totalfloat.fields/1");
    let many = fields["activity_codes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "Many")
        .unwrap();
    let values: Vec<&str> = many["values"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["value"].as_str().unwrap())
        .collect();
    let expected: Vec<String> = (0..150).rev().map(|k| format!("V{k:03}")).collect();
    assert_eq!(values, expected);
    assert_eq!(many["values"][0]["description"], "Value 149");
    let labels: Vec<&str> = fields["activity_udfs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["label"].as_str().unwrap())
        .collect();
    assert_eq!(labels.len(), 120);
    assert_eq!(labels[119], "Field 119");
    assert_eq!(fields["activity_udfs"][0]["data_type"], "text");
    // `inspect` still stops at 100 of each.
    let (_, summary) = against_mcp::call(&file, "GET", "/api/1/summary", None).await;
    let inspect = &summary["inspect"];
    assert_eq!(inspect["activity_udfs"].as_array().unwrap().len(), 100);
    assert_eq!(inspect["activity_udfs_total"], 120);
    let listed = inspect["activity_codes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "Many")
        .unwrap();
    assert_eq!(listed["values"].as_array().unwrap().len(), 100);
    assert_eq!(listed["values_total"], 150);
}

mod against_mcp {
    use std::sync::Arc;

    use axum::body::{Body, to_bytes};
    use axum::http::{Request, StatusCode};
    use serde_json::{Value, json};
    use tower::ServiceExt;

    use crate::history::Action;
    use crate::mcp::stores::{
        Decision, DiffParams, LogParams, MergeParams, RefAction, RefParams, RevertParams,
        ReviewParams, ShowParams, StoreParams, answer_of,
    };
    use crate::serve::{App, api, view::View};

    pub(super) const FIXTURE: &str = "../../fixtures/xer/batches/001/rel-fs-positive.xer";

    /// A store started from the fixture, with one edit recorded through the
    /// app and one proposal waiting.
    pub(super) fn started(directory: &std::path::Path) -> std::path::PathBuf {
        let file = directory.join("plant.xer");
        std::fs::copy(FIXTURE, &file).unwrap();
        let init = Action::Init {
            file,
            store: None,
            message: Some("As submitted".to_owned()),
            author: Some("Pat".to_owned()),
            json: true,
        };
        let (done, _) = crate::output::captured(|| crate::history::run(&init));
        done.unwrap();
        directory.join("plant.tfdb")
    }

    pub(super) async fn call(
        store: &std::path::Path,
        method: &str,
        path: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let app = Arc::new(App::new(store.to_owned(), View::open(store).unwrap()));
        let request = Request::builder().method(method).uri(path);
        let request = match body {
            Some(body) => request.body(Body::from(body.to_string())),
            None => request.body(Body::empty()),
        }
        .unwrap();
        let response = api(app).oneshot(request).await.unwrap();
        let status = response.status();
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        (status, serde_json::from_slice(&bytes).unwrap())
    }

    /// One risk analysis at a time: another meanwhile is refused, and the
    /// turn comes back when the first ends.
    #[tokio::test]
    async fn one_risk_analysis_runs_at_a_time() {
        let directory = tempfile::tempdir().unwrap();
        let store = started(directory.path());
        let app = Arc::new(App::new(store.clone(), View::open(&store).unwrap()));
        let body = json!({"inputs": {"default": {"min": -10, "max": 25}}, "iterations": 20});
        let post = |app: Arc<App>| {
            let request = Request::builder()
                .method("POST")
                .uri("/api/1/risk")
                .body(Body::from(body.to_string()))
                .unwrap();
            async move {
                let response = api(app).oneshot(request).await.unwrap();
                let status = response.status();
                let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
                (status, serde_json::from_slice::<Value>(&bytes).unwrap())
            }
        };
        app.risk_running
            .store(true, std::sync::atomic::Ordering::Release);
        let (status, busy) = post(Arc::clone(&app)).await;
        assert_ne!(status, StatusCode::OK);
        assert_eq!(busy["error"]["code"], "risk-busy");
        app.risk_running
            .store(false, std::sync::atomic::Ordering::Release);
        let (status, done) = post(Arc::clone(&app)).await;
        assert_eq!(status, StatusCode::OK, "{done}");
        assert_eq!(done["schema"], "totalfloat.risk-analysis/1");
        // The turn came back.
        let (status, _) = post(app).await;
        assert_eq!(status, StatusCode::OK);
    }

    /// A portfolio of two schedules in `directory`, `plant.xer` and
    /// `usage.xer`, served.
    pub(super) fn portfolio_app(directory: &std::path::Path) -> Arc<App> {
        std::fs::copy(FIXTURE, directory.join("plant.xer")).unwrap();
        std::fs::copy("../../fixtures/xer/usage.xer", directory.join("usage.xer")).unwrap();
        let path = directory.join("plan.portfolio.toml");
        std::fs::write(
            &path,
            "schema = \"totalfloat.portfolio/1\"\n[[eps]]\nid = \"ENT\"\nname = \"Enterprise\"\n[[eps]]\nid = \"NA\"\nname = \"North America\"\nparent = \"ENT\"\n[[project]]\nfile = \"plant.xer\"\neps = \"NA\"\n[[project]]\nfile = \"usage.xer\"\neps = \"ENT\"\n",
        )
        .unwrap();
        let mut app = App::new(
            path.clone(),
            super::super::first_of_portfolio(&path).unwrap(),
        );
        app.portfolio = Some(path);
        Arc::new(app)
    }

    /// One request to `app`.
    pub(super) async fn call_app(
        app: &Arc<App>,
        method: &str,
        path: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let request = Request::builder().method(method).uri(path);
        let request = match body {
            Some(body) => request.body(Body::from(body.to_string())),
            None => request.body(Body::empty()),
        }
        .unwrap();
        let response = api(Arc::clone(app)).oneshot(request).await.unwrap();
        let status = response.status();
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        (status, serde_json::from_slice(&bytes).unwrap())
    }

    #[tokio::test]
    async fn a_portfolio_serves_its_projects_one_at_a_time_read_only() {
        let directory = tempfile::tempdir().unwrap();
        let app = portfolio_app(directory.path());
        let (status, summary) = call_app(&app, "GET", "/api/1/summary", None).await;
        assert!(status.is_success(), "{summary}");
        assert!(
            summary["portfolio"]
                .as_str()
                .unwrap()
                .ends_with("plan.portfolio.toml")
        );
        assert!(summary["location"].as_str().unwrap().ends_with("plant.xer"));
        assert_eq!(summary["writable"], false);
        let (status, shown) = call_app(&app, "GET", "/api/1/portfolio", None).await;
        assert!(status.is_success(), "{shown}");
        assert_eq!(shown["schema"], "totalfloat.portfolio-show/1");
        assert_eq!(shown["nodes"][0]["totals"]["projects"], 2);
        let (status, capacity) =
            call_app(&app, "GET", "/api/1/portfolio/capacity?by=month", None).await;
        assert!(status.is_success(), "{capacity}");
        assert_eq!(capacity["resources"][0]["short_name"], "CREW");
        // Open the other schedule: the grid's rows are now its.
        let (status, opened) = call_app(
            &app,
            "POST",
            "/api/1/portfolio/open",
            Some(json!({"file": "usage.xer"})),
        )
        .await;
        assert!(status.is_success(), "{opened}");
        assert!(opened["location"].as_str().unwrap().ends_with("usage.xer"));
        let (_, rows) = call_app(&app, "GET", "/api/1/rows", None).await;
        assert!(rows["code"].as_array().unwrap().iter().any(|c| c == "A2"));
        // Only the portfolio's own files open.
        let (status, refused) = call_app(
            &app,
            "POST",
            "/api/1/portfolio/open",
            Some(json!({"file": "../elsewhere.xer"})),
        )
        .await;
        assert!(!status.is_success());
        assert_eq!(refused["error"]["code"], "not-in-portfolio");
    }

    #[tokio::test]
    async fn a_served_portfolio_rolls_up_what_the_grid_shows() {
        // rel-fs-positive stores no dates; the grid shows it calculated, so
        // the Projects panel must too.
        let directory = tempfile::tempdir().unwrap();
        let app = portfolio_app(directory.path());
        let (_, shown) = call_app(&app, "GET", "/api/1/portfolio", None).await;
        let plant = &shown["projects"][0];
        assert_eq!(plant["file"], "plant.xer");
        assert_eq!(plant["results"], "calculated");
        // The grid's columns; neither activity has started.
        let rows = call_app(&app, "GET", "/api/1/rows", None).await.1;
        let column = |name: &str| rows[name].as_array().unwrap().clone();
        let critical = column("critical").iter().filter(|c| **c == true).count();
        let starts = column("early_start");
        let finishes = column("early_finish");
        let starts: Vec<&str> = starts.iter().filter_map(Value::as_str).collect();
        let finishes: Vec<&str> = finishes.iter().filter_map(Value::as_str).collect();
        assert_eq!(critical, 2);
        assert_eq!(plant["totals"]["critical"], critical);
        assert_eq!(plant["totals"]["start"], *starts.iter().min().unwrap());
        assert_eq!(plant["totals"]["finish"], *finishes.iter().max().unwrap());
        assert_eq!(plant["totals"]["stored_results"], 0);
        assert_eq!(shown["nodes"][0]["totals"]["critical"], 2);

        // A file the engine refuses, here for a loop, keeps its stored
        // results, and says so up the tree.
        let text = std::fs::read_to_string(FIXTURE).unwrap().replace(
            "%R\t1\t2\t1\t1\t1\tPR_FS\t1\t\t\t\t\r\n",
            "%R\t1\t2\t1\t1\t1\tPR_FS\t1\t\t\t\t\r\n%R\t2\t1\t2\t1\t1\tPR_FS\t0\t\t\t\t\r\n",
        );
        assert!(text.contains("%R\t2\t1\t2"), "the loop wasn't added");
        std::fs::write(directory.path().join("plant.xer"), text).unwrap();
        let (status, shown) = call_app(&app, "GET", "/api/1/portfolio", None).await;
        assert!(status.is_success(), "{shown}");
        let plant = &shown["projects"][0];
        assert_eq!(plant["results"], "stored");
        assert_eq!(plant["totals"]["start"], Value::Null);
        assert_eq!(plant["totals"]["stored_results"], 1);
        assert_eq!(shown["projects"][1]["results"], "calculated");
        assert_eq!(shown["nodes"][0]["totals"]["stored_results"], 1);
        assert_eq!(shown["nodes"][1]["totals"]["stored_results"], 1);
    }

    #[tokio::test]
    async fn a_store_branch_in_a_portfolio_takes_no_edits() {
        // The portfolio names a store's checked-out branch; served through
        // the portfolio, it's read-only, so an edit is refused.
        let directory = tempfile::tempdir().unwrap();
        let store = started(directory.path());
        let name = store.file_name().unwrap().to_str().unwrap().to_owned();
        let path = directory.path().join("plan.portfolio.toml");
        std::fs::write(
            &path,
            format!("schema = \"totalfloat.portfolio/1\"\n[[eps]]\nid = \"E\"\nname = \"E\"\n[[project]]\nfile = \"{name}\"\neps = \"E\"\n"),
        )
        .unwrap();
        let mut app = App::new(
            path.clone(),
            super::super::first_of_portfolio(&path).unwrap(),
        );
        app.portfolio = Some(path);
        let app = Arc::new(app);
        let (_, summary) = call_app(&app, "GET", "/api/1/summary", None).await;
        assert_eq!(summary["writable"], false, "{summary}");
        let edit = json!({
            "edits": [{"op": "set_duration", "activity": "A2", "original": "16h"}],
            "head": summary["head"],
            "message": "Through the portfolio",
        });
        let (status, refused) = call_app(&app, "POST", "/api/1/edit", Some(edit)).await;
        assert!(!status.is_success(), "{refused}");
        assert_eq!(refused["error"]["code"], "read-only");
    }

    #[tokio::test]
    async fn one_schedule_served_has_no_portfolio() {
        let directory = tempfile::tempdir().unwrap();
        let store = started(directory.path());
        let (status, refused) = call(&store, "GET", "/api/1/portfolio", None).await;
        assert!(!status.is_success());
        assert_eq!(refused["error"]["code"], "not-a-portfolio");
        let (_, summary) = call(&store, "GET", "/api/1/summary", None).await;
        assert!(summary.get("portfolio").is_none());
    }

    fn mcp(action: Result<Action, crate::error::CliError>) -> Value {
        answer_of(&action.unwrap(), "test-agent").unwrap().1
    }

    /// The shape of a document: its keys, all the way down, with the first
    /// item standing for each list.
    fn shape(value: &Value) -> Value {
        match value {
            Value::Object(object) => {
                Value::Object(object.iter().map(|(k, v)| (k.clone(), shape(v))).collect())
            }
            Value::Array(items) => Value::Array(items.first().map(shape).into_iter().collect()),
            _ => Value::Null,
        }
    }

    #[tokio::test]
    async fn each_history_read_answers_as_its_mcp_tool_does() {
        let directory = tempfile::tempdir().unwrap();
        let store = started(directory.path());
        let text = store.display().to_string();
        let head = call(&store, "GET", "/api/1/summary", None).await.1["head"].clone();
        let edit = json!({"edits": [{"op": "set_duration", "activity": "A1", "original": "24h"}], "head": head, "message": "Steel late"});
        assert_eq!(
            call(&store, "POST", "/api/1/edit", Some(edit)).await.0,
            StatusCode::OK
        );
        let reads: Vec<(&str, Action)> = vec![
            (
                "/api/1/history/log",
                LogParams {
                    store: text.clone(),
                    branch: None,
                    limit: None,
                    all: false,
                }
                .action(),
            ),
            (
                "/api/1/history/log?limit=1&all=true",
                LogParams {
                    store: text.clone(),
                    branch: None,
                    limit: Some(1),
                    all: true,
                }
                .action(),
            ),
            (
                "/api/1/history/log?branch=main",
                LogParams {
                    store: text.clone(),
                    branch: Some("main".to_owned()),
                    limit: None,
                    all: false,
                }
                .action(),
            ),
            (
                "/api/1/history/show/main",
                ShowParams {
                    store: text.clone(),
                    at: "main".to_owned(),
                }
                .action(),
            ),
            (
                "/api/1/history/diff",
                DiffParams {
                    store: text.clone(),
                    from: None,
                    to: None,
                    limit: None,
                }
                .action(),
            ),
            (
                "/api/1/history/proposals",
                StoreParams {
                    store: text.clone(),
                }
                .proposals_action(),
            ),
        ];
        for (path, action) in reads {
            let (status, served) = call(&store, "GET", path, None).await;
            assert_eq!(status, StatusCode::OK, "{path}: {served}");
            assert_eq!(served, mcp(Ok(action)), "{path}");
        }
    }

    /// `limit` means the same on `/api/1/history/diff` as on MCP's
    /// `history_diff`, and a malformed one is refused.
    #[tokio::test]
    async fn a_diff_lists_as_many_as_asked_as_its_mcp_tool_does() {
        let directory = tempfile::tempdir().unwrap();
        let store = started(directory.path());
        let text = store.display().to_string();
        let head = call(&store, "GET", "/api/1/summary", None).await.1["head"].clone();
        let edit = json!({"edits": [
            {"op": "set_name", "activity": "A1", "name": "One"},
            {"op": "set_name", "activity": "A2", "name": "Two"}
        ], "head": head, "message": "Renamed"});
        let (status, answer) = call(&store, "POST", "/api/1/edit", Some(edit)).await;
        assert_eq!(status, StatusCode::OK, "{answer}");
        for limit in [None, Some(1), Some(0)] {
            let path = limit.map_or("/api/1/history/diff".to_owned(), |n| {
                format!("/api/1/history/diff?limit={n}")
            });
            let (status, served) = call(&store, "GET", &path, None).await;
            assert_eq!(status, StatusCode::OK, "{path}: {served}");
            let tool = DiffParams {
                store: text.clone(),
                from: None,
                to: None,
                limit,
            };
            assert_eq!(served, mcp(Ok(tool.action())), "{path}");
            let changed = served["diff"]["projects"][0]["changed"]
                .as_array()
                .unwrap()
                .len();
            assert_eq!(changed, if limit == Some(1) { 1 } else { 2 }, "{path}");
        }
        let (status, refused) = call(&store, "GET", "/api/1/history/diff?limit=many", None).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{refused}");
        assert_eq!(refused["error"]["code"], "bad-limit");
    }

    #[tokio::test]
    async fn usage_answers_as_query_usage_does() {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("usage.xer");
        std::fs::copy("../../fixtures/xer/usage.xer", &file).unwrap();
        let (status, served) =
            call(&file, "GET", "/api/1/usage?resource=CREW&by=month", None).await;
        assert_eq!(status, StatusCode::OK, "{served}");
        assert_eq!(served["schema"], "totalfloat.usage/1");
        assert_eq!(served["periods"][0]["total"], 160.0, "{served}");
        // The served view is calculated; the fixture's stored dates are the
        // engine's, so the weeks are the ones `query --usage` gives.
        let (_, weekly) = call(&file, "GET", "/api/1/usage?resource=crew", None).await;
        let totals: Vec<f64> = weekly["periods"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["total"].as_f64().unwrap())
            .collect();
        assert_eq!(totals, [16.0, 24.0, 60.0, 60.0], "{weekly}");
        for (path, code) in [
            ("/api/1/usage", "missing-argument"),
            ("/api/1/usage?resource=CREW&by=year", "bad-period"),
            ("/api/1/usage?resource=NOPE", "no-such-resource"),
        ] {
            let (status, served) = call(&file, "GET", path, None).await;
            assert_ne!(status, StatusCode::OK, "{path}");
            assert!(served.to_string().contains(code), "{path}: {served}");
        }
    }

    #[tokio::test]
    async fn query_answers_as_query_does_with_every_match() {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("codes.xer");
        std::fs::copy("../../fixtures/xer/two-projects.xer", &file).unwrap();
        let (status, all) = call(&file, "GET", "/api/1/query?columns=id", None).await;
        assert_eq!(status, StatusCode::OK, "{all}");
        assert_eq!(all["schema"], "totalfloat.query/1");
        // With no limit, every match, not a page of 50.
        assert_eq!(
            (all["total"].clone(), all["returned"].clone()),
            (json!(3), json!(3))
        );
        let path = "/api/1/query?code=synthetic%20global%20codes%3DA\
                    &columns=id,code:Synthetic%20global%20codes,udf:Synthetic%20task%20number";
        let (status, found) = call(&file, "GET", path, None).await;
        assert_eq!(status, StatusCode::OK, "{found}");
        assert_eq!(
            found["rows"],
            json!([{
                "id": 101,
                "code:Synthetic global codes": "A",
                "udf:Synthetic task number": 1.25
            }])
        );
        let (_, paged) = call(
            &file,
            "GET",
            "/api/1/query?status=not-started,completed&limit=1",
            None,
        )
        .await;
        assert_eq!(paged["returned"], 1, "{paged}");
        for (path, code) in [
            ("/api/1/query?baseline=other.xer", "unknown-argument"),
            ("/api/1/query?critical=yes", "bad-query"),
            ("/api/1/query?status=late", "bad-query"),
            ("/api/1/query?columns=code:Phase", "no-such-code-type"),
            ("/api/1/query?code=Phase%3DX", "no-such-code"),
        ] {
            let (status, served) = call(&file, "GET", path, None).await;
            assert_ne!(status, StatusCode::OK, "{path}");
            assert!(served.to_string().contains(code), "{path}: {served}");
        }
    }

    #[tokio::test]
    async fn each_history_change_answers_in_its_mcp_tool_s_form() {
        let directory = tempfile::tempdir().unwrap();
        let served = started(directory.path());
        // The same store for MCP, so both make the same changes.
        let other = directory.path().join("other.tfdb");
        std::fs::copy(&served, &other).unwrap();
        let text = other.display().to_string();
        let changes: Vec<(&str, Value, Result<Action, crate::error::CliError>)> = vec![
            (
                "/api/1/history/branch",
                json!({"action": "create", "name": "what-if"}),
                RefParams {
                    store: text.clone(),
                    action: RefAction::Create,
                    name: Some("what-if".to_owned()),
                    at: None,
                }
                .branch_action(),
            ),
            (
                "/api/1/history/baseline",
                json!({"action": "create", "name": "contract"}),
                RefParams {
                    store: text.clone(),
                    action: RefAction::Create,
                    name: Some("contract".to_owned()),
                    at: None,
                }
                .baseline_action(),
            ),
            (
                "/api/1/history/branch",
                json!({"action": "list"}),
                RefParams {
                    store: text.clone(),
                    action: RefAction::List,
                    name: None,
                    at: None,
                }
                .branch_action(),
            ),
            (
                "/api/1/history/merge",
                json!({"branch": "what-if", "dry_run": true}),
                MergeParams {
                    store: text.clone(),
                    branch: "what-if".to_owned(),
                    reason: None,
                    dry_run: true,
                }
                .action(),
            ),
        ];
        for (path, body, action) in changes {
            let (status, answer) = call(&served, "POST", path, Some(body)).await;
            assert_eq!(status, StatusCode::OK, "{path}: {answer}");
            let expected = mcp(action);
            assert_eq!(answer["schema"], expected["schema"], "{path}");
            assert_eq!(shape(&answer), shape(&expected), "{path}");
        }
        reviewed_and_reverted(&served, &other).await;
    }

    /// A proposal on each side, accepted, then the latest change reverted:
    /// the app's answers have the MCP tools' form.
    async fn reviewed_and_reverted(served: &std::path::Path, other: &std::path::Path) {
        let text = other.display().to_string();
        // A proposal each side, then review and revert.
        let head = |store: &std::path::Path| {
            let store = store.to_owned();
            async move { call(&store, "GET", "/api/1/summary", None).await.1["head"].clone() }
        };
        let propose = json!({"edits": [{"op": "set_duration", "activity": "A1", "original": "24h"}], "head": head(served).await, "message": "Maybe", "propose": true});
        assert_eq!(
            call(served, "POST", "/api/1/edit", Some(propose)).await.0,
            StatusCode::OK
        );
        let propose = json!({"edits": [{"op": "set_duration", "activity": "A1", "original": "24h"}], "head": head(other).await, "message": "Maybe", "propose": true});
        assert_eq!(
            call(other, "POST", "/api/1/edit", Some(propose)).await.0,
            StatusCode::OK
        );
        let proposal = |doc: Value| doc["proposals"][0]["name"].as_str().unwrap().to_owned();
        let here = proposal(
            call(served, "GET", "/api/1/history/proposals", None)
                .await
                .1,
        );
        let there = proposal(mcp(Ok(StoreParams {
            store: text.clone(),
        }
        .proposals_action())));
        let (status, answer) = call(
            served,
            "POST",
            "/api/1/history/accept",
            Some(json!({"proposal": here, "reason": "Agreed"})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{answer}");
        let expected = mcp(Ok(ReviewParams {
            store: text.clone(),
            proposal: there,
            decision: Decision::Accept,
            reason: Some("Agreed".to_owned()),
        }
        .action()));
        assert_eq!(shape(&answer), shape(&expected), "accept");
        let (status, answer) = call(
            served,
            "POST",
            "/api/1/history/revert",
            Some(json!({"reason": "Not after all"})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{answer}");
        let expected = mcp(RevertParams {
            store: text.clone(),
            at: None,
            reason: Some("Not after all".to_owned()),
        }
        .action());
        assert_eq!(shape(&answer), shape(&expected), "revert");
    }
}

#[test]
fn a_damaged_store_is_the_server_s_fault_even_with_problems() {
    let mut error = crate::error::CliError::new("store-damaged", "a changeset won't apply", "");
    error.problems.push(crate::error::Problem {
        edit: Some(1),
        source: "edit 1".to_owned(),
        code: "no-such-activity",
        message: "gone".to_owned(),
        hint: String::new(),
        depends_on: None,
        line: None,
        column: None,
    });
    assert_eq!(
        super::refusal(&error).status(),
        axum::http::StatusCode::INTERNAL_SERVER_ERROR
    );
    error.code = "edits-refused";
    assert_eq!(
        super::refusal(&error).status(),
        axum::http::StatusCode::UNPROCESSABLE_ENTITY
    );
}

mod feed {
    use std::sync::Arc;

    use totalfloat_server::Events;

    use super::against_mcp::started;
    use crate::history::Asked;
    use crate::serve::feed::{Feed, SCHEMA};
    use crate::serve::{App, view::View};

    /// Records an edit through its own store handle, as another process
    /// would.
    fn edit_elsewhere(store: &std::path::Path, duration: &str, propose: bool) {
        let edits: Vec<totalfloat_edit::Edit> = serde_json::from_value(serde_json::json!([
            {"op": "set_duration", "activity": "A2", "original": duration}
        ]))
        .unwrap();
        let asked = Asked {
            message: Some("Elsewhere"),
            author: Some("Robin"),
            propose,
        };
        crate::edit::edit_store(
            (store, None, None),
            &edits,
            (crate::edit::Options::default(), None),
            (asked, false),
        )
        .unwrap();
    }

    /// One look, which must bring the view up to date.
    fn tick(feed: &mut Feed, app: &App, events: &Events) -> Vec<serde_json::Value> {
        let (sent, refreshed) = feed.tick(app, events);
        refreshed.unwrap();
        sent
    }

    fn history(action: &crate::history::Action) {
        let (done, _) = crate::output::captured(|| crate::history::run(action));
        done.unwrap();
    }

    fn branch(store: &std::path::Path, name: &str, delete: bool) -> crate::history::Action {
        crate::history::Action::Branch {
            store: store.to_owned(),
            name: name.to_owned(),
            at: None,
            delete,
            json: true,
        }
    }

    #[test]
    fn a_change_made_while_the_first_view_was_calculated_is_caught_up() {
        let directory = tempfile::tempdir().unwrap();
        let store = started(directory.path());
        let app = App::new(store.clone(), View::open(&store).unwrap());
        edit_elsewhere(&store, "32h", false);
        let mut feed = Feed::new(&app).unwrap().unwrap();
        let sent = tick(&mut feed, &app, &Events::default());
        assert_eq!(kinds(&sent), ["view"], "{sent:?}");
        let (_, head) = app.current().point().unwrap();
        assert_eq!(sent[0]["head"], head.as_str());
    }

    #[test]
    fn a_change_under_way_isn_t_waited_for() {
        let directory = tempfile::tempdir().unwrap();
        let store = started(directory.path());
        let app = App::new(store.clone(), View::open(&store).unwrap());
        let events = Events::default();
        let mut feed = Feed::new(&app).unwrap().unwrap();
        assert_eq!(
            tick(&mut feed, &app, &events),
            Vec::<serde_json::Value>::new()
        );
        let shown = app.current();
        let turn = app.writing.try_lock().unwrap();
        edit_elsewhere(&store, "32h", false);
        let sent = tick(&mut feed, &app, &events);
        assert_eq!(kinds(&sent), ["changeset", "branch"], "{sent:?}");
        assert!(
            Arc::ptr_eq(&shown, &app.current()),
            "the turn is a change's"
        );
        drop(turn);
        let sent = tick(&mut feed, &app, &events);
        assert_eq!(kinds(&sent), ["view"], "{sent:?}");
    }

    #[test]
    fn a_view_that_can_t_be_refreshed_doesn_t_hold_back_the_news() {
        let directory = tempfile::tempdir().unwrap();
        let store = started(directory.path());
        history(&branch(&store, "what-if", false));
        let served = std::path::PathBuf::from(format!("{}@what-if", store.display()));
        let app = App::new(served.clone(), View::open(&served).unwrap());
        let events = Events::default();
        let mut feed = Feed::new(&app).unwrap().unwrap();
        assert_eq!(
            tick(&mut feed, &app, &events),
            Vec::<serde_json::Value>::new()
        );
        history(&branch(&store, "what-if", true));
        let (sent, refreshed) = feed.tick(&app, &events);
        assert_eq!(kinds(&sent), ["branch"], "{sent:?}");
        assert_eq!(sent[0]["change"], "deleted");
        assert_eq!(refreshed.unwrap_err().code, "no-such-ref");
        // Tried again at every look, without telling the news twice.
        let (sent, refreshed) = feed.tick(&app, &events);
        assert_eq!(sent, Vec::<serde_json::Value>::new());
        assert!(refreshed.is_err());
        edit_elsewhere(&store, "32h", false);
        history(&branch(&store, "what-if", false));
        let sent = tick(&mut feed, &app, &events);
        assert_eq!(
            kinds(&sent),
            ["changeset", "branch", "branch", "view"],
            "{sent:?}"
        );
    }

    fn kinds(sent: &[serde_json::Value]) -> Vec<&str> {
        sent.iter().map(|e| e["event"].as_str().unwrap()).collect()
    }

    #[test]
    fn an_unchanged_store_is_looked_at_and_nothing_more() {
        let directory = tempfile::tempdir().unwrap();
        let store = started(directory.path());
        let app = App::new(store.clone(), View::open(&store).unwrap());
        let events = Events::default();
        let mut feed = Feed::new(&app).unwrap().unwrap();
        let shown = app.current();
        for _ in 0..3 {
            assert_eq!(
                tick(&mut feed, &app, &events),
                Vec::<serde_json::Value>::new()
            );
        }
        assert!(
            Arc::ptr_eq(&shown, &app.current()),
            "nothing was calculated"
        );

        edit_elsewhere(&store, "32h", false);
        let sent = tick(&mut feed, &app, &events);
        assert_eq!(kinds(&sent), ["changeset", "branch", "view"], "{sent:?}");
        assert!(sent.iter().all(|e| e["schema"] == SCHEMA));
        assert_eq!(sent[0]["branch"], "main");
        assert_eq!(sent[0]["changeset"]["message"], "Elsewhere");
        assert_eq!(sent[1]["change"], "moved");
        let now = app.current();
        assert!(!Arc::ptr_eq(&shown, &now), "the view caught up");
        assert_eq!(sent[2]["head"], sent[0]["changeset"]["id"]);
        assert_eq!(
            tick(&mut feed, &app, &events),
            Vec::<serde_json::Value>::new()
        );
        assert!(Arc::ptr_eq(&now, &app.current()));
    }

    #[test]
    fn a_proposal_is_announced_when_made_and_when_decided() {
        let directory = tempfile::tempdir().unwrap();
        let store = started(directory.path());
        let app = App::new(store.clone(), View::open(&store).unwrap());
        let events = Events::default();
        let mut feed = Feed::new(&app).unwrap().unwrap();
        let shown = app.current();
        edit_elsewhere(&store, "32h", true);
        let sent = tick(&mut feed, &app, &events);
        assert_eq!(kinds(&sent), ["changeset", "proposal"], "{sent:?}");
        assert_eq!(sent[1]["change"], "new");
        assert_eq!(sent[1]["branch"], "main");
        assert_eq!(sent[1]["changeset"]["state"], "proposed");
        assert!(Arc::ptr_eq(&shown, &app.current()), "main didn't move");
        let id = sent[1]["changeset"]["id"].as_str().unwrap().to_owned();
        let reject = crate::history::Action::Reject {
            store: store.clone(),
            at: id,
            message: Some("Not yet".to_owned()),
            author: None,
            json: true,
        };
        let (done, _) = crate::output::captured(|| crate::history::run(&reject));
        done.unwrap();
        let sent = tick(&mut feed, &app, &events);
        assert_eq!(kinds(&sent), ["proposal"], "{sent:?}");
        assert_eq!(sent[0]["change"], "decided");
        assert_eq!(sent[0]["changeset"]["state"], "rejected");
    }

    /// Each event has exactly the keys its kind publishes, and its changeset
    /// those of a log entry.
    fn published(sent: &[serde_json::Value]) {
        let path = super::root().join("docs/schemas/event-1.schema.json");
        let schema: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let keys = |v: &serde_json::Value| {
            v.as_object()
                .unwrap()
                .keys()
                .cloned()
                .collect::<std::collections::BTreeSet<_>>()
        };
        assert_eq!(schema["$id"], SCHEMA);
        for event in sent {
            let kind = event["event"].as_str().unwrap();
            let def = &schema["$defs"][kind];
            assert_eq!(keys(event), keys(&def["properties"]), "{kind}");
            if let Some(changeset) = event.get("changeset") {
                assert_eq!(
                    keys(changeset),
                    keys(&schema["$defs"]["entry"]["properties"])
                );
            }
        }
    }

    #[test]
    fn every_event_is_as_published() {
        let directory = tempfile::tempdir().unwrap();
        let store = started(directory.path());
        let app = App::new(store.clone(), View::open(&store).unwrap());
        let events = Events::default();
        let mut feed = Feed::new(&app).unwrap().unwrap();
        edit_elsewhere(&store, "32h", false);
        edit_elsewhere(&store, "40h", true);
        history(&branch(&store, "what-if", false));
        let sent = tick(&mut feed, &app, &events);
        let mut seen = kinds(&sent);
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen, ["branch", "changeset", "proposal", "view"]);
        published(&sent);
    }

    #[test]
    fn a_plain_file_has_no_feed() {
        let file = std::path::Path::new(super::against_mcp::FIXTURE);
        let app = App::new(file.to_owned(), View::open(file).unwrap());
        assert!(Feed::new(&app).unwrap().is_none());
    }
}

/// Golden examples of the API's answers, one per published schema the web app
/// reads, in ui/src/api/golden/. The app's tests check each against its
/// schema and its generated type, so the app and the server can't drift
/// apart unnoticed. IDs, times and paths are made stable.
///
/// Set `TOTALFLOAT_UPDATE_CONTRACT=1` to rewrite them after a deliberate
/// change, and say why in the PR.
mod golden {
    use std::collections::BTreeMap;

    use serde_json::{Value, json};
    use totalfloat_server::Events;

    use super::against_mcp::{call, started};
    use crate::serve::feed::Feed;
    use crate::serve::{App, view::View};

    /// Each 64-digit ID, numbered in the order first seen, so the examples
    /// stay the same from run to run while keeping which ID is which.
    #[derive(Default)]
    struct Stable {
        ids: BTreeMap<String, String>,
        /// The temporary directory, as given and as resolved, each with its
        /// trailing separator, which differ by OS.
        directories: Vec<String>,
        /// Who the changes are by, which comes from the account running them.
        author: String,
    }

    impl Stable {
        fn collect(&mut self, value: &Value) {
            match value {
                Value::String(text) => {
                    for id in hex_runs(text, 64) {
                        let next = format!("{:02x}", self.ids.len() + 1).repeat(32);
                        self.ids.entry(id.to_owned()).or_insert(next);
                    }
                }
                Value::Array(items) => items.iter().for_each(|v| self.collect(v)),
                Value::Object(map) => map.values().for_each(|v| self.collect(v)),
                _ => {}
            }
        }

        fn apply(&self, value: &mut Value) {
            match value {
                Value::String(text) if *text == self.author => *text = "Pat".to_owned(),
                Value::String(text) => {
                    let mut stable = self
                        .directories
                        .iter()
                        .fold(text.clone(), |text, directory| text.replace(directory, ""));
                    // What's left of a path in the directory is written with
                    // `/`, as on Unix, so Windows gives the same example.
                    if stable != *text {
                        stable = stable.replace(std::path::MAIN_SEPARATOR, "/");
                    }
                    let mut stable = stable.replace(&format!("by {}", self.author), "by Pat");
                    for (id, replacement) in &self.ids {
                        stable = stable.replace(id, replacement);
                        stable = stable.replace(&id[..10], &replacement[..10]);
                    }
                    *text = stable_time(&stable);
                }
                Value::Array(items) => items.iter_mut().for_each(|v| self.apply(v)),
                Value::Object(map) => map.values_mut().for_each(|v| self.apply(v)),
                _ => {}
            }
        }
    }

    /// Runs of exactly `length` lower-case hex digits in `text`.
    fn hex_runs(text: &str, length: usize) -> Vec<&str> {
        let hex = |b: u8| b.is_ascii_digit() || (b'a'..=b'f').contains(&b);
        let bytes = text.as_bytes();
        let mut runs = Vec::new();
        let mut start = 0;
        while start < bytes.len() {
            let end = start + bytes[start..].iter().take_while(|b| hex(**b)).count();
            if end - start == length {
                runs.push(&text[start..end]);
            }
            start = end + 1;
        }
        runs
    }

    /// Any `YYYY-MM-DDTHH:MM:SSZ` time, which comes from the clock, as one
    /// fixed time.
    fn stable_time(text: &str) -> String {
        let shape = |w: &[u8]| {
            w.len() == 20
                && w.iter().enumerate().all(|(i, b)| match i {
                    4 | 7 => *b == b'-',
                    10 => *b == b'T',
                    13 | 16 => *b == b':',
                    19 => *b == b'Z',
                    _ => b.is_ascii_digit(),
                })
        };
        let mut out = text.to_owned();
        let mut at = 0;
        while at + 20 <= out.len() {
            if out.is_char_boundary(at)
                && out.is_char_boundary(at + 20)
                && shape(&out.as_bytes()[at..at + 20])
            {
                out.replace_range(at..at + 20, "2026-11-02T09:00:00Z");
                at += 20;
            } else {
                at += 1;
            }
        }
        out
    }

    async fn ok(store: &std::path::Path, method: &str, path: &str, body: Option<Value>) -> Value {
        let (status, answer) = call(store, method, path, body).await;
        assert!(status.is_success(), "{method} {path}: {status} {answer}");
        answer
    }

    type Answers = BTreeMap<&'static str, Value>;

    fn edit(head: &Value, propose: bool) -> Value {
        let duration = if propose { "40h" } else { "32h" };
        json!({
            "edits": [{"op": "set_duration", "activity": "A2", "original": duration}],
            "head": head, "message": "Crane late", "propose": propose,
        })
    }

    async fn head(store: &std::path::Path) -> Value {
        ok(store, "GET", "/api/1/summary", None).await["head"].clone()
    }

    /// The answers, by schema file name.
    async fn answers(directory: &std::path::Path) -> Answers {
        let store = started(directory);
        let app = App::new(store.clone(), View::open(&store).unwrap());
        let mut feed = Feed::new(&app).unwrap().unwrap();
        let mut out = Answers::new();
        let edited = ok(
            &store,
            "POST",
            "/api/1/edit",
            Some(edit(&head(&store).await, false)),
        )
        .await;
        out.insert("edit-1", edited);
        let refused = json!({
            "edits": [
                {"op": "set_duration", "activity": "NOPE1", "original": "2d"},
                {"op": "set_duration", "activity": "NOPE2", "original": "2d"}
            ],
            "head": head(&store).await,
            "message": "Typos",
        });
        let refused = call(&store, "POST", "/api/1/edit", Some(refused)).await.1;
        out.insert("error-1", refused);
        reads(&store, &mut out).await;
        refs(&store, &mut out).await;
        ok(
            &store,
            "POST",
            "/api/1/edit",
            Some(edit(&head(&store).await, true)),
        )
        .await;
        reviewed(&store, &mut out).await;
        portfolio(directory, &mut out).await;
        let (sent, refreshed) = feed.tick(&app, &Events::default());
        refreshed.unwrap();
        out.insert("event-1", Value::Array(sent));
        out
    }

    /// A portfolio's figures and capacity.
    async fn portfolio(directory: &std::path::Path, out: &mut Answers) {
        let inside = directory.join("portfolio");
        std::fs::create_dir(&inside).unwrap();
        let app = super::against_mcp::portfolio_app(&inside);
        for (name, path) in [
            ("portfolio-show-1", "/api/1/portfolio"),
            ("portfolio-capacity-1", "/api/1/portfolio/capacity?by=week"),
        ] {
            let (status, answer) = super::against_mcp::call_app(&app, "GET", path, None).await;
            assert!(status.is_success(), "{path}: {answer}");
            out.insert(name, answer);
        }
    }

    async fn reads(store: &std::path::Path, out: &mut Answers) {
        let layout: Value = serde_json::from_str(crate::serve::layouts::tests::GROUPED).unwrap();
        ok(store, "PUT", "/api/1/layouts/Look%20ahead", Some(layout)).await;
        let filter: Value =
            serde_json::from_str(crate::serve::filters::tests::CRITICAL_DESIGN).unwrap();
        ok(
            store,
            "PUT",
            "/api/1/filters/Critical%20design",
            Some(filter),
        )
        .await;
        for (name, path) in [
            ("summary-1", "/api/1/summary"),
            ("rows-1", "/api/1/rows"),
            ("links-1", "/api/1/links"),
            ("wbs-1", "/api/1/wbs"),
            ("explain-1", "/api/1/explain?activity=A2"),
            ("check-1", "/api/1/check"),
            (
                "query-1",
                "/api/1/query?status=not-started&columns=id,code,name",
            ),
            ("layouts-1", "/api/1/layouts"),
            ("fields-1", "/api/1/fields"),
            ("layout-1", "/api/1/layouts/Look%20ahead"),
            ("filters-1", "/api/1/filters"),
            ("filter-1", "/api/1/filters/Critical%20design"),
            ("history-log-1", "/api/1/history/log"),
            ("changeset-1", "/api/1/history/show/main"),
            ("history-diff-1", "/api/1/history/diff"),
        ] {
            out.insert(name, ok(store, "GET", path, None).await);
        }
        let risk = json!({"inputs": {"default": {"min": -10, "max": 25}}, "iterations": 200});
        out.insert(
            "risk-analysis-1",
            ok(store, "POST", "/api/1/risk", Some(risk)).await,
        );
    }

    /// Branches and baselines made and listed.
    async fn refs(store: &std::path::Path, out: &mut Answers) {
        for (name, path, body) in [
            (
                "history-branch-1",
                "branch",
                json!({"action": "create", "name": "what-if"}),
            ),
            ("history-branches-1", "branch", json!({"action": "list"})),
            (
                "history-baseline-1",
                "baseline",
                json!({"action": "create", "name": "submitted"}),
            ),
            ("history-baselines-1", "baseline", json!({"action": "list"})),
        ] {
            let path = format!("/api/1/history/{path}");
            out.insert(name, ok(store, "POST", &path, Some(body)).await);
        }
    }

    /// A proposal listed and accepted, a merge tried, and a revert.
    async fn reviewed(store: &std::path::Path, out: &mut Answers) {
        let proposals = ok(store, "GET", "/api/1/history/proposals", None).await;
        let name = proposals["proposals"][0]["name"].clone();
        out.insert("history-proposals-1", proposals);
        for (key, path, body) in [
            (
                "history-review-1",
                "accept",
                json!({"proposal": name, "reason": "Agreed"}),
            ),
            (
                "history-merge-1",
                "merge",
                json!({"branch": "what-if", "dry_run": true}),
            ),
            (
                "history-revert-1",
                "revert",
                json!({"reason": "Not after all"}),
            ),
        ] {
            let path = format!("/api/1/history/{path}");
            out.insert(key, ok(store, "POST", &path, Some(body)).await);
        }
    }

    #[tokio::test]
    async fn the_app_s_golden_examples_are_the_server_s_answers() {
        let directory = tempfile::tempdir().unwrap();
        let answers = answers(directory.path()).await;
        let mut stable = Stable {
            directories: [
                directory.path().to_path_buf(),
                directory.path().canonicalize().unwrap(),
            ]
            .iter()
            .map(|path| format!("{}{}", path.display(), std::path::MAIN_SEPARATOR))
            .collect(),
            author: crate::history::as_person(|| crate::history::author(None).0),
            ..Stable::default()
        };
        for answer in answers.values() {
            stable.collect(answer);
        }
        let golden = super::root().join("ui/src/api/golden");
        let update = std::env::var_os("TOTALFLOAT_UPDATE_CONTRACT").is_some();
        for (name, mut answer) in answers {
            stable.apply(&mut answer);
            let text = serde_json::to_string_pretty(&answer).unwrap() + "\n";
            let path = golden.join(format!("{name}.json"));
            if update {
                std::fs::create_dir_all(&golden).unwrap();
                std::fs::write(&path, &text).unwrap();
                continue;
            }
            let published = std::fs::read_to_string(&path).unwrap_or_default();
            assert_eq!(
                published, text,
                "ui/src/api/golden/{name}.json is out of date; rerun with TOTALFLOAT_UPDATE_CONTRACT=1"
            );
        }
    }
}

/// The view after a recorded edit, built from the edit's own work where it
/// can be, answers as a server started on the new head does.
mod after_an_edit {
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    use axum::body::{Body, to_bytes};
    use axum::http::{Request, StatusCode};
    use serde_json::{Value, json};
    use tower::ServiceExt;

    use crate::history::Action;
    use crate::serve::{App, api, view::View};

    /// A store started from `text`, as `name.xer`.
    fn store_of(directory: &Path, name: &str, text: &str) -> PathBuf {
        let file = directory.join(format!("{name}.xer"));
        std::fs::write(&file, text).unwrap();
        let init = Action::Init {
            file,
            store: None,
            message: Some("Start".to_owned()),
            author: Some("Pat".to_owned()),
            json: true,
        };
        let (done, _) = crate::output::captured(|| crate::history::run(&init));
        done.unwrap();
        directory.join(format!("{name}.tfdb"))
    }

    /// A store started from a copy of `file`, if it can be.
    fn store_copied(directory: &Path, file: &Path) -> Option<PathBuf> {
        let copy = directory.join("corpus.xer");
        std::fs::copy(file, &copy).ok()?;
        let init = Action::Init {
            file: copy,
            store: None,
            message: Some("Start".to_owned()),
            author: Some("Pat".to_owned()),
            json: true,
        };
        let (done, _) = crate::output::captured(|| crate::history::run(&init));
        done.ok().map(|_| directory.join("corpus.tfdb"))
    }

    /// The corpus's files that calculate, one store each from a duration
    /// edit recorded on its first unstarted activity: the view built from
    /// the edit answers as one opened afresh. Counts only, never content.
    /// Skips when `TOTALFLOAT_CORPUS` is unset.
    #[test]
    fn on_every_corpus_file_the_view_after_an_edit_is_the_fresh_one() {
        let Some(root) = std::env::var_os("TOTALFLOAT_CORPUS") else {
            eprintln!("TOTALFLOAT_CORPUS unset; skipping the view-after-an-edit corpus test");
            return;
        };
        let mut files = Vec::new();
        for entry in std::fs::read_dir(Path::new(&root).join("files")).unwrap() {
            for file in std::fs::read_dir(entry.unwrap().path()).unwrap() {
                files.push(file.unwrap().path());
            }
        }
        files.sort();
        let (mut stood, mut again, mut skipped) = (0, 0, 0);
        for (index, file) in files.iter().enumerate() {
            let directory = tempfile::tempdir().unwrap();
            let Some(store) = store_copied(directory.path(), file) else {
                skipped += 1;
                continue;
            };
            let Ok(view) = View::open(&store) else {
                skipped += 1;
                continue;
            };
            let Some(held) = view.held() else {
                skipped += 1;
                continue;
            };
            let unstarted = held.read.schedules.iter().find_map(|s| {
                s.activities.values().find(|a| {
                    a.status == totalfloat_model::ActivityStatus::NotStarted
                        && !a.activity_type.is_milestone()
                        && !a.activity_type.is_span()
                })
            });
            let Some(activity) = unstarted else {
                skipped += 1;
                continue;
            };
            let edits: Vec<totalfloat_edit::Edit> = serde_json::from_value(json!([
                {"op": "set_duration", "activity": activity.code, "original": "40h"}
            ]))
            .unwrap();
            let (branch, tip) = view.point().unwrap();
            let options = crate::edit::Options {
                project: None,
                data_date: None,
                as_planned: false,
                check: false,
                skip_failed: false,
            };
            let asked = crate::history::Asked {
                message: Some("Edited"),
                author: Some("Pat"),
                propose: false,
            };
            let edited = crate::edit::edit_store_calculated(
                (&store, branch.as_deref(), Some(tip.as_str())),
                &edits,
                (options, Some(held)),
                (asked, false),
            );
            let Ok((report, _, calculation)) = edited else {
                skipped += 1;
                continue;
            };
            let Some(recorded) = report.recorded() else {
                skipped += 1;
                continue;
            };
            let after = calculation.map(|calculation| crate::serve::view::After {
                head: recorded.id.clone(),
                calculation,
            });
            let stands = after.is_some();
            let shown = View::open_after(&store, after).unwrap();
            let fresh = View::open(&store).unwrap();
            assert_eq!(shown.stands_on_an_edit(), stands, "file {index}");
            assert!(shown.rows() == fresh.rows(), "file {index}: rows");
            assert!(shown.links() == fresh.links(), "file {index}: links");
            assert!(shown.wbs() == fresh.wbs(), "file {index}: wbs");
            let (kept, read) = (shown.held().unwrap(), fresh.held().unwrap());
            assert!(kept.read.schedules == read.read.schedules, "file {index}");
            assert!(
                kept.calculated.schedules == read.calculated.schedules,
                "file {index}"
            );
            assert!(kept.critical == read.critical, "file {index}");
            if stands {
                stood += 1;
            } else {
                again += 1;
            }
        }
        eprintln!(
            "view after an edit: {stood} stood on the edit, {again} calculated again, {skipped} skipped"
        );
        assert!(stood > 0);
    }

    fn fixture(name: &str) -> String {
        std::fs::read_to_string(Path::new("../../fixtures/xer").join(name)).unwrap()
    }

    /// Records `edit` (its edits and settings) through the app, then checks
    /// the view shown against one opened afresh, and whether it stood on
    /// the edit's calculation.
    async fn recorded(store: &Path, edit: Value, stands: bool) {
        let app = Arc::new(App::new(store.to_owned(), View::open(store).unwrap()));
        let head = app.current().point().unwrap().1;
        let mut body = json!({"head": head, "message": "Edited"});
        body.as_object_mut()
            .unwrap()
            .extend(edit.as_object().unwrap().clone());
        let request = Request::builder()
            .method("POST")
            .uri("/api/1/edit")
            .body(Body::from(body.to_string()))
            .unwrap();
        let response = api(Arc::clone(&app)).oneshot(request).await.unwrap();
        let status = response.status();
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let answer: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(status, StatusCode::OK, "{answer}");
        assert!(answer["recorded"]["id"].is_string(), "{answer}");
        let shown = app.current();
        assert_ne!(shown.point().unwrap().1, head, "the view moved on");
        let fresh = View::open(store).unwrap();
        assert_eq!(shown.stands_on_an_edit(), stands, "{edit}");
        assert_eq!(shown.rows().unwrap(), fresh.rows().unwrap(), "{edit}");
        assert_eq!(shown.links().unwrap(), fresh.links().unwrap(), "{edit}");
        assert_eq!(shown.wbs().unwrap(), fresh.wbs().unwrap(), "{edit}");
        assert_eq!(
            serde_json::to_value(shown.summary()).unwrap(),
            serde_json::to_value(fresh.summary()).unwrap()
        );
        // What the next edit is measured against is what a fresh server holds.
        let (kept, again) = (shown.held().unwrap(), fresh.held().unwrap());
        assert_eq!(kept.snapshot, again.snapshot);
        assert_eq!(kept.read.schedules, again.read.schedules);
        assert_eq!(kept.calculated.schedules, again.calculated.schedules);
        assert_eq!(kept.critical, again.critical);
    }

    #[tokio::test]
    async fn one_project_stands_on_the_edit() {
        let directory = tempfile::tempdir().unwrap();
        let store = store_of(
            directory.path(),
            "one",
            &fixture("batches/001/rel-fs-positive.xer"),
        );
        recorded(
            &store,
            json!({"edits": [{"op": "set_duration", "activity": "A2", "original": "32h"}]}),
            true,
        )
        .await;
        // Its data date moved, with progress, on work with an assignment.
        let store = store_of(directory.path(), "curves", &fixture("curves.xer"));
        let progress = json!({
            "edits": [{"op": "set_progress", "activity": "A1", "remaining": "8h", "percent": 60.0}],
            "data_date": "2026-01-07 08:00",
        });
        recorded(&store, progress, true).await;
    }

    #[tokio::test]
    async fn linked_projects_calculate_the_new_head_again() {
        let directory = tempfile::tempdir().unwrap();
        let store = store_of(directory.path(), "linked", &fixture("two-projects.xer"));
        let edit = json!({"edits": [{"op": "set_duration", "activity": "A1", "original": "40h"}]});
        recorded(&store, edit, false).await;
        let moved = json!({"edits": [], "data_date": "2026-01-07 08:00"});
        recorded(&store, moved, false).await;
    }

    #[tokio::test]
    async fn unlinked_projects_stand_on_the_edit_only_when_it_calculated_both() {
        let directory = tempfile::tempdir().unwrap();
        let link = "%R\t1002\t2\t201\t1\t101\tPR_SS\t-1\n";
        let text = fixture("two-projects.xer");
        assert!(text.contains(link));
        let store = store_of(directory.path(), "apart", &text.replace(link, ""));
        // One project's edit calculates that project alone.
        let edit = json!({"edits": [{"op": "set_duration", "activity": "A1", "original": "40h"}]});
        recorded(&store, edit, false).await;
        // A new data date calculates both.
        let moved = json!({"edits": [], "data_date": "2026-01-07 08:00"});
        recorded(&store, moved, true).await;
    }
}
