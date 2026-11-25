//! The per-project schedule container and its integrity checks.

use std::collections::BTreeMap;
use std::fmt;

use crate::{
    Activity, ActivityCode, ActivityCodeId, ActivityCodeType, ActivityCodeTypeId, ActivityId,
    Assignment, AssignmentId, Calendar, CalendarId, CalendarType, CodeScope, CostAccount,
    CostAccountId, Expense, ExpenseCategory, ExpenseCategoryId, ExpenseId, NotebookTopic,
    NotebookTopicId, Obs, ObsId, Project, ProjectCode, ProjectCodeId, ProjectCodeType,
    ProjectCodeTypeId, ProjectId, Relationship, RelationshipId, Resource, ResourceCurve,
    ResourceCurveId, ResourceId, Role, RoleId, UdfDataType, UdfSubject, UdfType, UdfTypeId,
    UdfValue, Wbs, WbsId, Weekday, WorkInterval,
};

/// One project and everything it references.
///
/// A multi-project XER becomes one `Schedule` per project. Shared entities
/// (global calendars, resources, code dictionaries, UDF definitions) are
/// copied into each schedule that uses them, so a `Schedule` is
/// self-contained. All collections are ordered maps keyed by source ID, so
/// iteration order, and therefore output, is deterministic.
#[derive(Debug, Clone, PartialEq)]
pub struct Schedule {
    /// The project.
    pub project: Project,
    /// WBS nodes. XER: `PROJWBS`.
    pub wbs: BTreeMap<WbsId, Wbs>,
    /// Activities. XER: `TASK`.
    pub activities: BTreeMap<ActivityId, Activity>,
    /// Relationships with at least one end in this project. XER: `TASKPRED`.
    pub relationships: BTreeMap<RelationshipId, Relationship>,
    /// Calendars used by the project, its activities and resources. XER: `CALENDAR`.
    pub calendars: BTreeMap<CalendarId, Calendar>,
    /// Resources. XER: `RSRC`.
    pub resources: BTreeMap<ResourceId, Resource>,
    /// Roles. XER: `ROLES`.
    pub roles: BTreeMap<RoleId, Role>,
    /// Resource assignments. XER: `TASKRSRC`.
    pub assignments: BTreeMap<AssignmentId, Assignment>,
    /// Resource curves. XER: `RSRCCURVDATA`.
    pub resource_curves: BTreeMap<ResourceCurveId, ResourceCurve>,
    /// Cost accounts. XER: `ACCOUNT`.
    pub cost_accounts: BTreeMap<CostAccountId, CostAccount>,
    /// Expense categories. XER: `COSTTYPE`.
    pub expense_categories: BTreeMap<ExpenseCategoryId, ExpenseCategory>,
    /// Expenses. XER: `PROJCOST`.
    pub expenses: BTreeMap<ExpenseId, Expense>,
    /// Activity code dictionaries. XER: `ACTVTYPE`.
    pub activity_code_types: BTreeMap<ActivityCodeTypeId, ActivityCodeType>,
    /// Activity code values. XER: `ACTVCODE`.
    pub activity_codes: BTreeMap<ActivityCodeId, ActivityCode>,
    /// User-defined field definitions. XER: `UDFTYPE`.
    pub udf_types: BTreeMap<UdfTypeId, UdfType>,
    /// Project code dictionaries, every one in the file. XER: `PCATTYPE`.
    pub project_code_types: BTreeMap<ProjectCodeTypeId, ProjectCodeType>,
    /// Project code values, every one in the file. XER: `PCATVAL`.
    pub project_codes: BTreeMap<ProjectCodeId, ProjectCode>,
    /// The organizational breakdown structure, every node in the file. XER: `OBS`.
    pub obs: BTreeMap<ObsId, Obs>,
    /// Notebook topics, every one in the file. XER: `MEMOTYPE`.
    pub notebook_topics: BTreeMap<NotebookTopicId, NotebookTopic>,
}

impl Schedule {
    /// A schedule containing only `project`.
    #[must_use]
    pub fn new(project: Project) -> Self {
        Self {
            project,
            wbs: BTreeMap::new(),
            activities: BTreeMap::new(),
            relationships: BTreeMap::new(),
            calendars: BTreeMap::new(),
            resources: BTreeMap::new(),
            roles: BTreeMap::new(),
            assignments: BTreeMap::new(),
            resource_curves: BTreeMap::new(),
            cost_accounts: BTreeMap::new(),
            expense_categories: BTreeMap::new(),
            expenses: BTreeMap::new(),
            activity_code_types: BTreeMap::new(),
            activity_codes: BTreeMap::new(),
            udf_types: BTreeMap::new(),
            project_code_types: BTreeMap::new(),
            project_codes: BTreeMap::new(),
            obs: BTreeMap::new(),
            notebook_topics: BTreeMap::new(),
        }
    }

    /// The project's ID.
    #[must_use]
    pub fn project_id(&self) -> ProjectId {
        self.project.id
    }

    /// The activity with internal ID `id`.
    #[must_use]
    pub fn activity(&self, id: ActivityId) -> Option<&Activity> {
        self.activities.get(&id)
    }

    /// The activity whose user-facing Activity ID is `code`. Codes are unique
    /// within a project in an XER file; if a file breaks that, the lowest internal ID wins.
    #[must_use]
    pub fn activity_by_code(&self, code: &str) -> Option<&Activity> {
        self.activities.values().find(|a| a.code == code)
    }

    /// The calendar an activity is scheduled on, if it exists.
    #[must_use]
    pub fn activity_calendar(&self, activity: &Activity) -> Option<&Calendar> {
        self.calendars.get(&activity.calendar)
    }

