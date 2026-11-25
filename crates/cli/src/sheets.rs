//! `totalfloat import plan.xlsx`, or a folder of CSV files: build a new
//! schedule from a spreadsheet, through the same path as `new
//! --edits` and schedule text.
//!
//! A workbook has a sheet, and a folder a `.csv` file, for each of: Project
//! (one row: `code`, `name`, `data_date`, and `must_finish` and `calendar` if
//! wanted), Activities, and if wanted WBS, Resources and Relationships.
//! Columns are named as `query` names them where it has the column. Each row
//! becomes edits, and every problem is reported at its sheet and row; nothing
//! is built unless every row is good.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde_json::{Map, Value, json};
use totalfloat_edit::Edit;
use totalfloat_xer::RawXer;

use crate::edit;
use crate::error::{CliError, Problem, exit, parse_date};
use crate::loaded::Loaded;
use crate::new;
use crate::output::{create_new, print_line, refuse_existing};

/// A sheet's rows: each with its row number in the sheet, counting the
/// header as 1, and its cells as text.
#[derive(Debug, Default)]
pub(crate) struct Sheet {
    pub(crate) rows: Vec<(usize, Vec<String>)>,
}

/// The sheets read, by name in lower case.
pub(crate) type Book = BTreeMap<String, Sheet>;

/// Each sheet's columns: those it must have, then those it may.
const SHEETS: [(&str, &[&str], &[&str]); 5] = [
    (
        "project",
        &["code", "name", "data_date"],
        &["must_finish", "calendar"],
    ),
    ("wbs", &["wbs_path", "name"], &[]),
    (
        "resources",
        &["resource", "name"],
        &["type", "units_per_hour", "calendar", "max_units_per_hour"],
    ),
    (
        "activities",
        &["code", "name"],
        &[
            "wbs_path",
            "type",
            "duration",
            "original_duration_hours",
            "calendar",
            "constraint",
            "constraint_date",
            "resources",
        ],
    ),
    (
        "relationships",
        &["predecessor", "successor"],
        &["link_type", "lag", "link_lag_hours"],
    ),
];

/// Whether `input` is a spreadsheet to build from: a workbook, or a folder
/// holding any of the sheets' CSV files.
pub fn is_sheets(input: &Path) -> bool {
    input
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("xlsx"))
        || (input.is_dir() && !csv_files(input).is_empty())
}

fn unreadable(input: &Path, why: &str) -> CliError {
    CliError::new(
        "file-not-readable",
        format!("can't read {}: {why}", input.display()),
        "give an .xlsx workbook, or a folder of CSV files with activities.csv in it",
    )
}

/// An Excel date cell as `YYYY-MM-DD HH:MM`, to the nearest minute, in
/// either of Excel's date systems (from 1900 or from 1904).
fn excel_date(date: &calamine::ExcelDateTime) -> Option<String> {
    let (y, mo, d, h, mi, sec, milli) = date.to_ymd_hms_milli();
    let base: totalfloat_model::DateTime = format!("{y:04}-{mo:02}-{d:02} {h:02}:{mi:02}")
        .parse()
        .ok()?;
    if u32::from(sec) * 1000 + u32::from(milli) >= 30_000 {
        base.checked_add(totalfloat_model::Minutes::new(1))
    } else {
        Some(base)
    }
    .map(|d| d.to_string())
}

/// A number as a sheet shows it: whole numbers without a point.
fn number_text(f: f64) -> String {
    if f.fract() == 0.0 && f.abs() < 1e15 {
        format!("{f:.0}")
    } else {
        f.to_string()
    }
}

