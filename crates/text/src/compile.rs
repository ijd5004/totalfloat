//! Turning a [`Document`] into a new project and the edit commands that build
//! it (ARCHITECTURE.md, Schedule as text).

use std::collections::{BTreeMap, BTreeSet};

use totalfloat_edit::{DEFAULT_CODE_LENGTH, Edit, Kind};

use crate::diagnostic::closest;
use crate::parse::DAYS;
use crate::{
    Activity, ActivityDetail, ActivityKind, At, Calendar, CalendarDetail, CodeType, CodeValue,
    Date, DateTime, Day, Detail, Diagnostic, Document, Item, ProjectDetail, Resource,
    ResourceDetail, Role, RoleDetail, Statement, TypeKind, Value, Word,
};

/// Dates a calendar's lines give an exception on.
fn exception_dates(calendar: &Statement<Calendar, CalendarDetail>) -> BTreeSet<Date> {
    details(&calendar.details)
        .filter_map(|(d, _)| match d {
            CalendarDetail::Holiday(date) | CalendarDetail::Exception { date, .. } => {
                Some(date.value)
            }
            CalendarDetail::Week { .. } | CalendarDetail::HoursPerDay(_) => None,
        })
        .collect()
}

/// The hours per day a calendar's lines give, and the line that gives them:
/// its `hours-per-day` line, or with `whole_week` (days its week lines don't
/// mention are off), the hours of its working days when they're all the
/// same. `None` leaves the calendar's hours per day as they are.
pub(crate) fn hours_per_day(
    calendar: &Statement<Calendar, CalendarDetail>,
    whole_week: bool,
) -> Option<(f64, At)> {
    let mut week: BTreeMap<usize, (u32, At)> = BTreeMap::new();
    for (detail, line) in details(&calendar.details) {
        match detail {
            CalendarDetail::HoursPerDay(hours) => return Some((hours.value, line)),
            CalendarDetail::Week { from, to, shifts } => {
                let length = shifts
                    .iter()
                    .filter_map(|s| crate::parse::shift_minutes(&s.value))
                    .sum();
                for day in day_index(*from)..=day_index(*to) {
                    week.insert(day, (length, line));
                }
            }
            CalendarDetail::Holiday(_) | CalendarDetail::Exception { .. } => {}
        }
    }
    if !whole_week {
        return None;
    }
    let mut working = week.values().filter(|(length, _)| *length > 0);
    let (first, line) = *working.next()?;
    working
        .all(|(length, _)| *length == first)
        .then(|| (f64::from(first) / 60.0, line))
}

/// The calendar a new schedule starts with when the project names none.
pub const STARTER_CALENDAR: &str = "Standard 5-Day";

/// The project a text file describes.
#[derive(Debug, Clone, PartialEq)]
pub struct NewProject {
    /// The project code (P6 `proj_short_name`).
    pub code: String,
    /// The project name.
    pub name: String,
    /// The data date.
    pub data_date: DateTime,
    /// The must-finish-by date.
    pub must_finish: Option<DateTime>,
    /// The name of the project's calendar, which the new schedule starts
    /// with; [`STARTER_CALENDAR`] when the file names none.
    pub calendar: String,
}

/// An edit command and the place in the text it came from.
#[derive(Debug, Clone, PartialEq)]
pub struct Placed {
    /// The command.
    pub edit: Edit,
    /// Where it came from.
    pub at: At,
}

/// A text file ready to build: the project to start, and the commands that
/// build on it, in an order where no command needs something a later one
/// makes.
#[derive(Debug, Clone, PartialEq)]
pub struct Compiled {
    /// The project.
    pub project: NewProject,
    /// The commands. A line with a problem gives none.
    pub edits: Vec<Placed>,
    /// Problems with names and with what a new schedule can hold. The file
    /// can't be built while there are any, but the commands still let the
    /// edits be checked, so every problem can be reported in one run.
    pub problems: Vec<Diagnostic>,
}

