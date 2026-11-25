//! Layouts as files: how the grid and Gantt are set out (columns, sorting,
//! grouping, filters, zoom, logic lines and the split), saved as
//! `totalfloat.layout/1` JSON in a `<name>.layouts/` directory beside the
//! store or file served, one file per layout, so they can be shared and
//! diffed. Each is written pretty-printed in a fixed key order.

use std::collections::BTreeMap;
use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Path as UrlPath, State};
use axum::response::Response;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::shelf::{self, Kind, blocking, bytes_response};
use super::{App, json, refusal};
use crate::error::CliError;

/// How the grid and Gantt are set out (`totalfloat.layout/1`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Layout {
    /// Always `totalfloat.layout/1`.
    pub schema: LayoutSchema,
    /// The grid's columns, in order, each with its width, each once. Columns
    /// not listed are hidden.
    #[schemars(length(min = 1))]
    pub columns: Vec<Column>,
    /// Sort keys, first key first.
    #[serde(default)]
    pub sort: Vec<SortKey>,
    /// How rows are grouped.
    pub group: Group,
    /// Further levels of grouping, in order, each splitting every band of the
    /// level before it, such as `["code:Phase"]` under `"group": "wbs"`. Any
    /// grouping but `none`, each once, and none of them `group`; three at
    /// most, four levels in all. Left out when there are none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[schemars(length(max = 3))]
    pub then_by: Vec<Group>,
    /// What the grid shows.
    #[serde(default)]
    pub filters: Filters,
    /// The Gantt chart's settings.
    pub gantt: Gantt,
    /// The grid's share of the width beside the Gantt, from 0.15 to 0.85.
    #[schemars(range(min = 0.15, max = 0.85))]
    pub split: f64,
}

/// The schema name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum LayoutSchema {
    /// The only version.
    #[serde(rename = "totalfloat.layout/1")]
    V1,
}

/// A grid column shown.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Column {
    /// Which column.
    pub field: Field,
    /// Its width in CSS pixels, from 40 to 2000.
    #[schemars(range(min = 40, max = 2000))]
    pub width: u32,
}

/// The grid's columns, by the field each shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[allow(missing_docs, reason = "each names the grid column of the same name")]
pub enum Field {
    Code,
    Name,
    Original,
    Remaining,
    Start,
    Finish,
    Float,
    Calendar,
    Wbs,
    ActualStart,
    ActualFinish,
    Constraint,
    ConstraintDate,
}

/// A sort key.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SortKey {
    /// The column sorted on.
    pub field: Field,
    /// Largest or latest first.
    #[serde(default)]
    pub descending: bool,
}

/// How rows are grouped: `wbs` for WBS bands, `none`, an activity field
/// (`status`, `type`, `calendar`, `constraint`, or the month work starts or
/// finishes, `start_month` or `finish_month`), or a field as `query` names a
/// column, `code:TYPE` for an activity code type or `udf:LABEL` for an
/// activity user-defined field, for a band per value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct Group(
    #[schemars(regex(
        pattern = r"^(wbs|none|status|type|calendar|constraint|start_month|finish_month|code:.+|udf:.+)$"
    ))]
    pub String,
);

/// Further levels a layout keeps at most, as the app's grid offers.
const MAX_THEN_BY: usize = 3;

/// The groupings a layout names outright; codes and user fields are named
/// by prefix.
const GROUPS: [&str; 8] = [
    "wbs",
    "none",
    "status",
    "type",
    "calendar",
    "constraint",
    "start_month",
    "finish_month",
];

/// The filters a built filter can hold, as `query` names its arguments.
pub(super) const BUILT_FILTERS: [&str; 14] = [
    "code",
    "wbs",
    "resource",
    "status",
    "type",
    "float_min",
    "float_max",
    "critical",
    "constrained",
    "start_from",
    "start_to",
    "finish_from",
    "finish_to",
    "text",
];

/// What the grid shows.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Filters {
    /// Text the activity ID or name holds, in any case.
    #[serde(default)]
    pub text: String,
    /// Quick filters: critical narrows; the statuses widen each other.
    #[serde(default)]
    pub quick: Vec<Quick>,
    /// The filter builder's conditions, as `query`'s parameters, such as
    /// `{"code": "Phase=DESIGN", "float_max": "40"}`. Left out when there are
    /// none.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub built: BTreeMap<String, String>,
    /// Groups of conditions in the same form, as `query`'s `any`: an activity
    /// has to meet every condition of at least one group, besides `built`.
    /// Left out when there are none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[schemars(extend("items" = {"type": "object", "minProperties": 1, "additionalProperties": {"type": "string"}, "propertyNames": {"enum": BUILT_FILTERS}}))]
    pub any: Vec<BTreeMap<String, String>>,
}

