//! Named filters as files: a filter builder's conditions kept under a
//! name, apart from any layout, as `totalfloat.filter/1` JSON in a
//! `<name>.filters/` directory beside the store or file served, one file per
//! filter. Each is a set of `query`'s parameters, as a layout's built filter
//! is, written pretty-printed in a fixed key order.

use std::collections::BTreeMap;
use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Path as UrlPath, State};
use axum::response::Response;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::layouts::{BUILT_FILTERS, check_any, check_built};
use super::shelf::{self, Kind, blocking, bytes_response};
use super::{App, json, refusal};
use crate::error::CliError;

/// A named filter (`totalfloat.filter/1`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(extend("anyOf" = [
    {"required": ["built"], "properties": {"built": {"minProperties": 1}}},
    {"required": ["any"], "properties": {"any": {"minItems": 1}}}
]))]
pub struct Filter {
    /// Always `totalfloat.filter/1`.
    pub schema: FilterSchema,
    /// The conditions, as `query`'s parameters, such as
    /// `{"code": "Phase=DESIGN", "float_max": "40"}`. An activity has to meet
    /// them all.
    #[serde(default)]
    #[schemars(extend("propertyNames" = {"enum": BUILT_FILTERS}))]
    pub built: BTreeMap<String, String>,
    /// Groups of conditions in the same form, as `query`'s `any`: an activity
    /// has to meet every condition of at least one group, besides `built`.
    /// Left out when there are none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[schemars(extend("items" = {"type": "object", "minProperties": 1, "additionalProperties": {"type": "string"}, "propertyNames": {"enum": BUILT_FILTERS}}))]
    pub any: Vec<BTreeMap<String, String>>,
}

/// The schema name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum FilterSchema {
    /// The only version.
    #[serde(rename = "totalfloat.filter/1")]
    V1,
}

/// `GET /api/1/filters`: the filters saved, by name.
#[derive(Debug, Serialize, JsonSchema)]
pub struct Listing {
    #[schemars(extend("const" = "totalfloat.filters/1"))]
    schema: &'static str,
    filters: Vec<Named>,
}

#[derive(Debug, Serialize, JsonSchema)]
struct Named {
    /// The file's name in the filters folder, without `.json`.
    name: String,
}

/// `DELETE /api/1/filters/{name}`: the filter is gone.
#[derive(Debug, Serialize, JsonSchema)]
pub struct Deleted {
    #[schemars(extend("const" = "totalfloat.filters/1"))]
    schema: &'static str,
    #[schemars(extend("const" = true))]
    deleted: bool,
}

fn bad_filter(message: String, hint: &str) -> CliError {
    CliError::new("bad-filter", message, hint)
}

/// Reads a named filter, naming the field that's wrong, if any.
///
/// # Errors
///
/// `bad-filter`, naming the field.
pub fn parse(bytes: &[u8]) -> Result<Filter, CliError> {
    let reader = &mut serde_json::Deserializer::from_slice(bytes);
    let filter: Filter = serde_path_to_error::deserialize(&mut *reader).map_err(|error| {
        let path = error.path().to_string();
        let at = if path == "." {
            "the filter".to_owned()
        } else {
            path
        };
        bad_filter(
            format!("{at}: {}", error.inner()),
            "a named filter is totalfloat.filter/1; see docs/schemas/filter-1.schema.json",
        )
    })?;
    reader.end().map_err(|error| {
        bad_filter(
            format!("the filter: more follows it ({error})"),
            "send one JSON object",
        )
    })?;
    if filter.built.is_empty() && filter.any.is_empty() {
        return Err(bad_filter(
            "built: a named filter has at least one condition".to_owned(),
            "give query's parameters, such as {\"critical\": \"true\"}",
        ));
    }
    check_built(&filter.built)
        .map_err(|(field, why, hint)| bad_filter(format!("built.{field}: {why}"), hint))?;
    check_any(&filter.any)
        .map_err(|(at, why, hint)| bad_filter(format!("any{at}: {why}"), hint))?;
    Ok(filter)
}