/// Turns a document into a new project and its commands. Names are checked
/// first, against the document itself, so a mistyped predecessor or calendar
/// is reported at its line with a suggestion, before anything is built.
///
/// Problems that leave the rest buildable go in [`Compiled::problems`]:
/// unknown or repeated names, and WBS paths that don't start at the project.
///
/// # Errors
///
/// Every problem found, when there's no project to build: none in the file,
/// or one without a data date.
pub fn compile(doc: &Document, file: &str) -> Result<Compiled, Vec<Diagnostic>> {
    let mut c = Compiler::new(file);
    let project = c.project(doc);
    let calendars = c.calendars(doc, project.as_ref());
    let known = Known {
        codes: c.code_types(doc),
        fields: c.fields(doc),
        resources: c.resources(doc, &calendars),
        roles: c.roles(doc),
        calendars,
    };
    let wbs = c.wbs(doc, project.as_ref());
    c.activities(doc, project.as_ref(), &known, &wbs);
    match project {
        Some(project) => Ok(Compiled {
            project,
            edits: c.edits,
            problems: c.problems,
        }),
        None => Err(c.problems),
    }
}

/// The names a file defines, which its activities can use.
struct Known {
    calendars: Vec<String>,
    /// Code types by lowered name: the name as written, and its values.
    codes: BTreeMap<String, (String, Vec<String>)>,
    /// Resources' short names.
    resources: Vec<String>,
    /// Roles' short names.
    roles: Vec<String>,
    /// Activity fields by lowered name: the name as written.
    fields: BTreeMap<String, String>,
}

/// The command that adds an activity field.
pub(crate) fn new_field(statement: &Statement<crate::Field, crate::Never>) -> Edit {
    Edit::AddUdfType {
        field: statement.head.name.value.clone(),
        applies_to: totalfloat_edit::UdfFor::Activity,
        data_type: statement.head.kind.value,
    }
}

/// The command that adds a code type: its values' length, when one is longer
/// than a new code type takes.
pub(crate) fn new_code_type(statement: &Statement<CodeType, CodeValue>) -> Edit {
    let longest = details(&statement.details)
        .map(|(v, _)| v.value.value.chars().count())
        .max()
        .and_then(|n| u32::try_from(n).ok())
        .filter(|n| *n > DEFAULT_CODE_LENGTH);
    Edit::AddCodeType {
        name: statement.head.name.value.clone(),
        scope: None,
        max_length: longest,
    }
}

/// The command that adds a resource.
pub(crate) fn new_resource(statement: &Statement<Resource, ResourceDetail>) -> Edit {
    let mut calendar = None;
    let mut units = None;
    let mut limit = None;
    for (detail, _) in details(&statement.details) {
        match detail {
            ResourceDetail::Calendar(word) => calendar = Some(word.value.clone()),
            ResourceDetail::UnitsPerHour(value) => units = Some(value.value),
            ResourceDetail::MaxUnitsPerHour(value) => limit = Some(value.value),
        }
    }
    Edit::AddResource {
        resource: statement.head.code.value.clone(),
        name: statement.head.name.value.clone(),
        resource_type: statement.head.kind.as_ref().map(|k| k.value),
        units_per_hour: units,
        calendar,
        max_units_per_hour: limit,
    }
}

/// The command that adds a role.
pub(crate) fn new_role(statement: &Statement<Role, RoleDetail>) -> Edit {
    let (mut parent, mut limit) = (None, None);
    for (detail, _) in details(&statement.details) {
        match detail {
            RoleDetail::Parent(word) => parent = Some(word.value.clone()),
            RoleDetail::MaxUnitsPerHour(value) => limit = Some(value.value),
        }
    }
    Edit::AddRole {
        role: statement.head.code.value.clone(),
        name: statement.head.name.value.clone(),
        parent,
        max_units_per_hour: limit,
    }
}

pub(crate) struct Compiler<'f> {
    file: &'f str,
    pub(crate) problems: Vec<Diagnostic>,
    pub(crate) edits: Vec<Placed>,
}

pub(crate) fn details<D>(details: &[Detail<D>]) -> impl Iterator<Item = (&D, At)> {
    details.iter().filter_map(|d| match d {
        Detail::Line { value, at, .. } => Some((value, *at)),
        Detail::Comment(_) => None,
    })
}

impl<'f> Compiler<'f> {
    pub(crate) fn new(file: &'f str) -> Self {
        Self {
            file,
            problems: Vec::new(),
            edits: Vec::new(),
        }
    }

    pub(crate) fn problem(&mut self, code: &'static str, at: At, message: String, hint: String) {
        self.problems.push(Diagnostic {
            code,
            message,
            hint,
            file: self.file.to_owned(),
            at,
        });
    }