    /// Relationships whose successor is `id`, in relationship ID order.
    ///
    /// Scans every relationship (O(R)). Fine for queries; the engine builds
    /// its own adjacency index rather than calling this per activity.
    pub fn predecessors_of(&self, id: ActivityId) -> impl Iterator<Item = &Relationship> {
        self.relationships
            .values()
            .filter(move |r| r.successor == id)
    }

    /// Relationships whose predecessor is `id`, in relationship ID order.
    /// Scans every relationship, like [`Schedule::predecessors_of`].
    pub fn successors_of(&self, id: ActivityId) -> impl Iterator<Item = &Relationship> {
        self.relationships
            .values()
            .filter(move |r| r.predecessor == id)
    }

    /// True if the relationship has an end in another project.
    #[must_use]
    pub fn is_external(&self, relationship: &Relationship) -> bool {
        relationship.predecessor_project != self.project.id
            || relationship.successor_project != self.project.id
    }

    /// Integrity problems: references to entities that aren't in the
    /// schedule, and values the model's invariants forbid. Returns them in a
    /// deterministic order (by entity kind, then ID). An empty list means the
    /// schedule is internally consistent; it says nothing about whether it
    /// schedules well.
    #[must_use]
    pub fn validate(&self) -> Vec<Diagnostic> {
        let mut out = Diagnostics::default();
        self.validate_project(&mut out);
        self.validate_wbs(&mut out);
        self.validate_activities(&mut out);
        self.validate_relationships(&mut out);
        self.validate_calendars_and_resources(&mut out);
        self.validate_assignments(&mut out);
        self.validate_expenses(&mut out);
        self.validate_activity_codes(&mut out);
        self.validate_project_codes(&mut out);
        self.validate_obs(&mut out);
        self.validate_notes(&mut out);
        self.validate_hierarchies(&mut out);
        out.0
    }

    fn validate_project(&self, out: &mut Diagnostics) {
        let e = EntityRef::Project(self.project.id);
        if let Some(cal) = self.project.default_calendar {
            out.dangling_if(
                !self.calendars.contains_key(&cal),
                e,
                "default calendar",
                cal,
            );
        }
        if let Some(end) = self.project.options.float_paths.end_activity {
            out.dangling_if(
                !self.activities.contains_key(&end),
                e,
                "float path end activity",
                end,
            );
        }
        self.check_udfs(out, e, &self.project.udfs, &UdfSubject::Project);
    }

    fn validate_wbs(&self, out: &mut Diagnostics) {
        for w in self.wbs.values() {
            let e = EntityRef::Wbs(w.id);
            out.project_if(w.project != self.project.id, e, w.project);
            if let Some(parent) = w.parent {
                out.dangling_if(!self.wbs.contains_key(&parent), e, "parent WBS", parent);
            }
            self.check_udfs(out, e, &w.udfs, &UdfSubject::Wbs);
        }
    }

    fn validate_activities(&self, out: &mut Diagnostics) {
        for a in self.activities.values() {
            let e = EntityRef::Activity(a.id);
            out.project_if(a.project != self.project.id, e, a.project);
            out.dangling_if(!self.wbs.contains_key(&a.wbs), e, "WBS", a.wbs);
            out.dangling_if(
                !self.calendars.contains_key(&a.calendar),
                e,
                "calendar",
                a.calendar,
            );
            if let Some(pct) = a.physical_percent_complete
                && !(0.0..=100.0).contains(&pct)
            {
                out.push(
                    e,
                    "percent-out-of-range",
                    format!("physical percent complete {pct} is not between 0 and 100"),
                );
            }
            for (which, c) in [
                ("primary", a.primary_constraint),
                ("secondary", a.secondary_constraint),
            ] {
                if let Some(c) = c
                    && c.kind.requires_date()
                    && c.date.is_none()
                {
                    out.push(
                        e,
                        "constraint-without-date",
                        format!("{which} constraint {:?} has no date", c.kind),
                    );
                }
            }
            for (&type_id, &code_id) in &a.codes {
                out.dangling_if(
                    !self.activity_code_types.contains_key(&type_id),
                    e,
                    "activity code type",
                    type_id,
                );
                match self.activity_codes.get(&code_id) {
                    None => out.dangling(e, "activity code", code_id),
                    Some(code) if code.code_type != type_id => out.push(
                        e,
                        "code-type-mismatch",
                        format!(
                            "activity code {code_id} belongs to code type {}, not {type_id}",
                            code.code_type
                        ),
                    ),
                    Some(_) => {}
                }
            }
            self.check_udfs(out, e, &a.udfs, &UdfSubject::Activity);
        }
    }

    fn validate_relationships(&self, out: &mut Diagnostics) {
        let pid = self.project.id;
        for r in self.relationships.values() {
            let e = EntityRef::Relationship(r.id);
            if r.predecessor_project != pid && r.successor_project != pid {
                out.push(
                    e,
                    "foreign-relationship",
                    "neither end is in this project".to_owned(),
                );
            }
            if r.predecessor_project == pid {
                out.dangling_if(
                    !self.activities.contains_key(&r.predecessor),
                    e,
                    "predecessor activity",
                    r.predecessor,
                );
            }
            if r.successor_project == pid {
                out.dangling_if(
                    !self.activities.contains_key(&r.successor),
                    e,
                    "successor activity",
                    r.successor,
                );
            }
            if r.predecessor == r.successor {
                out.push(
                    e,
                    "self-relationship",
                    format!("activity {} is its own predecessor", r.predecessor),
                );
            }
        }
    }

