//! `edit FILE --from-sheet SHEET`: edits made in a spreadsheet read back into
//! the schedule. The sheet is what `query --xlsx` or `query --csv`
//! wrote, or a folder of CSV files: an Activities sheet keyed by `code`,
//! the Activity ID, and if wanted a Relationships sheet. Each cell that
//! differs from the schedule becomes an ordinary edit, so the run writes
//! what the same `edit` batch would, and nothing else. Columns `query` gives
//! that can't be edited here, such as dates and floats, are left as they are.
//! Every problem is reported at its sheet, row and column, and nothing
//! changes unless every row is good.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde_json::{Map, Value};
use totalfloat_edit::{ConstraintKind, Edit, LinkType, Span};
use totalfloat_model::{RelationshipType, Schedule};

use crate::error::{CliError, Problem};
use crate::loaded::Loaded;
use crate::query::{self, Query};
use crate::sheets::{self, Book};

/// The Activities columns that become edits; `code:TYPE` and `udf:LABEL`
/// columns do too.
const EDITABLE: [&str; 6] = [
    "name",
    "original_duration_hours",
    "remaining_duration_hours",
    "calendar",
    "constraint",
    "constraint_date",
];

/// What a sheet comes to: its edits, where each came from, and the project.
pub struct Expanded {
    /// The edits, Activities rows first, then Relationships.
    pub edits: Vec<Edit>,
    /// Each edit's sheet, row and column.
    pub sources: Vec<String>,
    /// The project the activities are in (`proj_id`).
    pub project: u64,
}

/// A problem at a sheet's row and, if it's about one cell, its column.
fn problem(
    (file, sheet, row): (&str, &str, usize),
    column: Option<(usize, &str)>,
    code: &'static str,
    message: String,
    hint: &str,
) -> Problem {
    let at = column.map_or_else(String::new, |(_, name)| format!(" column {name}"));
    Problem {
        edit: None,
        source: format!("{file} {sheet} row {row}{at}"),
        code,
        message,
        hint: hint.to_owned(),
        depends_on: None,
        line: Some(row),
        column: column.map(|(k, _)| k + 1),
    }
}

/// A cell as `query --csv` meant it: text it guarded from running as a
/// formula with a leading `'` loses the `'`.
pub(crate) fn unguarded(text: &str) -> &str {
    text.strip_prefix('\'')
        .filter(|rest| rest.starts_with(['=', '+', '-', '@', '\t', '\r']))
        .unwrap_or(text)
}

/// The sheets to read: a workbook, a folder of CSV files, or one CSV file
/// of activities.
fn book_of(input: &Path, file: &str, problems: &mut Vec<Problem>) -> Result<Book, CliError> {
    let csv = input
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("csv"));
    if csv {
        let text = std::fs::read_to_string(input).map_err(|e| unreadable(input, &e.to_string()))?;
        let rows = crate::table::records(&text).map_err(|why| unreadable(input, &why))?;
        return Ok(Book::from([(
            "activities".to_owned(),
            sheets::Sheet { rows },
        )]));
    }
    if input.is_dir() {
        sheets::folder(input, problems, file)
    } else {
        sheets::workbook(input, problems, file)
    }
}

fn unreadable(input: &Path, why: &str) -> CliError {
    CliError::new(
        "file-not-readable",
        format!("can't read {}: {why}", input.display()),
        "give the .xlsx or .csv `query` wrote, or a folder with activities.csv in it",
    )
}

/// A column's name as `query` names it: fixed names in lower case, and the
/// type or label after `code:` or `udf:` as written.
fn column_name(header: &str) -> String {
    let header = header.trim();
    let lower = header.to_lowercase();
    for prefix in ["code:", "udf:"] {
        if lower.starts_with(prefix) {
            return format!("{prefix}{}", &header[prefix.len()..]);
        }
    }
    lower
}

/// A value `query` gives, as the text a sheet would hold.
fn text_of(value: Option<&Value>) -> String {
    match value {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(text)) => text.clone(),
        Some(other) => other.to_string(),
    }
}