    pub(crate) fn push(&mut self, edit: Edit, at: At) {
        self.edits.push(Placed { edit, at });
    }

    pub(crate) fn unknown(&mut self, code: &'static str, what: &str, word: &Word, known: &[&str]) {
        let hint = closest(&word.value, known).map_or_else(
            || format!("define the {what} in this file first"),
            |near| format!("did you mean {near}?"),
        );
        self.problem(
            code,
            word.at,
            format!(
                "{} isn't {} {what} in this file",
                word.value,
                if what.starts_with(['a', 'e', 'i', 'o', 'u']) {
                    "an"
                } else {
                    "a"
                }
            ),
            hint,
        );
    }

    fn project(&mut self, doc: &Document) -> Option<NewProject> {
        let mut found = doc.items.iter().filter_map(|i| match i {
            Item::Project(p) => Some(p),
            _ => None,
        });
        let Some(project) = found.next() else {
            self.problem(
                "missing-project",
                At {
                    line: 1,
                    column: 1,
                    length: 1,
                },
                "the file has no project".to_owned(),
                "add a line such as project PLANT \"New Plant\", with a data-date under it"
                    .to_owned(),
            );
            return None;
        };
        for other in found {
            self.problem(
                "second-project",
                other.at,
                "a schedule text file holds one project".to_owned(),
                "put each project in a file of its own".to_owned(),
            );
        }
        let (mut data_date, mut must_finish, mut calendar) = (None, None, None);
        for (detail, _) in details(&project.details) {
            match detail {
                ProjectDetail::DataDate(d) => data_date = Some(d.value),
                ProjectDetail::MustFinish(d) => must_finish = Some(d.value),
                ProjectDetail::Calendar(name) => calendar = Some(name.value.clone()),
            }
        }
        let Some(data_date) = data_date else {
            self.problem(
                "missing-data-date",
                project.at,
                "the project has no data date".to_owned(),
                "add a detail under it such as data-date 2026-03-02 08:00".to_owned(),
            );
            return None;
        };
        Some(NewProject {
            code: project.head.code.value.clone(),
            name: project.head.name.value.clone(),
            data_date,
            must_finish,
            calendar: calendar.unwrap_or_else(|| STARTER_CALENDAR.to_owned()),
        })
    }

    /// The calendars' commands. Returns every calendar name the schedule will
    /// have.
    fn calendars(&mut self, doc: &Document, project: Option<&NewProject>) -> Vec<String> {
        let starter = project.map_or(STARTER_CALENDAR, |p| p.calendar.as_str());
        let mut known: Vec<String> = vec![starter.to_owned()];
        let statements: Vec<&Statement<Calendar, CalendarDetail>> = doc
            .items
            .iter()
            .filter_map(|i| match i {
                Item::Calendar(c) => Some(c),
                _ => None,
            })
            .collect();
        // The project's calendar comes first, whatever its place in the file:
        // the others start as copies of it.
        let first = statements
            .iter()
            .position(|c| c.head.name.value.eq_ignore_ascii_case(starter));
        let inherited = first.map_or_else(BTreeSet::new, |i| self.starter(statements[i]));
        let starter_defined = first.is_some();
        for (index, calendar) in statements.iter().enumerate() {
            if Some(index) == first {
                continue;
            }
            let name = &calendar.head.name;
            if known.iter().any(|k| k.eq_ignore_ascii_case(&name.value)) {
                self.duplicate_calendar(name);
                continue;
            }
            let mut copy_of = calendar.head.copy_of.as_ref().map(|c| c.value.clone());
            if let Some(copy) = &calendar.head.copy_of
                && !known.iter().any(|k| k.eq_ignore_ascii_case(&copy.value))
            {
                let names: Vec<&str> = known.iter().map(String::as_str).collect();
                self.unknown(
                    "unknown-calendar",
                    "calendar defined above it",
                    copy,
                    &names,
                );
                // Reported here; it's still added, as a copy of the project's
                // calendar, so what uses it is checked without more refusals.
                copy_of = None;
            }
            let whole = calendar.head.copy_of.is_none();
            self.push(
                Edit::AddCalendar {
                    name: name.value.clone(),
                    copy_of,
                },
                calendar.at,
            );
            known.push(name.value.clone());
            self.calendar_body(calendar, whole, calendar.at);
            if whole {
                // Without copy-of, a calendar is what its lines say: the
                // project calendar's exceptions it was copied with go.
                let own = exception_dates(calendar);
                for date in inherited.difference(&own) {
                    self.push(
                        Edit::RemoveCalendarException {
                            calendar: name.value.clone(),
                            date: date.to_string(),
                        },
                        calendar.at,
                    );
                }
            }
        }
        if let Some(project) = project
            && !starter_defined
            && !project.calendar.eq_ignore_ascii_case(STARTER_CALENDAR)
        {
            let at = doc
                .items
                .iter()
                .find_map(|i| match i {
                    Item::Project(p) => details(&p.details).find_map(|(d, _)| match d {
                        ProjectDetail::Calendar(word) => Some(word.at),
                        _ => None,
                    }),
                    _ => None,
                })
                .unwrap_or_default();
            self.problem(
                "unknown-calendar",
                at,
                format!("{} isn't a calendar in this file", project.calendar),
                format!(
                    "define it with calendar \"{}\" and week lines, or leave the project's calendar out to start with {STARTER_CALENDAR}",
                    project.calendar
                ),
            );
        }
        known
    }

