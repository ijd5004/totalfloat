//! Writing a schedule out as a [`Document`] (ARCHITECTURE.md, Schedule as text): what the format
//! holds, in a fixed order, so the same schedule always gives the same text.

use std::collections::{BTreeMap, BTreeSet};

use totalfloat_edit::{day_length, even_day, lag_day_length};
use totalfloat_model::{
    Activity, ActivityId, ActivityStatus, ActivityType, Assignment, Calendar, CalendarId,
    ConstraintType, DurationType, Minutes, PercentCompleteType, Relationship, RelationshipType,
    ResourceType, RoleId, Schedule, ScheduleOptions, WbsId, Weekday,
};

use crate::parse::DAYS;
use crate::{
    Activity as Line, ActivityDetail, ActivityKind, Calendar as CalendarHead, CalendarDetail,
    CodeType, CodeValue, ConstraintKind, Detail, Document, ExpenseAccrual, Item, LinkType,
    Predecessor, Project, ProjectDetail, Resource as ResourceHead, ResourceDetail, ResourceKind,
    Role as RoleHead, RoleDetail, Span, Statement, TypeKind, Value, Wbs,
};

/// What the format couldn't hold, by kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeftOut {
    /// Stable code, such as `udfs`.
    pub kind: &'static str,
    /// What was left out, in words.
    pub message: &'static str,
    /// How many.
    pub count: usize,
    /// The first few activity IDs or names.
    pub examples: Vec<String>,
}

/// A schedule as text, with what was left out.
#[derive(Debug, Clone, PartialEq)]
pub struct Rendered {
    /// The document.
    pub document: Document,
    /// What the format couldn't hold, in the order first met.
    pub left_out: Vec<LeftOut>,
}

const EXAMPLES: usize = 10;

#[derive(Default)]
struct Notes(Vec<LeftOut>);

impl Notes {
    fn add(&mut self, kind: &'static str, message: &'static str, example: &str) {
        let index = self
            .0
            .iter()
            .position(|n| n.kind == kind)
            .unwrap_or_else(|| {
                self.0.push(LeftOut {
                    kind,
                    message,
                    count: 0,
                    examples: Vec::new(),
                });
                self.0.len() - 1
            });
        let note = &mut self.0[index];
        note.count += 1;
        if note.examples.len() < EXAMPLES {
            note.examples.push(example.to_owned());
        }
    }
}

/// What every activity's lines look things up in, gathered once so writing
/// a large schedule stays linear.
struct Context<'a> {
    schedule: &'a Schedule,
    /// The name written for each calendar in use: a calendar that shares
    /// its name with another is written as the first.
    calendar_of: BTreeMap<CalendarId, String>,
    /// The project's calendar, as written.
    default: Option<String>,
    links_into: BTreeMap<ActivityId, Vec<&'a Relationship>>,
    assigned: BTreeMap<ActivityId, Vec<&'a Assignment>>,
    expenses: BTreeMap<ActivityId, Vec<&'a totalfloat_model::Expense>>,
}

impl<'a> Context<'a> {
    fn new(schedule: &'a Schedule, calendars: &BTreeMap<String, &Calendar>) -> Self {
        let calendar_of: BTreeMap<CalendarId, String> = schedule
            .calendars
            .values()
            .filter_map(|c| Some((c.id, calendars.get(&c.name.to_lowercase())?.name.clone())))
            .collect();
        let default = schedule
            .project
            .default_calendar
            .and_then(|id| calendar_of.get(&id).cloned());
        let mut links_into: BTreeMap<ActivityId, Vec<&Relationship>> = BTreeMap::new();
        for link in schedule.relationships.values() {
            links_into.entry(link.successor).or_default().push(link);
        }
        let mut assigned: BTreeMap<ActivityId, Vec<&Assignment>> = BTreeMap::new();
        for assignment in schedule.assignments.values() {
            assigned
                .entry(assignment.activity)
                .or_default()
                .push(assignment);
        }
        let mut expenses: BTreeMap<ActivityId, Vec<&totalfloat_model::Expense>> = BTreeMap::new();
        for x in schedule.expenses.values() {
            expenses.entry(x.activity).or_default().push(x);
        }
        Self {
            schedule,
            calendar_of,
            default,
            links_into,
            assigned,
            expenses,
        }
    }
}

fn word(text: &str) -> Value<String> {
    Value::new(text.to_owned())
}

fn line<D>(value: D) -> Detail<D> {
    Detail::new(value)
}

/// A span in whole or half days of the calendar's day, which a schedule
/// built from the text has too, otherwise in hours, so a lag of an hour
/// reads `1h`, not `0.125d`; `None` past what a span can hold.
fn span(minutes: Minutes, day: Option<Minutes>) -> Option<Span> {
    match day {
        Some(day) if day.get() > 0 && (minutes.get() * 2) % day.get() == 0 => {
            Span::days_of(minutes, day).or_else(|| Span::hours_of(minutes))
        }
        _ => Span::hours_of(minutes),
    }
}

