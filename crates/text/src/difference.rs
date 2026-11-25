//! What it takes to make a schedule match a text file (ARCHITECTURE.md, Schedule as text): the
//! difference, as ordinary edit commands.

use std::collections::{BTreeMap, BTreeSet};

use totalfloat_edit::{Edit, Kind, day_length, lag_day_length};
use totalfloat_model::{
    Activity as Model, ActivityType, CalendarId, DateTime, Minutes, ResourceType, RoleId, Schedule,
};

use crate::compile::{Compiler, details, new_code_type, new_field, new_resource, new_role};
use crate::parse::DAYS;
use crate::render::{constraint_kind, shift_texts};
use crate::{
    Activity, ActivityDetail, ActivityKind, At, Calendar, CalendarDetail, CodeType, CodeValue,
    ConstraintKind, Diagnostic, Document, Item, LinkType, Placed, Predecessor, ProjectDetail,
    Resource, ResourceDetail, Role, RoleDetail, Span, Statement, TypeKind, render,
};

/// Something the schedule has that the text leaves out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Missing {
    /// `activity`, `link` or `wbs`.
    pub kind: &'static str,
    /// Which: an Activity ID, `A>B` for a link, or a WBS path.
    pub name: String,
}

/// What to do with what the text leaves out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Leaving {
    /// Keep it.
    Keep,
    /// Delete it.
    Delete,
}

/// The edits that make a schedule match a text file.
#[derive(Debug, Clone, PartialEq)]
pub struct Difference {
    /// The commands, each with the line it came from, in a safe order: adds,
    /// then changes, then link removals, then deletions.
    pub edits: Vec<Placed>,
    /// What the text leaves out.
    pub missing: Vec<Missing>,
    /// A new data date, when the text's differs.
    pub data_date: Option<DateTime>,
    /// Changes the edits can't make, and names the text doesn't define.
    pub problems: Vec<Diagnostic>,
    /// Activities deleted and added that look like one renamed.
    pub warnings: Vec<String>,
}

/// One project of a schedule's text, indexed by name.
struct Index<'d> {
    calendars: BTreeMap<String, &'d Statement<Calendar, CalendarDetail>>,
    calendar_order: Vec<&'d Statement<Calendar, CalendarDetail>>,
    /// Activity IDs and calendar names defined more than once.
    repeated: Vec<(String, At)>,
    code_types: BTreeMap<String, &'d Statement<CodeType, CodeValue>>,
    fields: BTreeMap<String, &'d Statement<crate::Field, crate::Never>>,
    resources: Vec<&'d Statement<Resource, ResourceDetail>>,
    roles: Vec<&'d Statement<Role, RoleDetail>>,
    wbs: Vec<(&'d str, &'d str, At)>,
    activities: BTreeMap<&'d str, (&'d Statement<Activity, ActivityDetail>, String)>,
    order: Vec<&'d str>,
    project: Option<&'d Statement<crate::Project, ProjectDetail>>,
}

impl<'d> Index<'d> {
    fn of(doc: &'d Document, root: &str) -> Self {
        let mut index = Index {
            calendars: BTreeMap::new(),
            calendar_order: Vec::new(),
            repeated: Vec::new(),
            code_types: BTreeMap::new(),
            fields: BTreeMap::new(),
            resources: Vec::new(),
            roles: Vec::new(),
            wbs: Vec::new(),
            activities: BTreeMap::new(),
            order: Vec::new(),
            project: None,
        };
        let mut path = root.to_owned();
        for item in &doc.items {
            match item {
                Item::Project(p) => index.project = Some(p),
                Item::Calendar(c) => {
                    let key = c.head.name.value.to_lowercase();
                    if index.calendars.insert(key, c).is_some() {
                        index
                            .repeated
                            .push((c.head.name.value.clone(), c.head.name.at));
                    } else {
                        index.calendar_order.push(c);
                    }
                }
                Item::CodeType(t) => {
                    // The first definition counts, as when building.
                    let name = &t.head.name;
                    match index.code_types.entry(name.value.to_lowercase()) {
                        std::collections::btree_map::Entry::Occupied(_) => {
                            index.repeated.push((name.value.clone(), name.at));
                        }
                        std::collections::btree_map::Entry::Vacant(slot) => {
                            slot.insert(t);
                        }
                    }
                }
                Item::Field(f) => {
                    let name = &f.head.name;
                    match index.fields.entry(name.value.to_lowercase()) {
                        std::collections::btree_map::Entry::Occupied(_) => {
                            index.repeated.push((name.value.clone(), name.at));
                        }
                        std::collections::btree_map::Entry::Vacant(slot) => {
                            slot.insert(f);
                        }
                    }
                }
                Item::Resource(r) => {
                    let code = &r.head.code;
                    let seen = index.resources.iter();
                    if seen
                        .clone()
                        .any(|s| s.head.code.value.eq_ignore_ascii_case(&code.value))
                    {
                        index.repeated.push((code.value.clone(), code.at));
                    } else {
                        index.resources.push(r);
                    }
                }
                Item::Role(r) => {
                    let code = &r.head.code;
                    if index
                        .roles
                        .iter()
                        .any(|s| s.head.code.value.eq_ignore_ascii_case(&code.value))
                    {
                        index.repeated.push((code.value.clone(), code.at));
                    } else {
                        index.roles.push(r);
                    }
                }
                Item::Wbs(w) => {
                    path.clone_from(&w.head.path.value);
                    index
                        .wbs
                        .push((&w.head.path.value, &w.head.name.value, w.at));
                }
                Item::Activity(a) => {
                    let code = a.head.code.value.as_str();
                    if index.activities.contains_key(code) {
                        index.repeated.push((code.to_owned(), a.head.code.at));
                    } else {
                        index.activities.insert(code, (a, path.clone()));
                        index.order.push(code);
                    }
                }
                Item::Blank | Item::Comment(_) => {}
            }
        }
        index
    }
}