/// A cell as text: numbers as written, dates as `YYYY-MM-DD HH:MM`.
pub(crate) fn cell_text(cell: &calamine::Data) -> Result<String, String> {
    use calamine::Data;
    Ok(match cell {
        Data::Empty => String::new(),
        Data::String(text) | Data::DurationIso(text) => text.clone(),
        Data::Int(n) => n.to_string(),
        Data::Float(f) => number_text(*f),
        Data::Bool(b) => b.to_string(),
        // A duration, formatted as [h]:mm, is a number of hours.
        Data::DateTime(date) if date.is_duration() => number_text(date.as_f64() * 24.0),
        Data::DateTime(date) => excel_date(date)
            .ok_or_else(|| format!("{} isn't a date this can read", date.as_f64()))?,
        Data::DateTimeIso(text) => {
            let text = text.replacen('T', " ", 1);
            text.get(..16).unwrap_or(&text).to_owned()
        }
        Data::Error(error) => return Err(format!("the cell holds an error ({error})")),
    })
}

/// Reads a workbook's sheets.
pub(crate) fn workbook(
    input: &Path,
    problems: &mut Vec<Problem>,
    file: &str,
) -> Result<Book, CliError> {
    use calamine::Reader;
    let mut workbook = calamine::open_workbook::<calamine::Xlsx<_>, _>(input)
        .map_err(|e| unreadable(input, &e.to_string()))?;
    let mut book = Book::new();
    for name in workbook.sheet_names() {
        // Other sheets, such as notes or workings, are left alone.
        let key = name.to_lowercase();
        if !SHEETS.iter().any(|(sheet, _, _)| *sheet == key) {
            continue;
        }
        if book.contains_key(&key) {
            problems.push(problem(
                (file, &name, 1),
                "repeated-sheet",
                format!("there's more than one {name} sheet"),
                "keep one sheet of each name, in any case",
            ));
            continue;
        }
        let range = workbook
            .worksheet_range(&name)
            .map_err(|e| unreadable(input, &e.to_string()))?;
        let first = range.start().map_or(0, |(row, _)| row as usize);
        let mut sheet = Sheet::default();
        for (k, row) in range.rows().enumerate() {
            let number = first + k + 1;
            let mut cells = Vec::with_capacity(row.len());
            for (column, cell) in row.iter().enumerate() {
                match cell_text(cell) {
                    Ok(text) => cells.push(text),
                    Err(why) => {
                        problems.push(problem(
                            (file, &name, number),
                            "bad-cell",
                            format!("column {}: {why}", column + 1),
                            "fix or clear the cell",
                        ));
                        cells.push(String::new());
                    }
                }
            }
            if cells.iter().any(|c| !c.trim().is_empty()) {
                sheet.rows.push((number, cells));
            }
        }
        book.insert(key, sheet);
    }
    Ok(book)
}

/// The folder's CSV files for the sheets, by sheet name, whatever their case.
fn csv_files(input: &Path) -> BTreeMap<String, Vec<std::path::PathBuf>> {
    let mut found: BTreeMap<String, Vec<std::path::PathBuf>> = BTreeMap::new();
    for entry in std::fs::read_dir(input).into_iter().flatten().flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let lower = name.to_lowercase();
        if let Some(stem) = lower.strip_suffix(".csv")
            && SHEETS.iter().any(|(sheet, _, _)| *sheet == stem)
            && path.is_file()
        {
            found.entry(stem.to_owned()).or_default().push(path);
        }
    }
    for paths in found.values_mut() {
        paths.sort();
    }
    found
}

/// Reads a folder's CSV files, a sheet each.
pub(crate) fn folder(
    input: &Path,
    problems: &mut Vec<Problem>,
    file: &str,
) -> Result<Book, CliError> {
    let mut book = Book::new();
    for (name, paths) in csv_files(input) {
        if paths.len() > 1 {
            problems.push(problem(
                (file, sheet_title(&name), 1),
                "repeated-sheet",
                format!("there's more than one {name}.csv, in different cases"),
                "keep one file of each name",
            ));
            continue;
        }
        let path = &paths[0];
        let text = std::fs::read_to_string(path).map_err(|e| unreadable(path, &e.to_string()))?;
        let rows = crate::table::records(&text).map_err(|why| unreadable(path, &why))?;
        book.insert(name, Sheet { rows });
    }
    Ok(book)
}