/// [`span`], noting one too long to write.
fn written_span(
    minutes: Minutes,
    day: Option<Minutes>,
    code: &str,
    notes: &mut Notes,
) -> Option<Span> {
    let written = span(minutes, day);
    if written.is_none() {
        notes.add(
            "span-too-long",
            "durations and lags past a million hours",
            code,
        );
    }
    written
}

/// Writes one project's schedule as text.
#[must_use]
pub fn render(schedule: &Schedule) -> Rendered {
    let mut notes = Notes::default();
    let mut items = Vec::new();
    let calendars = used_calendars(schedule, &mut notes);
    items.push(project(schedule, &calendars, &mut notes));
    for calendar in calendars.values() {
        items.push(calendar_item(calendar));
    }
    items.extend(code_types(schedule, &mut notes));
    items.extend(field_items(schedule, &mut notes));
    let context = Context::new(schedule, &calendars);
    items.extend(resource_items(&context, &mut notes));
    items.extend(role_items(schedule, &mut notes));
    let (written, homes) = wbs_nodes(schedule, &mut notes);
    let mut by_node: BTreeMap<Option<WbsId>, Vec<&Activity>> = BTreeMap::new();
    for activity in schedule.activities.values() {
        let home = homes.get(&activity.wbs).copied().flatten();
        by_node.entry(home).or_default().push(activity);
    }
    for list in by_node.values_mut() {
        list.sort_by(|a, b| a.code.cmp(&b.code));
    }
    let mut write = |items: &mut Vec<Item>, node: Option<WbsId>| {
        for activity in by_node.get(&node).into_iter().flatten() {
            items.push(activity_item(&context, activity, &mut notes));
        }
    };
    write(&mut items, None);
    for (id, path, name) in &written {
        items.push(Item::Wbs(Statement::new(Wbs {
            path: word(path),
            name: word(name),
        })));
        write(&mut items, Some(*id));
    }
    other_notes(schedule, &mut notes);
    Rendered {
        document: Document {
            comment: None,
            items,
        },
        left_out: notes.0,
    }
}

/// The calendars the project and its activities use, by name; the first of
/// two with the same name is kept.
fn used_calendars<'a>(schedule: &'a Schedule, notes: &mut Notes) -> BTreeMap<String, &'a Calendar> {
    let mut ids: BTreeSet<CalendarId> = schedule.activities.values().map(|a| a.calendar).collect();
    ids.extend(schedule.project.default_calendar);
    ids.extend(schedule.resources.values().filter_map(|r| r.calendar));
    let mut by_name: BTreeMap<String, &Calendar> = BTreeMap::new();
    for id in ids {
        let Some(calendar) = schedule.calendars.get(&id) else {
            continue;
        };
        let key = calendar.name.to_lowercase();
        if let Some(kept) = by_name.get(&key) {
            if kept.id != calendar.id {
                notes.add(
                    "calendar-name-repeated",
                    "calendars that share a name with another; activities on them use the first",
                    &calendar.name,
                );
            }
            continue;
        }
        if day_length(schedule, calendar.id).is_none() {
            notes.add(
                "calendar-day-length",
                "calendars with no hours per day and working days of different lengths: spans on them are written in hours, and a schedule built from the text counts a day there as the project calendar does",
                &calendar.name,
            );
        }
        by_name.insert(key, calendar);
    }
    by_name
}

fn project(
    schedule: &Schedule,
    calendars: &BTreeMap<String, &Calendar>,
    notes: &mut Notes,
) -> Item {
    let p = &schedule.project;
    let mut statement = Statement::new(Project {
        code: word(&p.short_name),
        name: word(p.name.as_deref().unwrap_or(&p.short_name)),
    });
    if let Some(date) = p.data_date {
        statement
            .details
            .push(line(ProjectDetail::DataDate(Value::new(date))));
    }
    if let Some(date) = p.must_finish_by {
        statement
            .details
            .push(line(ProjectDetail::MustFinish(Value::new(date))));
    }
    let default = p
        .default_calendar
        .and_then(|id| schedule.calendars.get(&id))
        .and_then(|c| calendars.get(&c.name.to_lowercase()));
    if let Some(calendar) = default {
        statement
            .details
            .push(line(ProjectDetail::Calendar(word(&calendar.name))));
    }
    if p.options != ScheduleOptions::default() {
        notes.add(
            "schedule-options",
            "scheduling options that differ from the defaults; a schedule built from the text uses the defaults",
            &p.short_name,
        );
    }
    Item::Project(statement)
}

fn weekday(index: usize) -> Weekday {
    DAYS[index].1.into()
}

