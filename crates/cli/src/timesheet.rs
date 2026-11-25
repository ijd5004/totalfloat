//! Timesheets: `totalfloat timesheet` writes each team member's work
//! for a period as a sheet to fill in, and `edit --from-timesheet` applies
//! the hours and progress they report.
//!
//! A sheet lists one resource's assignments on unfinished activities whose
//! dates overlap the period. `hours` worked is added to the assignment's
//! actual units and `remaining_hours` sets its remaining units, through
//! `set_assignment_units`. `actual_start`, `actual_finish` and `percent`
//! become one `set_progress` per activity. Blank cells change nothing.
//! `actual_hours` is what the file recorded when the sheet was made, so a
//! sheet made from another version, or applied already, is refused.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use calamine::Reader;
use serde_json::{Map, Value, json};
use totalfloat_edit::Edit;
use totalfloat_model::{
    Activity, ActivityStatus, Assignment, Date, DateTime, PercentCompleteType, Resource,
    ResourceId, ResourceType, Schedule,
};

use crate::error::{CliError, Problem, parse_day};
use crate::loaded::Loaded;
use crate::output::print_line;

/// A timesheet's columns, in order: context, then the ones to fill in.
pub const COLUMNS: [&str; 12] = [
    "activity",
    "name",
    "resource",
    "planned_start",
    "planned_finish",
    "budget_hours",
    "actual_hours",
    "hours",
    "remaining_hours",
    "actual_start",
    "actual_finish",
    "percent",
];

/// How far apart `actual_hours` and the file may be and still agree: the
/// sheet's number may have been rounded on the way.
const SAME_HOURS: f64 = 0.005;

/// Where the timesheets go.
#[derive(Debug, Clone, Copy)]
pub enum Out<'a> {
    /// One workbook, a sheet per resource.
    Xlsx(&'a Path),
    /// A new folder, a CSV file per resource.
    Csv(&'a Path),
}

/// What `timesheet` is asked for.
#[derive(Debug)]
pub struct Ask<'a> {
    /// The project (`proj_id`), needed when the file has more than one.
    pub project: Option<u64>,
    /// The period's first day, as `YYYY-MM-DD`.
    pub from: &'a str,
    /// The period's last day.
    pub to: &'a str,
    /// Only these resources, by short name or name. Default is every labor
    /// and nonlabor resource with work in the period.
    pub resources: &'a [String],
}

/// The dates an activity's work runs over as scheduled: its actual start
/// once started, otherwise its early start, to its early finish.
fn dates_of(activity: &Activity) -> (Option<DateTime>, Option<DateTime>) {
    let stored = &activity.stored;
    (
        activity.actual_start.or(stored.early_start),
        stored.early_finish,
    )
}

fn overlaps(activity: &Activity, (from, to): (Date, Date)) -> bool {
    match dates_of(activity) {
        (Some(start), Some(finish)) => start.date() <= to && finish.date() >= from,
        _ => false,
    }
}

fn hours(units: Option<f64>) -> Value {
    json!(units.unwrap_or(0.0))
}

/// The first letters of `text` that fit in `room` UTF-16 units, the way
/// Excel counts a sheet name's length.
fn fitting(text: &str, room: usize) -> String {
    let mut used = 0;
    text.chars()
        .take_while(|c| {
            used += c.len_utf16();
            used <= room
        })
        .collect()
}

/// A name not yet in `taken`, ignoring case, by adding " (2)", " (3)" and so
/// on, kept to `room` letters as `fit` counts them.
fn different(base: &str, taken: &mut Vec<String>, fit: impl Fn(&str, usize) -> String) -> String {
    let room = 31;
    let mut name = base.to_owned();
    let mut k = 2;
    while taken
        .iter()
        .any(|t| t.to_lowercase() == name.to_lowercase())
    {
        let suffix = format!(" ({k})");
        name = format!("{}{suffix}", fit(base, room - suffix.len()));
        k += 1;
    }
    taken.push(name.clone());
    name
}