fn problem(
    (file, sheet, row): (&str, &str, usize),
    code: &'static str,
    message: String,
    hint: &str,
) -> Problem {
    Problem {
        edit: None,
        source: format!("{file} {sheet} row {row}"),
        code,
        message,
        hint: hint.to_owned(),
        depends_on: None,
        line: Some(row),
        column: None,
    }
}

/// A sheet's rows as maps by column name, checking the header: unknown
/// columns and missing ones are problems at row 1.
fn rows_of(
    book: &Book,
    (name, required, optional): (&str, &[&str], &[&str]),
    file: &str,
    problems: &mut Vec<Problem>,
) -> Vec<Row> {
    let title = sheet_title(name);
    let Some(sheet) = book.get(name) else {
        return Vec::new();
    };
    let Some(((header_row, header), rest)) = sheet.rows.split_first() else {
        return Vec::new();
    };
    let columns: Vec<String> = header.iter().map(|c| c.trim().to_lowercase()).collect();
    let at = (file, title, *header_row);
    for (k, column) in columns.iter().enumerate() {
        if !column.is_empty() && columns[..k].contains(column) {
            problems.push(problem(
                at,
                "repeated-column",
                format!("{title} has the column {column} twice"),
                "keep one column of each name, in any case",
            ));
        }
    }
    for column in columns.iter().filter(|c| !c.is_empty()) {
        if !required.contains(&column.as_str()) && !optional.contains(&column.as_str()) {
            let mut known = required.to_vec();
            known.extend(optional);
            problems.push(problem(
                at,
                "unknown-column",
                format!("{title} has no column {column}"),
                &format!("its columns are {}", known.join(", ")),
            ));
        }
    }
    for column in required {
        if !columns.iter().any(|c| c == column) {
            problems.push(problem(
                at,
                "missing-column",
                format!("{title} needs a {column} column"),
                &format!("{title} needs {}", required.join(", ")),
            ));
        }
    }
    let known: Vec<&&str> = required
        .iter()
        .filter(|c| columns.iter().any(|k| k == *c))
        .collect();
    rest.iter()
        .filter_map(|(row, cells)| {
            let values: BTreeMap<String, String> = columns
                .iter()
                .zip(cells.iter().chain(std::iter::repeat(&String::new())))
                .filter(|(c, v)| !c.is_empty() && !v.trim().is_empty())
                .map(|(c, v)| (c.clone(), v.trim().to_owned()))
                .collect();
            // A row missing a value it must have is reported, not dropped.
            let blank: Vec<&str> = known
                .iter()
                .filter(|c| !values.contains_key(***c))
                .map(|c| **c)
                .collect();
            if blank.is_empty() {
                Some((*row, values))
            } else {
                problems.push(problem(
                    (file, title, *row),
                    "missing-value",
                    format!(
                        "{} {} blank",
                        blank.join(" and "),
                        if blank.len() == 1 { "is" } else { "are" }
                    ),
                    &format!("every {title} row needs {}", required.join(", ")),
                ));
                None
            }
        })
        .collect()
}

/// A sheet's name as a workbook shows it.
fn sheet_title(name: &str) -> &'static str {
    match name {
        "project" => "Project",
        "wbs" => "WBS",
        "resources" => "Resources",
        "relationships" => "Relationships",
        _ => "Activities",
    }
}

/// A row's number in its sheet, and its values by column.
type Row = (usize, BTreeMap<String, String>);

/// What the rows ask for: the project, and its edits with where each came from.
struct Plan {
    project: BTreeMap<String, String>,
    /// The Project sheet's row, if it has one.
    project_row: Option<usize>,
    edits: Vec<(Edit, String)>,
}

/// An edit from its JSON form, or why the row can't give one.
fn edit_of(value: Value) -> Result<Edit, String> {
    serde_json::from_value(value).map_err(|e| e.to_string())
}