/// Whether a sheet's cell in `column` says the same as the schedule:
/// numbers by value, hours to a thousandth of an hour, the rest as text.
/// Sheets lose the spaces around a value and keep line breaks as either
/// `\r\n` or `\n`, so those don't count as changes.
fn same(cell: &str, current: Option<&Value>, column: &str) -> bool {
    if let (Some(Value::Number(b)), Ok(a)) = (current, cell.parse::<f64>()) {
        let tolerance = if column.ends_with("_hours") {
            0.0005
        } else {
            0.0
        };
        return b.as_f64().is_some_and(|b| (a - b).abs() <= tolerance);
    }
    let now = text_of(current);
    let plain = |text: &str| text.replace("\r\n", "\n").trim().to_owned();
    plain(cell) == plain(&now)
}

/// Reads the sheet and works out its edits.
///
/// # Errors
///
/// `file-not-readable`, `needs-project`, `sheet-refused` with every problem
/// at its sheet, row and column, or `nothing-to-change`.
pub fn expand(
    file: &str,
    loaded: &Loaded,
    input: &Path,
    project: Option<u64>,
) -> Result<Expanded, CliError> {
    let sheet_file = input.display().to_string();
    let mut problems = Vec::new();
    let book = book_of(input, &sheet_file, &mut problems)?;
    let Some(activities) = book.get("activities") else {
        return Err(unreadable(input, "it has no Activities sheet"));
    };
    let project = one_project(loaded, project)?;
    let Some(schedule) = loaded.schedules.iter().find(|s| s.project.id.0 == project) else {
        return Err(CliError::new(
            "no-such-project",
            format!("the file has no project {project}"),
            "`inspect` lists the projects in the file",
        ));
    };
    let csv = input.is_dir()
        || input
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("csv"));
    let mut planned = Planned {
        csv,
        file: sheet_file,
        edits: Vec::new(),
        sources: Vec::new(),
        problems,
    };
    let codes = planned.activities(file, loaded, schedule, activities)?;
    if let Some(links) = book.get("relationships") {
        planned.relationships(schedule, links, &codes);
    }
    let Planned {
        csv: _,
        file: sheet_file,
        edits,
        sources,
        problems,
    } = planned;
    if !problems.is_empty() {
        let count = problems.len();
        let mut error = CliError::new(
            "sheet-refused",
            format!(
                "{sheet_file} has {count} problem{}, so nothing changed",
                if count == 1 { "" } else { "s" }
            ),
            "fix each cell named, then run again",
        );
        error.problems = problems;
        return Err(error);
    }
    Ok(Expanded {
        edits,
        sources,
        project,
    })
}

/// The project the sheet is about: the one given, or the file's only one.
pub(crate) fn one_project(loaded: &Loaded, project: Option<u64>) -> Result<u64, CliError> {
    if let Some(project) = project {
        return Ok(project);
    }
    match loaded.schedules.as_slice() {
        [one] => Ok(one.project.id.0),
        _ => Err(CliError::new(
            "needs-project",
            format!("the file has {} projects", loaded.schedules.len()),
            "give --project, the proj_id of the project the sheet is about",
        )),
    }
}

/// The edits and problems found so far.
struct Planned {
    /// CSV, whose formula guard `'` comes off; a workbook's cells are as typed.
    csv: bool,
    file: String,
    edits: Vec<Edit>,
    sources: Vec<String>,
    problems: Vec<Problem>,
}

impl Planned {
    fn push(&mut self, edit: Edit, (sheet, row): (&str, usize), column: &str) {
        self.edits.push(edit);
        self.sources
            .push(format!("{} {sheet} row {row} column {column}", self.file));
    }

    fn refuse(
        &mut self,
        (sheet, row): (&str, usize),
        column: Option<(usize, &str)>,
        code: &'static str,
        message: String,
        hint: &str,
    ) {
        let problem = problem((&self.file, sheet, row), column, code, message, hint);
        self.problems.push(problem);
    }