/// Shifts as the text writes them, such as `08:00-12:00`.
pub(crate) fn shift_texts(intervals: &[totalfloat_model::WorkInterval]) -> Vec<String> {
    intervals
        .iter()
        .map(|i| format!("{}-{}", i.start, i.finish))
        .collect()
}

fn shifts(intervals: &[totalfloat_model::WorkInterval]) -> Vec<Value<String>> {
    shift_texts(intervals).into_iter().map(Value::new).collect()
}

fn calendar_item(calendar: &Calendar) -> Item {
    let mut statement = Statement::new(CalendarHead {
        name: word(&calendar.name),
        copy_of: None,
    });
    // Built from text, a calendar's day is its working day when every one
    // is the same length, so the line is needed only where it isn't.
    if let Some(day) = calendar.hours_per_period.day
        && even_day(calendar) != Some(day)
    {
        #[allow(clippy::cast_precision_loss, reason = "a day's minutes are small")]
        let hours = day.get() as f64 / 60.0;
        statement
            .details
            .push(line(CalendarDetail::HoursPerDay(Value::new(hours))));
    }
    // Runs of days with the same shifts, Monday first.
    let mut start = 0;
    while start < 7 {
        let day = calendar.work_week.day(weekday(start));
        let mut end = start;
        while end + 1 < 7 && calendar.work_week.day(weekday(end + 1)) == day {
            end += 1;
        }
        statement.details.push(line(CalendarDetail::Week {
            from: DAYS[start].1,
            to: DAYS[end].1,
            shifts: shifts(day),
        }));
        start = end + 1;
    }
    for (date, intervals) in &calendar.exceptions {
        statement.details.push(line(if intervals.is_empty() {
            CalendarDetail::Holiday(Value::new(*date))
        } else {
            CalendarDetail::Exception {
                date: Value::new(*date),
                shifts: shifts(intervals),
            }
        }));
    }
    Item::Calendar(statement)
}

fn code_types(schedule: &Schedule, notes: &mut Notes) -> Vec<Item> {
    let mut types: Vec<_> = schedule.activity_code_types.values().collect();
    types.sort_by(|a, b| (&a.name, a.id).cmp(&(&b.name, b.id)));
    let mut items = Vec::new();
    let mut written = BTreeSet::new();
    for code_type in types {
        if !written.insert(code_type.name.to_lowercase()) {
            notes.add(
                "code-type-name-repeated",
                "code types that share a name with another; only the first is written",
                &code_type.name,
            );
            continue;
        }
        let mut statement = Statement::new(CodeType {
            name: word(&code_type.name),
        });
        let mut values: Vec<_> = schedule
            .activity_codes
            .values()
            .filter(|v| v.code_type == code_type.id)
            .collect();
        values.sort_by(|a, b| (a.sequence, &a.value).cmp(&(b.sequence, &b.value)));
        for value in values {
            if value.parent.is_some() {
                notes.add(
                    "code-value-hierarchy",
                    "code values under other values; they're written as plain values",
                    &value.value,
                );
            }
            statement.details.push(line(CodeValue {
                value: word(&value.value),
                description: (!value.description.is_empty()).then(|| word(&value.description)),
            }));
        }
        items.push(Item::CodeType(statement));
    }
    items
}

/// Every role, by short name, each under its parent once the parent is
/// written, so a parent is always defined above the roles under it.
fn role_items(schedule: &Schedule, notes: &mut Notes) -> Vec<Item> {
    let mut roles: Vec<_> = schedule.roles.values().collect();
    roles.sort_by(|a, b| (&a.short_name, a.id).cmp(&(&b.short_name, b.id)));
    let mut written: BTreeSet<RoleId> = BTreeSet::new();
    let mut items = Vec::new();
    // Each pass writes the roles whose parent is written or absent; a loop of
    // parents, which a file could hold, is written last without its parent.
    while written.len() < roles.len() {
        let ready: Vec<_> = roles
            .iter()
            .filter(|r| !written.contains(&r.id))
            .filter(|r| {
                r.parent
                    .is_none_or(|p| written.contains(&p) || !schedule.roles.contains_key(&p))
            })
            .copied()
            .collect();
        let pass: Vec<_> = if ready.is_empty() {
            let first = roles.iter().find(|r| !written.contains(&r.id)).copied();
            if let Some(role) = first {
                notes.add(
                    "role-parent-loops",
                    "roles in a loop of parents; one is written without its parent",
                    &role.short_name,
                );
            }
            first.into_iter().collect()
        } else {
            ready
        };
        for role in pass {
            written.insert(role.id);
            let mut statement = Statement::new(RoleHead {
                code: word(&role.short_name),
                name: word(&role.name),
            });
            if let Some(parent) = role
                .parent
                .filter(|p| written.contains(p) && *p != role.id)
                .and_then(|p| schedule.roles.get(&p))
            {
                statement
                    .details
                    .push(line(RoleDetail::Parent(word(&parent.short_name))));
            }
            if let Some(limit) = one_limit(&role.rates, |r| r.max_units_per_hour) {
                statement
                    .details
                    .push(line(RoleDetail::MaxUnitsPerHour(Value::new(limit))));
            }
            items.push(Item::Role(statement));
        }
    }
    items
}

