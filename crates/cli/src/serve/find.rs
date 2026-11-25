//! `GET /api/1/query`: `query`'s filters and columns over HTTP, for the
//! grid's filter builder and grouping.

use std::sync::Arc;

use axum::extract::rejection::QueryRejection;
use axum::extract::{Query, State};
use axum::response::Response;
use serde_json::{Map, Value};

use super::{App, Params, json, on_view};
use crate::error::CliError;
use crate::query;

/// The parameters taken: `query`'s, less a baseline file, which would read
/// any file the server can.
pub(super) const PARAMETERS: [&str; 23] = [
    "project",
    "wbs",
    "code",
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
    "any",
    "preds",
    "succs",
    "driving_path_to",
    "period",
    "columns",
    "limit",
    "offset",
];

/// Lists, given with commas between their items; `\,` is a comma within
/// an item and `\\` a backslash, for names such as a user field's label.
const LISTS: [&str; 3] = ["status", "type", "columns"];
/// Switches, `true` or `false`.
const SWITCHES: [&str; 2] = ["critical", "constrained"];
/// Numbers.
const NUMBERS: [&str; 5] = ["project", "float_min", "float_max", "limit", "offset"];

/// The query the parameters ask for, read as MCP's `query` reads its
/// arguments.
fn parsed(params: &Params) -> Result<query::Query, CliError> {
    let mut args = Map::new();
    let skipped = |name: &str| name == "at" || name == "any";
    for (name, text) in params.iter().filter(|(name, _)| !skipped(name)) {
        let value = if LISTS.contains(&name.as_str()) {
            Value::from(items(text))
        } else if SWITCHES.contains(&name.as_str()) {
            match text.as_str() {
                "true" => Value::Bool(true),
                "false" => Value::Bool(false),
                _ => return Err(bad(name, text, "it's true or false")),
            }
        } else if NUMBERS.contains(&name.as_str()) {
            let number: serde_json::Number =
                text.parse().map_err(|_| bad(name, text, "it's a number"))?;
            Value::Number(number)
        } else {
            Value::from(text.as_str())
        };
        args.insert(name.clone(), value);
    }
    let mut query: query::Query = serde_json::from_value(Value::Object(args)).map_err(|error| {
        CliError::new(
            "bad-query",
            format!("the query couldn't be read: {error}"),
            "parameters are `query`'s arguments: lists with commas, switches true or false",
        )
    })?;
    if let Some(text) = params.get("any") {
        query.any = any_groups(text)?;
    }
    Ok(query)
}

/// `any`: a JSON array of groups, each an object of `query`'s filters with
/// their values as text, as a named filter's `built` holds them, such as
/// `[{"code": "Phase=DESIGN"}, {"critical": "true"}]`.
fn any_groups(text: &str) -> Result<Vec<query::Conditions>, CliError> {
    let groups: Vec<std::collections::BTreeMap<String, String>> = serde_json::from_str(text)
        .map_err(|error| {
            CliError::new(
                "bad-query",
                format!("any can't be read: {error}"),
                "any is a JSON array of objects, such as [{\"critical\": \"true\"}, {\"float_max\": \"0\"}]",
            )
        })?;
    groups.iter().map(query::conditions).collect()
}

/// A list's items: split at commas not escaped, each trimmed, empty ones left out.
fn items(text: &str) -> Vec<String> {
    let mut items = Vec::new();
    let mut item = String::new();
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => item.extend(chars.next()),
            ',' => items.push(std::mem::take(&mut item)),
            _ => item.push(c),
        }
    }
    items.push(item);
    items
        .into_iter()
        .map(|item| item.trim().to_owned())
        .filter(|item| !item.is_empty())
        .collect()
}

fn bad(name: &str, text: &str, hint: &str) -> CliError {
    CliError::new(
        "bad-query",
        format!("{name}={text} can't be read"),
        hint.to_owned(),
    )
}

/// `GET /api/1/query`: `totalfloat.query/1`. With no `limit`, every match.
pub(super) async fn query(
    State(app): State<Arc<App>>,
    params: Result<Query<Params>, QueryRejection>,
) -> Response {
    on_view(app, params, &PARAMETERS, |view, params| {
        let mut query = parsed(params)?;
        if !params.contains_key("limit") {
            query.limit = view
                .loaded
                .schedules
                .iter()
                .map(|s| s.activities.len())
                .sum();
        }
        Ok(json(&query::report(view.label(), &view.loaded, &query)?))
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(pairs: &[(&str, &str)]) -> Params {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    }

    #[test]
    fn lists_switches_and_numbers_are_read() {
        let q = parsed(&params(&[
            ("status", "not-started, in-progress"),
            ("critical", "true"),
            ("float_max", "40"),
            ("columns", "id,code:Phase"),
            ("code", "Phase=DESIGN"),
        ]))
        .unwrap();
        assert_eq!(q.status.len(), 2);
        assert!(q.critical);
        assert_eq!(q.float_max, Some(40.0));
        assert_eq!(q.columns, ["id", "code:Phase"]);
        assert_eq!(q.code.as_deref(), Some("Phase=DESIGN"));
    }

    #[test]
    fn any_groups_are_read_from_json() {
        let q = parsed(&params(&[
            ("float_max", "40"),
            (
                "any",
                r#"[{"code": "Phase=DESIGN", "critical": "true"}, {"status": "in-progress, completed"}]"#,
            ),
        ]))
        .unwrap();
        assert_eq!(q.any.len(), 2);
        assert_eq!(q.any[0].code.as_deref(), Some("Phase=DESIGN"));
        assert!(q.any[0].critical);
        assert_eq!(q.any[1].status.len(), 2);
        for wrong in [
            r#"{"critical": "true"}"#,
            r#"[{"critical": "yes"}]"#,
            "[{}]",
        ] {
            assert!(parsed(&params(&[("any", wrong)])).is_err(), "{wrong}");
        }
    }

    #[test]
    fn escaped_commas_stay_in_their_item() {
        let q = parsed(&params(&[("columns", r"id, udf:Cost\, phase 2,code:A\\B")])).unwrap();
        assert_eq!(q.columns, ["id", "udf:Cost, phase 2", r"code:A\B"]);
    }

    #[test]
    fn unreadable_values_are_refused() {
        for (name, text) in [
            ("critical", "yes"),
            ("float_min", "lots"),
            ("status", "late"),
        ] {
            let error = parsed(&params(&[(name, text)])).unwrap_err();
            assert_eq!(error.code, "bad-query", "{name}={text}");
        }
    }
}