/// A quick filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
#[allow(missing_docs, reason = "each names the filter of the same name")]
pub enum Quick {
    Critical,
    NotStarted,
    InProgress,
    Completed,
}

/// The Gantt chart's settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Gantt {
    /// The time scale.
    pub zoom: Zoom,
    /// Which logic lines show.
    pub lines: Lines,
    /// Whether today's line shows.
    #[serde(default)]
    pub today: bool,
}

/// The Gantt's time scale: `fit`, which fits the dates shown into the
/// chart's width each time they change, or a fixed scale.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum Zoom {
    /// `fit`.
    Fit(Fit),
    /// A fixed scale.
    Scale(Scale),
}

/// Fit the dates shown into the chart's width.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Fit {
    /// The only value.
    Fit,
}

/// A fixed time scale and its two tiers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Scale {
    /// CSS pixels per day, from 0.01 to 10000.
    #[schemars(range(min = 0.01, max = 10000))]
    pub px_per_day: f64,
    /// The upper tier of the time axis.
    pub top: Tier,
    /// The lower tier of the time axis.
    pub bottom: Tier,
}

/// A tier of the time axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[allow(missing_docs, reason = "each names its unit of time")]
pub enum Tier {
    Year,
    Quarter,
    Month,
    Week,
    Day,
}

/// Which logic lines show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Lines {
    /// Every one.
    All,
    /// The selected activity's.
    Selected,
    /// None.
    None,
}

/// `GET /api/1/layouts`: the layouts saved, by name.
#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct Listing {
    #[schemars(extend("const" = "totalfloat.layouts/1"))]
    schema: &'static str,
    layouts: Vec<Named>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct Named {
    /// The file's name in the layouts folder, without `.json`.
    name: String,
}

/// `DELETE /api/1/layouts/{name}`: the layout is gone.
#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct Deleted {
    #[schemars(extend("const" = "totalfloat.layouts/1"))]
    schema: &'static str,
    #[schemars(extend("const" = true))]
    deleted: bool,
}

fn bad_layout(message: String, hint: &str) -> CliError {
    CliError::new("bad-layout", message, hint)
}

/// Reads a layout, naming the field that's wrong, if any.
///
/// # Errors
///
/// `bad-layout`, naming the field.
pub fn parse(bytes: &[u8]) -> Result<Layout, CliError> {
    let reader = &mut serde_json::Deserializer::from_slice(bytes);
    let layout: Layout = serde_path_to_error::deserialize(&mut *reader).map_err(|error| {
        let path = error.path().to_string();
        let at = if path == "." {
            "the layout".to_owned()
        } else {
            path
        };
        bad_layout(
            format!("{at}: {}", error.inner()),
            "a layout is totalfloat.layout/1; see docs/schemas/layout-1.schema.json",
        )
    })?;
    // One layout, and nothing after it.
    reader.end().map_err(|error| {
        bad_layout(
            format!("the layout: more follows it ({error})"),
            "send one JSON object",
        )
    })?;
    check(&layout)?;
    Ok(layout)
}

/// Checks `any` groups as [`check_built`] checks a built filter, each one
/// holding a condition at least. The error's first part is where, as `[1].code`.
///
/// # Errors
///
/// Where, what's wrong, and a hint.
pub(super) fn check_any(
    any: &[BTreeMap<String, String>],
) -> Result<(), (String, String, &'static str)> {
    for (at, group) in any.iter().enumerate() {
        if group.is_empty() {
            return Err((
                format!("[{at}]"),
                "a group holds no condition".to_owned(),
                "give each group one of query's filters at least, or leave the group out",
            ));
        }
        check_built(group).map_err(|(field, why, hint)| (format!("[{at}].{field}"), why, hint))?;
    }
    Ok(())
}