    /// The project's calendar, reshaped to its lines. Returns the dates of
    /// its exceptions, which calendars copied from it start with.
    fn starter(&mut self, calendar: &Statement<Calendar, CalendarDetail>) -> BTreeSet<Date> {
        if let Some(copy) = &calendar.head.copy_of {
            self.problem(
                "project-calendar-copy",
                copy.at,
                format!(
                    "{} is the project's calendar, so it can't be a copy",
                    calendar.head.name.value
                ),
                "give its working time with week lines instead".to_owned(),
            );
            return BTreeSet::new();
        }
        self.calendar_body(calendar, true, calendar.at);
        exception_dates(calendar)
    }

    fn duplicate_calendar(&mut self, name: &Word) {
        self.problem(
            "duplicate-calendar",
            name.at,
            format!("{} is defined twice", name.value),
            "keep one definition".to_owned(),
        );
    }

    /// A calendar's week and exceptions. With `whole_week`, days its week
    /// lines don't mention are days off; otherwise they keep what the
    /// calendar was copied from.
    pub(crate) fn calendar_body(
        &mut self,
        calendar: &Statement<Calendar, CalendarDetail>,
        whole_week: bool,
        at: At,
    ) {
        let name = calendar.head.name.value.clone();
        // Hours per day first, so the week never looks out of step with them.
        if let Some((hours, line)) = hours_per_day(calendar, whole_week) {
            self.push(
                Edit::SetHoursPerDay {
                    calendar: name.clone(),
                    hours,
                },
                line,
            );
        }
        let mut week: BTreeMap<usize, (Vec<String>, At)> = BTreeMap::new();
        for (detail, line) in details(&calendar.details) {
            if let CalendarDetail::Week { from, to, shifts } = detail {
                let shifts: Vec<String> = shifts.iter().map(|s| s.value.clone()).collect();
                for day in day_index(*from)..=day_index(*to) {
                    week.insert(day, (shifts.clone(), line));
                }
            }
        }
        if !week.is_empty() {
            let mut days: Vec<(usize, Vec<String>, At)> = (0..7)
                .filter_map(|day| match week.get(&day) {
                    Some((shifts, line)) => Some((day, shifts.clone(), *line)),
                    None => whole_week.then_some((day, Vec::new(), at)),
                })
                .collect();
            // Working days first, so the calendar never passes through a
            // week with none.
            days.sort_by_key(|(day, shifts, _)| (shifts.is_empty(), *day));
            for (day, shifts, line) in days {
                self.push(
                    Edit::SetWorkWeek {
                        calendar: name.clone(),
                        day: DAYS[day].1,
                        shifts,
                    },
                    line,
                );
            }
        }
        for (detail, line) in details(&calendar.details) {
            match detail {
                CalendarDetail::Holiday(date) => self.push(
                    Edit::AddCalendarException {
                        calendar: name.clone(),
                        date: date.value.to_string(),
                        shifts: Vec::new(),
                    },
                    line,
                ),
                CalendarDetail::Exception { date, shifts } => self.push(
                    Edit::AddCalendarException {
                        calendar: name.clone(),
                        date: date.value.to_string(),
                        shifts: shifts.iter().map(|s| s.value.clone()).collect(),
                    },
                    line,
                ),
                CalendarDetail::Week { .. } | CalendarDetail::HoursPerDay(_) => {}
            }
        }
    }