/// Every resource, by short name.
fn resource_items(context: &Context<'_>, notes: &mut Notes) -> Vec<Item> {
    let mut resources: Vec<_> = context.schedule.resources.values().collect();
    resources.sort_by(|a, b| (&a.short_name, a.id).cmp(&(&b.short_name, b.id)));
    resources
        .into_iter()
        .map(|resource| {
            let mut statement = Statement::new(ResourceHead {
                code: word(&resource.short_name),
                name: word(&resource.name),
                kind: Some(Value::new(match resource.kind {
                    ResourceType::Labor => ResourceKind::Labor,
                    ResourceType::Nonlabor => ResourceKind::Nonlabor,
                    ResourceType::Material => ResourceKind::Material,
                })),
            });
            if let Some(name) = resource
                .calendar
                .and_then(|id| context.calendar_of.get(&id))
            {
                if context.default.as_ref() != Some(name) {
                    notes.add(
                        "resource-calendar",
                        "resources on a calendar other than the project's; a schedule built from the text can't give them one, as it makes the file's other calendars the project's own",
                        &resource.short_name,
                    );
                }
                statement
                    .details
                    .push(line(ResourceDetail::Calendar(word(name))));
            } else if resource.calendar.is_none() {
                notes.add(
                    "resource-no-calendar",
                    "resources with no calendar; a schedule built from the text gives them the project's",
                    &resource.short_name,
                );
            }
            if let Some(units) = resource
                .default_units_per_hour
                .filter(|u| u.is_finite() && *u >= 0.0 && (*u - 1.0).abs() >= 1e-9)
            {
                statement
                    .details
                    .push(line(ResourceDetail::UnitsPerHour(Value::new(units))));
            }
            if let Some(limit) = one_limit(&resource.rates, |r| r.max_units_per_hour) {
                statement
                    .details
                    .push(line(ResourceDetail::MaxUnitsPerHour(Value::new(limit))));
            }
            Item::Resource(statement)
        })
        .collect()
}

/// The WBS nodes to write, in tree order, with their paths and names, and
/// each node's home: the nearest node that is written (`None` for the
/// project's own node).
#[allow(clippy::type_complexity, reason = "a private pair of lookups")]
fn wbs_nodes(
    schedule: &Schedule,
    notes: &mut Notes,
) -> (Vec<(WbsId, String, String)>, BTreeMap<WbsId, Option<WbsId>>) {
    let mut children: BTreeMap<Option<WbsId>, Vec<&totalfloat_model::Wbs>> = BTreeMap::new();
    let roots: Vec<WbsId> = schedule
        .wbs
        .values()
        .filter(|w| w.is_project_node)
        .map(|w| w.id)
        .collect();
    for node in schedule.wbs.values().filter(|w| !w.is_project_node) {
        let parent = node.parent.filter(|p| !roots.contains(p));
        children.entry(parent).or_default().push(node);
    }
    for list in children.values_mut() {
        list.sort_by(|a, b| (a.sequence, &a.code).cmp(&(b.sequence, &b.code)));
    }
    let mut written = Vec::new();
    let mut homes: BTreeMap<WbsId, Option<WbsId>> = roots.iter().map(|r| (*r, None)).collect();
    let mut stack: Vec<(&totalfloat_model::Wbs, String, Option<WbsId>)> = children
        .get(&None)
        .into_iter()
        .flatten()
        .rev()
        .map(|n| (*n, schedule.project.short_name.clone(), None))
        .collect();
    while let Some((node, parent_path, parent_home)) = stack.pop() {
        let (path, home) = if node.code.contains('.') || node.code.is_empty() {
            notes.add(
                "wbs-code-with-dot",
                "WBS nodes whose code has a dot or is empty, which a path can't hold; their activities go to the node above",
                &node.code,
            );
            (parent_path.clone(), parent_home)
        } else {
            let path = format!("{parent_path}.{}", node.code);
            written.push((node.id, path.clone(), node.name.clone()));
            (path, Some(node.id))
        };
        homes.insert(node.id, home);
        for child in children.get(&Some(node.id)).into_iter().flatten().rev() {
            stack.push((*child, path.clone(), home));
        }
    }
    for node in schedule.wbs.values() {
        if let std::collections::btree_map::Entry::Vacant(home) = homes.entry(node.id) {
            notes.add(
                "wbs-orphan",
                "WBS nodes whose parent isn't in the file; their activities go to the project",
                &node.code,
            );
            home.insert(None);
        }
    }
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    for (id, _, _) in &written {
        *seen
            .entry(schedule.wbs[id].code.to_lowercase())
            .or_default() += 1;
    }
    for (id, path, _) in &written {
        if seen[&schedule.wbs[id].code.to_lowercase()] > 1 {
            notes.add(
                "wbs-code-repeated",
                "WBS codes used by more than one node: edit commands find a node by its code, so the text can't be built until each is unique",
                path,
            );
        }
    }
    (written, homes)
}