/// A filter as it's saved: pretty-printed, in the type's key order, with a
/// final newline, so the files diff cleanly.
fn canonical(filter: &Filter) -> Result<Vec<u8>, CliError> {
    let mut bytes = serde_json::to_vec_pretty(filter).map_err(|error| {
        CliError::new(
            "serve-failed",
            format!("the filter couldn't be written as JSON: {error}"),
            "this is a bug",
        )
    })?;
    bytes.push(b'\n');
    Ok(bytes)
}

/// Named filters on the shelf beside the schedule.
pub static FILTERS: Kind = Kind {
    folder: "filters",
    noun: "filter",
    bad_name: "bad-filter-name",
    missing: "no-such-filter",
    bad_file: "bad-filter-file",
    schema_file: "docs/schemas/filter-1.schema.json",
    check: |body| parse(body).map(drop),
    canonical: |body| canonical(&parse(body)?),
};

/// `GET /api/1/filters`.
pub async fn get_list(State(app): State<Arc<App>>) -> Response {
    // Not while a save or delete is part way through.
    let _turn = app.writing.lock().await;
    let view = app.current();
    match blocking(&FILTERS, move || shelf::list(&FILTERS, &view)).await {
        Ok(names) => json(&Listing {
            schema: "totalfloat.filters/1",
            filters: names.into_iter().map(|name| Named { name }).collect(),
        }),
        Err(error) => refusal(&error),
    }
}

/// `GET /api/1/filters/{name}`: the file as saved.
pub async fn get_one(State(app): State<Arc<App>>, UrlPath(name): UrlPath<String>) -> Response {
    let _turn = app.writing.lock().await;
    let view = app.current();
    match blocking(&FILTERS, move || shelf::read(&FILTERS, &view, &name)).await {
        Ok(bytes) => bytes_response(bytes),
        Err(error) => refusal(&error),
    }
}

/// `PUT /api/1/filters/{name}`: saves it, answering with the file written.
pub async fn put_one(
    State(app): State<Arc<App>>,
    UrlPath(name): UrlPath<String>,
    body: Bytes,
) -> Response {
    let _turn = app.writing.lock().await;
    let view = app.current();
    match blocking(&FILTERS, move || {
        shelf::write(&FILTERS, &view, &name, &body)
    })
    .await
    {
        Ok(bytes) => bytes_response(bytes),
        Err(error) => refusal(&error),
    }
}