/// Commands by when they must run.
#[derive(Default)]
struct Phases {
    calendars: Vec<Placed>,
    codes: Vec<Placed>,
    resources: Vec<Placed>,
    wbs: Vec<Placed>,
    added: Vec<Placed>,
    changed: Vec<Placed>,
    links: Vec<Placed>,
    unlinked: Vec<Placed>,
    deleted: Vec<Placed>,
}

fn place(edit: Edit, at: At) -> Placed {
    Placed { edit, at }
}

/// The edits that make `schedule`, one project, match `wanted`.
#[must_use]
pub fn difference(
    schedule: &Schedule,
    wanted: &Document,
    file: &str,
    leaving: Leaving,
) -> Difference {
    let current = render(schedule).document;
    let root = schedule.project.short_name.as_str();
    let (cur, want) = (Index::of(&current, root), Index::of(wanted, root));
    let mut c = Compiler::new(file);
    let mut phases = Phases::default();
    let mut diff = Difference {
        edits: Vec::new(),
        missing: Vec::new(),
        data_date: None,
        problems: Vec::new(),
        warnings: Vec::new(),
    };
    for (name, at) in &want.repeated {
        c.problem(
            "defined-twice",
            *at,
            format!("{name} is defined twice"),
            "keep one definition".to_owned(),
        );
    }
    project(schedule, &want, &mut c, &mut diff);
    let hours = calendars(schedule, &want, &mut c, &mut phases);
    // The text's spans mean days of the calendars as the text leaves them.
    let days = (!hours.is_empty()).then(|| {
        let mut days = Schedule::new(schedule.project.clone());
        days.calendars.clone_from(&schedule.calendars);
        for (id, day) in hours {
            if let Some(calendar) = days.calendars.get_mut(&id) {
                calendar.hours_per_period.day = Some(day);
            }
        }
        days
    });
    code_values(schedule, &want, &mut phases);
    fields(schedule, &want, &mut phases);
    resources(schedule, &want, &mut c, &mut phases);
    roles(schedule, &want, &mut c, &mut phases);
    wbs(root, &cur, &want, &mut phases, &mut diff);
    let models: BTreeMap<&str, &Model> = schedule
        .activities
        .values()
        .map(|a| (a.code.as_str(), a))
        .collect();
    let diffing = Diffing {
        schedule,
        days: days.as_ref().unwrap_or(schedule),
        models: &models,
        cur: &cur,
        want: &want,
        root,
    };
    for code in &want.order {
        diffing.activity(code, &mut phases);
        diffing.links(code, &mut c, &mut phases, &mut diff, leaving);
    }
    diffing.left_out(&mut phases, &mut diff, leaving);
    diff.problems.append(&mut c.problems);
    diff.edits = [
        phases.calendars,
        phases.codes,
        phases.resources,
        phases.wbs,
        phases.added,
        phases.changed,
        phases.links,
        phases.unlinked,
        phases.deleted,
    ]
    .concat();
    diff
}

fn unsupported(c: &mut Compiler<'_>, at: At, what: &str) {
    c.problem(
        "unsupported-change",
        at,
        format!("the edit commands can't change {what} yet"),
        "put it back as the schedule has it, or change it in P6".to_owned(),
    );
}

fn project(schedule: &Schedule, want: &Index<'_>, c: &mut Compiler<'_>, diff: &mut Difference) {
    let Some(p) = want.project else {
        return;
    };
    let current = &schedule.project;
    if p.head.code.value != current.short_name {
        unsupported(c, p.head.code.at, "the project's code");
    }
    if Some(p.head.name.value.as_str()) != current.name.as_deref().or(Some(&current.short_name)) {
        unsupported(c, p.head.name.at, "the project's name");
    }
    for (detail, at) in details(&p.details) {
        match detail {
            ProjectDetail::DataDate(d) if Some(d.value) != current.data_date => {
                diff.data_date = Some(d.value);
            }
            ProjectDetail::MustFinish(d) if Some(d.value) != current.must_finish_by => {
                unsupported(c, at, "the must-finish date");
            }
            ProjectDetail::Calendar(name) => {
                let now = current
                    .default_calendar
                    .and_then(|id| schedule.calendars.get(&id))
                    .map(|c| c.name.to_lowercase());
                if now.as_deref() != Some(name.value.to_lowercase().as_str()) {
                    unsupported(c, name.at, "the project's calendar");
                }
            }
            _ => {}
        }
    }
}

fn calendar_named(schedule: &Schedule, name: &str) -> Option<CalendarId> {
    schedule
        .calendars
        .values()
        .find(|c| c.name.eq_ignore_ascii_case(name))
        .map(|c| c.id)
}