    fn validate_calendars_and_resources(&self, out: &mut Diagnostics) {
        for c in self.calendars.values() {
            let e = EntityRef::Calendar(c.id);
            if let Some(base) = c.base {
                out.dangling_if(
                    !self.calendars.contains_key(&base),
                    e,
                    "base calendar",
                    base,
                );
            }
            if c.kind == CalendarType::Project
                && let Some(owner) = c.project
            {
                out.project_if(owner != self.project.id, e, owner);
            }
            for day in Weekday::ALL {
                check_intervals(out, e, c.work_week.day(day), || format!("{day:?}"));
            }
            for (date, intervals) in &c.exceptions {
                check_intervals(out, e, intervals, || format!("exception {date}"));
            }
        }
        for r in self.resources.values() {
            let e = EntityRef::Resource(r.id);
            if let Some(cal) = r.calendar {
                out.dangling_if(!self.calendars.contains_key(&cal), e, "calendar", cal);
            }
            if let Some(parent) = r.parent {
                out.dangling_if(
                    !self.resources.contains_key(&parent),
                    e,
                    "parent resource",
                    parent,
                );
            }
            for filled in &r.roles {
                out.dangling_if(
                    !self.roles.contains_key(&filled.role),
                    e,
                    "role",
                    filled.role,
                );
            }
            self.check_udfs(out, e, &r.udfs, &UdfSubject::Resource);
        }
        for role in self.roles.values() {
            if let Some(parent) = role.parent {
                out.dangling_if(
                    !self.roles.contains_key(&parent),
                    EntityRef::Role(role.id),
                    "parent role",
                    parent,
                );
            }
        }
    }

    fn validate_assignments(&self, out: &mut Diagnostics) {
        for a in self.assignments.values() {
            let e = EntityRef::Assignment(a.id);
            out.project_if(a.project != self.project.id, e, a.project);
            out.dangling_if(
                !self.activities.contains_key(&a.activity),
                e,
                "activity",
                a.activity,
            );
            if let Some(resource) = a.resource {
                out.dangling_if(
                    !self.resources.contains_key(&resource),
                    e,
                    "resource",
                    resource,
                );
            }
            if let Some(role) = a.role {
                out.dangling_if(!self.roles.contains_key(&role), e, "role", role);
            }
            if let Some(curve) = a.curve {
                out.dangling_if(
                    !self.resource_curves.contains_key(&curve),
                    e,
                    "resource curve",
                    curve,
                );
            }
            self.check_udfs(out, e, &a.udfs, &UdfSubject::Assignment);
        }
    }

    fn validate_expenses(&self, out: &mut Diagnostics) {
        for x in self.expenses.values() {
            let e = EntityRef::Expense(x.id);
            out.project_if(x.project != self.project.id, e, x.project);
            out.dangling_if(
                !self.activities.contains_key(&x.activity),
                e,
                "activity",
                x.activity,
            );
            if let Some(account) = x.cost_account {
                out.dangling_if(
                    !self.cost_accounts.contains_key(&account),
                    e,
                    "cost account",
                    account,
                );
            }
            if let Some(category) = x.category {
                out.dangling_if(
                    !self.expense_categories.contains_key(&category),
                    e,
                    "expense category",
                    category,
                );
            }
        }
        for a in self.assignments.values() {
            if let Some(account) = a.cost_account {
                out.dangling_if(
                    !self.cost_accounts.contains_key(&account),
                    EntityRef::Assignment(a.id),
                    "cost account",
                    account,
                );
            }
        }
        for account in self.cost_accounts.values() {
            if let Some(parent) = account.parent {
                out.dangling_if(
                    !self.cost_accounts.contains_key(&parent),
                    EntityRef::CostAccount(account.id),
                    "parent cost account",
                    parent,
                );
            }
        }
    }

    fn validate_notes(&self, out: &mut Diagnostics) {
        let topics = &self.notebook_topics;
        for a in self.activities.values() {
            for topic in a.notes.keys() {
                out.dangling_if(
                    !topics.contains_key(topic),
                    EntityRef::Activity(a.id),
                    "notebook topic",
                    topic,
                );
            }
        }
        for node in self.wbs.values() {
            for topic in node.notes.keys() {
                out.dangling_if(
                    !topics.contains_key(topic),
                    EntityRef::Wbs(node.id),
                    "notebook topic",
                    topic,
                );
            }
        }
    }

    fn validate_obs(&self, out: &mut Diagnostics) {
        for node in self.wbs.values() {
            if let Some(obs) = node.obs {
                out.dangling_if(
                    !self.obs.contains_key(&obs),
                    EntityRef::Wbs(node.id),
                    "OBS node",
                    obs,
                );
            }
        }
        let e = EntityRef::Project(self.project.id);
        for node in self.obs.values() {
            if let Some(parent) = node.parent {
                out.dangling_if(
                    !self.obs.contains_key(&parent),
                    e,
                    "parent OBS node",
                    parent,
                );
            }
        }
    }

    fn validate_project_codes(&self, out: &mut Diagnostics) {
        let e = EntityRef::Project(self.project.id);
        for (&type_id, &code_id) in &self.project.codes {
            match self.project_codes.get(&code_id) {
                None => out.dangling(e, "project code", code_id),
                Some(code) if code.code_type != type_id => out.push(
                    e,
                    "code-type-mismatch",
                    format!(
                        "project code {code_id} belongs to code type {}, not {type_id}",
                        code.code_type
                    ),
                ),
                Some(_) => {}
            }
        }
        for c in self.project_codes.values() {
            if !self.project_code_types.contains_key(&c.code_type) {
                out.dangling(e, "project code type", c.code_type);
            }
            if let Some(parent) = c.parent
                && !self.project_codes.contains_key(&parent)
            {
                out.dangling(e, "parent project code", parent);
            }
        }
    }