    /// The WBS commands, parents first. Returns each node's code by its path.
    pub(crate) fn defined_twice(&mut self, name: &Word) {
        self.problem(
            "defined-twice",
            name.at,
            format!("{} is defined twice", name.value),
            "keep one definition".to_owned(),
        );
    }

    /// The code types' commands, and their names and values.
    fn code_types(&mut self, doc: &Document) -> BTreeMap<String, (String, Vec<String>)> {
        let mut known: BTreeMap<String, (String, Vec<String>)> = BTreeMap::new();
        for item in &doc.items {
            let Item::CodeType(statement) = item else {
                continue;
            };
            let name = &statement.head.name;
            let key = name.value.to_lowercase();
            if known.contains_key(&key) {
                self.defined_twice(name);
                continue;
            }
            self.push(new_code_type(statement), statement.at);
            let mut values = Vec::new();
            for (value, at) in details(&statement.details) {
                values.push(value.value.value.clone());
                self.push(
                    Edit::AddCodeValue {
                        code_type: name.value.clone(),
                        value: value.value.value.clone(),
                        description: value.description.as_ref().map(|d| d.value.clone()),
                    },
                    at,
                );
            }
            known.insert(key, (name.value.clone(), values));
        }
        known
    }

    /// The fields' commands, and their names.
    fn fields(&mut self, doc: &Document) -> BTreeMap<String, String> {
        let mut known = BTreeMap::new();
        for item in &doc.items {
            let Item::Field(statement) = item else {
                continue;
            };
            let name = &statement.head.name;
            let key = name.value.to_lowercase();
            if known.contains_key(&key) {
                self.defined_twice(name);
                continue;
            }
            self.push(new_field(statement), statement.at);
            known.insert(key, name.value.clone());
        }
        known
    }

    /// The roles' commands, and their short names. A parent must be defined
    /// above the role under it.
    fn roles(&mut self, doc: &Document) -> Vec<String> {
        let mut known: Vec<String> = Vec::new();
        for item in &doc.items {
            let Item::Role(statement) = item else {
                continue;
            };
            let code = &statement.head.code;
            if known.iter().any(|k| k.eq_ignore_ascii_case(&code.value)) {
                self.defined_twice(code);
                continue;
            }
            let mut edit = new_role(statement);
            for (detail, _) in details(&statement.details) {
                let RoleDetail::Parent(word) = detail else {
                    continue;
                };
                if !known.iter().any(|k| k.eq_ignore_ascii_case(&word.value)) {
                    let names: Vec<&str> = known.iter().map(String::as_str).collect();
                    self.unknown("unknown-role", "role", word, &names);
                    if let Edit::AddRole { parent, .. } = &mut edit {
                        *parent = None;
                    }
                }
            }
            known.push(code.value.clone());
            self.push(edit, statement.at);
        }
        known
    }

    /// The resources' commands, and their short names.
    fn resources(&mut self, doc: &Document, calendars: &[String]) -> Vec<String> {
        let mut known: Vec<String> = Vec::new();
        let names: Vec<&str> = calendars.iter().map(String::as_str).collect();
        for item in &doc.items {
            let Item::Resource(statement) = item else {
                continue;
            };
            let code = &statement.head.code;
            if known.iter().any(|k| k.eq_ignore_ascii_case(&code.value)) {
                self.defined_twice(code);
                continue;
            }
            known.push(code.value.clone());
            let mut edit = new_resource(statement);
            for (detail, _) in details(&statement.details) {
                let ResourceDetail::Calendar(word) = detail else {
                    continue;
                };
                let known = names
                    .iter()
                    .position(|k| k.eq_ignore_ascii_case(&word.value));
                // Reported here; the resource is still added, on the
                // project's calendar, so its assignments are checked.
                match known {
                    None => self.unknown("unknown-calendar", "calendar", word, &names),
                    // The project's calendar is global. The file's others are
                    // the project's own, which a resource can't use.
                    Some(0) => continue,
                    Some(_) => self.problem(
                        "resource-calendar",
                        word.at,
                        format!(
                            "{} is this project's own calendar, which a resource can't use",
                            word.value
                        ),
                        format!(
                            "take the line out, and the resource uses the project's calendar, {}",
                            names[0]
                        ),
                    ),
                }
                if let Edit::AddResource { calendar, .. } = &mut edit {
                    *calendar = None;
                }
            }
            self.push(edit, statement.at);
        }
        known
    }