/// The calendars' edits. Returns the hours per day they change.
fn calendars(
    schedule: &Schedule,
    want: &Index<'_>,
    c: &mut Compiler<'_>,
    phases: &mut Phases,
) -> Vec<(CalendarId, Minutes)> {
    let mut hours = Vec::new();
    for statement in &want.calendar_order {
        let name = &statement.head.name.value;
        let Some(id) = calendar_named(schedule, name) else {
            c.push(
                Edit::AddCalendar {
                    name: name.clone(),
                    copy_of: statement.head.copy_of.as_ref().map(|w| w.value.clone()),
                },
                statement.at,
            );
            c.calendar_body(statement, statement.head.copy_of.is_none(), statement.at);
            phases.calendars.append(&mut c.edits);
            continue;
        };
        let calendar = &schedule.calendars[&id];
        // The week: days its lines give, and with no copy-of the rest off.
        let mut week: BTreeMap<usize, (Vec<String>, At)> = BTreeMap::new();
        let mut exceptions: BTreeMap<totalfloat_model::Date, (Vec<String>, At)> = BTreeMap::new();
        for (detail, at) in details(&statement.details) {
            match detail {
                CalendarDetail::Week { from, to, shifts } => {
                    let shifts: Vec<String> = shifts.iter().map(|s| s.value.clone()).collect();
                    for day in day_index(*from)..=day_index(*to) {
                        week.insert(day, (shifts.clone(), at));
                    }
                }
                CalendarDetail::Holiday(date) => {
                    exceptions.insert(date.value, (Vec::new(), at));
                }
                CalendarDetail::Exception { date, shifts } => {
                    let shifts = shifts.iter().map(|s| s.value.clone()).collect();
                    exceptions.insert(date.value, (shifts, at));
                }
                CalendarDetail::HoursPerDay(_) => {}
            }
        }
        let whole = statement.head.copy_of.is_none() && !week.is_empty();
        if let Some((edit, day)) = hours_per_day(schedule, id, statement, whole) {
            phases.calendars.push(edit);
            hours.push((id, day));
        }
        let mut days: Vec<(usize, Vec<String>, At)> = (0..7)
            .filter_map(|day| match week.get(&day) {
                Some((shifts, at)) => Some((day, shifts.clone(), *at)),
                None => whole.then_some((day, Vec::new(), statement.at)),
            })
            .filter(|(day, shifts, _)| {
                *shifts != shift_texts(calendar.work_week.day(DAYS[*day].1.into()))
            })
            .collect();
        days.sort_by_key(|(day, shifts, _)| (shifts.is_empty(), *day));
        for (day, shifts, at) in days {
            phases.calendars.push(place(
                Edit::SetWorkWeek {
                    calendar: name.clone(),
                    day: DAYS[day].1,
                    shifts,
                },
                at,
            ));
        }
        for (date, (shifts, at)) in &exceptions {
            if calendar
                .exceptions
                .get(date)
                .map(|s| shift_texts(s))
                .as_ref()
                != Some(shifts)
            {
                phases.calendars.push(place(
                    Edit::AddCalendarException {
                        calendar: name.clone(),
                        date: date.to_string(),
                        shifts: shifts.clone(),
                    },
                    *at,
                ));
            }
        }
        for date in calendar
            .exceptions
            .keys()
            .filter(|d| !exceptions.contains_key(d))
        {
            phases.calendars.push(place(
                Edit::RemoveCalendarException {
                    calendar: name.clone(),
                    date: date.to_string(),
                },
                statement.at,
            ));
        }
    }
    hours
}

/// The edit that gives a calendar the hours per day its lines give, if they
/// give some it doesn't have.
fn hours_per_day(
    schedule: &Schedule,
    id: CalendarId,
    statement: &Statement<Calendar, CalendarDetail>,
    whole: bool,
) -> Option<(Placed, Minutes)> {
    let (hours, at) = crate::compile::hours_per_day(statement, whole)?;
    #[allow(
        clippy::cast_possible_truncation,
        reason = "the parser keeps hours from 0 to 24"
    )]
    let minutes = Minutes::new((hours * 60.0).round() as i64);
    (day_length(schedule, id) != Some(minutes)).then(|| {
        let edit = place(
            Edit::SetHoursPerDay {
                calendar: statement.head.name.value.clone(),
                hours,
            },
            at,
        );
        (edit, minutes)
    })
}

fn day_index(day: totalfloat_edit::Day) -> usize {
    DAYS.iter().position(|(_, d)| *d == day).unwrap_or(0)
}

/// Fields the text defines and the schedule lacks are added; one it has is
/// left as it is.
fn fields(schedule: &Schedule, want: &Index<'_>, phases: &mut Phases) {
    for statement in want.fields.values() {
        let name = &statement.head.name.value;
        let lower = name.to_lowercase();
        let held = schedule.udf_types.values().any(|t| {
            t.subject == totalfloat_model::UdfSubject::Activity && t.label.to_lowercase() == lower
        });
        if !held {
            phases.codes.push(place(new_field(statement), statement.at));
        }
    }
}

fn code_values(schedule: &Schedule, want: &Index<'_>, phases: &mut Phases) {
    for statement in want.code_types.values() {
        let name = &statement.head.name.value;
        let found = schedule
            .activity_code_types
            .values()
            .find(|t| t.name.eq_ignore_ascii_case(name));
        let have: BTreeSet<&str> = if let Some(code_type) = found {
            schedule
                .activity_codes
                .values()
                .filter(|v| v.code_type == code_type.id)
                .map(|v| v.value.as_str())
                .collect()
        } else {
            phases
                .codes
                .push(place(new_code_type(statement), statement.at));
            BTreeSet::new()
        };
        let name = found.map_or(name, |t| &t.name);
        for (value, at) in details(&statement.details) {
            if !have.contains(value.value.value.as_str()) {
                phases.codes.push(place(
                    Edit::AddCodeValue {
                        code_type: name.clone(),
                        value: value.value.value.clone(),
                        description: value.description.as_ref().map(|d| d.value.clone()),
                    },
                    at,
                ));
            }
        }
    }
}