fn kind_of(activity: &Activity, notes: &mut Notes) -> (ActivityKind, bool) {
    match activity.activity_type {
        ActivityType::StartMilestone => (ActivityKind::StartMilestone, false),
        ActivityType::FinishMilestone => (ActivityKind::Milestone, false),
        ActivityType::ResourceDependent => (ActivityKind::Activity, true),
        ActivityType::TaskDependent => (ActivityKind::Activity, false),
        ActivityType::LevelOfEffort | ActivityType::WbsSummary => {
            notes.add(
                "activity-type",
                "level of effort and WBS summary activities, written as tasks",
                &activity.code,
            );
            (ActivityKind::Activity, false)
        }
    }
}

fn link_type(kind: RelationshipType) -> LinkType {
    match kind {
        RelationshipType::FinishToStart => LinkType::FS,
        RelationshipType::StartToStart => LinkType::SS,
        RelationshipType::FinishToFinish => LinkType::FF,
        RelationshipType::StartToFinish => LinkType::SF,
    }
}

fn predecessors(context: &Context<'_>, activity: &Activity, notes: &mut Notes) -> Vec<Predecessor> {
    let schedule = context.schedule;
    let mut after: Vec<(String, Predecessor)> = Vec::new();
    for link in context.links_into.get(&activity.id).into_iter().flatten() {
        let Some(before) = schedule.activities.get(&link.predecessor) else {
            notes.add(
                "cross-project-link",
                "links from activities in other projects",
                &activity.code,
            );
            continue;
        };
        let lag = (link.lag != Minutes::ZERO)
            .then(|| {
                let day = lag_day_length(schedule, before, activity);
                written_span(link.lag, day, &activity.code, notes)
            })
            .flatten();
        after.push((
            before.code.clone(),
            Predecessor {
                code: word(&before.code),
                link: Some(link_type(link.kind)),
                lag,
                link_at: None,
            },
        ));
    }
    after.sort_by(|a, b| (&a.0, link_order(&a.1)).cmp(&(&b.0, link_order(&b.1))));
    after.into_iter().map(|(_, p)| p).collect()
}

fn link_order(p: &Predecessor) -> u8 {
    match p.link {
        Some(LinkType::FS) | None => 0,
        Some(LinkType::SS) => 1,
        Some(LinkType::FF) => 2,
        Some(LinkType::SF) => 3,
    }
}

pub(crate) fn constraint_kind(kind: ConstraintType) -> ConstraintKind {
    match kind {
        ConstraintType::StartOn => ConstraintKind::StartOn,
        ConstraintType::StartOnOrAfter => ConstraintKind::StartOnOrAfter,
        ConstraintType::StartOnOrBefore => ConstraintKind::StartOnOrBefore,
        ConstraintType::FinishOn => ConstraintKind::FinishOn,
        ConstraintType::FinishOnOrAfter => ConstraintKind::FinishOnOrAfter,
        ConstraintType::FinishOnOrBefore => ConstraintKind::FinishOnOrBefore,
        ConstraintType::AsLateAsPossible => ConstraintKind::AsLateAsPossible,
        ConstraintType::MandatoryStart => ConstraintKind::MandatoryStart,
        ConstraintType::MandatoryFinish => ConstraintKind::MandatoryFinish,
    }
}

fn activity_item(context: &Context<'_>, activity: &Activity, notes: &mut Notes) -> Item {
    let schedule = context.schedule;
    let (kind, resource) = kind_of(activity, notes);
    // Resource-dependent work runs on its resources' calendars, so its spans
    // are written in hours, as edits need them.
    let day = (activity.activity_type != ActivityType::ResourceDependent)
        .then(|| day_length(schedule, activity.calendar))
        .flatten();
    let milestone = kind != ActivityKind::Activity;
    let mut statement = Statement::new(Line {
        kind,
        code: word(&activity.code),
        name: word(&activity.name),
        duration: (!milestone)
            .then(|| written_span(activity.original_duration, day, &activity.code, notes))
            .flatten()
            .map(Value::new),
        after: predecessors(context, activity, notes),
    });
    let details = &mut statement.details;
    if let Some(name) = context.calendar_of.get(&activity.calendar)
        && Some(name) != context.default.as_ref()
    {
        details.push(line(ActivityDetail::Calendar(word(name))));
    }
    if resource {
        details.push(line(ActivityDetail::Type(TypeKind::Resource)));
    }
    let started = activity.actual_start.is_some();
    if !milestone
        && (started || activity.remaining_duration != activity.original_duration)
        && let Some(remaining) =
            written_span(activity.remaining_duration, day, &activity.code, notes)
    {
        details.push(line(ActivityDetail::Remaining(Value::new(remaining))));
    }
    for (constraint, secondary) in [
        (activity.primary_constraint, false),
        (activity.secondary_constraint, true),
    ] {
        if let Some(c) = constraint {
            details.push(line(ActivityDetail::Constraint {
                secondary,
                kind: constraint_kind(c.kind),
                date: c.date.map(Value::new),
            }));
        }
    }
    details.extend(codes(schedule, activity));
    details.extend(udfs(schedule, activity, notes));
    details.extend(resources(context, activity, notes));
    details.extend(roles(context, activity, notes));
    details.extend(expenses(context, activity, notes));
    activity_progress(activity, details);
    activity_notes(activity, notes);
    Item::Activity(statement)
}