    fn validate_activity_codes(&self, out: &mut Diagnostics) {
        for t in self.activity_code_types.values() {
            if t.scope == CodeScope::Project
                && let Some(owner) = t.project
            {
                out.project_if(
                    owner != self.project.id,
                    EntityRef::ActivityCodeType(t.id),
                    owner,
                );
            }
        }
        for c in self.activity_codes.values() {
            let e = EntityRef::ActivityCode(c.id);
            out.dangling_if(
                !self.activity_code_types.contains_key(&c.code_type),
                e,
                "activity code type",
                c.code_type,
            );
            if let Some(parent) = c.parent {
                match self.activity_codes.get(&parent) {
                    None => out.dangling(e, "parent activity code", parent),
                    Some(p) if p.code_type != c.code_type => out.push(
                        e,
                        "code-type-mismatch",
                        format!(
                            "parent activity code {parent} belongs to code type {}, not {}",
                            p.code_type, c.code_type
                        ),
                    ),
                    Some(_) => {}
                }
            }
        }
    }

    /// Parent chains must end. Reports each cycle once, on its lowest ID.
    fn validate_hierarchies(&self, out: &mut Diagnostics) {
        let looped = "hierarchy-cycle";
        for id in cycles(self.wbs.values().map(|w| (w.id, w.parent))) {
            out.push(
                EntityRef::Wbs(id),
                looped,
                "parent WBS chain loops".to_owned(),
            );
        }
        for id in cycles(self.calendars.values().map(|c| (c.id, c.base))) {
            out.push(
                EntityRef::Calendar(id),
                looped,
                "base calendar chain loops".to_owned(),
            );
        }
        for id in cycles(self.resources.values().map(|r| (r.id, r.parent))) {
            out.push(
                EntityRef::Resource(id),
                looped,
                "parent resource chain loops".to_owned(),
            );
        }
        for id in cycles(self.cost_accounts.values().map(|a| (a.id, a.parent))) {
            out.push(
                EntityRef::CostAccount(id),
                looped,
                "parent cost account chain loops".to_owned(),
            );
        }
        for id in cycles(self.roles.values().map(|r| (r.id, r.parent))) {
            out.push(
                EntityRef::Role(id),
                looped,
                "parent role chain loops".to_owned(),
            );
        }
        for id in cycles(self.activity_codes.values().map(|c| (c.id, c.parent))) {
            out.push(
                EntityRef::ActivityCode(id),
                looped,
                "parent code chain loops".to_owned(),
            );
        }
    }

    fn check_udfs(
        &self,
        out: &mut Diagnostics,
        entity: EntityRef,
        values: &BTreeMap<UdfTypeId, UdfValue>,
        subject: &UdfSubject,
    ) {
        for type_id in values.keys() {
            match self.udf_types.get(type_id) {
                None => out.dangling(entity, "UDF type", *type_id),
                Some(t) if &t.subject != subject => out.push(
                    entity,
                    "udf-subject-mismatch",
                    format!(
                        "UDF type {type_id} applies to {:?}, not {subject:?}",
                        t.subject
                    ),
                ),
                Some(t) if !udf_value_fits(&t.data_type, &values[type_id]) => out.push(
                    entity,
                    "udf-value-mismatch",
                    format!(
                        "UDF type {type_id} holds {:?} data, but the value is {:?}",
                        t.data_type, values[type_id]
                    ),
                ),
                Some(_) => {}
            }
        }
    }
}

/// True if `value` is the kind a UDF of `data_type` stores. Types whose XER
/// storage isn't confirmed yet accept any value.
fn udf_value_fits(data_type: &UdfDataType, value: &UdfValue) -> bool {
    match data_type {
        UdfDataType::Text => matches!(value, UdfValue::Text(_)),
        UdfDataType::StartDate | UdfDataType::FinishDate => matches!(value, UdfValue::Date(_)),
        UdfDataType::Cost | UdfDataType::Number | UdfDataType::Integer => {
            matches!(value, UdfValue::Number(_))
        }
        UdfDataType::Code => matches!(value, UdfValue::Code(_)),
        UdfDataType::Indicator | UdfDataType::Other(_) => true,
    }
}

/// Work intervals within one day must be sorted and must not overlap.
fn check_intervals(
    out: &mut Diagnostics,
    entity: EntityRef,
    intervals: &[WorkInterval],
    day: impl Fn() -> String,
) {
    if intervals.iter().any(|i| i.start >= i.finish) {
        out.push(
            entity,
            "empty-work-interval",
            format!("{}: a work interval is empty or reversed", day()),
        );
    }
    if intervals.windows(2).any(|w| w[1].start < w[0].finish) {
        out.push(
            entity,
            "overlapping-work-intervals",
            format!("{}: work intervals overlap or are out of order", day()),
        );
    }
}