    fn wbs(&mut self, doc: &Document, project: Option<&NewProject>) -> BTreeMap<String, String> {
        let mut nodes: BTreeMap<String, String> = BTreeMap::new();
        let Some(project) = project else {
            return nodes;
        };
        nodes.insert(project.code.clone(), project.code.clone());
        let mut lines: Vec<&Statement<crate::Wbs, crate::Never>> = doc
            .items
            .iter()
            .filter_map(|i| match i {
                Item::Wbs(w) => Some(w),
                _ => None,
            })
            .collect();
        lines.sort_by_key(|w| w.head.path.value.split('.').count());
        let mut codes: BTreeMap<String, String> = BTreeMap::new();
        codes.insert(project.code.to_lowercase(), project.code.clone());
        for line in lines {
            let path = &line.head.path;
            // The project's code may itself hold a dot, so the root is
            // matched whole rather than as the first segment.
            let under_root = path
                .value
                .strip_prefix(project.code.as_str())
                .is_some_and(|rest| rest.starts_with('.') && rest.len() > 1);
            let split = path.value.rsplit_once('.');
            let (Some((parent, code)), true) = (split, under_root) else {
                self.wbs_root(path, &project.code);
                continue;
            };
            if nodes.contains_key(&path.value) {
                self.problem(
                    "duplicate-wbs",
                    path.at,
                    format!("WBS {} is defined twice", path.value),
                    "keep one wbs line for it".to_owned(),
                );
                continue;
            }
            if !nodes.contains_key(parent) {
                let known: Vec<&str> = nodes.keys().map(String::as_str).collect();
                let word = Value {
                    value: parent.to_owned(),
                    at: path.at,
                };
                self.unknown("unknown-wbs", "WBS path", &word, &known);
                continue;
            }
            if let Some(other) = codes.get(&code.to_lowercase()) {
                self.problem(
                    "repeated-wbs-code",
                    path.at,
                    format!("the WBS code {code} is already used by {other}"),
                    "edit commands find a WBS node by its own code, so give each node a code used once in the project".to_owned(),
                );
                continue;
            }
            codes.insert(code.to_lowercase(), path.value.clone());
            self.push(
                Edit::AddWbs {
                    code: code.to_owned(),
                    name: line.head.name.value.clone(),
                    parent: nodes[parent].clone(),
                },
                line.at,
            );
            nodes.insert(path.value.clone(), code.to_owned());
        }
        nodes
    }

    fn wbs_root(&mut self, path: &Word, code: &str) {
        self.problem(
            "wrong-wbs-root",
            path.at,
            format!("WBS {} doesn't start at the project", path.value),
            format!(
                "start the path with the project code, as in {code}.{}",
                path.value.rsplit('.').next().unwrap_or("CIV")
            ),
        );
    }