/// An activity field's data type as a `field` line writes it, if it has one.
fn field_kind(kind: &totalfloat_model::UdfDataType) -> Option<totalfloat_edit::UdfKind> {
    use totalfloat_edit::UdfKind as K;
    use totalfloat_model::UdfDataType as D;
    Some(match kind {
        D::Text => K::Text,
        D::Number => K::Number,
        D::Integer => K::Integer,
        D::Cost => K::Cost,
        D::StartDate => K::StartDate,
        D::FinishDate => K::FinishDate,
        D::Indicator => K::Indicator,
        D::Code | D::Other(_) => return None,
    })
}

/// The activity fields text can hold: every one with a data type an edit can
/// add and a name no other activity field has, since text names a field by
/// its name.
fn written_fields(schedule: &Schedule) -> Vec<&totalfloat_model::UdfType> {
    let activity =
        |t: &&totalfloat_model::UdfType| t.subject == totalfloat_model::UdfSubject::Activity;
    let mut names: BTreeMap<String, usize> = BTreeMap::new();
    for t in schedule.udf_types.values().filter(activity) {
        *names.entry(t.label.to_lowercase()).or_default() += 1;
    }
    let mut fields: Vec<_> = schedule
        .udf_types
        .values()
        .filter(activity)
        .filter(|t| field_kind(&t.data_type).is_some() && names[&t.label.to_lowercase()] == 1)
        .collect();
    fields.sort_by(|a, b| (&a.label, a.id).cmp(&(&b.label, b.id)));
    fields
}

fn field_items(schedule: &Schedule, notes: &mut Notes) -> Vec<Item> {
    let written = written_fields(schedule);
    for t in schedule.udf_types.values() {
        if t.subject == totalfloat_model::UdfSubject::Activity && !written.contains(&t) {
            notes.add(
                "udf-field",
                "activity fields of code-list types, or sharing a name with another",
                &t.label,
            );
        }
    }
    written
        .into_iter()
        .filter_map(|t| {
            Some(Item::Field(Statement::new(crate::Field {
                name: word(&t.label),
                kind: Value::new(field_kind(&t.data_type)?),
            })))
        })
        .collect()
}

/// A field value as a `udf` line writes it, which `set_udf` reads back.
fn udf_text(value: &totalfloat_model::UdfValue) -> Option<String> {
    use totalfloat_model::UdfValue as V;
    match value {
        V::Text(text) => Some(text.clone()),
        V::Number(number) => Some(number.to_string()),
        V::Date(date) => Some(date.to_string()),
        V::Code(_) => None,
    }
}

fn udfs(
    schedule: &Schedule,
    activity: &Activity,
    notes: &mut Notes,
) -> Vec<Detail<ActivityDetail>> {
    let written = written_fields(schedule);
    let mut lines = Vec::new();
    for (id, value) in &activity.udfs {
        let field = written.iter().find(|t| t.id == *id);
        match (field, udf_text(value)) {
            (Some(field), Some(text)) => lines.push((field.label.clone(), text)),
            _ => notes.add(
                "udfs",
                "user-defined field values text can't hold",
                &activity.code,
            ),
        }
    }
    lines.sort();
    lines
        .into_iter()
        .map(|(field, value)| {
            line(ActivityDetail::Udf {
                field: word(&field),
                value: word(&value),
            })
        })
        .collect()
}

fn codes(schedule: &Schedule, activity: &Activity) -> Vec<Detail<ActivityDetail>> {
    let mut codes: Vec<(String, String)> = activity
        .codes
        .iter()
        .filter_map(|(t, v)| {
            Some((
                schedule.activity_code_types.get(t)?.name.clone(),
                schedule.activity_codes.get(v)?.value.clone(),
            ))
        })
        .collect();
    codes.sort();
    codes
        .into_iter()
        .map(|(t, v)| {
            line(ActivityDetail::Code {
                code_type: word(&t),
                value: word(&v),
            })
        })
        .collect()
}