/// New resources are added. One the schedule has keeps its name, type,
/// calendar and units per hour, so a change to them is a problem; a
/// `max-units-per-hour` line sets its limit on every rate. With no such line,
/// its limit stays as it is.
fn resources(schedule: &Schedule, want: &Index<'_>, c: &mut Compiler<'_>, phases: &mut Phases) {
    for statement in &want.resources {
        let head = &statement.head;
        let Some(resource) = schedule
            .resources
            .values()
            .find(|r| r.short_name.eq_ignore_ascii_case(&head.code.value))
        else {
            phases
                .resources
                .push(place(new_resource(statement), statement.at));
            continue;
        };
        if resource.name != head.name.value {
            unsupported(c, head.name.at, "a resource's name");
        }
        if let Some(kind) = &head.kind
            && ResourceType::from(kind.value) != resource.kind
        {
            unsupported(c, kind.at, "a resource's type");
        }
        for (detail, at) in details(&statement.details) {
            let same = match detail {
                ResourceDetail::Calendar(word) => resource
                    .calendar
                    .and_then(|id| schedule.calendars.get(&id))
                    .is_some_and(|c| c.name.eq_ignore_ascii_case(&word.value)),
                ResourceDetail::UnitsPerHour(units) => {
                    (resource.default_units_per_hour.unwrap_or(1.0) - units.value).abs() < 1e-9
                }
                // The limit can change: every rate takes the one the text gives.
                ResourceDetail::MaxUnitsPerHour(units) => {
                    let matches = |r: &totalfloat_model::ResourceRate| {
                        r.max_units_per_hour
                            .is_some_and(|l| (l - units.value).abs() < 1e-9)
                    };
                    if resource.rates.is_empty() || !resource.rates.iter().all(matches) {
                        phases.resources.push(place(
                            Edit::SetResourceLimit {
                                resource: resource.short_name.clone(),
                                max_units_per_hour: Some(units.value),
                                from: None,
                            },
                            at,
                        ));
                    }
                    true
                }
            };
            if !same {
                unsupported(c, at, "a resource's calendar or units per hour");
            }
        }
    }
}

/// New roles are added, after the resources, with their parents above them.
/// One the schedule has keeps its name and parent, so a change to them is a
/// problem; a `max-units-per-hour` line sets its limit on every rate, and
/// with no such line its limit stays as it is.
fn roles(schedule: &Schedule, want: &Index<'_>, c: &mut Compiler<'_>, phases: &mut Phases) {
    let named = |code: &str| {
        schedule
            .roles
            .values()
            .find(|r| r.short_name.eq_ignore_ascii_case(code))
    };
    for statement in &want.roles {
        let head = &statement.head;
        let Some(role) = named(&head.code.value) else {
            phases
                .resources
                .push(place(new_role(statement), statement.at));
            continue;
        };
        if role.name != head.name.value {
            unsupported(c, head.name.at, "a role's name");
        }
        let parent = role
            .parent
            .and_then(|id| schedule.roles.get(&id))
            .map(|p| p.short_name.as_str());
        let wanted = details(&statement.details)
            .filter_map(|(detail, at)| match detail {
                RoleDetail::Parent(word) => Some((word.value.as_str(), at)),
                RoleDetail::MaxUnitsPerHour(_) => None,
            })
            .last();
        for (detail, at) in details(&statement.details) {
            let RoleDetail::MaxUnitsPerHour(units) = detail else {
                continue;
            };
            let matches = |r: &totalfloat_model::RoleRate| {
                r.max_units_per_hour
                    .is_some_and(|l| (l - units.value).abs() < 1e-9)
            };
            if role.rates.is_empty() || !role.rates.iter().all(matches) {
                phases.resources.push(place(
                    Edit::SetRoleLimit {
                        role: role.short_name.clone(),
                        max_units_per_hour: Some(units.value),
                        from: None,
                    },
                    at,
                ));
            }
        }
        let same = match (parent, wanted) {
            (None, None) => true,
            (Some(is), Some((was, _))) => is.eq_ignore_ascii_case(was),
            // Text breaks a loop of parents by leaving one out.
            (Some(_), None) => in_parent_loop(schedule, role.id),
            (None, Some(_)) => false,
        };
        if !same {
            let at = wanted.map_or(head.code.at, |(_, at)| at);
            unsupported(c, at, "a role's parent");
        }
    }
}

/// Whether following `id`'s parents comes back to it.
fn in_parent_loop(schedule: &Schedule, id: RoleId) -> bool {
    let parent = |id: RoleId| schedule.roles.get(&id).and_then(|r| r.parent);
    let mut at = parent(id);
    for _ in 0..schedule.roles.len() {
        match at {
            Some(p) if p == id => return true,
            Some(p) => at = parent(p),
            None => return false,
        }
    }
    false
}

fn last(path: &str, root: &str) -> String {
    if path == root {
        return root.to_owned();
    }
    path.rsplit('.').next().unwrap_or(path).to_owned()
}

fn parent_of(path: &str) -> &str {
    path.rsplit_once('.').map_or(path, |(parent, _)| parent)
}

fn wbs(root: &str, cur: &Index<'_>, want: &Index<'_>, phases: &mut Phases, diff: &mut Difference) {
    let cur_paths: BTreeMap<&str, &str> = cur.wbs.iter().map(|(p, n, _)| (*p, *n)).collect();
    let want_paths: BTreeSet<&str> = want.wbs.iter().map(|(p, _, _)| *p).collect();
    let mut by_code: BTreeMap<String, Vec<&str>> = BTreeMap::new();
    for (path, _, _) in &cur.wbs {
        by_code.entry(last(path, root)).or_default().push(path);
    }
    let mut lines = want.wbs.clone();
    lines.sort_by_key(|(p, _, _)| p.split('.').count());
    let mut moved: BTreeSet<String> = BTreeSet::new();
    // Nodes moved so far, old path to new: what's under them moves too.
    let mut shifted: Vec<(String, String)> = Vec::new();
    for (path, name, at) in lines {
        let code = last(path, root);
        let parent = last(parent_of(path), root);
        let was = shifted.iter().find_map(|(old, new)| {
            path.strip_prefix(new.as_str())
                .filter(|rest| rest.starts_with('.'))
                .map(|rest| format!("{old}{rest}"))
        });
        if let Some(was) = was
            && let Some(now) = cur_paths.get(was.as_str())
        {
            if *now != name {
                phases.wbs.push(place(
                    Edit::RenameWbs {
                        wbs: code.clone(),
                        code: None,
                        name: Some(name.to_owned()),
                    },
                    at,
                ));
            }
            moved.insert(was);
            continue;
        }
        let rename = |phases: &mut Phases| {
            phases.wbs.push(place(
                Edit::RenameWbs {
                    wbs: code.clone(),
                    code: None,
                    name: Some(name.to_owned()),
                },
                at,
            ));
        };
        if let Some(now) = cur_paths.get(path) {
            if *now != name {
                rename(phases);
            }
            continue;
        }
        // A node moved: the only one with its code, and gone from its path.
        let from = match by_code.get(&code).map(Vec::as_slice) {
            Some([old]) if !want_paths.contains(old) => Some(*old),
            _ => None,
        };
        if let Some(old) = from {
            moved.insert(old.to_owned());
            shifted.push((old.to_owned(), path.to_owned()));
            phases.wbs.push(place(
                Edit::MoveWbs {
                    wbs: code.clone(),
                    parent,
                },
                at,
            ));
            if cur_paths.get(old) != Some(&name) {
                rename(phases);
            }
            continue;
        }
        phases.wbs.push(place(
            Edit::AddWbs {
                code: code.clone(),
                name: name.to_owned(),
                parent,
            },
            at,
        ));
    }
    for (path, _, _) in &cur.wbs {
        if !want_paths.contains(path) && !moved.contains(*path) {
            diff.missing.push(Missing {
                kind: "wbs",
                name: (*path).to_owned(),
            });
        }
    }
}