    fn activities(
        &mut self,
        doc: &Document,
        project: Option<&NewProject>,
        known: &Known,
        wbs: &BTreeMap<String, String>,
    ) {
        let Some(project) = project else {
            return;
        };
        let mut seen: BTreeSet<&str> = BTreeSet::new();
        let mut placed: Vec<(&Statement<Activity, ActivityDetail>, String)> = Vec::new();
        let mut current = project.code.clone();
        for item in &doc.items {
            match item {
                Item::Wbs(w) => current.clone_from(&w.head.path.value),
                Item::Activity(a) => {
                    if !seen.insert(a.head.code.value.as_str()) {
                        self.problem(
                            "duplicate-activity",
                            a.head.code.at,
                            format!("{} is defined twice", a.head.code.value),
                            "give each activity its own ID".to_owned(),
                        );
                        continue;
                    }
                    placed.push((a, current.clone()));
                }
                _ => {}
            }
        }
        let codes: Vec<&str> = seen.iter().copied().collect();
        let calendar_names: Vec<&str> = known.calendars.iter().map(String::as_str).collect();
        for (activity, path) in &placed {
            // Under a WBS line that was refused, the activity goes on the
            // project's own node, so its links and details are still
            // checked without each being refused for its sake.
            let node = wbs.get(path).unwrap_or(&project.code);
            let calendar = details(&activity.details).find_map(|(d, _)| match d {
                ActivityDetail::Calendar(word) => Some(word),
                _ => None,
            });
            let calendar = match calendar {
                Some(word)
                    if !calendar_names
                        .iter()
                        .any(|k| k.eq_ignore_ascii_case(&word.value)) =>
                {
                    // Reported here; the activity is still added, on the
                    // project's calendar, so what follows from it is checked.
                    self.unknown("unknown-calendar", "calendar", word, &calendar_names);
                    None
                }
                other => other,
            };
            let resource = details(&activity.details)
                .any(|(d, _)| matches!(d, ActivityDetail::Type(TypeKind::Resource)));
            let head = &activity.head;
            self.push(
                Edit::AddActivity {
                    code: head.code.value.clone(),
                    name: head.name.value.clone(),
                    wbs: node.clone(),
                    activity_type: Some(match head.kind {
                        ActivityKind::Milestone => Kind::FinishMilestone,
                        ActivityKind::StartMilestone => Kind::StartMilestone,
                        ActivityKind::Activity if resource => Kind::Resource,
                        ActivityKind::Activity => Kind::Task,
                    }),
                    calendar: calendar.map(|c| c.value.clone()),
                    duration: head.duration.as_ref().map(|d| d.value),
                },
                activity.at,
            );
        }
        for (activity, _) in &placed {
            for predecessor in &activity.head.after {
                if !seen.contains(predecessor.code.value.as_str()) {
                    self.unknown("unknown-activity", "activity", &predecessor.code, &codes);
                    continue;
                }
                self.push(
                    Edit::AddLink {
                        predecessor: predecessor.code.value.clone(),
                        successor: activity.head.code.value.clone(),
                        link_type: predecessor.link,
                        lag: predecessor.lag,
                    },
                    predecessor.code.at,
                );
            }
        }
        for (activity, _) in &placed {
            self.activity_details(activity, known);
        }
    }

    /// Assigns a resource, or a role when `role`, that the file defines, or
    /// reports the name.
    fn staff(
        &mut self,
        code: &str,
        (role, name, units): (bool, &Word, Option<&Value<f64>>),
        at: At,
        known: &Known,
    ) {
        let names = if role { &known.roles } else { &known.resources };
        if !names.iter().any(|k| k.eq_ignore_ascii_case(&name.value)) {
            let names: Vec<&str> = names.iter().map(String::as_str).collect();
            let (problem, kind) = if role {
                ("unknown-role", "role")
            } else {
                ("unknown-resource", "resource")
            };
            self.unknown(problem, kind, name, &names);
            return;
        }
        let (activity, units) = (code.to_owned(), units.map(|u| u.value));
        let edit = if role {
            Edit::AssignRole {
                activity,
                role: name.value.clone(),
                units,
            }
        } else {
            Edit::AssignResource {
                activity,
                resource: name.value.clone(),
                units,
            }
        };
        self.push(edit, at);
    }