fn resources(
    context: &Context<'_>,
    activity: &Activity,
    notes: &mut Notes,
) -> Vec<Detail<ActivityDetail>> {
    let mut on: Vec<(String, Option<f64>)> = context
        .assigned
        .get(&activity.id)
        .into_iter()
        .flatten()
        .filter_map(|a| {
            Some((
                context
                    .schedule
                    .resources
                    .get(&a.resource?)?
                    .short_name
                    .clone(),
                a.amounts.budgeted_units,
            ))
        })
        .collect();
    on.sort_by(|a, b| a.0.cmp(&b.0));
    on.into_iter()
        .map(|(resource, units)| {
            let units = units.filter(|u| u.is_finite() && *u >= 0.0);
            if units.is_none() {
                notes.add(
                    "assignment-units",
                    "assignments with no budgeted units; a schedule built from the text gives them the resource's default",
                    &activity.code,
                );
            }
            line(ActivityDetail::Resource {
                resource: word(&resource),
                units: units.map(Value::new),
            })
        })
        .collect()
}

/// The roles on an activity with no resource, by short name. A role on it
/// twice with no resource is one line, with the units of both, as the text
/// can assign a role only once.
fn roles(
    context: &Context<'_>,
    activity: &Activity,
    notes: &mut Notes,
) -> Vec<Detail<ActivityDetail>> {
    let mut on: BTreeMap<String, Option<f64>> = BTreeMap::new();
    for a in context
        .assigned
        .get(&activity.id)
        .into_iter()
        .flatten()
        .filter(|a| a.resource.is_none())
    {
        let Some(role) = a.role.and_then(|r| context.schedule.roles.get(&r)) else {
            continue;
        };
        match on.entry(role.short_name.clone()) {
            std::collections::btree_map::Entry::Vacant(slot) => {
                slot.insert(a.amounts.budgeted_units);
            }
            std::collections::btree_map::Entry::Occupied(mut slot) => {
                notes.add(
                    "repeated-roles",
                    "a role on an activity more than once with no resource; the text gives it once, with the units of each",
                    &activity.code,
                );
                let both = match (*slot.get(), a.amounts.budgeted_units) {
                    (None, None) => None,
                    (one, other) => Some(one.unwrap_or(0.0) + other.unwrap_or(0.0)),
                };
                slot.insert(both);
            }
        }
    }
    on.into_iter()
        .map(|(role, units)| {
            line(ActivityDetail::Role {
                role: word(&role),
                units: units.filter(|u| u.is_finite() && *u >= 0.0).map(Value::new),
            })
        })
        .collect()
}

/// The activity's expenses, by item. Actual cost is written when there is
/// some, remaining cost when it isn't the rest of the budget, and accrual
/// when it isn't uniform. What a line can't hold is noted.
#[allow(
    clippy::float_cmp,
    reason = "a line leaves out only what reads back exactly"
)]
fn expenses(
    context: &Context<'_>,
    activity: &Activity,
    notes: &mut Notes,
) -> Vec<Detail<ActivityDetail>> {
    let mut on: Vec<&totalfloat_model::Expense> = context
        .expenses
        .get(&activity.id)
        .cloned()
        .unwrap_or_default();
    on.sort_by_key(|x| (totalfloat_edit::item_key(&x.name), x.id));
    let code = &activity.code;
    let mut seen = BTreeSet::new();
    let mut lines = Vec::new();
    for x in on {
        if !seen.insert(totalfloat_edit::item_key(&x.name)) {
            notes.add(
                "repeated-expenses",
                "expense items on an activity more than once; edits can't tell them apart",
                code,
            );
        }
        let budget = x.budgeted_cost.unwrap_or(0.0);
        let actual = x.actual_cost.unwrap_or(0.0);
        let remaining = x.remaining_cost.unwrap_or(0.0);
        let as_built = x.cost_account.is_none()
            && x.category.is_none()
            && x.budgeted_units == Some(1.0)
            && x.price_per_unit == Some(budget)
            && x.unit_of_measure.is_empty()
            && x.po_number.is_empty()
            && x.vendor.is_empty()
            && x.description.is_empty()
            && !x.auto_compute_actuals;
        if !as_built {
            notes.add(
                "expense-details",
                "expenses' cost accounts, categories, units, prices, vendors and other details",
                code,
            );
        }
        if x.accrual.is_none() {
            notes.add(
                "expense-accrual",
                "expenses with an accrual code TotalFloat doesn't know; written as uniform",
                code,
            );
        }
        let accrual = match x.accrual {
            Some(totalfloat_model::Accrual::Start) => Some(ExpenseAccrual::Start),
            Some(totalfloat_model::Accrual::End) => Some(ExpenseAccrual::End),
            Some(totalfloat_model::Accrual::Uniform) | None => None,
        };
        lines.push(line(ActivityDetail::Expense(crate::Expense {
            item: word(&x.name),
            budget: Value::new(budget),
            actual: (actual != 0.0).then(|| Value::new(actual)),
            remaining: (remaining != totalfloat_edit::rest_of(budget, actual))
                .then(|| Value::new(remaining)),
            accrual: accrual.map(Value::new),
        })));
    }
    lines
}