/// Checks a built filter's conditions, as a layout or a named filter holds
/// them: each one of `query`'s filters, with a value the filter builder
/// shows as it is, so applying it in the app finds what `query` would.
///
/// # Errors
///
/// The condition's name, what's wrong, and a hint.
pub(super) fn check_built(
    built: &BTreeMap<String, String>,
) -> Result<(), (String, String, &'static str)> {
    for (name, value) in built {
        let wrong = |why: &str, hint: &'static str| Err((name.clone(), why.to_owned(), hint));
        if !BUILT_FILTERS.contains(&name.as_str()) {
            return wrong(
                "that isn't a filter `query` takes",
                "the filters are query's: code, wbs, resource, status, type, float_min, float_max, critical, constrained, start_from, start_to, finish_from, finish_to, text",
            );
        }
        let value = value.trim();
        if value.is_empty() {
            return wrong(
                "it has no value",
                "give the value query takes, such as Phase=DESIGN or 40",
            );
        }
        match name.as_str() {
            "critical" | "constrained" if value != "true" => {
                return wrong(
                    &format!("{value} isn't true; leave the condition out to not filter on it"),
                    "a switch condition is \"true\"",
                );
            }
            "float_min" | "float_max" if value.parse::<f64>().map_or(true, |v| !v.is_finite()) => {
                return wrong(
                    &format!("{value} isn't a number of hours"),
                    "give hours, such as 40 or -8",
                );
            }
            "start_from" | "finish_from" | "start_to" | "finish_to" => {
                let time = if name.ends_with("_to") {
                    "23:59"
                } else {
                    "00:00"
                };
                let day = value.strip_suffix(&format!(" {time}")).unwrap_or("");
                let whole_day =
                    day.len() == 10 && value.parse::<totalfloat_model::DateTime>().is_ok();
                if !whole_day {
                    return wrong(
                        &format!("{value} isn't a day as the filter builder gives one"),
                        "give YYYY-MM-DD 00:00 for a _from bound and YYYY-MM-DD 23:59 for a _to bound",
                    );
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// What a layout's types can't say: sizes in range, and each column once.
fn check(layout: &Layout) -> Result<(), CliError> {
    if !(0.15..=0.85).contains(&layout.split) {
        return Err(bad_layout(
            format!("split: {} isn't from 0.15 to 0.85", layout.split),
            "it's the grid's share of the width",
        ));
    }
    if layout.columns.is_empty() {
        return Err(bad_layout(
            "columns: a layout shows at least one column".to_owned(),
            "list the columns to show, in order",
        ));
    }
    for (k, column) in layout.columns.iter().enumerate() {
        if !(40..=2000).contains(&column.width) {
            return Err(bad_layout(
                format!("columns[{k}].width: {} isn't from 40 to 2000", column.width),
                "widths are CSS pixels",
            ));
        }
        if layout.columns[..k].iter().any(|c| c.field == column.field) {
            return Err(bad_layout(
                format!("columns[{k}].field: that column is listed already"),
                "list each column once",
            ));
        }
    }
    check_group(&layout.group, "group")?;
    if layout.then_by.len() > MAX_THEN_BY {
        return Err(bad_layout(
            format!(
                "then_by: {} further levels is more than {MAX_THEN_BY}",
                layout.then_by.len()
            ),
            "group by four levels at most in all",
        ));
    }
    for (k, level) in layout.then_by.iter().enumerate() {
        let at = format!("then_by[{k}]");
        check_group(level, &at)?;
        if level.0 == "none" || layout.group.0 == "none" {
            return Err(bad_layout(
                format!("{at}: further levels need a grouping, and can't be none"),
                "group by something first, then by the further levels",
            ));
        }
        if level == &layout.group || layout.then_by[..k].contains(level) {
            return Err(bad_layout(
                format!("{at}: {} is a level already", level.0),
                "name each level once",
            ));
        }
    }
    check_built(&layout.filters.built)
        .map_err(|(field, why, hint)| bad_layout(format!("filters.built.{field}: {why}"), hint))?;
    check_any(&layout.filters.any)
        .map_err(|(at, why, hint)| bad_layout(format!("filters.any{at}: {why}"), hint))?;
    if let Zoom::Scale(Scale {
        px_per_day: zoom, ..
    }) = layout.gantt.zoom
        && !(0.01..=10_000.0).contains(&zoom)
    {
        return Err(bad_layout(
            format!("gantt.zoom.px_per_day: {zoom} isn't from 0.01 to 10000"),
            "it's CSS pixels per day",
        ));
    }
    Ok(())
}

/// Refuses a grouping that isn't `wbs`, `none`, an activity field,
/// `code:TYPE` or `udf:LABEL`; `at` says where it is.
fn check_group(group: &Group, at: &str) -> Result<(), CliError> {
    let group = group.0.as_str();
    let named = |prefix: &str| {
        group
            .strip_prefix(prefix)
            .is_some_and(|name| !name.is_empty())
    };
    if !(GROUPS.contains(&group) || named("code:") || named("udf:")) {
        return Err(bad_layout(
            format!(
                "{at}: {group} isn't {}, code:TYPE or udf:LABEL",
                GROUPS.join(", ")
            ),
            "group by the WBS, not at all, an activity field, an activity code type or a user field",
        ));
    }
    Ok(())
}

/// A layout as it's saved: pretty-printed, in the types' key order, with a
/// final newline, so the files diff cleanly.
///
/// # Errors
///
/// `serve-failed` if it can't be written as JSON.
pub fn canonical(layout: &Layout) -> Result<Vec<u8>, CliError> {
    let mut bytes = serde_json::to_vec_pretty(layout).map_err(|error| {
        CliError::new(
            "serve-failed",
            format!("the layout couldn't be written as JSON: {error}"),
            "this is a bug",
        )
    })?;
    bytes.push(b'\n');
    Ok(bytes)
}

/// Layouts on the shelf beside the schedule.
pub static LAYOUTS: Kind = Kind {
    folder: "layouts",
    noun: "layout",
    bad_name: "bad-layout-name",
    missing: "no-such-layout",
    bad_file: "bad-layout-file",
    schema_file: "docs/schemas/layout-1.schema.json",
    check: |body| parse(body).map(drop),
    canonical: |body| canonical(&parse(body)?),
};

/// `GET /api/1/layouts`.
pub async fn get_list(State(app): State<Arc<App>>) -> Response {
    // Not while a save or delete is part way through.
    let _turn = app.writing.lock().await;
    let view = app.current();
    match blocking(&LAYOUTS, move || shelf::list(&LAYOUTS, &view)).await {
        Ok(names) => json(&Listing {
            schema: "totalfloat.layouts/1",
            layouts: names.into_iter().map(|name| Named { name }).collect(),
        }),
        Err(error) => refusal(&error),
    }
}

/// `GET /api/1/layouts/{name}`: the file as saved.
pub async fn get_one(State(app): State<Arc<App>>, UrlPath(name): UrlPath<String>) -> Response {
    let _turn = app.writing.lock().await;
    let view = app.current();
    match blocking(&LAYOUTS, move || shelf::read(&LAYOUTS, &view, &name)).await {
        Ok(bytes) => bytes_response(bytes),
        Err(error) => refusal(&error),
    }
}

/// `PUT /api/1/layouts/{name}`: saves it, answering with the file written.
pub async fn put_one(
    State(app): State<Arc<App>>,
    UrlPath(name): UrlPath<String>,
    body: Bytes,
) -> Response {
    // One change at a time, as with the schedule's.
    let _turn = app.writing.lock().await;
    let view = app.current();
    match blocking(&LAYOUTS, move || {
        shelf::write(&LAYOUTS, &view, &name, &body)
    })
    .await
    {
        Ok(bytes) => bytes_response(bytes),
        Err(error) => refusal(&error),
    }
}

/// `DELETE /api/1/layouts/{name}`.
pub async fn delete_one(State(app): State<Arc<App>>, UrlPath(name): UrlPath<String>) -> Response {
    let _turn = app.writing.lock().await;
    let view = app.current();
    match blocking(&LAYOUTS, move || shelf::delete(&LAYOUTS, &view, &name)).await {
        Ok(()) => json(&Deleted {
            schema: "totalfloat.layouts/1",
            deleted: true,
        }),
        Err(error) => refusal(&error),
    }
}

#[cfg(test)]
pub(super) mod tests {
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    use axum::body::{Body, to_bytes};
    use axum::http::{Request, StatusCode};
    use serde_json::Value;
    use tower::ServiceExt;

    use super::Layout;
    use crate::serve::{App, api, view::View};

    const FIXTURE: &str = "../../fixtures/xer/batches/001/rel-fs-positive.xer";

    fn root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    /// The published schema: generated from the type, with the version as `$id`.
    fn generated() -> String {
        let mut schema = serde_json::to_value(schemars::schema_for!(Layout)).unwrap();
        let object = schema.as_object_mut().unwrap();
        object.insert("$id".into(), Value::from("totalfloat.layout/1"));
        object.insert("title".into(), Value::from("totalfloat.layout/1"));
        serde_json::to_string_pretty(&schema).unwrap() + "\n"
    }

    /// Set `TOTALFLOAT_UPDATE_CONTRACT=1` to rewrite it after a deliberate,
    /// compatible change, and say why in the PR.
    #[test]
    fn the_published_layout_schema_matches_the_type() {
        let path = root().join("docs/schemas/layout-1.schema.json");
        let generated = generated();
        if std::env::var_os("TOTALFLOAT_UPDATE_CONTRACT").is_some() {
            std::fs::write(&path, &generated).unwrap();
            return;
        }
        let published = std::fs::read_to_string(&path).unwrap_or_default();
        assert_eq!(
            published, generated,
            "docs/schemas/layout-1.schema.json is out of date; rerun with TOTALFLOAT_UPDATE_CONTRACT=1"
        );
    }

    /// A layout as the app writes it.
    /// A layout as the app saves one.
    pub(in crate::serve) const LAYOUT: &str = r#"{
  "schema": "totalfloat.layout/1",
  "columns": [
    {
      "field": "name",
      "width": 300
    },
    {
      "field": "code",
      "width": 90
    },
    {
      "field": "finish",
      "width": 120
    }
  ],
  "sort": [
    {
      "field": "finish",
      "descending": true
    }
  ],
  "group": "none",
  "filters": {
    "text": "pour",
    "quick": [
      "critical"
    ]
  },
  "gantt": {
    "zoom": {
      "px_per_day": 24.0,
      "top": "month",
      "bottom": "day"
    },
    "lines": "selected",
    "today": true
  },
  "split": 0.55
}
"#;

    /// A layout grouped by an activity code, with a built filter.
    pub(in crate::serve) const GROUPED: &str = r#"{
  "schema": "totalfloat.layout/1",
  "columns": [
    {
      "field": "code",
      "width": 90
    }
  ],
  "sort": [],
  "group": "code:Phase",
  "filters": {
    "text": "",
    "quick": [],
    "built": {
      "code": "Phase=DESIGN",
      "float_max": "40",
      "start_from": "2026-01-05 00:00"
    }
  },
  "gantt": {
    "zoom": {
      "px_per_day": 24.0,
      "top": "month",
      "bottom": "day"
    },
    "lines": "all",
    "today": false
  },
  "split": 0.5
}
"#;

    #[tokio::test]
    async fn a_code_grouping_and_a_built_filter_are_kept() {
        let directory = tempfile::tempdir().unwrap();
        let (app, _) = app(directory.path());
        let (status, written) = send(&app, "PUT", "/api/1/layouts/Design", GROUPED).await;
        assert_eq!(
            status,
            StatusCode::OK,
            "{}",
            String::from_utf8_lossy(&written)
        );
        assert_eq!(written, GROUPED.as_bytes());
        let (_, read) = send(&app, "GET", "/api/1/layouts/Design", "").await;
        assert_eq!(read, GROUPED.as_bytes());
        for (from, to, field) in [
            ("\"code:Phase\"", "\"phase\"", "group"),
            ("\"code:Phase\"", "\"code:\"", "group"),
            ("\"code:Phase\"", "\"month\"", "group"),
            ("\"float_max\"", "\"colour\"", "filters.built.colour"),
            ("\"40\"", "\" \"", "filters.built.float_max"),
        ] {
            let body = GROUPED.replace(from, to);
            let (status, answer) = send(&app, "PUT", "/api/1/layouts/x", &body).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{field}");
            let error = error_of(&answer);
            assert!(
                error["message"].as_str().unwrap().contains(field),
                "{field}: {error}"
            );
        }
    }

    /// A Gantt fitted to its dates is kept as `"zoom": "fit"`; any
    /// other word is refused.
    #[tokio::test]
    async fn a_fitted_zoom_is_kept() {
        let directory = tempfile::tempdir().unwrap();
        let (app, _) = app(directory.path());
        let scale = "{\n      \"px_per_day\": 24.0,\n      \"top\": \"month\",\n      \"bottom\": \"day\"\n    }";
        assert!(LAYOUT.contains(scale));
        let fitted = LAYOUT.replace(scale, "\"fit\"");
        let (status, written) = send(&app, "PUT", "/api/1/layouts/Fitted", &fitted).await;
        assert_eq!(
            status,
            StatusCode::OK,
            "{}",
            String::from_utf8_lossy(&written)
        );
        assert_eq!(written, fitted.as_bytes());
        let (_, read) = send(&app, "GET", "/api/1/layouts/Fitted", "").await;
        assert_eq!(read, fitted.as_bytes());
        let (status, _) = send(
            &app,
            "PUT",
            "/api/1/layouts/x",
            &LAYOUT.replace(scale, "\"fill\""),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        let schema: Value = serde_json::from_str(&generated()).unwrap();
        assert!(schema["$defs"]["Fit"].to_string().contains("\"fit\""));
    }

    /// Grouping by an activity's own fields is kept as written.
    #[tokio::test]
    async fn an_activity_field_grouping_is_kept() {
        let directory = tempfile::tempdir().unwrap();
        let (app, _) = app(directory.path());
        for group in super::GROUPS {
            let body = GROUPED.replace("\"code:Phase\"", &format!("\"{group}\""));
            let (status, written) = send(&app, "PUT", "/api/1/layouts/Fields", &body).await;
            assert_eq!(
                status,
                StatusCode::OK,
                "{group}: {}",
                String::from_utf8_lossy(&written)
            );
            let (_, read) = send(&app, "GET", "/api/1/layouts/Fields", "").await;
            assert_eq!(read, body.as_bytes(), "{group}");
        }
    }

    /// Further levels of grouping are kept in order, and a layout without
    /// them is written as before.
    #[tokio::test]
    async fn further_levels_of_grouping_are_kept() {
        let directory = tempfile::tempdir().unwrap();
        let (app, _) = app(directory.path());
        let levels = GROUPED.replace(
            "\"group\": \"code:Phase\",",
            "\"group\": \"wbs\",\n  \"then_by\": [\n    \"code:Phase\",\n    \"status\"\n  ],",
        );
        let (status, written) = send(&app, "PUT", "/api/1/layouts/Levels", &levels).await;
        assert_eq!(
            status,
            StatusCode::OK,
            "{}",
            String::from_utf8_lossy(&written)
        );
        assert_eq!(written, levels.as_bytes());
        let (_, read) = send(&app, "GET", "/api/1/layouts/Levels", "").await;
        assert_eq!(read, levels.as_bytes());
        let empty = GROUPED.replace(
            "\"group\": \"code:Phase\",",
            "\"group\": \"code:Phase\",\n  \"then_by\": [],",
        );
        let (_, written) = send(&app, "PUT", "/api/1/layouts/One", &empty).await;
        assert_eq!(written, GROUPED.as_bytes());
        for (from, to, field) in [
            ("\"status\"\n", "\"none\"\n", "then_by[1]"),
            ("\"status\"\n", "\"wbs\"\n", "then_by[1]"),
            ("\"status\"\n", "\"code:Phase\"\n", "then_by[1]"),
            ("\"status\"\n", "\"month\"\n", "then_by[1]"),
            ("\"group\": \"wbs\"", "\"group\": \"none\"", "then_by[0]"),
            (
                "\"status\"\n",
                "\"status\",\n    \"type\",\n    \"calendar\"\n",
                "more than 3",
            ),
        ] {
            let body = levels.replace(from, to);
            let (status, answer) = send(&app, "PUT", "/api/1/layouts/x", &body).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{to}");
            let error = error_of(&answer);
            assert!(
                error["message"].as_str().unwrap().contains(field),
                "{field}: {error}"
            );
        }
    }

    /// An app serving a copy of the fixture in `directory`.
    fn app(directory: &Path) -> (Arc<App>, PathBuf) {
        let file = directory.join("plant.xer");
        std::fs::copy(FIXTURE, &file).unwrap();
        (
            Arc::new(App::new(file.clone(), View::open(&file).unwrap())),
            file,
        )
    }

    async fn send(app: &Arc<App>, method: &str, path: &str, body: &str) -> (StatusCode, Vec<u8>) {
        let request = Request::builder()
            .method(method)
            .uri(path)
            .body(Body::from(body.to_owned()))
            .unwrap();
        let response = api(Arc::clone(app)).oneshot(request).await.unwrap();
        let status = response.status();
        (
            status,
            to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap()
                .to_vec(),
        )
    }

    fn error_of(bytes: &[u8]) -> Value {
        serde_json::from_slice::<Value>(bytes).unwrap()["error"].clone()
    }

    #[tokio::test]
    async fn a_layout_round_trips_byte_for_byte() {
        let directory = tempfile::tempdir().unwrap();
        let (app, _) = app(directory.path());
        let (status, written) = send(&app, "PUT", "/api/1/layouts/Look%20ahead", LAYOUT).await;
        assert_eq!(
            status,
            StatusCode::OK,
            "{}",
            String::from_utf8_lossy(&written)
        );
        assert_eq!(written, LAYOUT.as_bytes());
        let (status, read) = send(&app, "GET", "/api/1/layouts/Look%20ahead", "").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(read, LAYOUT.as_bytes());
        let on_disk =
            std::fs::read(directory.path().join("plant.layouts/Look ahead.json")).unwrap();
        assert_eq!(on_disk, LAYOUT.as_bytes());
        // Written any other way, it's saved in the one form, which reads back the same.
        let compact: Value = serde_json::from_str(LAYOUT).unwrap();
        let (_, again) = send(&app, "PUT", "/api/1/layouts/Compact", &compact.to_string()).await;
        assert_eq!(again, LAYOUT.as_bytes());
    }

    #[tokio::test]
    async fn layouts_are_listed_by_name_and_deleted() {
        let directory = tempfile::tempdir().unwrap();
        let (app, _) = app(directory.path());
        let (_, empty) = send(&app, "GET", "/api/1/layouts", "").await;
        assert_eq!(
            serde_json::from_slice::<Value>(&empty).unwrap()["layouts"],
            serde_json::json!([])
        );
        for name in ["b", "a"] {
            send(&app, "PUT", &format!("/api/1/layouts/{name}"), LAYOUT).await;
        }
        let (_, listed) = send(&app, "GET", "/api/1/layouts", "").await;
        assert_eq!(
            serde_json::from_slice::<Value>(&listed).unwrap()["layouts"],
            serde_json::json!([{"name": "a"}, {"name": "b"}])
        );
        let (status, _) = send(&app, "DELETE", "/api/1/layouts/a", "").await;
        assert_eq!(status, StatusCode::OK);
        let (status, gone) = send(&app, "GET", "/api/1/layouts/a", "").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(error_of(&gone)["code"], "no-such-layout");
    }

    #[tokio::test]
    async fn names_that_leave_the_layouts_directory_are_refused() {
        let directory = tempfile::tempdir().unwrap();
        let (app, _) = app(directory.path());
        for name in [
            "..%2Fx",
            "%2E%2E%2Fx",
            "%2Ftmp%2Fx",
            ".hidden",
            "a%2Fb",
            "..",
            "x%5Cy",
        ] {
            for method in ["PUT", "GET", "DELETE"] {
                let (status, answer) =
                    send(&app, method, &format!("/api/1/layouts/{name}"), LAYOUT).await;
                assert_eq!(status, StatusCode::BAD_REQUEST, "{method} {name}");
                assert_eq!(
                    error_of(&answer)["code"],
                    "bad-layout-name",
                    "{method} {name}"
                );
            }
        }
        // Nothing was written, there or anywhere beside it.
        assert!(!directory.path().join("x").exists());
        assert!(!directory.path().join("plant.layouts").exists());
    }

    #[tokio::test]
    async fn a_layout_that_doesnt_fit_the_schema_is_refused_with_the_field_named() {
        let directory = tempfile::tempdir().unwrap();
        let (app, _) = app(directory.path());
        let cases = [
            (LAYOUT.replace("\"selected\"", "\"some\""), "gantt.lines"),
            (
                LAYOUT.replace("\"width\": 300", "\"width\": 10"),
                "columns[0].width",
            ),
            (
                LAYOUT.replace("\"field\": \"code\"", "\"field\": \"name\""),
                "columns[1].field",
            ),
            (LAYOUT.replace("\"split\": 0.55", "\"split\": 2"), "split"),
            (
                LAYOUT.replace("\"group\": \"none\"", "\"group\": \"none\", \"colour\": 1"),
                "colour",
            ),
            (
                LAYOUT.replace("\"totalfloat.layout/1\"", "\"totalfloat.layout/9\""),
                "schema",
            ),
            ("[]".to_owned(), "the layout"),
        ];
        for (body, field) in cases {
            let (status, answer) = send(&app, "PUT", "/api/1/layouts/x", &body).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{field}");
            let error = error_of(&answer);
            assert_eq!(error["code"], "bad-layout", "{field}");
            assert!(
                error["message"].as_str().unwrap().contains(field),
                "{field}: {error}"
            );
        }
        assert!(!directory.path().join("plant.layouts/x.json").exists());
    }

    #[tokio::test]
    async fn names_are_portable_and_compared_without_case() {
        let directory = tempfile::tempdir().unwrap();
        let (app, _) = app(directory.path());
        for name in ["CON", "nul.x", "Com1", "trailing.", "trailing%20"] {
            let (status, answer) =
                send(&app, "PUT", &format!("/api/1/layouts/{name}"), LAYOUT).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{name}");
            assert_eq!(error_of(&answer)["code"], "bad-layout-name", "{name}");
        }
        send(&app, "PUT", "/api/1/layouts/Print", LAYOUT).await;
        let (status, _) = send(&app, "PUT", "/api/1/layouts/print", LAYOUT).await;
        assert_eq!(status, StatusCode::OK);
        let (_, listed) = send(&app, "GET", "/api/1/layouts", "").await;
        assert_eq!(
            serde_json::from_slice::<Value>(&listed).unwrap()["layouts"],
            serde_json::json!([{"name": "print"}])
        );
        let (status, _) = send(&app, "GET", "/api/1/layouts/PRINT", "").await;
        assert_eq!(status, StatusCode::OK);
        let (status, _) = send(&app, "DELETE", "/api/1/layouts/Print", "").await;
        assert_eq!(status, StatusCode::OK);
        let (_, listed) = send(&app, "GET", "/api/1/layouts", "").await;
        assert_eq!(
            serde_json::from_slice::<Value>(&listed).unwrap()["layouts"],
            serde_json::json!([])
        );
    }

    #[tokio::test]
    async fn one_layout_and_nothing_after_it() {
        let directory = tempfile::tempdir().unwrap();
        let (app, _) = app(directory.path());
        for extra in ["garbage", "{}"] {
            let (status, answer) =
                send(&app, "PUT", "/api/1/layouts/x", &format!("{LAYOUT}{extra}")).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{extra}");
            assert!(
                error_of(&answer)["message"]
                    .as_str()
                    .unwrap()
                    .contains("more follows"),
                "{extra}"
            );
        }
    }

    #[tokio::test]
    async fn a_saved_file_is_checked_as_it_is_read() {
        let directory = tempfile::tempdir().unwrap();
        let (app, _) = app(directory.path());
        let folder = directory.path().join("plant.layouts");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(
            folder.join("edited.json"),
            LAYOUT.replace(
                "\"finish\",\n      \"descending\"",
                "\"colour\",\n      \"descending\"",
            ),
        )
        .unwrap();
        let (status, answer) = send(&app, "GET", "/api/1/layouts/edited", "").await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        let error = error_of(&answer);
        assert_eq!(error["code"], "bad-layout-file");
        assert!(
            error["message"].as_str().unwrap().contains("sort[0].field"),
            "{error}"
        );
    }

    #[tokio::test]
    async fn saves_at_once_leave_one_whole_layout_and_no_partial_files() {
        let directory = tempfile::tempdir().unwrap();
        let (app, _) = app(directory.path());
        // Splits written as the canonical form writes them, with no trailing zero.
        let bodies: Vec<String> = [31, 32, 33, 34, 35, 36, 37, 38, 39, 41, 42, 43]
            .iter()
            .map(|k| LAYOUT.replace("0.55", &format!("0.{k}")))
            .collect();
        // All sent at once, each on a task of its own.
        let tasks: Vec<_> = bodies
            .iter()
            .cloned()
            .map(|body| {
                let app = Arc::clone(&app);
                tokio::spawn(async move { send(&app, "PUT", "/api/1/layouts/same", &body).await })
            })
            .collect();
        for task in tasks {
            let (status, answer) = task.await.unwrap();
            assert_eq!(status, StatusCode::OK);
            // Each answers with the whole layout it saved.
            assert!(bodies.iter().any(|b| b.as_bytes() == answer.as_slice()));
        }
        let saved =
            std::fs::read_to_string(directory.path().join("plant.layouts/same.json")).unwrap();
        assert!(bodies.contains(&saved), "{saved}");
        let left: Vec<_> = std::fs::read_dir(directory.path().join("plant.layouts"))
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        assert_eq!(left, ["same.json"]);
    }

    #[test]
    fn the_schema_says_the_bounds_the_server_checks() {
        let schema: Value = serde_json::from_str(&generated()).unwrap();
        let defs = &schema["$defs"];
        assert_eq!(schema["properties"]["columns"]["minItems"], 1);
        assert_eq!(schema["properties"]["split"]["minimum"], 0.15);
        assert_eq!(schema["properties"]["split"]["maximum"], 0.85);
        assert_eq!(defs["Column"]["properties"]["width"]["minimum"], 40);
        assert_eq!(defs["Column"]["properties"]["width"]["maximum"], 2000);
        assert_eq!(defs["Scale"]["properties"]["px_per_day"]["minimum"], 0.01);
        // And the server refuses just past each.
        for (from, to) in [
            ("\"width\": 300", "\"width\": 39"),
            ("\"split\": 0.55", "\"split\": 0.86"),
            ("\"px_per_day\": 24.0", "\"px_per_day\": 0.009"),
        ] {
            assert!(
                super::parse(LAYOUT.replace(from, to).as_bytes()).is_err(),
                "{to}"
            );
        }
        assert!(super::parse(LAYOUT.replace("\"width\": 300", "\"width\": 40").as_bytes()).is_ok());
    }

    #[tokio::test]
    async fn a_failed_rename_by_case_keeps_the_layout() {
        let directory = tempfile::tempdir().unwrap();
        let (app, _) = app(directory.path());
        send(&app, "PUT", "/api/1/layouts/Print", LAYOUT).await;
        // Where the new name's file would go is taken by a directory, so the
        // save fails as it moves into place.
        let blocker = directory.path().join("plant.layouts/print.json");
        if std::fs::create_dir(&blocker).is_err() {
            // A filesystem that ignores case: Print.json is already there.
            return;
        }
        let (status, _) = send(&app, "PUT", "/api/1/layouts/print", LAYOUT).await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        std::fs::remove_dir(&blocker).unwrap();
        let (status, kept) = send(&app, "GET", "/api/1/layouts/Print", "").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(kept, LAYOUT.as_bytes());
        let left: Vec<_> = std::fs::read_dir(directory.path().join("plant.layouts"))
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        assert_eq!(left, ["Print.json"]);
    }
}