    fn activity_details(&mut self, activity: &Statement<Activity, ActivityDetail>, known: &Known) {
        let code = activity.head.code.value.clone();
        let mut progress: Option<(At, ProgressParts)> = None;
        let mut remaining = None;
        for (detail, at) in details(&activity.details) {
            match detail {
                ActivityDetail::Calendar(_) | ActivityDetail::Type(_) => {}
                ActivityDetail::Remaining(span) => remaining = Some((span.value, at)),
                ActivityDetail::Constraint {
                    secondary,
                    kind,
                    date,
                } => self.push(
                    Edit::SetConstraint {
                        activity: code.clone(),
                        constraint: *kind,
                        date: date.as_ref().map(|d| d.value.to_string()),
                        secondary: *secondary,
                    },
                    at,
                ),
                ActivityDetail::Code { code_type, value } => {
                    self.code(&code, (code_type, value), at, known);
                }
                ActivityDetail::Udf { field, value } => {
                    let Some(name) = known.fields.get(&field.value.to_lowercase()) else {
                        let names: Vec<&str> = known.fields.values().map(String::as_str).collect();
                        self.unknown("unknown-field", "field", field, &names);
                        continue;
                    };
                    self.push(
                        Edit::SetUdf {
                            field: name.clone(),
                            value: value.value.clone(),
                            activity: Some(code.clone()),
                            wbs: None,
                            resource: None,
                        },
                        at,
                    );
                }
                ActivityDetail::Resource { resource, units } => {
                    self.staff(&code, (false, resource, units.as_ref()), at, known);
                }
                ActivityDetail::Role { role, units } => {
                    self.staff(&code, (true, role, units.as_ref()), at, known);
                }
                ActivityDetail::Expense(x) => {
                    for edit in expense_edits(&code, x) {
                        self.push(edit, at);
                    }
                }
                ActivityDetail::ExpectedFinish(date) => self.push(
                    Edit::SetExpectedFinish {
                        activity: code.clone(),
                        date: Some(date.value.to_string()),
                    },
                    at,
                ),
                ActivityDetail::ActualStart(d) => {
                    progress
                        .get_or_insert((at, ProgressParts::default()))
                        .1
                        .start = Some(d.value);
                }
                ActivityDetail::ActualFinish(d) => {
                    progress
                        .get_or_insert((at, ProgressParts::default()))
                        .1
                        .finish = Some(d.value);
                }
                ActivityDetail::Percent(p) => {
                    progress
                        .get_or_insert((at, ProgressParts::default()))
                        .1
                        .percent = Some(p.value);
                }
            }
        }
        match (progress, remaining) {
            (Some((at, parts)), remaining) => self.push(
                Edit::SetProgress {
                    activity: code,
                    actual_start: parts.start.map(|d| d.to_string()),
                    actual_finish: parts.finish.map(|d| d.to_string()),
                    remaining: remaining.map(|(span, _)| span),
                    percent: parts.percent,
                },
                at,
            ),
            (None, Some((span, at))) => self.push(
                Edit::SetDuration {
                    activity: code,
                    original: None,
                    remaining: Some(span),
                },
                at,
            ),
            (None, None) => {}
        }
    }
}

impl Compiler<'_> {
    /// An activity's code, if the file defines its type and value.
    fn code(&mut self, activity: &str, (code_type, value): (&Word, &Word), at: At, known: &Known) {
        let Some((name, values)) = known.codes.get(&code_type.value.to_lowercase()) else {
            let names: Vec<&str> = known.codes.values().map(|(n, _)| n.as_str()).collect();
            self.unknown("unknown-code-type", "code type", code_type, &names);
            return;
        };
        if !values.iter().any(|v| v.eq_ignore_ascii_case(&value.value)) {
            let names: Vec<&str> = values.iter().map(String::as_str).collect();
            self.unknown(
                "unknown-code-value",
                &format!("value of {name}"),
                value,
                &names,
            );
            return;
        }
        self.push(
            Edit::SetCode {
                activity: activity.to_owned(),
                code_type: name.clone(),
                value: value.value.clone(),
            },
            at,
        );
    }
}

#[derive(Default)]
struct ProgressParts {
    start: Option<DateTime>,
    finish: Option<DateTime>,
    percent: Option<f64>,
}

fn day_index(day: Day) -> usize {
    DAYS.iter().position(|(_, d)| *d == day).unwrap_or(0)
}

/// The edits that put an `expense` line on an activity: added at its budget,
/// then given its actual and remaining cost when they aren't the budget's.
#[allow(
    clippy::float_cmp,
    reason = "a line's costs are exact, as written and read"
)]
pub(crate) fn expense_edits(code: &str, x: &crate::Expense) -> Vec<Edit> {
    let (budget, actual, remaining, _) = x.meant();
    let mut edits = vec![Edit::AddExpense {
        activity: code.to_owned(),
        expense: x.item.value.clone(),
        cost: budget,
        account: None,
        category: None,
        accrual: x.accrual.as_ref().map(|a| a.value),
    }];
    if actual != 0.0 || remaining != budget {
        edits.push(Edit::SetExpense {
            activity: code.to_owned(),
            expense: x.item.value.clone(),
            budgeted_cost: None,
            actual_cost: Some(actual),
            remaining_cost: Some(remaining),
            accrual: None,
        });
    }
    edits
}