/// Nodes of a parent forest that sit on a cycle: one per cycle (its lowest
/// ID), ascending. A parent that isn't a node ends the chain.
fn cycles<K: Ord + Copy>(nodes: impl Iterator<Item = (K, Option<K>)>) -> Vec<K> {
    #[derive(Clone, Copy, PartialEq)]
    enum Seen {
        OnWalk,
        Ends,
    }
    let parents: BTreeMap<K, Option<K>> = nodes.collect();
    let mut seen: BTreeMap<K, Seen> = BTreeMap::new();
    let mut found = Vec::new();
    for &start in parents.keys() {
        let mut walk = Vec::new();
        let mut node = Some(start);
        while let Some(n) = node {
            match seen.get(&n) {
                Some(Seen::Ends) => break,
                Some(Seen::OnWalk) => {
                    // `n` closes a loop: the cycle is the walk from `n` onward.
                    let from = walk.iter().position(|&w| w == n).unwrap_or(0);
                    found.extend(walk[from..].iter().min());
                    break;
                }
                None => match parents.get(&n) {
                    None => break,
                    Some(&parent) => {
                        seen.insert(n, Seen::OnWalk);
                        walk.push(n);
                        node = parent;
                    }
                },
            }
        }
        for w in walk {
            seen.insert(w, Seen::Ends);
        }
    }
    found.sort();
    found
}

/// A reference to one entity in a schedule, for diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EntityRef {
    /// A project.
    Project(ProjectId),
    /// A WBS node.
    Wbs(WbsId),
    /// An activity.
    Activity(ActivityId),
    /// A relationship.
    Relationship(RelationshipId),
    /// A calendar.
    Calendar(CalendarId),
    /// A resource.
    Resource(ResourceId),
    /// A role.
    Role(RoleId),
    /// A resource assignment.
    Assignment(AssignmentId),
    /// A cost account.
    CostAccount(CostAccountId),
    /// An expense.
    Expense(ExpenseId),
    /// An activity code dictionary.
    ActivityCodeType(ActivityCodeTypeId),
    /// An activity code value.
    ActivityCode(ActivityCodeId),
}

impl fmt::Display for EntityRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Project(id) => write!(f, "project {id}"),
            Self::Wbs(id) => write!(f, "WBS {id}"),
            Self::Activity(id) => write!(f, "activity {id}"),
            Self::Relationship(id) => write!(f, "relationship {id}"),
            Self::Calendar(id) => write!(f, "calendar {id}"),
            Self::Resource(id) => write!(f, "resource {id}"),
            Self::Role(id) => write!(f, "role {id}"),
            Self::Assignment(id) => write!(f, "assignment {id}"),
            Self::CostAccount(id) => write!(f, "cost account {id}"),
            Self::Expense(id) => write!(f, "expense {id}"),
            Self::ActivityCodeType(id) => write!(f, "activity code type {id}"),
            Self::ActivityCode(id) => write!(f, "activity code {id}"),
        }
    }
}

/// One integrity problem found by [`Schedule::validate`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// The entity with the problem.
    pub entity: EntityRef,
    /// A stable, machine-readable kind, e.g. `dangling-reference`.
    pub code: &'static str,
    /// A human-readable description.
    pub message: String,
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {} ({})", self.entity, self.message, self.code)
    }
}

#[derive(Default)]
struct Diagnostics(Vec<Diagnostic>);

impl Diagnostics {
    fn push(&mut self, entity: EntityRef, code: &'static str, message: String) {
        self.0.push(Diagnostic {
            entity,
            code,
            message,
        });
    }

    fn dangling(&mut self, entity: EntityRef, what: &str, id: impl fmt::Display) {
        self.push(
            entity,
            "dangling-reference",
            format!("references {what} {id}, which is not in the schedule"),
        );
    }

    fn dangling_if(&mut self, missing: bool, entity: EntityRef, what: &str, id: impl fmt::Display) {
        if missing {
            self.dangling(entity, what, id);
        }
    }