    /// The Activities sheet's edits; gives the Activity IDs it lists.
    fn activities(
        &mut self,
        file: &str,
        loaded: &Loaded,
        schedule: &Schedule,
        sheet: &sheets::Sheet,
    ) -> Result<BTreeSet<String>, CliError> {
        let mut codes = BTreeSet::new();
        let Some(((header_row, header), rows)) = sheet.rows.split_first() else {
            return Ok(codes);
        };
        let columns: Vec<String> = header.iter().map(|h| column_name(h)).collect();
        let Some(key) = columns.iter().position(|c| c == "code") else {
            self.refuse(
                ("Activities", *header_row),
                None,
                "missing-column",
                "Activities needs a code column, the Activity ID".to_owned(),
                "export with `query --columns code,...`",
            );
            return Ok(codes);
        };
        let editable: Vec<(usize, &String)> = columns
            .iter()
            .enumerate()
            .filter(|(_, c)| {
                EDITABLE.contains(&c.as_str()) || c.starts_with("code:") || c.starts_with("udf:")
            })
            .collect();
        // The schedule's values for the same columns, by Activity ID.
        let mut asked = vec!["code".to_owned()];
        asked.extend(editable.iter().map(|(_, c)| (*c).clone()));
        // A changed constraint keeps the other half as the schedule has it.
        if asked.iter().any(|c| c.starts_with("constraint")) {
            for both in ["constraint", "constraint_date"] {
                if !asked.iter().any(|c| c == both) {
                    asked.push(both.to_owned());
                }
            }
        }
        let query = Query {
            project: Some(schedule.project.id.0),
            columns: asked,
            limit: usize::MAX / 2,
            ..Query::default()
        };
        let report = query::report(file, loaded, &query)?;
        let current: BTreeMap<String, &Map<String, Value>> = report
            .rows
            .iter()
            .filter_map(|row| Some((row.get("code")?.as_str()?.to_owned(), row)))
            .collect();
        for (row, cells) in rows {
            let at = ("Activities", *row);
            let csv = self.csv;
            let raw = |k: usize| cells.get(k).map_or("", |c| c.trim());
            let cell = |k: usize| if csv { unguarded(raw(k)) } else { raw(k) }.to_owned();
            // A value that starts with `'` already isn't guarded on export,
            // so the cell as written is tried before the unguarded one.
            let code = Some(raw(key).to_owned())
                .filter(|code| current.contains_key(code))
                .unwrap_or_else(|| cell(key));
            let Some(now) = current.get(&code) else {
                self.refuse(
                    at,
                    Some((key, "code")),
                    "no-such-activity",
                    format!("the schedule has no activity {code}"),
                    "re-import edits only; add activities with `edit --add-activity` or `import`",
                );
                continue;
            };
            if !codes.insert(code.clone()) {
                self.refuse(
                    at,
                    Some((key, "code")),
                    "repeated-activity",
                    format!("{code} has more than one row"),
                    "keep one row for each activity",
                );
                continue;
            }
            let changed: BTreeMap<&str, (usize, String)> = editable
                .iter()
                .filter(|(k, c)| !same(raw(*k), now.get(c.as_str()), c))
                .map(|(k, c)| (c.as_str(), (*k, cell(*k))))
                .filter(|(c, (_, value))| !same(value, now.get(*c), c))
                .collect();
            self.activity_edits(&code, now, &changed, at);
        }
        Ok(codes)
    }

    /// The edits one Activities row's changed cells come to.
    fn activity_edits(
        &mut self,
        code: &str,
        now: &Map<String, Value>,
        changed: &BTreeMap<&str, (usize, String)>,
        at: (&str, usize),
    ) {
        let activity = || code.to_owned();
        self.text_edit(changed, at, "name", |name| Edit::SetName {
            activity: activity(),
            name,
        });
        self.duration_edit(code, changed, at);
        self.text_edit(changed, at, "calendar", |calendar| Edit::SetCalendar {
            activity: activity(),
            calendar,
        });
        self.constraint_edit(code, now, changed, at);
        self.field_edits(code, changed, at);
    }