struct Diffing<'a> {
    schedule: &'a Schedule,
    /// The schedule's project and calendars with the hours per day the text
    /// gives them, for reading the text's spans.
    days: &'a Schedule,
    models: &'a BTreeMap<&'a str, &'a Model>,
    cur: &'a Index<'a>,
    want: &'a Index<'a>,
    root: &'a str,
}

/// A wanted activity's details, gathered.
#[derive(Default)]
struct Wanted<'d> {
    calendar: Option<&'d str>,
    resource_type: bool,
    remaining: Option<Span>,
    primary: Option<(ConstraintKind, Option<DateTime>)>,
    secondary: Option<(ConstraintKind, Option<DateTime>)>,
    codes: BTreeMap<String, String>,
    /// Field values by lowered field name: the name as written, and the value.
    udfs: BTreeMap<String, (String, String)>,
    resources: BTreeMap<String, Option<String>>,
    roles: BTreeMap<String, Option<String>>,
    expenses: Expenses<'d>,
    start: Option<DateTime>,
    finish: Option<DateTime>,
    percent: Option<f64>,
    expected: Option<DateTime>,
    at: BTreeMap<&'static str, At>,
}

/// Takes off the resources, or roles when `role`, whose line changed or went,
/// and puts on those whose line changed or came, by short name and units.
fn restaff(code: &str, (was, is): (&Staff, &Staff), role: bool, at: At, target: &mut Vec<Placed>) {
    let activity = || code.to_owned();
    for (name, units) in was {
        if is.get(name) != Some(units) {
            let edit = if role {
                Edit::UnassignRole {
                    activity: activity(),
                    role: name.clone(),
                }
            } else {
                Edit::UnassignResource {
                    activity: activity(),
                    resource: name.clone(),
                }
            };
            target.push(place(edit, at));
        }
    }
    for (name, units) in is {
        if was.get(name) != Some(units) {
            let units = units.as_ref().and_then(|u| u.parse().ok());
            let edit = if role {
                Edit::AssignRole {
                    activity: activity(),
                    role: name.clone(),
                    units,
                }
            } else {
                Edit::AssignResource {
                    activity: activity(),
                    resource: name.clone(),
                    units,
                }
            };
            target.push(place(edit, at));
        }
    }
}

/// Resources or roles by short name, with their units as written.
type Staff = BTreeMap<String, Option<String>>;

/// Expense lines by item, as edits match them, in the order written.
type Expenses<'d> = BTreeMap<String, Vec<&'d crate::Expense>>;

/// An activity's codes, resources, roles and expenses, as the text writes them.
type Assigned<'d> = (BTreeMap<String, String>, Staff, Staff, Expenses<'d>);

/// Deletes the expenses whose line went, adds those whose line came, and
/// sets those whose line changed, by item. An item on an activity twice is
/// left to the edits, which refuse to tell the two apart: a line more is
/// added, which refuses as a duplicate, and otherwise the first is set, which
/// refuses as ambiguous.
fn reprice(
    code: &str,
    (was, is): (&Expenses<'_>, &Expenses<'_>),
    at: At,
    target: &mut Vec<Placed>,
) {
    let meant = |lines: &[&crate::Expense]| lines.iter().map(|x| x.meant()).collect::<Vec<_>>();
    for (item, lines) in was {
        if !is.contains_key(item) {
            let expense = lines[0].item.value.clone();
            let activity = code.to_owned();
            target.push(place(Edit::DeleteExpense { activity, expense }, at));
        }
    }
    for (item, lines) in is {
        let Some(before) = was.get(item) else {
            for x in lines {
                for edit in crate::compile::expense_edits(code, x) {
                    target.push(place(edit, x.item.at));
                }
            }
            continue;
        };
        if meant(before) == meant(lines) {
            continue;
        }
        if let Some(more) = lines.get(before.len()..).filter(|more| !more.is_empty()) {
            for x in more {
                for edit in crate::compile::expense_edits(code, x) {
                    target.push(place(edit, x.item.at));
                }
            }
            continue;
        }
        let x = lines[0];
        let (budget, actual, remaining, accrual) = x.meant();
        let (budgeted, _, _, accrued) = before[0].meant();
        // A new budget moves the price, so it's given only when it changed.
        #[allow(clippy::float_cmp, reason = "a line's costs are exact")]
        let moved = budget != budgeted;
        target.push(place(
            Edit::SetExpense {
                activity: code.to_owned(),
                expense: before[0].item.value.clone(),
                budgeted_cost: moved.then_some(budget),
                actual_cost: Some(actual),
                remaining_cost: Some(remaining),
                accrual: (accrual != accrued).then_some(accrual),
            },
            x.item.at,
        ));
    }
}