/// A span from a text such as `5d`, or a number of hours.
pub(crate) fn span_of(
    text: Option<&String>,
    hours: Option<&String>,
    what: &str,
) -> Result<Option<String>, String> {
    if let Some(text) = text {
        return Ok(Some(text.clone()));
    }
    let Some(hours) = hours else {
        return Ok(None);
    };
    let value: f64 = hours
        .parse()
        .map_err(|_| format!("{what} {hours} isn't a number of hours"))?;
    // In whole minutes, as an export from `query` gives a third of an hour.
    let minutes = (value * 60.0).round();
    if !value.is_finite() || minutes.abs() > 6e10 {
        return Err(format!("{what} {hours} isn't a number of hours"));
    }
    #[allow(
        clippy::cast_possible_truncation,
        reason = "checked above to be in range"
    )]
    let span = totalfloat_edit::Span::hours_of(totalfloat_model::Minutes::new(minutes as i64))
        .ok_or_else(|| format!("{what} {hours} is more hours than a schedule holds"))?;
    Ok(Some(span.to_string()))
}

#[allow(
    clippy::too_many_lines,
    reason = "one pass over each sheet in turn, in the order the edits apply"
)]
fn plan(book: &Book, file: &str, problems: &mut Vec<Problem>) -> Plan {
    let sheets: BTreeMap<&str, Vec<Row>> = SHEETS
        .iter()
        .map(|&sheet| (sheet.0, rows_of(book, sheet, file, problems)))
        .collect();
    let rows = |name: &str| sheets.get(name).map_or(&[][..], Vec::as_slice);
    let mut edits: Vec<(Edit, String)> = Vec::new();
    // Whether the row became an edit; if not, it's a problem at the row.
    let mut push = |value: Value, at: (&str, usize), problems: &mut Vec<Problem>| {
        let (sheet, row) = at;
        match edit_of(value) {
            Ok(edit) => {
                edits.push((edit, format!("{file} {sheet} row {row}")));
                true
            }
            Err(why) => {
                problems.push(problem(
                    (file, sheet, row),
                    "bad-row",
                    why,
                    "fix the row's values",
                ));
                false
            }
        }
    };

    let project = match rows("project") {
        // A row with a blank value is already reported at its row.
        [] if book
            .get("project")
            .is_some_and(|sheet| sheet.rows.len() > 1) =>
        {
            BTreeMap::new()
        }
        [] => {
            problems.push(problem(
                (file, "Project", 1),
                "missing-sheet",
                "there's no Project sheet, or it has no row".to_owned(),
                "add a Project sheet with code, name and data_date, and one row",
            ));
            BTreeMap::new()
        }
        [(row, only)] => {
            for date in ["data_date", "must_finish"] {
                if let Some(value) = only.get(date)
                    && parse_date(value).is_err()
                {
                    problems.push(problem(
                        (file, "Project", *row),
                        "bad-date",
                        format!("{date} {value} isn't a date and time"),
                        "write it as YYYY-MM-DD HH:MM",
                    ));
                }
            }
            only.clone()
        }
        [_, (row, _), ..] => {
            problems.push(problem(
                (file, "Project", *row),
                "one-project",
                "the Project sheet has more than one row".to_owned(),
                "a spreadsheet builds one project; keep one row",
            ));
            rows("project")[0].1.clone()
        }
    };
    let root = project.get("code").cloned().unwrap_or_default();
    if book
        .get("activities")
        .is_none_or(|sheet| sheet.rows.len() < 2)
    {
        problems.push(problem(
            (file, "Activities", 1),
            "missing-sheet",
            "there's no Activities sheet, or it has no activities".to_owned(),
            "add an Activities sheet with code and name, a row an activity",
        ));
    }

    // WBS nodes in the order the sheets give them: the WBS sheet's, then any
    // an activity names. Walking each path makes its parents first.
    let mut names: BTreeMap<String, (String, usize, &str)> = BTreeMap::new();
    let mut paths: Vec<String> = Vec::new();
    for (row, values) in rows("wbs") {
        if let (Some(path), Some(name)) = (values.get("wbs_path"), values.get("name"))
            && !names.contains_key(path)
        {
            names.insert(path.clone(), (name.clone(), *row, "WBS"));
            paths.push(path.clone());
        }
    }
    for (row, values) in rows("activities") {
        if let Some(path) = values.get("wbs_path")
            && !names.contains_key(path)
        {
            let code = path.rsplit('.').next().unwrap_or(path).to_owned();
            names.insert(path.clone(), (code, *row, "Activities"));
            paths.push(path.clone());
        }
    }
    let mut made: BTreeSet<String> = BTreeSet::from([root.clone()]);
    // Edits find a WBS node by its own code, so each code names one node.
    let mut code_paths: BTreeMap<String, String> = BTreeMap::new();
    for path in &paths {
        let (_, row, sheet) = &names[path];
        let Some(rest) = path.strip_prefix(&format!("{root}.")) else {
            // Without a project code, its own problem says so.
            if *path != root && !root.is_empty() {
                problems.push(problem(
                    (file, sheet, *row),
                    "wrong-wbs-root",
                    format!("WBS {path} doesn't start at the project, {root}"),
                    &format!("write the path from the project code, as {root}.CIV"),
                ));
            }
            continue;
        };
        // Each level along the path, made once, named by its code unless the WBS sheet names it.
        let mut parent = root.clone();
        let mut so_far = root.clone();
        for code in rest.split('.') {
            so_far = format!("{so_far}.{code}");
            if let Some(other) = code_paths.get(&code.to_lowercase())
                && *other != so_far
            {
                problems.push(problem(
                    (file, sheet, *row),
                    "repeated-wbs-code",
                    format!("the WBS code {code} is in both {other} and {so_far}"),
                    "edits find a WBS node by its own code, so give each node a code used once in the project",
                ));
                break;
            }
            code_paths.insert(code.to_lowercase(), so_far.clone());
            if made.insert(so_far.clone()) {
                let name = names.get(&so_far).map_or(code, |(n, _, _)| n.as_str());
                push(
                    json!({"op": "add_wbs", "code": code, "name": name, "parent": parent}),
                    (sheet, *row),
                    problems,
                );
            }
            code.clone_into(&mut parent);
        }
    }
    let wbs_of = |path: Option<&String>| {
        path.and_then(|p| p.rsplit('.').next())
            .map_or_else(|| root.clone(), str::to_owned)
    };

    for (row, values) in rows("resources") {
        let mut object = Map::new();
        object.insert("op".into(), "add_resource".into());
        for (column, key) in [
            ("resource", "resource"),
            ("name", "name"),
            ("type", "resource_type"),
            ("calendar", "calendar"),
        ] {
            if let Some(value) = values.get(column) {
                // Types in any case, as a spreadsheet capitalises them.
                let value = if column == "type" {
                    value.to_lowercase()
                } else {
                    value.clone()
                };
                object.insert(key.into(), value.into());
            }
        }
        let mut good = true;
        for (column, hint) in [
            ("units_per_hour", "give units an hour, such as 1 or 0.5"),
            (
                "max_units_per_hour",
                "give the most units an hour it can work, such as 2 for two people at once",
            ),
        ] {
            let Some(units) = values.get(column) else {
                continue;
            };
            match units.parse::<f64>() {
                Ok(u) if u.is_finite() => {
                    object.insert(column.into(), json!(u));
                }
                _ => {
                    problems.push(problem(
                        (file, "Resources", *row),
                        "bad-number",
                        format!("{column} {units} isn't a number"),
                        hint,
                    ));
                    good = false;
                }
            }
        }
        if good {
            push(Value::Object(object), ("Resources", *row), problems);
        }
    }

    let codes: BTreeSet<&str> = rows("activities")
        .iter()
        .filter_map(|(_, v)| v.get("code").map(String::as_str))
        .collect();
    let mut seen = BTreeSet::new();
    let mut added: BTreeSet<&str> = BTreeSet::new();
    let mut later: Vec<(Value, usize)> = Vec::new();
    for (row, values) in rows("activities") {
        let at = (file, "Activities", *row);
        let Some(code) = values.get("code") else {
            continue;
        };
        if !seen.insert(code.clone()) {
            problems.push(problem(
                at,
                "duplicate-activity",
                format!("{code} is listed twice"),
                "give each activity its own ID",
            ));
            continue;
        }
        let duration = match span_of(
            values.get("duration"),
            values.get("original_duration_hours"),
            "original_duration_hours",
        ) {
            Ok(d) => d,
            Err(why) => {
                problems.push(problem(
                    at,
                    "bad-number",
                    why,
                    "give hours, such as 40, or a duration such as 5d",
                ));
                continue;
            }
        };
        let mut object = json!({
            "op": "add_activity",
            "code": code,
            "name": values.get("name").cloned().unwrap_or_default(),
            "wbs": wbs_of(values.get("wbs_path")),
        });
        if let Some(kind) = values.get("type") {
            object["activity_type"] = kind.to_lowercase().into();
        }
        if let Some(calendar) = values.get("calendar") {
            object["calendar"] = calendar.clone().into();
        }
        if let Some(duration) = duration {
            object["duration"] = duration.into();
        }
        // A row that didn't become an activity has said so; what hangs on
        // it would only repeat that.
        if !push(object, ("Activities", *row), problems) {
            continue;
        }
        added.insert(code.as_str());
        if let Some(constraint) = values.get("constraint") {
            let constraint = constraint.to_lowercase();
            let mut set =
                json!({"op": "set_constraint", "activity": code, "constraint": constraint});
            if let Some(date) = values.get("constraint_date") {
                if parse_date(date).is_err() {
                    problems.push(problem(
                        at,
                        "bad-date",
                        format!("constraint_date {date} isn't a date and time"),
                        "write it as YYYY-MM-DD HH:MM",
                    ));
                    continue;
                }
                set["date"] = date.clone().into();
            }
            later.push((set, *row));
        } else if values.contains_key("constraint_date") {
            problems.push(problem(
                at,
                "bad-row",
                "constraint_date is given without a constraint".to_owned(),
                "give the constraint too, such as start-on-or-after",
            ));
        }
        for resource in values
            .get("resources")
            .map(|r| r.split(','))
            .into_iter()
            .flatten()
        {
            let resource = resource.trim();
            if !resource.is_empty() {
                later.push((
                    json!({"op": "assign_resource", "activity": code, "resource": resource}),
                    *row,
                ));
            }
        }
    }
    // Constraints and assignments once every activity is there.
    for (value, row) in later {
        push(value, ("Activities", row), problems);
    }

    for (row, values) in rows("relationships") {
        let at = (file, "Relationships", *row);
        let mut missing = false;
        for end in ["predecessor", "successor"] {
            if let Some(code) = values.get(end)
                && !codes.contains(code.as_str())
            {
                problems.push(problem(
                    at,
                    "no-such-activity",
                    format!("{end} {code} isn't on the Activities sheet"),
                    "link activities the Activities sheet lists",
                ));
                missing = true;
            }
        }
        // An activity whose own row was refused has said so already.
        let refused = ["predecessor", "successor"]
            .iter()
            .filter_map(|end| values.get(*end))
            .any(|code| !added.contains(code.as_str()));
        if missing || refused {
            continue;
        }
        let lag = match span_of(
            values.get("lag"),
            values.get("link_lag_hours"),
            "link_lag_hours",
        ) {
            Ok(lag) => lag,
            Err(why) => {
                problems.push(problem(
                    at,
                    "bad-number",
                    why,
                    "give hours, such as 8, or a lag such as 2d",
                ));
                continue;
            }
        };
        let mut object = json!({
            "op": "add_link",
            "predecessor": values.get("predecessor").cloned().unwrap_or_default(),
            "successor": values.get("successor").cloned().unwrap_or_default(),
        });
        if let Some(kind) = values.get("link_type") {
            object["link_type"] = kind.to_uppercase().into();
        }
        if let Some(lag) = lag {
            object["lag"] = lag.into();
        }
        push(object, ("Relationships", *row), problems);
    }
    let project_row = rows("project").first().map(|(row, _)| *row);
    Plan {
        project,
        project_row,
        edits,
    }
}