    /// An edit from a column that can't be blank, if it changed.
    fn text_edit(
        &mut self,
        changed: &BTreeMap<&str, (usize, String)>,
        at: (&str, usize),
        column: &str,
        edit: impl FnOnce(String) -> Edit,
    ) {
        let Some((k, value)) = changed.get(column) else {
            return;
        };
        if value.is_empty() {
            self.refuse(
                at,
                Some((*k, column)),
                "missing-value",
                format!("the {column} is blank"),
                "every activity has one",
            );
        } else {
            self.push(edit(value.clone()), at, column);
        }
    }

    /// One duration edit for a changed original or remaining duration, or both.
    fn duration_edit(
        &mut self,
        code: &str,
        changed: &BTreeMap<&str, (usize, String)>,
        at: (&str, usize),
    ) {
        let mut span = |column: &str| -> Option<Span> {
            let (k, text) = changed.get(column)?;
            let read = sheets::span_of(None, Some(text), column).and_then(|s| {
                s.map(|s| s.parse::<Span>().map_err(|e| e.to_string()))
                    .transpose()
            });
            let (code, why) = match read {
                Ok(Some(span)) => return Some(span),
                Ok(None) => ("missing-value", format!("{column} is blank")),
                Err(why) => ("bad-value", why),
            };
            self.refuse(at, Some((*k, column)), code, why, "give a number of hours");
            None
        };
        let original = span("original_duration_hours");
        let remaining = span("remaining_duration_hours");
        if original.is_none() && remaining.is_none() {
            return;
        }
        let column = if original.is_some() {
            "original_duration_hours"
        } else {
            "remaining_duration_hours"
        };
        let edit = Edit::SetDuration {
            activity: code.to_owned(),
            original,
            remaining,
        };
        self.push(edit, at, column);
    }

    /// Code and user field values set or taken off; a blank cell takes it off.
    fn field_edits(
        &mut self,
        code: &str,
        changed: &BTreeMap<&str, (usize, String)>,
        at: (&str, usize),
    ) {
        let activity = || code.to_owned();
        for (column, (_, value)) in changed {
            let edit = if let Some(code_type) = column.strip_prefix("code:") {
                let code_type = code_type.to_owned();
                if value.is_empty() {
                    Edit::ClearCode {
                        activity: activity(),
                        code_type,
                    }
                } else {
                    Edit::SetCode {
                        activity: activity(),
                        code_type,
                        value: value.clone(),
                    }
                }
            } else if let Some(field) = column.strip_prefix("udf:") {
                let (field, activity) = (field.to_owned(), Some(activity()));
                if value.is_empty() {
                    Edit::ClearUdf {
                        field,
                        activity,
                        wbs: None,
                        resource: None,
                    }
                } else {
                    Edit::SetUdf {
                        field,
                        value: value.clone(),
                        activity,
                        wbs: None,
                        resource: None,
                    }
                }
            } else {
                continue;
            };
            self.push(edit, at, column);
        }
    }

    /// The primary constraint, if its kind or date changed.
    fn constraint_edit(
        &mut self,
        code: &str,
        now: &Map<String, Value>,
        changed: &BTreeMap<&str, (usize, String)>,
        at: (&str, usize),
    ) {
        let kind_cell = changed.get("constraint");
        let date_cell = changed.get("constraint_date");
        if kind_cell.is_none() && date_cell.is_none() {
            return;
        }
        let kind = kind_cell.map_or_else(|| text_of(now.get("constraint")), |(_, v)| v.clone());
        let mut date =
            date_cell.map_or_else(|| text_of(now.get("constraint_date")), |(_, v)| v.clone());
        let (k, column) = kind_cell.map_or_else(
            || (date_cell.map_or(0, |(k, _)| *k), "constraint_date"),
            |(k, _)| (*k, "constraint"),
        );
        if kind.is_empty() {
            if date.is_empty() {
                self.push(
                    Edit::ClearConstraint {
                        activity: code.to_owned(),
                        secondary: false,
                    },
                    at,
                    column,
                );
            } else {
                self.refuse(
                    at,
                    Some((k, column)),
                    "missing-value",
                    "a constraint date needs a constraint".to_owned(),
                    "give the constraint too, such as start-on-or-after",
                );
            }
            return;
        }
        let constraint = crate::edit::constraint_kind(&kind);
        // A kind that takes no date doesn't keep the date it had before.
        if date_cell.is_none() && matches!(constraint, Ok(ConstraintKind::AsLateAsPossible)) {
            date.clear();
        }
        match constraint {
            Ok(constraint) => self.push(
                Edit::SetConstraint {
                    activity: code.to_owned(),
                    constraint,
                    date: (!date.is_empty()).then_some(date),
                    secondary: false,
                },
                at,
                column,
            ),
            Err(error) => self.refuse(
                at,
                Some((k, column)),
                "bad-value",
                error.message,
                &error.hint,
            ),
        }
    }