    fn project_if(&mut self, wrong: bool, entity: EntityRef, project: ProjectId) {
        if wrong {
            self.push(
                entity,
                "wrong-project",
                format!("belongs to project {project}, not this schedule's project"),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ActivityCodeType, CalendarType, CodeScope, Constraint, ConstraintType, HoursPerPeriod,
        RelationshipType, ResourceType, UdfDataType, WorkWeek,
    };

    const P: ProjectId = ProjectId(1);

    fn calendar(id: u64) -> Calendar {
        Calendar {
            id: CalendarId(id),
            name: format!("cal {id}"),
            kind: CalendarType::Global,
            project: None,
            base: None,
            is_default: false,
            hours_per_period: HoursPerPeriod::default(),
            work_week: WorkWeek::default(),
            exceptions: BTreeMap::new(),
            source: None,
        }
    }

    fn relationship(id: u64, pred: u64, succ: u64) -> Relationship {
        Relationship {
            id: RelationshipId(id),
            predecessor: ActivityId(pred),
            successor: ActivityId(succ),
            predecessor_project: P,
            successor_project: P,
            kind: RelationshipType::FinishToStart,
            lag: crate::Minutes::ZERO,
            source: None,
        }
    }

    /// Project 1 with calendar 10, WBS 100, activities 1000 → 1001, a resource,
    /// an assignment, a code type with one value, and an activity UDF.
    fn valid() -> Schedule {
        let mut project = Project::new(P, "P1");
        project.default_calendar = Some(CalendarId(10));
        let mut s = Schedule::new(project);
        s.calendars.insert(CalendarId(10), calendar(10));
        s.wbs.insert(
            WbsId(100),
            Wbs {
                id: WbsId(100),
                project: P,
                parent: None,
                code: "P1".into(),
                name: "Project".into(),
                sequence: 0,
                is_project_node: true,
                earned_value: crate::EarnedValue::default(),
                obs: None,
                notes: std::collections::BTreeMap::new(),
                udfs: BTreeMap::new(),
                source: None,
            },
        );
        for (id, code) in [(1000, "A1000"), (1001, "A1010")] {
            let a = Activity::new(ActivityId(id), P, WbsId(100), CalendarId(10), code, "work");
            s.activities.insert(a.id, a);
        }
        s.relationships
            .insert(RelationshipId(1), relationship(1, 1000, 1001));
        s.resources.insert(
            ResourceId(7),
            Resource {
                id: ResourceId(7),
                short_name: "LAB".into(),
                name: "Labor".into(),
                kind: ResourceType::Labor,
                calendar: Some(CalendarId(10)),
                parent: None,
                default_units_per_hour: None,
                default_cost_follows_units: false,
                rates: Vec::new(),
                roles: Vec::new(),
                udfs: BTreeMap::new(),
                source: None,
            },
        );
        s.assignments.insert(
            AssignmentId(70),
            Assignment {
                id: AssignmentId(70),
                activity: ActivityId(1000),
                resource: Some(ResourceId(7)),
                role: None,
                cost_account: None,
                project: P,
                curve: None,
                typed_spread: None,
                amounts: crate::Amounts::default(),
                udfs: BTreeMap::new(),
                source: None,
            },
        );
        s.activity_code_types.insert(
            ActivityCodeTypeId(5),
            ActivityCodeType {
                id: ActivityCodeTypeId(5),
                name: "Phase".into(),
                scope: CodeScope::Project,
                project: Some(P),
                max_length: None,
                sequence: 0,
                source: None,
            },
        );
        s.activity_codes.insert(
            ActivityCodeId(50),
            ActivityCode {
                id: ActivityCodeId(50),
                code_type: ActivityCodeTypeId(5),
                parent: None,
                value: "DES".into(),
                description: "Design".into(),
                sequence: 0,
                source: None,
            },
        );
        s.udf_types.insert(
            UdfTypeId(9),
            UdfType {
                id: UdfTypeId(9),
                subject: UdfSubject::Activity,
                label: "Area".into(),
                data_type: UdfDataType::Text,
                source: None,
            },
        );
        let a = s.activities.get_mut(&ActivityId(1000)).unwrap();
        a.codes.insert(ActivityCodeTypeId(5), ActivityCodeId(50));
        a.udfs.insert(UdfTypeId(9), UdfValue::Text("North".into()));
        s
    }

    fn codes(s: &Schedule) -> Vec<(EntityRef, &'static str)> {
        s.validate()
            .into_iter()
            .map(|d| (d.entity, d.code))
            .collect()
    }

    #[test]
    fn empty_or_reversed_intervals_are_reported() {
        let mut s = Schedule::new(Project::new(ProjectId(1), "AUTHORED"));
        let eight = crate::TimeOfDay::from_hm(8, 0).unwrap();
        let mut calendar = Calendar {
            id: CalendarId(1),
            name: "Authored".to_owned(),
            kind: CalendarType::Global,
            project: None,
            base: None,
            is_default: false,
            hours_per_period: crate::HoursPerPeriod::default(),
            work_week: crate::WorkWeek::default(),
            exceptions: std::collections::BTreeMap::new(),
            source: None,
        };
        calendar
            .work_week
            .day_mut(crate::Weekday::Monday)
            .push(WorkInterval {
                start: eight,
                finish: eight,
            });
        s.calendars.insert(CalendarId(1), calendar);
        let found = s.validate();
        assert!(
            found.iter().any(|d| d.code == "empty-work-interval"),
            "{found:?}"
        );
    }

    #[test]
    fn valid_schedule_has_no_diagnostics() {
        assert_eq!(valid().validate(), Vec::new());
    }

    #[test]
    fn lookups() {
        let s = valid();
        assert_eq!(
            s.activity_by_code("A1010").map(|a| a.id),
            Some(ActivityId(1001))
        );
        assert!(s.activity_by_code("nope").is_none());
        assert_eq!(s.predecessors_of(ActivityId(1001)).count(), 1);
        assert_eq!(s.successors_of(ActivityId(1000)).count(), 1);
        assert_eq!(s.successors_of(ActivityId(1001)).count(), 0);
        let a = s.activity(ActivityId(1000)).unwrap();
        assert_eq!(s.activity_calendar(a).map(|c| c.id), Some(CalendarId(10)));
    }

    #[test]
    fn dangling_activity_references() {
        let mut s = valid();
        let a = s.activities.get_mut(&ActivityId(1000)).unwrap();
        a.wbs = WbsId(999);
        a.calendar = CalendarId(999);
        assert_eq!(
            codes(&s),
            vec![
                (EntityRef::Activity(ActivityId(1000)), "dangling-reference"),
                (EntityRef::Activity(ActivityId(1000)), "dangling-reference"),
            ]
        );
    }

    #[test]
    fn dangling_relationship_ends_and_self_loops() {
        let mut s = valid();
        s.relationships
            .insert(RelationshipId(2), relationship(2, 1000, 4242));
        s.relationships
            .insert(RelationshipId(3), relationship(3, 1001, 1001));
        assert_eq!(
            codes(&s),
            vec![
                (
                    EntityRef::Relationship(RelationshipId(2)),
                    "dangling-reference"
                ),
                (
                    EntityRef::Relationship(RelationshipId(3)),
                    "self-relationship"
                ),
            ]
        );
    }

    #[test]
    fn external_relationship_is_not_dangling() {
        let mut s = valid();
        let mut r = relationship(2, 555, 1000);
        r.predecessor_project = ProjectId(2);
        s.relationships.insert(r.id, r);
        assert!(s.is_external(&s.relationships[&RelationshipId(2)]));
        assert!(!s.is_external(&s.relationships[&RelationshipId(1)]));
        assert_eq!(codes(&s), vec![]);

        let mut r = relationship(3, 555, 556);
        r.predecessor_project = ProjectId(2);
        r.successor_project = ProjectId(3);
        s.relationships.insert(r.id, r);
        assert_eq!(
            codes(&s),
            vec![(
                EntityRef::Relationship(RelationshipId(3)),
                "foreign-relationship"
            )]
        );
    }

    #[test]
    fn wrong_project_and_dangling_parents() {
        let mut s = valid();
        s.activities.get_mut(&ActivityId(1001)).unwrap().project = ProjectId(2);
        s.wbs.get_mut(&WbsId(100)).unwrap().parent = Some(WbsId(1));
        s.calendars.get_mut(&CalendarId(10)).unwrap().base = Some(CalendarId(11));
        s.resources.get_mut(&ResourceId(7)).unwrap().parent = Some(ResourceId(8));
        assert_eq!(
            codes(&s),
            vec![
                (EntityRef::Wbs(WbsId(100)), "dangling-reference"),
                (EntityRef::Activity(ActivityId(1001)), "wrong-project"),
                (EntityRef::Calendar(CalendarId(10)), "dangling-reference"),
                (EntityRef::Resource(ResourceId(7)), "dangling-reference"),
            ]
        );
    }

    #[test]
    fn project_and_assignment_references() {
        let mut s = valid();
        s.project.default_calendar = Some(CalendarId(3));
        s.project.options.float_paths.end_activity = Some(ActivityId(3));
        let asg = s.assignments.get_mut(&AssignmentId(70)).unwrap();
        asg.activity = ActivityId(3);
        asg.resource = Some(ResourceId(3));
        let d = s.validate();
        assert_eq!(d.len(), 4, "{d:#?}");
        assert!(d.iter().all(|d| d.code == "dangling-reference"));
        assert_eq!(
            d[0].to_string(),
            "project 1: references default calendar 3, which is not in the schedule (dangling-reference)"
        );
    }

    #[test]
    fn code_and_udf_integrity() {
        let mut s = valid();
        s.activity_code_types.insert(
            ActivityCodeTypeId(6),
            ActivityCodeType {
                id: ActivityCodeTypeId(6),
                name: "Area".into(),
                ..s.activity_code_types[&ActivityCodeTypeId(5)].clone()
            },
        );
        let a = s.activities.get_mut(&ActivityId(1001)).unwrap();
        a.codes.insert(ActivityCodeTypeId(6), ActivityCodeId(50)); // value of type 5
        a.codes.insert(ActivityCodeTypeId(5), ActivityCodeId(51)); // missing value
        s.wbs
            .get_mut(&WbsId(100))
            .unwrap()
            .udfs
            .insert(UdfTypeId(9), UdfValue::Number(1.5)); // activity UDF on WBS
        s.resources
            .get_mut(&ResourceId(7))
            .unwrap()
            .udfs
            .insert(UdfTypeId(99), UdfValue::Number(2.0));
        s.activity_codes
            .get_mut(&ActivityCodeId(50))
            .unwrap()
            .parent = Some(ActivityCodeId(49));
        assert_eq!(
            codes(&s),
            vec![
                (EntityRef::Wbs(WbsId(100)), "udf-subject-mismatch"),
                (EntityRef::Activity(ActivityId(1001)), "dangling-reference"),
                (EntityRef::Activity(ActivityId(1001)), "code-type-mismatch"),
                (EntityRef::Resource(ResourceId(7)), "dangling-reference"),
                (
                    EntityRef::ActivityCode(ActivityCodeId(50)),
                    "dangling-reference"
                ),
            ]
        );
    }

    #[test]
    fn constraints_that_need_dates() {
        let mut s = valid();
        let a = s.activities.get_mut(&ActivityId(1000)).unwrap();
        a.primary_constraint = Some(Constraint {
            kind: ConstraintType::AsLateAsPossible,
            date: None,
        });
        a.secondary_constraint = Some(Constraint {
            kind: ConstraintType::FinishOnOrBefore,
            date: None,
        });
        assert_eq!(
            codes(&s),
            vec![(
                EntityRef::Activity(ActivityId(1000)),
                "constraint-without-date"
            )]
        );
    }

    #[test]
    fn schedule_option_defaults_are_pinned() {
        // Changing a default changes calculations; update TF-008 and the
        // ScheduleOptions docs table together with this test.
        let o = crate::ScheduleOptions::default();
        assert_eq!(o.out_of_sequence, crate::OutOfSequenceMode::RetainedLogic);
        assert_eq!(o.lag_calendar, crate::LagCalendar::Predecessor);
        assert_eq!(o.float_type, crate::FloatType::FinishFloat);
        assert_eq!(
            o.critical,
            crate::CriticalDefinition::TotalFloatAtMost(crate::Minutes::ZERO)
        );
        assert!(!o.make_open_ended_critical);
        assert!(o.ss_lag_from_early_start);
        assert!(o.use_expected_finish);
        assert!(!o.ignore_external_relationships);
        assert!(o.float_from_project_finish);
        assert!(!o.set_planned_to_forecast);
        assert!(!o.float_paths.enabled);
        assert_eq!(o.float_paths.mode, crate::FloatPathMode::TotalFloat);
        assert_eq!(o.float_paths.max_paths, Some(10));
        assert_eq!(o.float_paths.end_activity, None);
    }

    #[test]
    fn hierarchy_cycles_are_reported_once_on_lowest_id() {
        let mut s = valid();
        let mut w = s.wbs[&WbsId(100)].clone();
        for (id, parent) in [(101, 102), (102, 103), (103, 101), (104, 101)] {
            w.id = WbsId(id);
            w.parent = Some(WbsId(parent));
            w.is_project_node = false;
            s.wbs.insert(w.id, w.clone());
        }
        s.calendars.get_mut(&CalendarId(10)).unwrap().base = Some(CalendarId(10));
        assert_eq!(
            codes(&s),
            vec![
                (EntityRef::Wbs(WbsId(101)), "hierarchy-cycle"),
                (EntityRef::Calendar(CalendarId(10)), "hierarchy-cycle"),
            ]
        );
    }

    #[test]
    fn a_loop_of_parent_roles_is_reported() {
        let mut s = valid();
        for (id, parent) in [(1, 2), (2, 1)] {
            s.roles.insert(
                RoleId(id),
                crate::Role {
                    id: RoleId(id),
                    short_name: format!("R{id}"),
                    name: String::new(),
                    parent: Some(RoleId(parent)),
                    rates: Vec::new(),
                    default_cost_follows_units: false,
                    source: None,
                },
            );
        }
        assert_eq!(
            codes(&s),
            vec![(EntityRef::Role(RoleId(1)), "hierarchy-cycle")]
        );
    }

    #[test]
    fn cycle_finder_handles_chains_and_separate_loops() {
        assert_eq!(
            cycles([(1, Some(2)), (2, Some(3)), (3, None)].into_iter()),
            Vec::<i32>::new()
        );
        assert_eq!(cycles([(1, Some(9))].into_iter()), Vec::<i32>::new());
        assert_eq!(
            cycles(
                [
                    (5, Some(6)),
                    (6, Some(5)),
                    (7, Some(5)),
                    (2, Some(3)),
                    (3, Some(2))
                ]
                .into_iter()
            ),
            vec![2, 5]
        );
    }

    #[test]
    fn code_parent_must_share_code_type() {
        let mut s = valid();
        let mut t = s.activity_code_types[&ActivityCodeTypeId(5)].clone();
        t.id = ActivityCodeTypeId(6);
        s.activity_code_types.insert(t.id, t);
        let mut c = s.activity_codes[&ActivityCodeId(50)].clone();
        c.id = ActivityCodeId(60);
        c.code_type = ActivityCodeTypeId(6);
        c.parent = Some(ActivityCodeId(50));
        s.activity_codes.insert(c.id, c);
        assert_eq!(
            codes(&s),
            vec![(
                EntityRef::ActivityCode(ActivityCodeId(60)),
                "code-type-mismatch"
            )]
        );
    }

    #[test]
    fn project_owned_calendars_and_code_types_must_be_this_projects() {
        let mut s = valid();
        let cal = s.calendars.get_mut(&CalendarId(10)).unwrap();
        cal.kind = CalendarType::Project;
        cal.project = Some(ProjectId(2));
        s.activity_code_types
            .get_mut(&ActivityCodeTypeId(5))
            .unwrap()
            .project = Some(ProjectId(2));
        assert_eq!(
            codes(&s),
            vec![
                (EntityRef::Calendar(CalendarId(10)), "wrong-project"),
                (
                    EntityRef::ActivityCodeType(ActivityCodeTypeId(5)),
                    "wrong-project"
                ),
            ]
        );
    }

    #[test]
    fn udf_values_must_fit_their_type() {
        let mut s = valid();
        let a = s.activities.get_mut(&ActivityId(1000)).unwrap();
        a.udfs.insert(UdfTypeId(9), UdfValue::Number(1.5)); // type 9 is Text
        assert_eq!(
            codes(&s),
            vec![(EntityRef::Activity(ActivityId(1000)), "udf-value-mismatch")]
        );
    }

    #[test]
    fn work_intervals_must_be_sorted_and_disjoint() {
        use crate::{Date, TimeOfDay, WorkInterval};
        let iv = |a: u8, b: u8| {
            WorkInterval::new(
                TimeOfDay::from_hm(a, 0).unwrap(),
                TimeOfDay::from_hm(b, 0).unwrap(),
            )
            .unwrap()
        };
        let mut s = valid();
        let cal = s.calendars.get_mut(&CalendarId(10)).unwrap();
        *cal.work_week.day_mut(Weekday::Monday) = vec![iv(8, 12), iv(13, 17)];
        cal.exceptions
            .insert(Date::from_ymd(2026, 12, 25).unwrap(), vec![]);
        assert_eq!(codes(&s), vec![]);
        let cal = s.calendars.get_mut(&CalendarId(10)).unwrap();
        *cal.work_week.day_mut(Weekday::Tuesday) = vec![iv(8, 12), iv(10, 14)];
        cal.exceptions.insert(
            Date::from_ymd(2026, 12, 24).unwrap(),
            vec![iv(13, 17), iv(8, 12)],
        );
        let d = s.validate();
        assert_eq!(d.len(), 2);
        assert!(d.iter().all(|d| d.code == "overlapping-work-intervals"));
        assert!(d[0].message.starts_with("Tuesday"), "{}", d[0].message);
        assert!(
            d[1].message.starts_with("exception 2026-12-24"),
            "{}",
            d[1].message
        );
    }

    #[test]
    fn percent_complete_range() {
        let mut s = valid();
        s.activities
            .get_mut(&ActivityId(1000))
            .unwrap()
            .physical_percent_complete = Some(100.0);
        assert_eq!(codes(&s), vec![]);
        for bad in [150.0, -1.0, f64::NAN] {
            s.activities
                .get_mut(&ActivityId(1000))
                .unwrap()
                .physical_percent_complete = Some(bad);
            assert_eq!(
                codes(&s),
                vec![(
                    EntityRef::Activity(ActivityId(1000)),
                    "percent-out-of-range"
                )],
                "{bad}"
            );
        }
    }
}