fn wanted(statement: &Statement<Activity, ActivityDetail>) -> Wanted<'_> {
    let mut w = Wanted::default();
    for (detail, at) in details(&statement.details) {
        let key = match detail {
            ActivityDetail::Calendar(name) => {
                w.calendar = Some(&name.value);
                "calendar"
            }
            ActivityDetail::Type(kind) => {
                w.resource_type = *kind == TypeKind::Resource;
                "type"
            }
            ActivityDetail::Remaining(span) => {
                w.remaining = Some(span.value);
                "remaining"
            }
            ActivityDetail::Constraint {
                secondary,
                kind,
                date,
            } => {
                let value = Some((*kind, date.as_ref().map(|d| d.value)));
                if *secondary {
                    w.secondary = value;
                    "secondary"
                } else {
                    w.primary = value;
                    "primary"
                }
            }
            ActivityDetail::Code { code_type, value } => {
                w.codes.insert(code_type.value.clone(), value.value.clone());
                "code"
            }
            ActivityDetail::Udf { field, value } => {
                w.udfs.insert(
                    field.value.to_lowercase(),
                    (field.value.clone(), value.value.clone()),
                );
                "udf"
            }
            ActivityDetail::Resource { resource, units } => {
                w.resources.insert(
                    resource.value.clone(),
                    units.as_ref().map(|u| u.value.to_string()),
                );
                "resource"
            }
            ActivityDetail::Role { role, units } => {
                w.roles.insert(
                    role.value.clone(),
                    units.as_ref().map(|u| u.value.to_string()),
                );
                "role"
            }
            ActivityDetail::Expense(x) => {
                w.expenses
                    .entry(totalfloat_edit::item_key(&x.item.value))
                    .or_default()
                    .push(x);
                "expense"
            }
            ActivityDetail::ActualStart(d) => {
                w.start = Some(d.value);
                "progress"
            }
            ActivityDetail::ActualFinish(d) => {
                w.finish = Some(d.value);
                "progress"
            }
            ActivityDetail::Percent(p) => {
                w.percent = Some(p.value);
                "progress"
            }
            ActivityDetail::ExpectedFinish(d) => {
                w.expected = Some(d.value);
                "expected"
            }
        };
        w.at.entry(key).or_insert(at);
    }
    w
}