    /// The Relationships sheet's edits: links among the Activities sheet's
    /// activities that it adds, changes or leaves out.
    fn relationships(
        &mut self,
        schedule: &Schedule,
        sheet: &sheets::Sheet,
        codes: &BTreeSet<String>,
    ) {
        let Some(((header_row, header), rows)) = sheet.rows.split_first() else {
            return;
        };
        let columns: Vec<String> = header.iter().map(|h| column_name(h)).collect();
        let find = |name: &str| columns.iter().position(|c| c == name);
        let (Some(pred), Some(succ)) = (find("predecessor"), find("successor")) else {
            self.refuse(
                ("Relationships", *header_row),
                None,
                "missing-column",
                "Relationships needs predecessor and successor columns".to_owned(),
                "give each link's predecessor and successor Activity IDs, and if wanted link_type and lag or link_lag_hours",
            );
            return;
        };
        let at = LinkColumns {
            pred,
            succ,
            kind: find("link_type"),
            lag: find("lag"),
            hours: find("link_lag_hours"),
        };
        let existing = existing_links(schedule, codes);
        let mut listed = BTreeSet::new();
        let mut wanted = Vec::new();
        for (row, cells) in rows {
            let here = ("Relationships", *row);
            let Some((p, s, kind, lag)) = self.link_row(here, cells, &at, codes) else {
                continue;
            };
            for kind in kinds_of(&existing, &p, &s, kind) {
                if !listed.insert((p.clone(), s.clone(), rank(kind))) {
                    self.refuse(
                        here,
                        Some((pred, "predecessor")),
                        "repeated-link",
                        format!("{p} to {s} {kind:?} is listed twice"),
                        "keep one row for each link",
                    );
                    continue;
                }
                wanted.push((*row, p.clone(), s.clone(), kind, lag));
            }
        }
        // Links left out go first, so one turned round isn't a loop meanwhile.
        self.remove_left_out(&existing, &listed);
        let by_code: BTreeMap<&str, _> = schedule
            .activities
            .values()
            .map(|a| (a.code.as_str(), a))
            .collect();
        for (row, p, s, kind, lag) in wanted {
            let here = ("Relationships", row);
            let Some(now) = existing.get(&(p.clone(), s.clone(), rank(kind))) else {
                let lag = lag.map(|(lag, _)| lag).filter(|lag| *lag != Span::ZERO);
                let edit = Edit::AddLink {
                    predecessor: p,
                    successor: s,
                    link_type: Some(kind),
                    lag,
                };
                self.push(edit, here, "predecessor");
                continue;
            };
            // No lag given keeps the link's own.
            let Some((lag, (k, column))) = lag else {
                continue;
            };
            // The lag in minutes as the link edits count it, on the lag calendar.
            let lengths = by_code
                .get(p.as_str())
                .zip(by_code.get(s.as_str()))
                .and_then(|(before, after)| {
                    totalfloat_edit::lag_lengths_for(schedule, before, after)
                });
            let Some(minutes) = lag.minutes_in(lengths.as_ref()) else {
                let why = totalfloat_edit::no_length(&s, lag);
                self.refuse(here, Some((k, column)), why.code, why.message, &why.hint);
                continue;
            };
            if minutes.get() != *now {
                let edit = Edit::ChangeLink {
                    predecessor: p,
                    successor: s,
                    link_type: Some(kind),
                    new_type: None,
                    lag: Some(lag),
                };
                self.push(edit, here, column);
            }
        }
    }