/// `DELETE /api/1/filters/{name}`.
pub async fn delete_one(State(app): State<Arc<App>>, UrlPath(name): UrlPath<String>) -> Response {
    let _turn = app.writing.lock().await;
    let view = app.current();
    match blocking(&FILTERS, move || shelf::delete(&FILTERS, &view, &name)).await {
        Ok(()) => json(&Deleted {
            schema: "totalfloat.filters/1",
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

    use super::Filter;
    use crate::serve::{App, api, view::View};

    const FIXTURE: &str = "../../fixtures/xer/batches/001/rel-fs-positive.xer";

    fn root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    /// Set `TOTALFLOAT_UPDATE_CONTRACT=1` to rewrite it after a deliberate,
    /// compatible change, and say why in the PR.
    #[test]
    fn the_published_filter_schema_matches_the_type() {
        let mut schema = serde_json::to_value(schemars::schema_for!(Filter)).unwrap();
        let object = schema.as_object_mut().unwrap();
        object.insert("$id".into(), Value::from("totalfloat.filter/1"));
        object.insert("title".into(), Value::from("totalfloat.filter/1"));
        let generated = serde_json::to_string_pretty(&schema).unwrap() + "\n";
        let path = root().join("docs/schemas/filter-1.schema.json");
        if std::env::var_os("TOTALFLOAT_UPDATE_CONTRACT").is_some() {
            std::fs::write(&path, &generated).unwrap();
            return;
        }
        let published = std::fs::read_to_string(&path).unwrap_or_default();
        assert_eq!(
            published, generated,
            "docs/schemas/filter-1.schema.json is out of date; rerun with TOTALFLOAT_UPDATE_CONTRACT=1"
        );
    }

    /// A named filter as the app saves one.
    pub(in crate::serve) const CRITICAL_DESIGN: &str = r#"{
  "schema": "totalfloat.filter/1",
  "built": {
    "code": "Phase=DESIGN",
    "critical": "true"
  }
}
"#;

    fn app(directory: &Path) -> (Arc<App>, PathBuf) {
        let file = directory.join("plant.xer");
        std::fs::copy(FIXTURE, &file).unwrap();
        (
            Arc::new(App::new(file.clone(), View::open(&file).unwrap())),
            file,
        )
    }

    async fn send(app: &Arc<App>, method: &str, uri: &str, body: &str) -> (StatusCode, Vec<u8>) {
        let request = Request::builder()
            .method(method)
            .uri(uri)
            .body(Body::from(body.to_owned()))
            .unwrap();
        let response = api(Arc::clone(app)).oneshot(request).await.unwrap();
        let status = response.status();
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        (status, bytes.to_vec())
    }

    fn error_of(bytes: &[u8]) -> Value {
        serde_json::from_slice::<Value>(bytes).unwrap()["error"].clone()
    }

    #[tokio::test]
    async fn a_named_filter_is_saved_listed_read_and_deleted() {
        let directory = tempfile::tempdir().unwrap();
        let (app, _) = app(directory.path());
        let (status, written) = send(
            &app,
            "PUT",
            "/api/1/filters/Critical%20design",
            CRITICAL_DESIGN,
        )
        .await;
        assert_eq!(
            status,
            StatusCode::OK,
            "{}",
            String::from_utf8_lossy(&written)
        );
        assert_eq!(written, CRITICAL_DESIGN.as_bytes());
        // Beside the schedule, as a file of its own.
        let file = directory.path().join("plant.filters/Critical design.json");
        assert_eq!(std::fs::read(&file).unwrap(), CRITICAL_DESIGN.as_bytes());
        let (_, listed) = send(&app, "GET", "/api/1/filters", "").await;
        let listed: Value = serde_json::from_slice(&listed).unwrap();
        assert_eq!(
            listed,
            serde_json::json!({"schema": "totalfloat.filters/1", "filters": [{"name": "Critical design"}]})
        );
        // Read back by name in any case.
        let (status, read) = send(&app, "GET", "/api/1/filters/critical%20DESIGN", "").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(read, CRITICAL_DESIGN.as_bytes());
        let (status, _) = send(&app, "DELETE", "/api/1/filters/Critical%20design", "").await;
        assert_eq!(status, StatusCode::OK);
        assert!(!file.exists());
        let (status, answer) = send(&app, "GET", "/api/1/filters/Critical%20design", "").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(error_of(&answer)["code"], "no-such-filter");
    }

    /// Whole days as the builder writes them are kept.
    #[tokio::test]
    async fn day_bounds_as_the_builder_writes_them_are_kept() {
        let directory = tempfile::tempdir().unwrap();
        let (app, _) = app(directory.path());
        let body = CRITICAL_DESIGN.replace(
            "\"critical\": \"true\"",
            "\"float_max\": \"-8.5\",\n    \"start_from\": \"2026-01-05 00:00\",\n    \"start_to\": \"2026-01-31 23:59\"",
        );
        let (status, written) = send(&app, "PUT", "/api/1/filters/Days", &body).await;
        assert_eq!(
            status,
            StatusCode::OK,
            "{}",
            String::from_utf8_lossy(&written)
        );
    }

    /// A filter of `any` groups alone is saved and read back as
    /// written, and `query` applies it; an old file with only `built` reads
    /// as before.
    #[tokio::test]
    async fn a_filter_of_any_groups_is_saved_and_applied() {
        let directory = tempfile::tempdir().unwrap();
        let (app, _) = app(directory.path());
        let body = r#"{
  "schema": "totalfloat.filter/1",
  "built": {},
  "any": [
    {
      "critical": "true"
    },
    {
      "text": "B"
    }
  ]
}
"#;
        let (status, written) = send(&app, "PUT", "/api/1/filters/Either", body).await;
        assert_eq!(
            status,
            StatusCode::OK,
            "{}",
            String::from_utf8_lossy(&written)
        );
        assert_eq!(written, body.as_bytes());
        let (_, read) = send(&app, "GET", "/api/1/filters/either", "").await;
        assert_eq!(read, body.as_bytes());
        let old = super::parse(CRITICAL_DESIGN.as_bytes()).unwrap();
        assert_eq!(old.any.len(), 0);
        assert_eq!(super::canonical(&old).unwrap(), CRITICAL_DESIGN.as_bytes());
        // The groups as `query`'s `any`, URL-encoded JSON, find what the
        // two conditions find apart.
        let codes = |found: &[u8]| -> std::collections::BTreeSet<String> {
            let found: Value = serde_json::from_slice(found).unwrap();
            found["rows"]
                .as_array()
                .unwrap()
                .iter()
                .map(|r| r["code"].as_str().unwrap().to_owned())
                .collect()
        };
        let any = url_encoded(r#"[{"critical":"true"},{"text":"B"}]"#);
        let (status, either) = send(
            &app,
            "GET",
            &format!("/api/1/query?columns=code&any={any}"),
            "",
        )
        .await;
        assert_eq!(
            status,
            StatusCode::OK,
            "{}",
            String::from_utf8_lossy(&either)
        );
        let (_, critical) = send(&app, "GET", "/api/1/query?columns=code&critical=true", "").await;
        let (_, named_b) = send(&app, "GET", "/api/1/query?columns=code&text=B", "").await;
        let union = &codes(&critical) | &codes(&named_b);
        assert!(!union.is_empty());
        assert_eq!(codes(&either), union);
    }

    fn url_encoded(text: &str) -> String {
        text.bytes()
            .map(|b| match b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' => {
                    (b as char).to_string()
                }
                _ => format!("%{b:02X}"),
            })
            .collect()
    }

    #[tokio::test]
    async fn a_bad_filter_or_name_is_refused_saying_why() {
        let directory = tempfile::tempdir().unwrap();
        let (app, _) = app(directory.path());
        for (body, field) in [
            (
                CRITICAL_DESIGN.replace("\"critical\"", "\"late\""),
                "built.late",
            ),
            (
                CRITICAL_DESIGN.replace("\"true\"", "\" \""),
                "built.critical",
            ),
            (
                "{\"schema\": \"totalfloat.filter/1\", \"built\": {}}".to_owned(),
                "at least one condition",
            ),
            (CRITICAL_DESIGN.replace("filter/1", "filter/2"), "schema"),
            (
                CRITICAL_DESIGN.replace("\"true\"", "\"false\""),
                "built.critical: false isn't true",
            ),
            (
                CRITICAL_DESIGN.replace("\"critical\": \"true\"", "\"float_max\": \"lots\""),
                "built.float_max",
            ),
            (
                CRITICAL_DESIGN.replace(
                    "\"critical\": \"true\"",
                    "\"start_from\": \"2026-01-05 12:00\"",
                ),
                "built.start_from",
            ),
            (
                CRITICAL_DESIGN.replace(
                    "\"critical\": \"true\"",
                    "\"finish_to\": \"2026-02-30 23:59\"",
                ),
                "built.finish_to",
            ),
            (
                "{\"schema\": \"totalfloat.filter/1\", \"any\": [{}]}".to_owned(),
                "any[0]: a group holds no condition",
            ),
            (
                "{\"schema\": \"totalfloat.filter/1\", \"any\": [{\"critical\": \"true\"}, {\"late\": \"1\"}]}".to_owned(),
                "any[1].late",
            ),
        ] {
            let (status, answer) = send(&app, "PUT", "/api/1/filters/x", &body).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{field}");
            let error = error_of(&answer);
            assert_eq!(error["code"], "bad-filter", "{field}");
            assert!(
                error["message"].as_str().unwrap().contains(field),
                "{field}: {error}"
            );
        }
        // Names follow the layout rules: no way out of the folder.
        for name in ["..%2Fplant", "CON", ".hidden", "a%2Fb", "trailing.", ""] {
            let uri = format!("/api/1/filters/{name}");
            let (status, answer) = send(&app, "PUT", &uri, CRITICAL_DESIGN).await;
            assert_ne!(status, StatusCode::OK, "{name}");
            if !name.is_empty() {
                assert_eq!(error_of(&answer)["code"], "bad-filter-name", "{name}");
            }
        }
        assert!(!directory.path().join("plant.filters").exists());
        assert!(!directory.path().join("plant").exists());
    }
}