/// The sheet name for a resource: Excel names hold at most 31 letters,
/// none of `[]:*?/\`, don't start or end with an apostrophe, aren't
/// `History`, and each must be different.
fn sheet_name(short_name: &str, taken: &mut Vec<String>) -> String {
    let clean: String = short_name
        .chars()
        .map(|c| {
            if "[]:*?/\\".contains(c) || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();
    let fit = |text: &str, room: usize| fitting(text, room).trim_matches('\'').to_owned();
    let clean = fit(clean.trim_matches('\''), 31);
    let base = if clean.trim().is_empty() {
        "Resource".to_owned()
    } else if clean.eq_ignore_ascii_case("history") {
        format!("{clean}_")
    } else {
        clean
    };
    different(&base, taken, fit)
}

/// The CSV file name for a resource, without `.csv`: none of the letters
/// Windows refuses, no trailing dot or space, none of its reserved names,
/// and each different.
fn file_name(short_name: &str, taken: &mut Vec<String>) -> String {
    let clean: String = short_name
        .chars()
        .map(|c| {
            if "<>:\"/\\|?*".contains(c) || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();
    let fit = |text: &str, room: usize| {
        let mut name: String = text.chars().take(room).collect();
        name.truncate(name.trim_end_matches(['.', ' ']).len());
        name
    };
    let clean = fit(clean.trim(), 31);
    let stem = clean.split('.').next().unwrap_or("").to_uppercase();
    let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ((stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.len() == 4
            && stem.as_bytes()[3].is_ascii_digit());
    let base = if clean.is_empty() {
        "Resource".to_owned()
    } else if reserved {
        format!("{clean}_")
    } else {
        clean
    };
    different(&base, taken, fit)
}

/// Each resource's rows, by short name.
type Sheets = Vec<(String, Vec<Map<String, Value>>)>;

fn rows_for(schedule: &Schedule, ask: &Ask<'_>) -> Result<Sheets, CliError> {
    let from = parse_day(ask.from)?;
    let to = parse_day(ask.to)?;
    if to < from {
        return Err(CliError::new(
            "bad-period",
            format!(
                "the period ends ({}) before it starts ({})",
                ask.to, ask.from
            ),
            "give --from, then a --to on or after it",
        ));
    }
    let wanted: Vec<ResourceId> = ask
        .resources
        .iter()
        .map(|name| {
            totalfloat_edit::resource_named(schedule, name).ok_or_else(|| {
                CliError::new(
                    "no-such-resource",
                    format!("the file has no resource {name}, or more than one"),
                    "`query --columns code,resources` lists the resources assigned",
                )
            })
        })
        .collect::<Result<_, _>>()?;
    let mut by_resource: BTreeMap<String, Vec<(&Activity, &Assignment, &Resource)>> =
        BTreeMap::new();
    for assignment in schedule.assignments.values() {
        let Some(resource) = assignment.resource.and_then(|r| schedule.resources.get(&r)) else {
            continue;
        };
        let Some(activity) = schedule.activities.get(&assignment.activity) else {
            continue;
        };
        let hourly = matches!(resource.kind, ResourceType::Labor | ResourceType::Nonlabor);
        let asked = wanted.is_empty() || wanted.contains(&resource.id);
        if !hourly
            || !asked
            || activity.status == ActivityStatus::Completed
            || !overlaps(activity, (from, to))
        {
            continue;
        }
        by_resource
            .entry(resource.short_name.clone())
            .or_default()
            .push((activity, assignment, resource));
    }
    Ok(by_resource
        .into_iter()
        .map(|(short_name, mut work)| {
            work.sort_by(|a, b| {
                (dates_of(a.0).0, &a.0.code, a.1.id).cmp(&(dates_of(b.0).0, &b.0.code, b.1.id))
            });
            let rows = work
                .into_iter()
                .map(|(activity, assignment, resource)| {
                    let (start, finish) = dates_of(activity);
                    let mut row = Map::new();
                    row.insert("activity".into(), json!(activity.code));
                    row.insert("name".into(), json!(activity.name));
                    row.insert("resource".into(), json!(resource.short_name));
                    row.insert("planned_start".into(), json!(start.map(|d| d.to_string())));
                    row.insert(
                        "planned_finish".into(),
                        json!(finish.map(|d| d.to_string())),
                    );
                    row.insert(
                        "budget_hours".into(),
                        hours(assignment.amounts.budgeted_units),
                    );
                    row.insert(
                        "actual_hours".into(),
                        hours(assignment.amounts.actual_units),
                    );
                    row
                })
                .collect();
            (short_name, rows)
        })
        .collect())
}

/// Runs `timesheet`: writes the sheets and says how many.
///
/// # Errors
///
/// `needs-project`, `invalid-date`, `bad-period`, `no-such-resource`,
/// `nothing-to-report` when no one has work in the period, or an output that
/// exists or can't be written.
pub fn run(file: &Path, ask: &Ask<'_>, out: Option<Out<'_>>) -> Result<u8, CliError> {
    let Some(out) = out else {
        return Err(CliError::new(
            "needs-output",
            "say where the timesheets go",
            "give --xlsx FILE or --csv DIR",
        ));
    };
    let loaded = Loaded::new(crate::location::read(file)?);
    let project = crate::resheet::one_project(&loaded, ask.project)?;
    let Some(schedule) = loaded.schedules.iter().find(|s| s.project.id.0 == project) else {
        return Err(CliError::new(
            "no-such-project",
            format!("the file has no project {project}"),
            "`inspect` lists the projects in the file",
        ));
    };
    let sheets = rows_for(schedule, ask)?;
    if sheets.is_empty() {
        return Err(CliError::new(
            "nothing-to-report",
            format!(
                "no one has work on unfinished activities from {} to {}",
                ask.from, ask.to
            ),
            "check the period, or the resources asked for",
        ));
    }
    let columns: Vec<String> = COLUMNS.iter().map(|c| (*c).to_owned()).collect();
    let (written, path) = match out {
        Out::Xlsx(path) => {
            let mut taken = Vec::new();
            let named: Vec<(String, &[Map<String, Value>])> = sheets
                .iter()
                .map(|(short_name, rows)| (sheet_name(short_name, &mut taken), rows.as_slice()))
                .collect();
            // Typed as text, a percent keeps what was typed: Excel stores
            // 50% in a number cell as 0.5.
            crate::export::xlsx_sheets(&columns, &named, &["percent"], path)?;
            ("a workbook", path)
        }
        Out::Csv(dir) => {
            write_folder(dir, &columns, &sheets)?;
            ("a folder", dir)
        }
    };
    let rows: usize = sheets.iter().map(|(_, rows)| rows.len()).sum();
    print_line(&format!(
        "Wrote {} timesheet{} ({rows} row{}) for {} to {} to {written}, {}.",
        sheets.len(),
        if sheets.len() == 1 { "" } else { "s" },
        if rows == 1 { "" } else { "s" },
        ask.from,
        ask.to,
        path.display()
    ));
    Ok(crate::error::exit::OK)
}

fn write_folder(dir: &Path, columns: &[String], sheets: &Sheets) -> Result<(), CliError> {
    crate::output::refuse_existing(dir)?;
    std::fs::create_dir_all(dir).map_err(|e| {
        CliError::new(
            "output-not-writable",
            format!("{} couldn't be made: {e}", dir.display()),
            "check the folder's parent exists and can be written",
        )
    })?;
    let mut taken = Vec::new();
    for (short_name, rows) in sheets {
        let path = dir.join(format!("{}.csv", file_name(short_name, &mut taken)));
        if let Err(e) = crate::output::create_new_text(&crate::export::csv(columns, rows), &path) {
            // The folder is new, so a half-written one goes rather than
            // blocking the next run.
            let _ = std::fs::remove_dir_all(dir);
            return Err(e);
        }
    }
    Ok(())
}

/// A sheet's rows with its name and row numbers, counting the header as 1.
struct Read {
    file: String,
    sheet: String,
    csv: bool,
    /// The sheet's first column, counting from 0: a workbook's used range
    /// may start after column A.
    first_column: usize,
    rows: Vec<(usize, Vec<String>)>,
}

fn unreadable(input: &Path, why: &str) -> CliError {
    CliError::new(
        "file-not-readable",
        format!("{} can't be read as a timesheet: {why}", input.display()),
        "give the .xlsx, the .csv, or the folder of CSV files that `timesheet` wrote",
    )
}

/// Every sheet in a workbook, a CSV file, or a folder of CSV files.
fn read_sheets(input: &Path, problems: &mut Vec<Problem>) -> Result<Vec<Read>, CliError> {
    let file = input.display().to_string();
    if input.is_dir() {
        let mut paths: Vec<PathBuf> = std::fs::read_dir(input)
            .map_err(|e| unreadable(input, &e.to_string()))?
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_file() && p.extension().is_some_and(|e| e.eq_ignore_ascii_case("csv")))
            .collect();
        paths.sort();
        if paths.is_empty() {
            return Err(unreadable(input, "the folder has no CSV files"));
        }
        return paths.iter().map(|p| read_csv(p)).collect();
    }
    if input
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("csv"))
    {
        return Ok(vec![read_csv(input)?]);
    }
    let mut workbook = calamine::open_workbook::<calamine::Xlsx<_>, _>(input)
        .map_err(|e| unreadable(input, &e.to_string()))?;
    let mut out = Vec::new();
    for name in workbook.sheet_names() {
        let range = workbook
            .worksheet_range(&name)
            .map_err(|e| unreadable(input, &e.to_string()))?;
        let (first, first_column) = range
            .start()
            .map_or((0, 0), |(row, column)| (row as usize, column as usize));
        let mut rows: Vec<(usize, Vec<String>)> = Vec::new();
        // The header's percent column, once the header is read.
        let mut percent_at = None;
        for (k, row) in range.rows().enumerate() {
            let number = first + k + 1;
            let mut cells = Vec::with_capacity(row.len());
            for (column, cell) in row.iter().enumerate() {
                let at = (file.as_str(), name.as_str(), number);
                match crate::sheets::cell_text(cell) {
                    // Excel stores 50% typed in a number cell as 0.5.
                    Ok(_)
                        if percent_at == Some(column)
                            && matches!(cell, calamine::Data::Float(f) if *f > 0.0 && *f < 1.0) =>
                    {
                        let mut p = problem(
                            at,
                            Some(first_column + column + 1),
                            (
                                "bad-percent",
                                "the percent is a number below 1, as Excel stores a percent typed with %".to_owned(),
                            ),
                            "type the percent as a number from 0 to 100, such as 50",
                        );
                        p.source = format!("{} column percent", p.source);
                        problems.push(p);
                        cells.push(String::new());
                    }
                    Ok(text) => cells.push(text),
                    Err(why) => {
                        problems.push(problem(
                            at,
                            Some(first_column + column + 1),
                            ("bad-cell", why),
                            "fix or clear the cell",
                        ));
                        cells.push(String::new());
                    }
                }
            }
            if cells.iter().any(|c| !c.trim().is_empty()) {
                if rows.is_empty() {
                    percent_at = cells
                        .iter()
                        .position(|c| c.trim().eq_ignore_ascii_case("percent"));
                }
                rows.push((number, cells));
            }
        }
        out.push(Read {
            file: file.clone(),
            sheet: name,
            csv: false,
            first_column,
            rows,
        });
    }
    Ok(out)
}

fn read_csv(path: &Path) -> Result<Read, CliError> {
    let text = std::fs::read_to_string(path).map_err(|e| unreadable(path, &e.to_string()))?;
    let rows = crate::table::records(&text).map_err(|why| unreadable(path, &why))?;
    Ok(Read {
        file: path.display().to_string(),
        sheet: path
            .file_stem()
            .map_or_else(String::new, |s| s.to_string_lossy().into_owned()),
        csv: true,
        first_column: 0,
        rows,
    })
}

/// A problem at a sheet's row and, if it's about one cell, its column.
fn problem(
    (file, sheet, row): (&str, &str, usize),
    column: Option<usize>,
    (code, message): (&'static str, String),
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
        column,
    }
}

/// The progress an activity's rows report, and the row each part came from.
#[derive(Default)]
struct Progress {
    actual_start: Option<(String, String)>,
    actual_finish: Option<(String, String)>,
    percent: Option<(f64, String)>,
}

/// One row's cells, by column name.
struct Row<'a> {
    file: &'a str,
    sheet: &'a str,
    number: usize,
    first_column: usize,
    cells: BTreeMap<&'a str, (usize, String)>,
}

impl Row<'_> {
    fn at(&self) -> String {
        format!("{} {} row {}", self.file, self.sheet, self.number)
    }

    fn cell(&self, column: &str) -> Option<&(usize, String)> {
        self.cells
            .get(column)
            .filter(|(_, text)| !text.trim().is_empty())
    }

    fn problem(
        &self,
        column: &str,
        (code, message): (&'static str, String),
        hint: &str,
    ) -> Problem {
        let mut p = problem(
            (self.file, self.sheet, self.number),
            self.cells
                .get(column)
                .map(|(k, _)| self.first_column + k + 1),
            (code, message),
            hint,
        );
        p.source = format!("{} column {column}", p.source);
        p
    }

    /// A number of hours, zero or more.
    fn hours(&self, column: &str, problems: &mut Vec<Problem>) -> Option<f64> {
        let (_, text) = self.cell(column)?;
        match text.trim().parse::<f64>() {
            Ok(h) if h.is_finite() && h >= 0.0 => Some(h),
            _ => {
                problems.push(self.problem(
                    column,
                    (
                        "bad-hours",
                        format!("{} isn't a number of hours", text.trim()),
                    ),
                    "give zero or more hours, such as 7.5",
                ));
                None
            }
        }
    }

    fn date(&self, column: &str, problems: &mut Vec<Problem>) -> Option<String> {
        let (_, text) = self.cell(column)?;
        let text = text.trim();
        if text.parse::<DateTime>().is_ok() {
            Some(text.to_owned())
        } else {
            problems.push(self.problem(
                column,
                ("bad-date", format!("{text} isn't a date and time")),
                "use the form YYYY-MM-DD HH:MM",
            ));
            None
        }
    }

    fn percent(&self, problems: &mut Vec<Problem>) -> Option<f64> {
        let (_, text) = self.cell("percent")?;
        let text = text.trim().trim_end_matches('%');
        match text.trim().parse::<f64>() {
            Ok(p) if (0.0..=100.0).contains(&p) => Some(p),
            _ => {
                problems.push(self.problem(
                    "percent",
                    (
                        "bad-percent",
                        format!("{text} isn't a percent from 0 to 100"),
                    ),
                    "give a number from 0 to 100",
                ));
                None
            }
        }
    }
}

/// What the timesheets come to: their edits, where each came from, and the
/// project.
pub struct Expanded {
    /// One `set_progress` per activity, then the assignments' units.
    pub edits: Vec<Edit>,
    /// Each edit's file, sheet, row and column.
    pub sources: Vec<String>,
    /// The project the activities are in (`proj_id`).
    pub project: u64,
}

/// Reads the returned timesheets and works out their edits.
///
/// # Errors
///
/// `file-not-readable`, `needs-project`, `timesheet-refused` with every
/// problem at its file, sheet, row and column, or `nothing-to-change`.
pub fn expand(
    loaded: &Loaded,
    inputs: &[PathBuf],
    project: Option<u64>,
) -> Result<Expanded, CliError> {
    let project = crate::resheet::one_project(loaded, project)?;
    let Some(schedule) = loaded.schedules.iter().find(|s| s.project.id.0 == project) else {
        return Err(CliError::new(
            "no-such-project",
            format!("the file has no project {project}"),
            "`inspect` lists the projects in the file",
        ));
    };
    let mut problems = Vec::new();
    let mut sheets = Vec::new();
    for input in inputs {
        sheets.extend(read_sheets(input, &mut problems)?);
    }
    let mut planned = Planned {
        schedule,
        problems,
        units: Vec::new(),
        progress: BTreeMap::new(),
        seen: BTreeMap::new(),
    };
    for sheet in &sheets {
        planned.sheet(sheet);
    }
    let Planned {
        problems,
        units,
        progress,
        ..
    } = planned;
    if !problems.is_empty() {
        let count = problems.len();
        let mut error = CliError::new(
            "timesheet-refused",
            format!(
                "the timesheets have {count} problem{}, so nothing changed",
                if count == 1 { "" } else { "s" }
            ),
            "fix each cell named, then run again",
        );
        error.problems = problems;
        return Err(error);
    }
    let mut edits = Vec::new();
    let mut sources = Vec::new();
    // Progress first: actual units need a started activity.
    for (activity, p) in progress {
        let mut from: Vec<String> = Vec::new();
        for at in [
            p.actual_start.as_ref().map(|(_, at)| at),
            p.actual_finish.as_ref().map(|(_, at)| at),
            p.percent.as_ref().map(|(_, at)| at),
        ]
        .into_iter()
        .flatten()
        {
            if !from.contains(at) {
                from.push(at.clone());
            }
        }
        edits.push(Edit::SetProgress {
            activity,
            actual_start: p.actual_start.map(|(d, _)| d),
            actual_finish: p.actual_finish.map(|(d, _)| d),
            remaining: None,
            percent: p.percent.map(|(v, _)| v),
        });
        sources.push(from.join(", "));
    }
    for (edit, source) in units {
        edits.push(edit);
        sources.push(source);
    }
    if edits.is_empty() {
        return Err(CliError::new(
            "nothing-to-change",
            "the timesheets have no hours or progress filled in",
            "fill in hours, remaining_hours, actual_start, actual_finish or percent",
        ));
    }
    Ok(Expanded {
        edits,
        sources,
        project,
    })
}

/// The edits and problems found so far.
struct Planned<'s> {
    schedule: &'s Schedule,
    problems: Vec<Problem>,
    /// Each assignment's units edit and where it came from.
    units: Vec<(Edit, String)>,
    /// Each activity's progress, by Activity ID.
    progress: BTreeMap<String, Progress>,
    /// The row each assignment was given on, so a second is refused.
    seen: BTreeMap<totalfloat_model::AssignmentId, String>,
}

impl<'s> Planned<'s> {
    fn sheet(&mut self, sheet: &Read) {
        let Some(((header_row, header), rows)) = sheet.rows.split_first() else {
            return;
        };
        let names: Vec<String> = header.iter().map(|h| h.trim().to_lowercase()).collect();
        // A sheet with none of the columns, such as notes, isn't a timesheet.
        if !names.iter().any(|n| COLUMNS.contains(&n.as_str())) {
            return;
        }
        let missing: Vec<&str> = ["activity", "resource", "actual_hours"]
            .into_iter()
            .filter(|c| !names.iter().any(|n| n == c))
            .collect();
        if !missing.is_empty() {
            self.problems.push(problem(
                (&sheet.file, &sheet.sheet, *header_row),
                None,
                (
                    "missing-column",
                    format!("the sheet has no {} column", missing.join(" or ")),
                ),
                "use the sheet `timesheet` wrote, with its header row",
            ));
            return;
        }
        for (number, cells) in rows {
            let mut by_name = BTreeMap::new();
            for (k, name) in names.iter().enumerate() {
                let text = cells.get(k).map_or("", String::as_str);
                let text = if sheet.csv {
                    crate::resheet::unguarded(text)
                } else {
                    text
                };
                by_name.entry(name.as_str()).or_insert((k, text.to_owned()));
            }
            let row = Row {
                file: &sheet.file,
                sheet: &sheet.sheet,
                number: *number,
                first_column: sheet.first_column,
                cells: by_name,
            };
            self.row(&row);
        }
    }

    fn row(&mut self, row: &Row<'_>) {
        let filled = [
            "hours",
            "remaining_hours",
            "actual_start",
            "actual_finish",
            "percent",
        ]
        .iter()
        .any(|c| row.cell(c).is_some());
        if !filled {
            return;
        }
        let problems_before = self.problems.len();
        let hours = row.hours("hours", &mut self.problems);
        let remaining = row.hours("remaining_hours", &mut self.problems);
        let actual_start = row.date("actual_start", &mut self.problems);
        let actual_finish = row.date("actual_finish", &mut self.problems);
        let percent = row.percent(&mut self.problems);
        let Some(assignment) = self.assignment(row) else {
            return;
        };
        if self.problems.len() > problems_before {
            return;
        }
        let activity = &self.schedule.activities[&assignment.activity];
        let at = row.at();
        if let Some(earlier) = self.seen.insert(assignment.id, at.clone()) {
            self.problems.push(row.problem(
                "resource",
                (
                    "repeated-assignment",
                    format!(
                        "{} on {} is also on {earlier}",
                        row.cell("resource").map_or("", |(_, t)| t.trim()),
                        activity.code
                    ),
                ),
                "report each assignment's hours on one row",
            ));
            return;
        }
        if percent.is_some() && activity.percent_complete_type == PercentCompleteType::Units {
            self.problems.push(row.problem(
                "percent",
                (
                    "percent-by-units",
                    format!(
                        "{}'s percent complete is by units, so its hours set it",
                        activity.code
                    ),
                ),
                "leave percent blank: hours and remaining_hours set this activity's percent",
            ));
            return;
        }
        if hours.is_some() || remaining.is_some() {
            let resource = assignment
                .resource
                .and_then(|r| self.schedule.resources.get(&r))
                .map_or_else(String::new, |r| r.short_name.clone());
            let actual = assignment.amounts.actual_units.unwrap_or(0.0);
            self.units.push((
                Edit::SetAssignmentUnits {
                    activity: activity.code.clone(),
                    resource,
                    actual_units: hours.map(|h| actual + h),
                    remaining_units: remaining,
                },
                match (hours, remaining) {
                    (Some(_), Some(_)) => format!("{at} columns hours and remaining_hours"),
                    (Some(_), None) => format!("{at} column hours"),
                    _ => format!("{at} column remaining_hours"),
                },
            ));
        }
        self.progress_of(row, &activity.code, (actual_start, actual_finish, percent));
    }

    /// The row's assignment, checked against what the file records now.
    fn assignment(&mut self, row: &Row<'_>) -> Option<&'s Assignment> {
        let schedule = self.schedule;
        let Some((_, code)) = row.cell("activity") else {
            self.problems.push(row.problem(
                "activity",
                ("missing-activity", "the row names no activity".to_owned()),
                "give the Activity ID the sheet was made with",
            ));
            return None;
        };
        let code = code.trim();
        let Some(activity) = schedule.activities.values().find(|a| a.code == code) else {
            self.problems.push(row.problem(
                "activity",
                (
                    "no-such-activity",
                    format!("the schedule has no activity {code}"),
                ),
                "give the Activity ID the sheet was made with",
            ));
            return None;
        };
        let name = row.cell("resource").map_or("", |(_, t)| t.trim());
        let Some(resource) = totalfloat_edit::resource_named(schedule, name)
            .and_then(|id| schedule.resources.get(&id))
        else {
            self.problems.push(row.problem(
                "resource",
                (
                    "no-such-resource",
                    format!("the schedule has no resource {name}, or more than one"),
                ),
                "give the resource's short name, as the sheet was made with",
            ));
            return None;
        };
        let mut held = schedule
            .assignments
            .values()
            .filter(|a| a.activity == activity.id && a.resource == Some(resource.id));
        let assignment = match (held.next(), held.next()) {
            (Some(one), None) => one,
            (Some(_), Some(_)) => {
                self.problems.push(row.problem(
                    "resource",
                    (
                        "ambiguous-assignment",
                        format!("{code} has {name} on it more than once"),
                    ),
                    "report these hours in the source schedule, where each assignment has its own row",
                ));
                return None;
            }
            (None, _) => {
                self.problems.push(row.problem(
                    "resource",
                    ("not-assigned", format!("{code} doesn't have {name}")),
                    "`timesheet` lists only the assignments the schedule has",
                ));
                return None;
            }
        };
        let recorded = assignment.amounts.actual_units.unwrap_or(0.0);
        let given = row
            .cell("actual_hours")
            .map(|(_, t)| t.trim().parse::<f64>());
        match given {
            Some(Ok(given)) if (given - recorded).abs() <= SAME_HOURS => Some(assignment),
            Some(Ok(given)) => {
                self.problems.push(row.problem(
                    "actual_hours",
                    (
                        "timesheet-out-of-date",
                        format!(
                            "the sheet says {name} had {given} actual hours on {code}, and the schedule says {recorded}"
                        ),
                    ),
                    "it was made from another version of the schedule, or it's been applied already; make a new timesheet",
                ));
                None
            }
            _ => {
                self.problems.push(row.problem(
                    "actual_hours",
                    (
                        "bad-hours",
                        "actual_hours is the hours recorded when the sheet was made, and it's blank or not a number".to_owned(),
                    ),
                    "leave actual_hours as `timesheet` wrote it",
                ));
                None
            }
        }
    }

    /// Adds the row's progress to its activity's, refusing what disagrees
    /// with another row.
    fn progress_of(
        &mut self,
        row: &Row<'_>,
        code: &str,
        (start, finish, percent): (Option<String>, Option<String>, Option<f64>),
    ) {
        if start.is_none() && finish.is_none() && percent.is_none() {
            return;
        }
        let at = row.at();
        let cell = |column: &str| format!("{at} column {column}");
        let entry = self.progress.entry(code.to_owned()).or_default();
        let mut conflicts = Vec::new();
        for (column, given, held) in [
            ("actual_start", start, &mut entry.actual_start),
            ("actual_finish", finish, &mut entry.actual_finish),
        ] {
            let Some(given) = given else { continue };
            match held {
                Some((value, earlier)) if *value != given => {
                    conflicts.push((column, format!("{given} here and {value} on {earlier}")));
                }
                Some(_) => {}
                None => *held = Some((given, cell(column))),
            }
        }
        if let Some(given) = percent {
            match &entry.percent {
                Some((value, earlier)) if (*value - given).abs() > f64::EPSILON => {
                    conflicts.push(("percent", format!("{given} here and {value} on {earlier}")));
                }
                Some(_) => {}
                None => entry.percent = Some((given, cell("percent"))),
            }
        }
        for (column, both) in conflicts {
            self.problems.push(row.problem(
                column,
                (
                    "conflicting-progress",
                    format!("{code}'s {column} is {both}"),
                ),
                "agree the activity's progress, then give it the same on every row",
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{file_name, sheet_name};

    #[test]
    fn sheet_names_are_short_clean_and_different() {
        let mut taken = Vec::new();
        assert_eq!(sheet_name("CREW/A", &mut taken), "CREW_A");
        assert_eq!(sheet_name("crew_a", &mut taken), "crew_a (2)");
        let long = "X".repeat(40);
        assert_eq!(sheet_name(&long, &mut taken).len(), 31);
        assert_eq!(
            sheet_name(&long, &mut taken),
            format!("{} (2)", "X".repeat(27))
        );
        assert_eq!(sheet_name("'TEMP'", &mut taken), "TEMP");
        assert_eq!(sheet_name("History", &mut taken), "History_");
        assert_eq!(sheet_name("ÉLEC", &mut taken), "ÉLEC");
        assert_eq!(sheet_name("élec", &mut taken), "élec (2)");
        // Excel counts a sheet name's length in UTF-16 units.
        let wide = "😀".repeat(20);
        let name = sheet_name(&wide, &mut taken);
        assert_eq!(name.encode_utf16().count(), 30);
        let mut taken = Vec::new();
        assert_eq!(
            sheet_name(&format!("{}'X", "A".repeat(30)), &mut taken),
            "A".repeat(30)
        );
    }

    #[test]
    fn file_names_are_ones_every_system_takes() {
        let mut taken = Vec::new();
        assert_eq!(file_name("CON", &mut taken), "CON_");
        assert_eq!(file_name("com1", &mut taken), "com1_");
        assert_eq!(file_name("A|B", &mut taken), "A_B");
        assert_eq!(file_name("a|b", &mut taken), "a_b (2)");
        assert_eq!(file_name("CREW. ", &mut taken), "CREW");
        assert_eq!(file_name("...", &mut taken), "Resource");
    }
}