    /// Removes the links among the sheet's activities that it leaves out.
    fn remove_left_out(&mut self, existing: &Links, listed: &BTreeSet<(String, String, u8)>) {
        for (p, s, rank) in existing.keys() {
            if !listed.contains(&(p.clone(), s.clone(), *rank)) {
                let kind = KINDS[usize::from(*rank)];
                self.sources.push(format!(
                    "{} Relationships: {p} to {s} {kind:?} left out",
                    self.file
                ));
                self.edits.push(Edit::RemoveLink {
                    predecessor: p.clone(),
                    successor: s.clone(),
                    link_type: Some(kind),
                });
            }
        }
    }

    /// A Relationships row's ends, its type and its lag with the column it
    /// came from, or `None` with its problems reported. A blank or missing
    /// type or lag is `None`.
    fn link_row(
        &mut self,
        here: (&str, usize),
        cells: &[String],
        at: &LinkColumns,
        codes: &BTreeSet<String>,
    ) -> Option<LinkRow> {
        let csv = self.csv;
        let raw = |k: Option<usize>| k.and_then(|k| cells.get(k)).map_or("", |c| c.trim());
        let cell = |k: Option<usize>| if csv { unguarded(raw(k)) } else { raw(k) }.to_owned();
        // An Activity ID as written, or without the quote `query --csv` adds.
        let end = |k: usize| {
            Some(raw(Some(k)).to_owned())
                .filter(|code| codes.contains(code))
                .unwrap_or_else(|| cell(Some(k)))
        };
        let (p, s) = (end(at.pred), end(at.succ));
        let mut ok = true;
        for (k, name, code) in [(at.pred, "predecessor", &p), (at.succ, "successor", &s)] {
            if !codes.contains(code) {
                self.refuse(
                    here,
                    Some((k, name)),
                    "no-such-activity",
                    format!("{code} isn't on the Activities sheet"),
                    "list both ends of each link on the Activities sheet",
                );
                ok = false;
            }
        }
        let kind_text = cell(at.kind);
        let kind = if kind_text.is_empty() {
            Some(None)
        } else {
            serde_json::from_value::<LinkType>(Value::from(kind_text.to_uppercase()))
                .ok()
                .map(Some)
        };
        let Some(kind) = kind else {
            self.refuse(
                here,
                Some((at.kind.unwrap_or(0), "link_type")),
                "bad-value",
                format!("{kind_text} isn't a link type"),
                "use FS, SS, FF or SF",
            );
            return None;
        };
        let (lag_text, hours) = (cell(at.lag), cell(at.hours));
        let lag = if !lag_text.is_empty() {
            // `+2d` as `--add-link` takes it, too.
            let plain = lag_text.strip_prefix('+').unwrap_or(&lag_text);
            plain
                .parse::<Span>()
                .map(|lag| Some((lag, (at.lag.unwrap_or(0), "lag"))))
                .map_err(|_| {
                    (
                        at.lag,
                        "lag",
                        format!("{lag_text} isn't a span such as 2d or -8h"),
                    )
                })
        } else if !hours.is_empty() {
            sheets::span_of(None, Some(&hours), "link_lag_hours")
                .ok()
                .flatten()
                .and_then(|s| s.parse::<Span>().ok())
                .map(|lag| Some((lag, (at.hours.unwrap_or(0), "link_lag_hours"))))
                .ok_or((
                    at.hours,
                    "link_lag_hours",
                    format!("{hours} isn't a number of hours"),
                ))
        } else {
            Ok(None)
        };
        match lag {
            Ok(lag) => ok.then_some((p, s, kind, lag)),
            Err((k, name, why)) => {
                self.refuse(
                    here,
                    Some((k.unwrap_or(0), name)),
                    "bad-value",
                    why,
                    "give a span such as 2d, or hours",
                );
                None
            }
        }
    }
}