impl Diffing<'_> {
    /// Minutes a span in the text means on the named calendar.
    fn minutes(&self, span: Span, calendar: Option<CalendarId>) -> Option<Minutes> {
        span.minutes(calendar.and_then(|id| day_length(self.days, id)))
    }

    /// The calendar a wanted activity is on: by name, and when the name is
    /// its current calendar's, that calendar, even if another shares it.
    fn calendar_for(&self, w: &Wanted<'_>, model: Option<&Model>) -> Option<CalendarId> {
        let default = self.schedule.project.default_calendar;
        let name = match w.calendar {
            Some(name) => name.to_owned(),
            None => default
                .and_then(|id| self.schedule.calendars.get(&id))?
                .name
                .clone(),
        };
        if let Some(model) = model
            && self
                .schedule
                .calendars
                .get(&model.calendar)
                .is_some_and(|c| c.name.eq_ignore_ascii_case(&name))
        {
            return Some(model.calendar);
        }
        match w.calendar {
            Some(name) => calendar_named(self.schedule, name),
            None => default,
        }
    }

    fn activity(&self, code: &str, phases: &mut Phases) {
        let (statement, path) = &self.want.activities[code];
        let w = wanted(statement);
        let head = &statement.head;
        let kind = match head.kind {
            ActivityKind::Milestone => Kind::FinishMilestone,
            ActivityKind::StartMilestone => Kind::StartMilestone,
            ActivityKind::Activity if w.resource_type => Kind::Resource,
            ActivityKind::Activity => Kind::Task,
        };
        let Some(model) = self.models.get(code) else {
            phases.added.push(place(
                Edit::AddActivity {
                    code: code.to_owned(),
                    name: head.name.value.clone(),
                    wbs: last(path, self.root),
                    activity_type: Some(kind),
                    calendar: w.calendar.map(str::to_owned),
                    duration: head.duration.as_ref().map(|d| d.value),
                },
                statement.at,
            ));
            self.details(code, &w, None, phases);
            return;
        };
        let at = statement.at;
        let mut change = |edit: Edit, at: At| phases.changed.push(place(edit, at));
        if head.name.value != model.name {
            change(
                Edit::SetName {
                    activity: code.to_owned(),
                    name: head.name.value.clone(),
                },
                head.name.at,
            );
        }
        let now = match model.activity_type {
            ActivityType::StartMilestone => Some(Kind::StartMilestone),
            ActivityType::FinishMilestone => Some(Kind::FinishMilestone),
            ActivityType::ResourceDependent => Some(Kind::Resource),
            ActivityType::TaskDependent => Some(Kind::Task),
            // Written as tasks: only an explicit other type changes them.
            ActivityType::LevelOfEffort | ActivityType::WbsSummary => {
                (kind != Kind::Task).then_some(Kind::Task)
            }
        };
        if now.is_some_and(|now| now != kind) {
            change(
                Edit::SetType {
                    activity: code.to_owned(),
                    activity_type: kind,
                    duration: head.duration.as_ref().map(|d| d.value),
                },
                at,
            );
        }
        let calendar = self.calendar_for(&w, Some(model));
        if calendar.is_some_and(|id| id != model.calendar)
            || (calendar.is_none() && w.calendar.is_some())
        {
            let name = w.calendar.map(str::to_owned).or_else(|| {
                self.schedule
                    .project
                    .default_calendar
                    .and_then(|id| self.schedule.calendars.get(&id))
                    .map(|c| c.name.clone())
            });
            if let Some(name) = name {
                change(
                    Edit::SetCalendar {
                        activity: code.to_owned(),
                        calendar: name,
                    },
                    w.at.get("calendar").copied().unwrap_or(at),
                );
            }
        }
        if let Some(duration) = &head.duration
            && self.minutes(duration.value, calendar) != Some(model.original_duration)
        {
            change(
                Edit::SetDuration {
                    activity: code.to_owned(),
                    original: Some(duration.value),
                    remaining: None,
                },
                duration.at,
            );
        }
        let (_, now_path) = &self.cur.activities[code];
        if now_path != path {
            change(
                Edit::SetWbs {
                    activity: code.to_owned(),
                    wbs: last(path, self.root),
                },
                at,
            );
        }
        self.details(code, &w, Some(model), phases);
    }

    /// Constraints, codes, resources, progress and expected finish, against
    /// what the activity has now (nothing, for a new one).
    fn details(&self, code: &str, w: &Wanted<'_>, model: Option<&Model>, phases: &mut Phases) {
        let target = if model.is_some() {
            &mut phases.changed
        } else {
            &mut phases.added
        };
        let at = |key: &str| w.at.get(key).copied().unwrap_or_default();
        let now =
            |c: Option<totalfloat_model::Constraint>| c.map(|c| (constraint_kind(c.kind), c.date));
        for (secondary, wanted, current, key) in [
            (
                false,
                w.primary,
                model.and_then(|m| now(m.primary_constraint)),
                "primary",
            ),
            (
                true,
                w.secondary,
                model.and_then(|m| now(m.secondary_constraint)),
                "secondary",
            ),
        ] {
            match (wanted, current) {
                (Some(want), now) if Some(want) != now => target.push(place(
                    Edit::SetConstraint {
                        activity: code.to_owned(),
                        constraint: want.0,
                        date: want.1.map(|d| d.to_string()),
                        secondary,
                    },
                    at(key),
                )),
                (None, Some(_)) => target.push(place(
                    Edit::ClearConstraint {
                        activity: code.to_owned(),
                        secondary,
                    },
                    at(key),
                )),
                _ => {}
            }
        }
        self.udf_values(code, w, at("udf"), target);
        let (codes, resources, roles, expenses) = self.current_assignments(code);
        for (code_type, value) in &w.codes {
            if codes.get(code_type) != Some(value) {
                target.push(place(
                    Edit::SetCode {
                        activity: code.to_owned(),
                        code_type: code_type.clone(),
                        value: value.clone(),
                    },
                    at("code"),
                ));
            }
        }
        for code_type in codes.keys().filter(|t| !w.codes.contains_key(*t)) {
            target.push(place(
                Edit::ClearCode {
                    activity: code.to_owned(),
                    code_type: code_type.clone(),
                },
                at("code"),
            ));
        }
        restaff(
            code,
            (&resources, &w.resources),
            false,
            at("resource"),
            target,
        );
        restaff(code, (&roles, &w.roles), true, at("role"), target);
        reprice(code, (&expenses, &w.expenses), at("expense"), target);
        self.progress(code, w, model, target);
    }

    /// Sets the field values the text changes, and clears those it leaves out.
    fn udf_values(&self, code: &str, w: &Wanted<'_>, at: At, target: &mut Vec<Placed>) {
        let held = self
            .cur
            .activities
            .get(code)
            .map(|(statement, _)| wanted(statement).udfs)
            .unwrap_or_default();
        for (key, (field, value)) in &w.udfs {
            if held.get(key).map(|(_, v)| v) != Some(value) {
                target.push(place(
                    Edit::SetUdf {
                        field: field.clone(),
                        value: value.clone(),
                        activity: Some(code.to_owned()),
                        wbs: None,
                        resource: None,
                    },
                    at,
                ));
            }
        }
        for (key, (field, _)) in &held {
            if !w.udfs.contains_key(key) {
                target.push(place(
                    Edit::ClearUdf {
                        field: field.clone(),
                        activity: Some(code.to_owned()),
                        wbs: None,
                        resource: None,
                    },
                    at,
                ));
            }
        }
    }

    /// The activity's codes, resources and roles as the text writes them now.
    fn current_assignments(&self, code: &str) -> Assigned<'_> {
        match self.cur.activities.get(code) {
            Some((statement, _)) => {
                let w = wanted(statement);
                (w.codes, w.resources, w.roles, w.expenses)
            }
            None => Assigned::default(),
        }
    }

    fn progress(
        &self,
        code: &str,
        w: &Wanted<'_>,
        model: Option<&Model>,
        target: &mut Vec<Placed>,
    ) {
        let at = |key: &str| w.at.get(key).copied().unwrap_or_default();
        let calendar = self.calendar_for(w, model);
        let wanted_progress = (w.start, w.finish, w.percent.filter(|p| *p > 0.0));
        let current_progress = model.map_or((None, None, None), |m| {
            (
                m.actual_start,
                m.actual_finish,
                m.physical_percent_complete.filter(|p| *p > 0.0),
            )
        });
        let remaining_changed = w.remaining.is_some_and(|span| {
            model.is_none_or(|m| self.minutes(span, calendar) != Some(m.remaining_duration))
        });
        let progress_changed = wanted_progress != current_progress;
        if wanted_progress == (None, None, None) && current_progress != (None, None, None) {
            target.push(place(
                Edit::ClearProgress {
                    activity: code.to_owned(),
                },
                at("progress"),
            ));
        }
        if wanted_progress != (None, None, None) && (progress_changed || remaining_changed) {
            target.push(place(
                Edit::SetProgress {
                    activity: code.to_owned(),
                    actual_start: w.start.map(|d| d.to_string()),
                    actual_finish: w.finish.map(|d| d.to_string()),
                    remaining: w.remaining,
                    percent: w.percent,
                },
                at("progress"),
            ));
        } else if remaining_changed && wanted_progress == (None, None, None) {
            target.push(place(
                Edit::SetDuration {
                    activity: code.to_owned(),
                    original: None,
                    remaining: w.remaining,
                },
                at("remaining"),
            ));
        }
        if w.expected != model.and_then(|m| m.expected_finish) {
            target.push(place(
                Edit::SetExpectedFinish {
                    activity: code.to_owned(),
                    date: w.expected.map(|d| d.to_string()),
                },
                at("expected"),
            ));
        }
    }

    fn links(
        &self,
        code: &str,
        c: &mut Compiler<'_>,
        phases: &mut Phases,
        diff: &mut Difference,
        leaving: Leaving,
    ) {
        let (statement, _) = &self.want.activities[code];
        let wanted: Vec<&Predecessor> = statement.head.after.iter().collect();
        let current: Vec<&Predecessor> = self
            .cur
            .activities
            .get(code)
            .map(|(s, _)| s.head.after.iter().collect())
            .unwrap_or_default();
        let known = |p: &Predecessor| {
            self.want.activities.contains_key(p.code.value.as_str())
                || self.models.contains_key(p.code.value.as_str())
        };
        let mut unmatched: Vec<&Predecessor> = Vec::new();
        let mut current_left: Vec<&Predecessor> = current.clone();
        for p in &wanted {
            if !known(p) {
                let names: Vec<&str> = self.want.order.clone();
                c.unknown("unknown-activity", "activity", &p.code, &names);
                continue;
            }
            match current_left
                .iter()
                .position(|q| q.code.value == p.code.value && link_of(q) == link_of(p))
            {
                Some(i) => {
                    let q = current_left.remove(i);
                    if !self.same_lag(p, q, code) {
                        phases.links.push(change_link(p, q, code));
                    }
                }
                None => unmatched.push(p),
            }
        }
        for p in unmatched {
            // One link left on each side between the same pair: a change of
            // type, not a removal and an add.
            let pair: Vec<usize> = current_left
                .iter()
                .enumerate()
                .filter(|(_, q)| q.code.value == p.code.value)
                .map(|(i, _)| i)
                .collect();
            if let [i] = pair.as_slice() {
                let q = current_left.remove(*i);
                phases.links.push(change_link(p, q, code));
                continue;
            }
            phases.links.push(place(
                Edit::AddLink {
                    predecessor: p.code.value.clone(),
                    successor: code.to_owned(),
                    link_type: p.link,
                    lag: p.lag,
                },
                p.code.at,
            ));
        }
        for q in current_left {
            if !self.want.activities.contains_key(q.code.value.as_str()) {
                // Its predecessor is left out too, and the link goes, or
                // stays, with it.
                continue;
            }
            diff.missing.push(Missing {
                kind: "link",
                name: format!("{}>{code}", q.code.value),
            });
            if leaving == Leaving::Delete {
                phases.unlinked.push(place(
                    Edit::RemoveLink {
                        predecessor: q.code.value.clone(),
                        successor: code.to_owned(),
                        link_type: Some(link_of(q)),
                    },
                    statement.at,
                ));
            }
        }
    }

    /// Minutes a link's lag means: in the text, on the calendars as the text
    /// leaves them, or as the schedule has it now.
    fn lag_minutes(&self, p: &Predecessor, code: &str, days: &Schedule) -> Option<Minutes> {
        let lag = p.lag?;
        let before = self.models.get(p.code.value.as_str())?;
        let after = self.models.get(code)?;
        lag.minutes(lag_day_length(days, before, after))
    }

    fn same_lag(&self, p: &Predecessor, q: &Predecessor, code: &str) -> bool {
        let zero = Some(Minutes::ZERO);
        let wanted = if p.lag.is_some() {
            self.lag_minutes(p, code, self.days)
        } else {
            zero
        };
        let current = if q.lag.is_some() {
            self.lag_minutes(q, code, self.schedule)
        } else {
            zero
        };
        wanted.is_some() && wanted == current
    }

    /// Activities the text leaves out, and the warnings about ones that
    /// look renamed.
    fn left_out(&self, phases: &mut Phases, diff: &mut Difference, leaving: Leaving) {
        let added: Vec<&str> = self
            .want
            .order
            .iter()
            .filter(|code| !self.models.contains_key(*code))
            .copied()
            .collect();
        for code in self
            .cur
            .order
            .iter()
            .filter(|c| !self.want.activities.contains_key(*c))
        {
            diff.missing.push(Missing {
                kind: "activity",
                name: (*code).to_owned(),
            });
            if leaving != Leaving::Delete {
                continue;
            }
            phases.deleted.push(place(
                Edit::DeleteActivity {
                    activity: (*code).to_owned(),
                },
                At::default(),
            ));
            let (gone, gone_path) = &self.cur.activities[code];
            for new in &added {
                let (made, made_path) = &self.want.activities[new];
                if made.head.name.value == gone.head.name.value
                    && made_path == gone_path
                    && made.head.duration.as_ref().map(|d| d.value)
                        == gone.head.duration.as_ref().map(|d| d.value)
                {
                    diff.warnings.push(format!(
                        "{code} is deleted and {new} added with the same name, WBS and duration; if you meant to rename it, there's no command for a new Activity ID yet, so its history and links go with {code}"
                    ));
                }
            }
        }
    }
}

fn link_of(p: &Predecessor) -> LinkType {
    p.link.unwrap_or(LinkType::FS)
}

fn change_link(p: &Predecessor, q: &Predecessor, code: &str) -> Placed {
    place(
        Edit::ChangeLink {
            predecessor: p.code.value.clone(),
            successor: code.to_owned(),
            link_type: Some(link_of(q)),
            new_type: Some(link_of(p)),
            lag: Some(p.lag.unwrap_or(Span::ZERO)),
        },
        p.code.at,
    )
}