/// The error for a spreadsheet's problems, each at its sheet and row, in
/// the order found: sheet by sheet, row by row.
fn refused(file: &str, problems: Vec<Problem>) -> CliError {
    let count = problems.len();
    let mut error = CliError::new(
        "sheet-refused",
        format!(
            "{file} has {count} problem{}, so nothing was built",
            if count == 1 { "" } else { "s" }
        ),
        "fix each row named, then run again",
    );
    error.problems = problems;
    error
}

/// Builds the schedule a spreadsheet describes.
///
/// # Errors
///
/// `file-not-readable`, `sheet-refused` with every problem at its sheet and
/// row, or what `new` refuses.
pub fn build(input: &Path) -> Result<(RawXer, Vec<String>, Vec<String>), CliError> {
    let file = input.display().to_string();
    let mut problems = Vec::new();
    let book = if input.is_dir() {
        folder(input, &mut problems, &file)?
    } else {
        workbook(input, &mut problems, &file)?
    };
    let plan = plan(&book, &file, &mut problems);
    let get = |key: &str| plan.project.get(key).map(String::as_str);
    let spec = new::Spec {
        code: get("code").unwrap_or_default(),
        name: get("name").unwrap_or_default(),
        data_date: get("data_date").unwrap_or_default(),
        must_finish: get("must_finish"),
        calendar: get("calendar"),
    };
    // Without a project there's nothing to check the edits against.
    let raw = match new::start(&spec) {
        Ok(raw) => raw,
        Err(error) => {
            // A bad date is already said at the row; anything else is new.
            let said = error.code == "invalid-date"
                && problems
                    .iter()
                    .any(|p| p.code == "bad-date" && p.source.contains(" Project row "));
            if let (Some(row), false) = (plan.project_row, said) {
                problems.push(problem(
                    (&file, "Project", row),
                    error.code,
                    error.message,
                    &error.hint,
                ));
            }
            return Err(refused(&file, problems));
        }
    };
    if plan.edits.is_empty() && problems.is_empty() {
        return Ok((raw, Vec::new(), Vec::new()));
    }
    // The rows that couldn't be read go in with the edits, so the edits are
    // checked too and every problem is reported in one run.
    let (edits, sources): (Vec<Edit>, Vec<String>) = plan.edits.into_iter().unzip();
    let loaded = Loaded::new(raw);
    match edit::report_from(
        &file,
        &loaded,
        (&edits, &sources),
        &problems,
        edit::Options::default(),
    ) {
        Ok(done) => {
            let (applied, warnings) = done.report.notes();
            Ok((done.raw, applied, warnings))
        }
        Err(error) if !error.problems.is_empty() => Err(refused(&file, error.problems)),
        Err(_) if !problems.is_empty() => Err(refused(&file, problems)),
        Err(error) => Err(error),
    }
}