/// A Relationships row: predecessor, successor, type if given, and lag if
/// given with the column (place and name) it came from.
type LinkRow = (
    String,
    String,
    Option<LinkType>,
    Option<(Span, (usize, &'static str))>,
);

/// Where a Relationships sheet's columns are.
struct LinkColumns {
    pred: usize,
    succ: usize,
    kind: Option<usize>,
    lag: Option<usize>,
    hours: Option<usize>,
}

/// Links by ends and type rank, with their lag in minutes.
type Links = BTreeMap<(String, String, u8), i64>;

/// The link types a row stands for: the one it gives, or with none given the
/// ones its ends already have, or a new FS link.
fn kinds_of(existing: &Links, p: &str, s: &str, kind: Option<LinkType>) -> Vec<LinkType> {
    if let Some(kind) = kind {
        return vec![kind];
    }
    let had: Vec<LinkType> = existing
        .keys()
        .filter(|(a, b, _)| a == p && b == s)
        .map(|(_, _, rank)| KINDS[usize::from(*rank)])
        .collect();
    if had.is_empty() {
        vec![LinkType::FS]
    } else {
        had
    }
}

/// The schedule's links among the sheet's activities: lag in minutes, by
/// ends and type.
fn existing_links(schedule: &Schedule, codes: &BTreeSet<String>) -> Links {
    let code_of: BTreeMap<_, &str> = schedule
        .activities
        .iter()
        .map(|(id, a)| (*id, a.code.as_str()))
        .collect();
    schedule
        .relationships
        .values()
        .filter_map(|link| {
            let p = *code_of.get(&link.predecessor)?;
            let s = *code_of.get(&link.successor)?;
            (codes.contains(p) && codes.contains(s)).then(|| {
                (
                    (p.to_owned(), s.to_owned(), rank(link_type(link.kind))),
                    link.lag.get(),
                )
            })
        })
        .collect()
}

/// The link types in a fixed order, so links sort.
const KINDS: [LinkType; 4] = [LinkType::FS, LinkType::SS, LinkType::FF, LinkType::SF];

/// A link type's place in [`KINDS`].
fn rank(kind: LinkType) -> u8 {
    match kind {
        LinkType::FS => 0,
        LinkType::SS => 1,
        LinkType::FF => 2,
        LinkType::SF => 3,
    }
}

const fn link_type(kind: RelationshipType) -> LinkType {
    match kind {
        RelationshipType::FinishToStart => LinkType::FS,
        RelationshipType::StartToStart => LinkType::SS,
        RelationshipType::FinishToFinish => LinkType::FF,
        RelationshipType::StartToFinish => LinkType::SF,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cell_is_the_same_whatever_spaces_and_line_breaks_a_sheet_keeps() {
        let text = |t: &str| Some(Value::from(t));
        assert!(same("Pour slab", text("Pour slab ").as_ref(), "name"));
        assert!(same(
            "Line one\nline two",
            text("Line one\r\nline two").as_ref(),
            "name"
        ));
        assert!(same(
            "8",
            Some(&Value::from(8.0)),
            "original_duration_hours"
        ));
        assert!(same(
            "7.9996",
            Some(&Value::from(8.0)),
            "original_duration_hours"
        ));
        assert!(same("", None, "udf:Notes"));
        assert!(!same("Pour slab B", text("Pour slab").as_ref(), "name"));
        assert!(!same(
            "7.99",
            Some(&Value::from(8.0)),
            "original_duration_hours"
        ));
        assert!(!same("x", None, "udf:Notes"));
    }

    #[test]
    fn only_hours_are_the_same_to_a_thousandth() {
        assert!(same("0.0002", Some(&Value::from(0.0002)), "udf:Rate"));
        assert!(!same("0.0006", Some(&Value::from(0.0002)), "udf:Rate"));
        assert!(!same("1", Some(&Value::from("001")), "udf:Ref"));
    }

    #[test]
    fn a_guarded_formula_loses_its_quote_and_other_text_keeps_it() {
        assert_eq!(unguarded("'=SUM(A1)"), "=SUM(A1)");
        assert_eq!(unguarded("'-3"), "-3");
        assert_eq!(unguarded("'quoted'"), "'quoted'");
        assert_eq!(column_name(" Code:Phase "), "code:Phase");
        assert_eq!(column_name("NAME"), "name");
    }
}