fn activity_progress(activity: &Activity, details: &mut Vec<Detail<ActivityDetail>>) {
    if let Some(date) = activity.actual_start {
        details.push(line(ActivityDetail::ActualStart(Value::new(date))));
    }
    if let Some(date) = activity.actual_finish {
        details.push(line(ActivityDetail::ActualFinish(Value::new(date))));
    }
    if let Some(percent) = activity.physical_percent_complete
        && percent > 0.0
        && percent <= 100.0
    {
        details.push(line(ActivityDetail::Percent(Value::new(percent))));
    }
    if let Some(date) = activity.expected_finish {
        details.push(line(ActivityDetail::ExpectedFinish(Value::new(date))));
    }
}

fn activity_notes(activity: &Activity, notes: &mut Notes) {
    let code = &activity.code;

    if activity.suspend.is_some() || activity.resume.is_some() {
        notes.add("suspend-resume", "suspend and resume dates", code);
    }
    if activity.external_early_start.is_some() || activity.external_late_finish.is_some() {
        notes.add(
            "external-dates",
            "external early start and late finish dates",
            code,
        );
    }
    if activity.duration_type != DurationType::default() {
        notes.add(
            "duration-type",
            "duration types other than the default",
            code,
        );
    }
    if activity.percent_complete_type != PercentCompleteType::default() {
        notes.add(
            "percent-complete-type",
            "percent complete types other than the default",
            code,
        );
    }
    if activity.status == ActivityStatus::InProgress && activity.actual_start.is_none() {
        notes.add(
            "status",
            "in-progress activities with no actual start",
            code,
        );
    }
}

/// A resource's limit, when every rate has the same one, as text writes it.
/// The limit every rate holds, if they all hold the same one.
fn one_limit<R>(rates: &[R], limit: fn(&R) -> Option<f64>) -> Option<f64> {
    let first = limit(rates.first()?)?;
    let usable = first.is_finite() && first >= 0.0;
    (usable && rates.iter().all(|r| limit(r) == Some(first))).then_some(first)
}

fn other_notes(schedule: &Schedule, notes: &mut Notes) {
    for resource in schedule.resources.values() {
        let priced = resource
            .rates
            .iter()
            .any(|r| r.prices.iter().any(Option::is_some));
        let limited = resource
            .rates
            .iter()
            .any(|r| r.max_units_per_hour.is_some());
        if priced
            || (limited && one_limit(&resource.rates, |r| r.max_units_per_hour).is_none())
            || resource.parent.is_some()
        {
            notes.add(
                "resource-details",
                "resources' prices, limits that change over time, and places in the resource hierarchy",
                &resource.short_name,
            );
        }
    }
    for role in schedule.roles.values() {
        let priced = role
            .rates
            .iter()
            .any(|r| r.prices.iter().any(Option::is_some));
        let limited = role.rates.iter().any(|r| r.max_units_per_hour.is_some());
        if priced || (limited && one_limit(&role.rates, |r| r.max_units_per_hour).is_none()) {
            notes.add(
                "role-rates",
                "roles' prices and limits that change over time",
                &role.short_name,
            );
        }
    }
    for assignment in schedule.assignments.values() {
        if let (Some(_), Some(activity)) = (
            assignment.resource.and(assignment.role),
            schedule.activities.get(&assignment.activity),
        ) {
            notes.add(
                "assignment-roles",
                "the roles resources fill on their assignments; the text gives the resource",
                &activity.code,
            );
        }
        if let Some(activity) = schedule.activities.get(&assignment.activity)
            && (assignment.amounts.budgeted_cost.is_some()
                || assignment.amounts.actual_cost.is_some())
        {
            notes.add("costs", "assignment costs", &activity.code);
        }
        if let (Some(_), Some(activity)) = (
            assignment.curve,
            schedule.activities.get(&assignment.activity),
        ) {
            notes.add(
                "curves",
                "assignments' resource curves; text spreads units evenly",
                &activity.code,
            );
        }
        if let (Some(_), Some(activity)) = (
            &assignment.typed_spread,
            schedule.activities.get(&assignment.activity),
        ) {
            notes.add(
                "typed-spreads",
                "assignments' hand-typed spreads; text spreads units evenly",
                &activity.code,
            );
        }
        if let (Some(_), Some(activity)) = (
            assignment.cost_account,
            schedule.activities.get(&assignment.activity),
        ) {
            notes.add(
                "assignment-accounts",
                "the cost accounts assignments' costs are booked to",
                &activity.code,
            );
        }
    }
    if !schedule.project.udfs.is_empty() {
        notes.add(
            "udfs",
            "user-defined field values",
            &schedule.project.short_name,
        );
    }
}