/// Runs `import` on a spreadsheet.
///
/// # Errors
///
/// As [`build`], or what writing the output refuses.
pub fn run(input: &Path, output: &Path, json: bool) -> Result<u8, CliError> {
    refuse_existing(output)?;
    let (raw, applied, warnings) = build(input)?;
    create_new(&raw, output)?;
    let report = new::report(output, &Loaded::new(raw), applied, warnings);
    if json {
        print_line(&serde_json::to_string_pretty(&report).unwrap_or_default());
    } else {
        print_line(&new::human(&report));
    }
    Ok(exit::OK)
}

#[cfg(test)]
mod tests {
    use super::excel_date;

    #[test]
    fn excel_dates_read_in_either_date_system_to_the_minute() {
        use calamine::{ExcelDateTime, ExcelDateTimeType};
        let date = |serial, is_1904| {
            excel_date(&ExcelDateTime::new(
                serial,
                ExcelDateTimeType::DateTime,
                is_1904,
            ))
        };
        assert_eq!(date(46_083.0, false).as_deref(), Some("2026-03-02 00:00"));
        // 08:00, a hair short as stored.
        assert_eq!(
            date(46_083.333_333_33, false).as_deref(),
            Some("2026-03-02 08:00")
        );
        // The same day, counted from 1904.
        assert_eq!(
            date(44_621.333_333_33, true).as_deref(),
            Some("2026-03-02 08:00")
        );
    }
}
