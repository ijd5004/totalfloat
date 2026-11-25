//! Shared dictionaries, resources/calendars and per-entity code/UDF assignments.
use super::{
    DiagnosticKind, Diagnostics, date, entities::insert, enums, number, optional_id, parent_id,
    text, unsigned,
};
use crate::{
    calendar_data::CalendarSet,
    typed::{self, CellState, TypedTable, TypedXer},
};
use std::collections::{BTreeMap, BTreeSet};
use totalfloat_model as m;

pub(super) fn apply(
    typed: &TypedXer<'_>,
    calendars: &CalendarSet<'_>,
    schedules: &mut BTreeMap<m::ProjectId, m::Schedule>,
    report: &mut Diagnostics,
) {
    let (mut resources, code_types, codes, udf_types) = dictionaries(typed, report);
    let roles = roles(typed, report);
    resource_roles(typed, &mut resources, &roles, report);
    let accounts = cost_accounts(typed, report);
    let categories = expense_categories(typed, report);
    let curves = resource_curves(typed, report);
    let (project_code_types, project_codes) = project_code_dictionaries(typed, report);
    let obs = obs(typed, report);
    // Roles, cost accounts, expense categories and resource curves are small
    // dictionaries for the whole file, so every project holds them, as with
    // UDF definitions.
    for schedule in schedules.values_mut() {
        schedule.udf_types.clone_from(&udf_types);
        schedule.roles.clone_from(&roles);
        schedule.cost_accounts.clone_from(&accounts);
        schedule.expense_categories.clone_from(&categories);
        schedule.resource_curves.clone_from(&curves);
        schedule.project_code_types.clone_from(&project_code_types);
        schedule.project_codes.clone_from(&project_codes);
        schedule.obs.clone_from(&obs);
    }
    project_code_assignments(typed, schedules, report);
    assignments(typed, schedules, report);
    expenses(typed, schedules, report);
    code_assignments(typed, schedules, report);
    let index = Index::new(typed, calendars, &code_types, &codes);
    let (spare_resources, spare_code_types) = unused(&index, &resources, &code_types, schedules);
    for schedule in schedules.values_mut() {
        let mut needed: BTreeSet<_> = schedule
            .assignments
            .values()
            .filter_map(|a| a.resource)
            .collect();
        needed.extend(&spare_resources);
        // Include the responsible resource when exported separately from assignments.
        if let Some(ids) = index.responsible.get(&schedule.project.id) {
            needed.extend(ids);
        }
        let mut pending: Vec<_> = needed.iter().copied().collect();
        while let Some(id) = pending.pop() {
            if let Some(resource) = resources.get(&id) {
                if let Some(parent) = resource.parent
                    && needed.insert(parent)
                {
                    pending.push(parent);
                }
                schedule.resources.insert(id, resource.clone());
            }
        }
        let mut selected: BTreeSet<_> = schedule
            .activities
            .values()
            .flat_map(|a| a.codes.keys().copied())
            .collect();
        if let Some(ids) = index.project_code_types.get(&schedule.project.id) {
            selected.extend(ids);
        }
        selected.extend(&spare_code_types);
        for id in selected {
            if let Some(kind) = code_types.get(&id) {
                schedule.activity_code_types.insert(id, kind.clone());
                for code in index.codes.get(&id).into_iter().flatten() {
                    schedule.activity_codes.insert(code.id, (*code).clone());
                }
            }
        }
        copy_calendars(&index, schedule, report);
    }
    udfs(typed, &udf_types, schedules, report);
}

/// Resources no activity in the file uses, with their parents, and code types
/// no activity uses that no one project owns. They belong to no project in
/// particular, so every project holds them: an edit can use them, and one an
/// edit adds is still there when the file is read again.
fn unused(
    index: &Index<'_>,
    resources: &BTreeMap<m::ResourceId, m::Resource>,
    code_types: &BTreeMap<m::ActivityCodeTypeId, m::ActivityCodeType>,
    schedules: &BTreeMap<m::ProjectId, m::Schedule>,
) -> (BTreeSet<m::ResourceId>, BTreeSet<m::ActivityCodeTypeId>) {
    let mut used: BTreeSet<m::ResourceId> = schedules
        .values()
        .flat_map(|s| s.assignments.values().filter_map(|a| a.resource))
        .chain(index.responsible.values().flatten().copied())
        .collect();
    let mut pending: Vec<_> = used.iter().copied().collect();
    while let Some(id) = pending.pop() {
        if let Some(parent) = resources.get(&id).and_then(|r| r.parent)
            && used.insert(parent)
        {
            pending.push(parent);
        }
    }
    let spare_resources = resources
        .keys()
        .filter(|id| !used.contains(id))
        .copied()
        .collect();
    let coded: BTreeSet<m::ActivityCodeTypeId> = schedules
        .values()
        .flat_map(|s| s.activities.values().flat_map(|a| a.codes.keys().copied()))
        .collect();
    let spare_code_types = code_types
        .values()
        .filter(|kind| kind.project.is_none() && !coded.contains(&kind.id))
        .map(|kind| kind.id)
        .collect();
    (spare_resources, spare_code_types)
}

/// Per-file lookups, built once so per-schedule work doesn't rescan whole
/// tables (enterprise exports hold 100+ projects).
struct Index<'a> {
    /// `TASK.rsrc_id` values by owning project.
    responsible: BTreeMap<m::ProjectId, BTreeSet<m::ResourceId>>,
    /// Project-scoped code types by owning project.
    project_code_types: BTreeMap<m::ProjectId, Vec<m::ActivityCodeTypeId>>,
    /// Code values by code type.
    codes: BTreeMap<m::ActivityCodeTypeId, Vec<&'a m::ActivityCode>>,
    /// Calendars with an effective model.
    calendars: BTreeMap<m::CalendarId, &'a m::Calendar>,
    /// Project-scoped calendars by owning project.
    project_calendars: BTreeMap<m::ProjectId, Vec<m::CalendarId>>,
}

impl<'a> Index<'a> {
    fn new(
        typed: &TypedXer<'_>,
        calendars: &'a CalendarSet<'_>,
        code_types: &BTreeMap<m::ActivityCodeTypeId, m::ActivityCodeType>,
        codes: &'a BTreeMap<m::ActivityCodeId, m::ActivityCode>,
    ) -> Self {
        let mut responsible: BTreeMap<_, BTreeSet<_>> = BTreeMap::new();
        for table in &typed.tables {
            if let TypedTable::Task { rows, .. } = table {
                for row in rows {
                    if let (Some(project), Some(id)) =
                        (optional_id(&row.proj_id), parent_id(&row.rsrc_id))
                    {
                        responsible
                            .entry(m::ProjectId(project))
                            .or_default()
                            .insert(m::ResourceId(id));
                    }
                }
            }
        }
        let mut project_code_types: BTreeMap<_, Vec<_>> = BTreeMap::new();
        for kind in code_types.values() {
            if let Some(project) = kind.project {
                project_code_types.entry(project).or_default().push(kind.id);
            }
        }
        let mut by_type: BTreeMap<_, Vec<_>> = BTreeMap::new();
        for code in codes.values() {
            by_type.entry(code.code_type).or_default().push(code);
        }
        let pool: BTreeMap<_, _> = calendars
            .entries
            .iter()
            .filter_map(|e| e.calendar.as_ref())
            .map(|c| (c.id, c))
            .collect();
        let mut project_calendars: BTreeMap<_, Vec<_>> = BTreeMap::new();
        for calendar in pool.values() {
            if let Some(project) = calendar.project {
                project_calendars
                    .entry(project)
                    .or_default()
                    .push(calendar.id);
            }
        }
        Self {
            responsible,
            project_code_types,
            codes: by_type,
            calendars: pool,
            project_calendars,
        }
    }
}

type Dictionaries = (
    BTreeMap<m::ResourceId, m::Resource>,
    BTreeMap<m::ActivityCodeTypeId, m::ActivityCodeType>,
    BTreeMap<m::ActivityCodeId, m::ActivityCode>,
    BTreeMap<m::UdfTypeId, m::UdfType>,
);
fn dictionaries(typed: &TypedXer<'_>, report: &mut Diagnostics) -> Dictionaries {
    let (mut resources, mut categories, mut codes, mut udfs) = (
        BTreeMap::new(),
        BTreeMap::new(),
        BTreeMap::new(),
        BTreeMap::new(),
    );
    for table in &typed.tables {
        match table {
            TypedTable::Resource { rows, .. } => {
                for row in rows {
                    if let Some(resource) = resource(row, report) {
                        insert(
                            &mut resources,
                            resource.id,
                            resource,
                            row.meta.source,
                            report,
                        );
                    }
                }
            }
            TypedTable::ActivityCodeType { rows, .. } => {
                for row in rows {
                    let source = row.meta.source;
                    let Some(id) = report
                        .id(&row.actv_code_type_id, source, "actv_code_type_id")
                        .map(m::ActivityCodeTypeId)
                    else {
                        continue;
                    };
                    let Some(scope) = report.code(
                        &row.actv_code_type_scope,
                        source,
                        "actv_code_type_scope",
                        enums::scope,
                    ) else {
                        continue;
                    };
                    let kind = m::ActivityCodeType {
                        id,
                        name: text(&row.actv_code_type),
                        scope,
                        project: if scope == m::CodeScope::Project {
                            optional_id(&row.proj_id).map(m::ProjectId)
                        } else {
                            None
                        },
                        max_length: unsigned(&row.actv_short_len),
                        sequence: row.seq_num.value().copied().unwrap_or(0),
                        source: source.to_source_ref(),
                    };
                    insert(&mut categories, id, kind, source, report);
                }
            }
            TypedTable::ActivityCode { rows, .. } => {
                for row in rows {
                    let source = row.meta.source;
                    let Some(id) = report
                        .id(&row.actv_code_id, source, "actv_code_id")
                        .map(m::ActivityCodeId)
                    else {
                        continue;
                    };
                    let Some(code_type) = report
                        .id(&row.actv_code_type_id, source, "actv_code_type_id")
                        .map(m::ActivityCodeTypeId)
                    else {
                        continue;
                    };
                    let code = m::ActivityCode {
                        id,
                        code_type,
                        parent: parent_id(&row.parent_actv_code_id).map(m::ActivityCodeId),
                        value: text(&row.short_name),
                        description: text(&row.actv_code_name),
                        sequence: row.seq_num.value().copied().unwrap_or(0),
                        source: source.to_source_ref(),
                    };
                    insert(&mut codes, id, code, source, report);
                }
            }
            TypedTable::UdfType { rows, .. } => {
                for row in rows {
                    if let Some(kind) = udf_definition(row, report) {
                        insert(&mut udfs, kind.id, kind, row.meta.source, report);
                    }
                }
            }
            _ => {}
        }
    }
    rates(typed, &mut resources, report);
    (resources, categories, codes, udfs)
}

/// Attaches each resource's prices over time, earliest first.
fn rates(
    typed: &TypedXer<'_>,
    resources: &mut BTreeMap<m::ResourceId, m::Resource>,
    report: &mut Diagnostics,
) {
    for table in &typed.tables {
        let TypedTable::ResourceRate { rows, .. } = table else {
            continue;
        };
        for row in rows {
            let source = row.meta.source;
            let Some(id) = report
                .id(&row.rsrc_rate_id, source, "rsrc_rate_id")
                .map(m::ResourceRateId)
            else {
                continue;
            };
            let owner = report
                .id(&row.rsrc_id, source, "rsrc_id")
                .map(m::ResourceId);
            let Some(resource) = owner.and_then(|owner| resources.get_mut(&owner)) else {
                report.warn(
                    DiagnosticKind::OrphanRow,
                    Some(source),
                    "rsrc_id",
                    "rate's resource is absent",
                );
                continue;
            };
            if resource.rates.iter().any(|r| r.id == id) {
                report.warn(
                    DiagnosticKind::DuplicateId,
                    Some(source),
                    "rsrc_rate_id",
                    "rate ID repeated; the first row wins",
                );
                continue;
            }
            resource.rates.push(m::ResourceRate {
                id,
                effective: date(&row.start_date),
                prices: [
                    number(&row.cost_per_qty),
                    number(&row.cost_per_qty2),
                    number(&row.cost_per_qty3),
                    number(&row.cost_per_qty4),
                    number(&row.cost_per_qty5),
                ],
                max_units_per_hour: number(&row.max_qty_per_hr),
                source: source.to_source_ref(),
            });
        }
    }
    for resource in resources.values_mut() {
        resource.rates.sort_by_key(|r| (r.effective, r.id));
    }
}

/// The roles, each with its prices over time, earliest first.
fn roles(typed: &TypedXer<'_>, report: &mut Diagnostics) -> BTreeMap<m::RoleId, m::Role> {
    let mut roles = BTreeMap::new();
    for table in &typed.tables {
        let TypedTable::Role { rows, .. } = table else {
            continue;
        };
        for row in rows {
            let source = row.meta.source;
            let Some(id) = report.id(&row.role_id, source, "role_id").map(m::RoleId) else {
                continue;
            };
            let role = m::Role {
                id,
                short_name: text(&row.role_short_name),
                name: text(&row.role_name),
                parent: parent_id(&row.parent_role_id).map(m::RoleId),
                rates: Vec::new(),
                default_cost_follows_units: row
                    .def_cost_qty_link_flag
                    .value()
                    .copied()
                    .unwrap_or(false),
                source: source.to_source_ref(),
            };
            insert(&mut roles, id, role, source, report);
        }
    }
    for table in &typed.tables {
        let TypedTable::RoleRate { rows, .. } = table else {
            continue;
        };
        for row in rows {
            let source = row.meta.source;
            let Some(id) = report
                .id(&row.role_rate_id, source, "role_rate_id")
                .map(m::RoleRateId)
            else {
                continue;
            };
            let owner = report.id(&row.role_id, source, "role_id").map(m::RoleId);
            let Some(role) = owner.and_then(|owner| roles.get_mut(&owner)) else {
                report.warn(
                    DiagnosticKind::OrphanRow,
                    Some(source),
                    "role_id",
                    "rate's role is absent",
                );
                continue;
            };
            if role.rates.iter().any(|r| r.id == id) {
                report.warn(
                    DiagnosticKind::DuplicateId,
                    Some(source),
                    "role_rate_id",
                    "rate ID repeated; the first row wins",
                );
                continue;
            }
            role.rates.push(m::RoleRate {
                id,
                effective: date(&row.start_date),
                prices: [
                    number(&row.cost_per_qty),
                    number(&row.cost_per_qty2),
                    number(&row.cost_per_qty3),
                    number(&row.cost_per_qty4),
                    number(&row.cost_per_qty5),
                ],
                max_units_per_hour: number(&row.max_qty_per_hr),
                source: source.to_source_ref(),
            });
        }
    }
    for role in roles.values_mut() {
        role.rates.sort_by_key(|r| (r.effective, r.id));
    }
    roles
}

/// Attaches the roles each resource can fill, in file order.
fn resource_roles(
    typed: &TypedXer<'_>,
    resources: &mut BTreeMap<m::ResourceId, m::Resource>,
    roles: &BTreeMap<m::RoleId, m::Role>,
    report: &mut Diagnostics,
) {
    for table in &typed.tables {
        let TypedTable::ResourceRole { rows, .. } = table else {
            continue;
        };
        for row in rows {
            let source = row.meta.source;
            let Some(id) = report
                .id(&row.rsrc_role_id, source, "rsrc_role_id")
                .map(m::ResourceRoleId)
            else {
                continue;
            };
            let Some(role) = report
                .id(&row.role_id, source, "role_id")
                .map(m::RoleId)
                .filter(|role| roles.contains_key(role))
            else {
                report.warn(
                    DiagnosticKind::OrphanRow,
                    Some(source),
                    "role_id",
                    "resource role's role is absent",
                );
                continue;
            };
            let owner = report
                .id(&row.rsrc_id, source, "rsrc_id")
                .map(m::ResourceId);
            let Some(resource) = owner.and_then(|owner| resources.get_mut(&owner)) else {
                report.warn(
                    DiagnosticKind::OrphanRow,
                    Some(source),
                    "rsrc_id",
                    "resource role's resource is absent",
                );
                continue;
            };
            if resource.roles.iter().any(|r| r.id == id) {
                report.warn(
                    DiagnosticKind::DuplicateId,
                    Some(source),
                    "rsrc_role_id",
                    "resource role ID repeated; the first row wins",
                );
                continue;
            }
            resource.roles.push(m::ResourceRole {
                id,
                role,
                skill_level: row.skill_level.value().copied(),
                source: source.to_source_ref(),
            });
        }
    }
}

fn assignments(
    typed: &TypedXer<'_>,
    schedules: &mut BTreeMap<m::ProjectId, m::Schedule>,
    report: &mut Diagnostics,
) {
    // Curves the file lists, read or not: naming one isn't the typed-spread
    // marker, even if its row couldn't be read (TF-040).
    let listed: BTreeSet<u64> = typed
        .tables
        .iter()
        .filter_map(|t| match t {
            TypedTable::ResourceCurve { rows, .. } => Some(rows),
            _ => None,
        })
        .flatten()
        .filter_map(|row| optional_id(&row.curv_id))
        .collect();
    for table in &typed.tables {
        if let TypedTable::TaskResource { rows, .. } = table {
            for row in rows {
                let source = row.meta.source;
                let Some(project) = report.id(&row.proj_id, source, "proj_id").map(m::ProjectId)
                else {
                    continue;
                };
                let Some(schedule) = schedules.get_mut(&project) else {
                    report.warn(
                        DiagnosticKind::OrphanRow,
                        Some(source),
                        "proj_id",
                        "assignment owner is absent",
                    );
                    continue;
                };
                let Some(id) = report
                    .id(&row.taskrsrc_id, source, "taskrsrc_id")
                    .map(m::AssignmentId)
                else {
                    continue;
                };
                let Some(activity) = report
                    .id(&row.task_id, source, "task_id")
                    .map(m::ActivityId)
                else {
                    continue;
                };
                let role = parent_id(&row.role_id).map(m::RoleId);
                // An assignment can name a role and no resource yet.
                let by_role = role.is_some()
                    && matches!(row.rsrc_id.state(), CellState::Empty | CellState::Missing);
                let resource = if by_role {
                    None
                } else {
                    let Some(resource) = report
                        .id(&row.rsrc_id, source, "rsrc_id")
                        .map(m::ResourceId)
                    else {
                        continue;
                    };
                    Some(resource)
                };
                let mut curve = optional_id(&row.curv_id).map(m::ResourceCurveId);
                let amounts = amounts(row, report);
                let typed_spread = if curve.is_some_and(|c| !listed.contains(&c.0)) {
                    typed_spread(row, &amounts, report)
                } else {
                    None
                };
                if typed_spread.is_some() {
                    curve = None;
                }
                insert(
                    &mut schedule.assignments,
                    id,
                    m::Assignment {
                        id,
                        activity,
                        resource,
                        role,
                        cost_account: parent_id(&row.acct_id).map(m::CostAccountId),
                        project,
                        curve,
                        typed_spread,
                        amounts,
                        udfs: BTreeMap::new(),
                        source: source.to_source_ref(),
                    },
                    source,
                    report,
                );
            }
        }
    }
}

/// The hand-typed spread of an assignment that names a curve the file
/// doesn't hold, the file's marker for one (TF-040), or `None` if it has no
/// stored spread or one can't be read, which is reported. Units that differ
/// from the assignment's are reported and kept.
fn typed_spread(
    row: &typed::TaskResourceRow<'_>,
    amounts: &m::Amounts,
    report: &mut Diagnostics,
) -> Option<m::TypedSpread> {
    let source = row.meta.source;
    let actual = match (amounts.actual_units, amounts.actual_overtime_units) {
        (None, None) => None,
        (regular, overtime) => Some(regular.unwrap_or(0.0) + overtime.unwrap_or(0.0)),
    };
    let cells = [
        ("target_crv", text(&row.target_crv), amounts.budgeted_units),
        ("remain_crv", text(&row.remain_crv), amounts.remaining_units),
        ("actual_crv", text(&row.actual_crv), actual),
    ];
    if cells.iter().all(|(_, cell, _)| cell.is_empty()) {
        return None;
    }
    let mut read = Vec::new();
    for (field, cell, units) in cells {
        match crate::spread_data::decode(&cell) {
            Ok(segments) => {
                if !segments.is_empty() && !matches_units(&segments, units) {
                    report.warn(
                        DiagnosticKind::InconsistentValues,
                        Some(source),
                        field,
                        "hand-typed spread units differ from the assignment's; spread by its shape",
                    );
                }
                read.push(segments);
            }
            Err(error) => {
                report.warn(
                    DiagnosticKind::UnrepresentedValue,
                    Some(source),
                    field,
                    format!("hand-typed spread is unreadable ({error}); retained raw"),
                );
            }
        }
    }
    // A part that can't be read leaves fewer than three.
    let [planned, remaining, actual] = <[Vec<m::SpreadSegment>; 3]>::try_from(read).ok()?;
    Some(m::TypedSpread {
        planned,
        remaining,
        actual,
    })
}

/// Whether the stretches hold the assignment's units, allowing for each
/// stretch's rounding to three decimal places.
#[allow(clippy::cast_precision_loss, reason = "a cell holds few stretches")]
fn matches_units(segments: &[m::SpreadSegment], units: Option<f64>) -> bool {
    let sum: f64 = segments.iter().map(|s| s.units).sum();
    let slack = 0.0005 * segments.len() as f64 + 1e-9 * sum.abs();
    (sum - units.unwrap_or(0.0)).abs() <= slack
}

/// The resource curves. A curve missing a point is left out, as its spread
/// can't be known.
fn resource_curves(
    typed: &TypedXer<'_>,
    report: &mut Diagnostics,
) -> BTreeMap<m::ResourceCurveId, m::ResourceCurve> {
    let mut curves = BTreeMap::new();
    for table in &typed.tables {
        let TypedTable::ResourceCurve { rows, .. } = table else {
            continue;
        };
        for row in rows {
            let source = row.meta.source;
            let Some(id) = report
                .id(&row.curv_id, source, "curv_id")
                .map(m::ResourceCurveId)
            else {
                continue;
            };
            let cells = [
                &row.pct_usage_0,
                &row.pct_usage_1,
                &row.pct_usage_2,
                &row.pct_usage_3,
                &row.pct_usage_4,
                &row.pct_usage_5,
                &row.pct_usage_6,
                &row.pct_usage_7,
                &row.pct_usage_8,
                &row.pct_usage_9,
                &row.pct_usage_10,
                &row.pct_usage_11,
                &row.pct_usage_12,
                &row.pct_usage_13,
                &row.pct_usage_14,
                &row.pct_usage_15,
                &row.pct_usage_16,
                &row.pct_usage_17,
                &row.pct_usage_18,
                &row.pct_usage_19,
                &row.pct_usage_20,
            ];
            let mut points = [0.0; m::CURVE_POINTS];
            let mut readable = true;
            for (point, cell) in points.iter_mut().zip(cells) {
                match number(cell) {
                    Some(value) => *point = value,
                    None => readable = false,
                }
            }
            if !readable {
                report.warn(
                    DiagnosticKind::SkippedRow,
                    Some(source),
                    "pct_usage",
                    "curve point is missing or unreadable; row retained raw",
                );
                continue;
            }
            let curve = m::ResourceCurve {
                id,
                name: text(&row.curv_name),
                default: row.default_flag.value().copied().unwrap_or(false),
                points,
                source: source.to_source_ref(),
            };
            insert(&mut curves, id, curve, source, report);
        }
    }
    curves
}

/// The OBS: who is responsible for work, a dictionary for the whole file.
fn obs(typed: &TypedXer<'_>, report: &mut Diagnostics) -> BTreeMap<m::ObsId, m::Obs> {
    let mut nodes = BTreeMap::new();
    for table in &typed.tables {
        let TypedTable::Obs { rows, .. } = table else {
            continue;
        };
        for row in rows {
            let source = row.meta.source;
            let Some(id) = report.id(&row.obs_id, source, "obs_id").map(m::ObsId) else {
                continue;
            };
            let node = m::Obs {
                id,
                parent: parent_id(&row.parent_obs_id).map(m::ObsId),
                name: text(&row.obs_name),
                description: text(&row.obs_descr),
                sequence: row.seq_num.value().copied().unwrap_or(0),
                source: source.to_source_ref(),
            };
            insert(&mut nodes, id, node, source, report);
        }
    }
    nodes
}

type ProjectCodeDictionaries = (
    BTreeMap<m::ProjectCodeTypeId, m::ProjectCodeType>,
    BTreeMap<m::ProjectCodeId, m::ProjectCode>,
);

/// Project code types and values. They belong to the whole database, so every
/// project holds them all. A value whose type isn't in the file is left out.
fn project_code_dictionaries(
    typed: &TypedXer<'_>,
    report: &mut Diagnostics,
) -> ProjectCodeDictionaries {
    let (mut kinds, mut codes) = (BTreeMap::new(), BTreeMap::new());
    for table in &typed.tables {
        if let TypedTable::ProjectCodeType { rows, .. } = table {
            for row in rows {
                let source = row.meta.source;
                let Some(id) = report
                    .id(&row.proj_catg_type_id, source, "proj_catg_type_id")
                    .map(m::ProjectCodeTypeId)
                else {
                    continue;
                };
                let kind = m::ProjectCodeType {
                    id,
                    name: text(&row.proj_catg_type),
                    max_length: unsigned(&row.proj_catg_short_len),
                    sequence: row.seq_num.value().copied().unwrap_or(0),
                    source: source.to_source_ref(),
                };
                insert(&mut kinds, id, kind, source, report);
            }
        }
    }
    for table in &typed.tables {
        if let TypedTable::ProjectCodeValue { rows, .. } = table {
            for row in rows {
                let source = row.meta.source;
                let Some(id) = report
                    .id(&row.proj_catg_id, source, "proj_catg_id")
                    .map(m::ProjectCodeId)
                else {
                    continue;
                };
                let Some(code_type) = report
                    .id(&row.proj_catg_type_id, source, "proj_catg_type_id")
                    .map(m::ProjectCodeTypeId)
                else {
                    continue;
                };
                if !kinds.contains_key(&code_type) {
                    report.warn(
                        DiagnosticKind::OrphanRow,
                        Some(source),
                        "proj_catg_type_id",
                        "project code value's type is not in the file",
                    );
                    continue;
                }
                let code = m::ProjectCode {
                    id,
                    code_type,
                    parent: parent_id(&row.parent_proj_catg_id).map(m::ProjectCodeId),
                    value: text(&row.proj_catg_short_name),
                    description: text(&row.proj_catg_name),
                    sequence: row.seq_num.value().copied().unwrap_or(0),
                    source: source.to_source_ref(),
                };
                insert(&mut codes, id, code, source, report);
            }
        }
    }
    (kinds, codes)
}

/// Each project's code values. One naming a value or project not in the file
/// is left out.
fn project_code_assignments(
    typed: &TypedXer<'_>,
    schedules: &mut BTreeMap<m::ProjectId, m::Schedule>,
    report: &mut Diagnostics,
) {
    for table in &typed.tables {
        let TypedTable::ProjectCodeAssignment { rows, .. } = table else {
            continue;
        };
        for row in rows {
            let source = row.meta.source;
            let (Some(project), Some(kind), Some(value)) = (
                optional_id(&row.proj_id).map(m::ProjectId),
                optional_id(&row.proj_catg_type_id).map(m::ProjectCodeTypeId),
                optional_id(&row.proj_catg_id).map(m::ProjectCodeId),
            ) else {
                report.warn(
                    DiagnosticKind::UnrepresentedValue,
                    Some(source),
                    "PROJPCAT",
                    "project code assignment identity is missing",
                );
                continue;
            };
            let Some(schedule) = schedules.get_mut(&project) else {
                report.warn(
                    DiagnosticKind::OrphanRow,
                    Some(source),
                    "proj_id",
                    "project code assignment has no represented project",
                );
                continue;
            };
            if schedule
                .project_codes
                .get(&value)
                .is_none_or(|code| code.code_type != kind)
            {
                report.warn(
                    DiagnosticKind::OrphanRow,
                    Some(source),
                    "proj_catg_id",
                    "project code assignment names a value not in the file",
                );
                continue;
            }
            insert(&mut schedule.project.codes, kind, value, source, report);
        }
    }
}

fn cost_accounts(
    typed: &TypedXer<'_>,
    report: &mut Diagnostics,
) -> BTreeMap<m::CostAccountId, m::CostAccount> {
    let mut accounts = BTreeMap::new();
    for table in &typed.tables {
        let TypedTable::CostAccount { rows, .. } = table else {
            continue;
        };
        for row in rows {
            let source = row.meta.source;
            let Some(id) = report
                .id(&row.acct_id, source, "acct_id")
                .map(m::CostAccountId)
            else {
                continue;
            };
            let account = m::CostAccount {
                id,
                short_name: text(&row.acct_short_name),
                name: text(&row.acct_name),
                parent: parent_id(&row.parent_acct_id).map(m::CostAccountId),
                sequence: row.acct_seq_num.value().copied(),
                description: text(&row.acct_descr),
                source: source.to_source_ref(),
            };
            insert(&mut accounts, id, account, source, report);
        }
    }
    accounts
}

fn expense_categories(
    typed: &TypedXer<'_>,
    report: &mut Diagnostics,
) -> BTreeMap<m::ExpenseCategoryId, m::ExpenseCategory> {
    let mut categories = BTreeMap::new();
    for table in &typed.tables {
        let TypedTable::ExpenseCategory { rows, .. } = table else {
            continue;
        };
        for row in rows {
            let source = row.meta.source;
            let Some(id) = report
                .id(&row.cost_type_id, source, "cost_type_id")
                .map(m::ExpenseCategoryId)
            else {
                continue;
            };
            let category = m::ExpenseCategory {
                id,
                name: text(&row.cost_type),
                sequence: row.seq_num.value().copied(),
                source: source.to_source_ref(),
            };
            insert(&mut categories, id, category, source, report);
        }
    }
    categories
}

fn expenses(
    typed: &TypedXer<'_>,
    schedules: &mut BTreeMap<m::ProjectId, m::Schedule>,
    report: &mut Diagnostics,
) {
    for table in &typed.tables {
        let TypedTable::Expense { rows, .. } = table else {
            continue;
        };
        for row in rows {
            let source = row.meta.source;
            let Some(project) = report.id(&row.proj_id, source, "proj_id").map(m::ProjectId) else {
                continue;
            };
            let Some(schedule) = schedules.get_mut(&project) else {
                report.warn(
                    DiagnosticKind::OrphanRow,
                    Some(source),
                    "proj_id",
                    "expense owner is absent",
                );
                continue;
            };
            let Some(id) = report
                .id(&row.cost_item_id, source, "cost_item_id")
                .map(m::ExpenseId)
            else {
                continue;
            };
            let Some(activity) = report
                .id(&row.task_id, source, "task_id")
                .map(m::ActivityId)
            else {
                continue;
            };
            let expense = m::Expense {
                id,
                activity,
                project,
                name: text(&row.cost_name),
                cost_account: optional_id(&row.acct_id).map(m::CostAccountId),
                category: optional_id(&row.cost_type_id).map(m::ExpenseCategoryId),
                accrual: report.optional_code(
                    &row.cost_load_type,
                    source,
                    "cost_load_type",
                    enums::accrual,
                ),
                budgeted_cost: number(&row.target_cost),
                actual_cost: number(&row.act_cost),
                remaining_cost: number(&row.remain_cost),
                price_per_unit: number(&row.cost_per_qty),
                budgeted_units: number(&row.target_qty),
                unit_of_measure: text(&row.qty_name),
                auto_compute_actuals: row.auto_compute_act_flag.value().copied().unwrap_or(false),
                po_number: text(&row.po_number),
                vendor: text(&row.vendor_name),
                description: text(&row.cost_descr),
                source: source.to_source_ref(),
            };
            insert(&mut schedule.expenses, id, expense, source, report);
        }
    }
}

fn amounts(row: &typed::TaskResourceRow<'_>, report: &mut Diagnostics) -> m::Amounts {
    let source = row.meta.source;
    m::Amounts {
        budgeted_units: number(&row.target_qty),
        actual_units: number(&row.act_reg_qty),
        actual_overtime_units: number(&row.act_ot_qty),
        remaining_units: number(&row.remain_qty),
        budgeted_units_per_hour: number(&row.target_qty_per_hr),
        remaining_units_per_hour: number(&row.remain_qty_per_hr),
        budgeted_cost: number(&row.target_cost),
        actual_cost: number(&row.act_reg_cost),
        actual_overtime_cost: number(&row.act_ot_cost),
        remaining_cost: number(&row.remain_cost),
        price_per_unit: number(&row.cost_per_qty),
        price_source: report.optional_code(
            &row.cost_per_qty_source_type,
            source,
            "cost_per_qty_source_type",
            enums::price_source,
        ),
        rate_type: report.optional_code(&row.rate_type, source, "rate_type", enums::rate_type),
        cost_follows_units: row.cost_qty_link_flag.value().copied().unwrap_or(false),
    }
}

fn code_assignments(
    typed: &TypedXer<'_>,
    schedules: &mut BTreeMap<m::ProjectId, m::Schedule>,
    report: &mut Diagnostics,
) {
    for table in &typed.tables {
        if let TypedTable::TaskCodeAssignment { rows, .. } = table {
            for row in rows {
                let source = row.meta.source;
                let (Some(project), Some(task), Some(kind), Some(value)) = (
                    optional_id(&row.proj_id).map(m::ProjectId),
                    optional_id(&row.task_id).map(m::ActivityId),
                    optional_id(&row.actv_code_type_id).map(m::ActivityCodeTypeId),
                    optional_id(&row.actv_code_id).map(m::ActivityCodeId),
                ) else {
                    report.warn(
                        DiagnosticKind::UnrepresentedValue,
                        Some(source),
                        "TASKACTV",
                        "code assignment identity is missing",
                    );
                    continue;
                };
                if let Some(activity) = schedules
                    .get_mut(&project)
                    .and_then(|s| s.activities.get_mut(&task))
                {
                    insert(&mut activity.codes, kind, value, source, report);
                } else {
                    report.warn(
                        DiagnosticKind::OrphanRow,
                        Some(source),
                        "task_id",
                        "activity code assignment has no represented target",
                    );
                }
            }
        }
    }
}

fn copy_calendars(index: &Index<'_>, schedule: &mut m::Schedule, report: &mut Diagnostics) {
    let mut needed: BTreeSet<_> = schedule
        .activities
        .values()
        .map(|a| a.calendar)
        .chain(schedule.project.default_calendar)
        .chain(schedule.resources.values().filter_map(|r| r.calendar))
        .collect();
    if let Some(ids) = index.project_calendars.get(&schedule.project.id) {
        needed.extend(ids);
    }
    // Any global calendar can be chosen, whether or not this project uses it yet.
    needed.extend(
        index
            .calendars
            .values()
            .filter(|c| c.kind == m::CalendarType::Global)
            .map(|c| c.id),
    );
    let mut pending: Vec<_> = needed.iter().copied().collect();
    while let Some(id) = pending.pop() {
        if let Some(calendar) = index.calendars.get(&id) {
            if let Some(base) = calendar.base
                && needed.insert(base)
            {
                pending.push(base);
            }
            schedule.calendars.insert(id, (*calendar).clone());
        } else {
            report.warn(
                DiagnosticKind::UnavailableCalendar,
                schedule
                    .project
                    .source
                    .and_then(typed::RowRef::from_source_ref),
                "clndr_id",
                "referenced calendar has no effective model; raw source retained",
            );
        }
    }
}

fn udf_value(row: &typed::UdfValueRow<'_>, kind: &m::UdfDataType) -> Option<m::UdfValue> {
    match kind {
        m::UdfDataType::Text => Some(m::UdfValue::Text(text(&row.udf_text))),
        m::UdfDataType::StartDate | m::UdfDataType::FinishDate => {
            date(&row.udf_date).map(m::UdfValue::Date)
        }
        m::UdfDataType::Cost | m::UdfDataType::Number | m::UdfDataType::Integer => {
            number(&row.udf_number).map(m::UdfValue::Number)
        }
        m::UdfDataType::Code => {
            optional_id(&row.udf_code_id).map(|id| m::UdfValue::Code(m::UdfCodeId(id)))
        }
        m::UdfDataType::Indicator | m::UdfDataType::Other(_) => optional_id(&row.udf_code_id)
            .map(|id| m::UdfValue::Code(m::UdfCodeId(id)))
            .or_else(|| number(&row.udf_number).map(m::UdfValue::Number))
            .or_else(|| date(&row.udf_date).map(m::UdfValue::Date))
            .or_else(|| Some(m::UdfValue::Text(text(&row.udf_text)))),
    }
}

fn udfs(
    typed: &TypedXer<'_>,
    definitions: &BTreeMap<m::UdfTypeId, m::UdfType>,
    schedules: &mut BTreeMap<m::ProjectId, m::Schedule>,
    report: &mut Diagnostics,
) {
    for table in &typed.tables {
        if let TypedTable::UdfValue { rows, .. } = table {
            for row in rows {
                let source = row.meta.source;
                let (Some(type_id), Some(target)) = (
                    optional_id(&row.udf_type_id).map(m::UdfTypeId),
                    optional_id(&row.fk_id),
                ) else {
                    report.warn(
                        DiagnosticKind::UnattachedUdf,
                        Some(source),
                        "fk_id",
                        "UDF identity is missing",
                    );
                    continue;
                };
                let Some(kind) = definitions.get(&type_id) else {
                    report.warn(
                        DiagnosticKind::UnattachedUdf,
                        Some(source),
                        "udf_type_id",
                        "UDF definition is absent",
                    );
                    continue;
                };
                let Some(value) = udf_value(row, &kind.data_type) else {
                    report.warn(
                        DiagnosticKind::UnattachedUdf,
                        Some(source),
                        "UDFVALUE",
                        "UDF value cannot be represented",
                    );
                    continue;
                };
                let assignment = UdfAssignment {
                    type_id,
                    target,
                    value,
                    source,
                };
                let attached = if let Some(project) = parent_id(&row.proj_id) {
                    schedules
                        .get_mut(&m::ProjectId(project))
                        .is_some_and(|schedule| assignment.attach(kind, schedule, report))
                } else {
                    let mut attached = false;
                    for schedule in schedules.values_mut() {
                        attached |= assignment.attach(kind, schedule, report);
                    }
                    attached
                };
                if !attached {
                    report.warn(
                        DiagnosticKind::UnattachedUdf,
                        Some(source),
                        "fk_id",
                        "UDF target is absent, unrepresented or outside imported projects",
                    );
                }
            }
        }
    }
}

/// A decoded `UDFVALUE` row, ready to attach to its target in a schedule.
struct UdfAssignment {
    type_id: m::UdfTypeId,
    target: u64,
    value: m::UdfValue,
    source: typed::RowRef,
}

impl UdfAssignment {
    fn attach(
        &self,
        kind: &m::UdfType,
        schedule: &mut m::Schedule,
        report: &mut Diagnostics,
    ) -> bool {
        let target = self.target;
        let values = match kind.subject {
            m::UdfSubject::Project if schedule.project.id == m::ProjectId(target) => {
                Some(&mut schedule.project.udfs)
            }
            m::UdfSubject::Wbs => schedule.wbs.get_mut(&m::WbsId(target)).map(|x| &mut x.udfs),
            m::UdfSubject::Activity => schedule
                .activities
                .get_mut(&m::ActivityId(target))
                .map(|x| &mut x.udfs),
            m::UdfSubject::Resource => schedule
                .resources
                .get_mut(&m::ResourceId(target))
                .map(|x| &mut x.udfs),
            m::UdfSubject::Assignment => schedule
                .assignments
                .get_mut(&m::AssignmentId(target))
                .map(|x| &mut x.udfs),
            _ => None,
        };
        let Some(values) = values else {
            return false;
        };
        insert(
            values,
            self.type_id,
            self.value.clone(),
            self.source,
            report,
        );
        true
    }
}

fn udf_definition(row: &typed::UdfTypeRow<'_>, report: &mut Diagnostics) -> Option<m::UdfType> {
    let source = row.meta.source;
    let id = report
        .id(&row.udf_type_id, source, "udf_type_id")
        .map(m::UdfTypeId)?;
    let subject = match row.table_name.value().copied().unwrap_or("") {
        "PROJECT" => m::UdfSubject::Project,
        "PROJWBS" => m::UdfSubject::Wbs,
        "TASK" => m::UdfSubject::Activity,
        "RSRC" => m::UdfSubject::Resource,
        "TASKRSRC" => m::UdfSubject::Assignment,
        other => m::UdfSubject::Other(other.to_owned()),
    };
    let data_type = match row.logical_data_type.original().unwrap_or("") {
        "FT_TEXT" => m::UdfDataType::Text,
        "FT_START_DATE" => m::UdfDataType::StartDate,
        "FT_END_DATE" => m::UdfDataType::FinishDate,
        "FT_MONEY" => m::UdfDataType::Cost,
        "FT_FLOAT_2_DECIMALS" => m::UdfDataType::Number,
        "FT_INT" => m::UdfDataType::Integer,
        "FT_STATICTYPE" => m::UdfDataType::Indicator,
        other => m::UdfDataType::Other(other.to_owned()),
    };
    let kind = m::UdfType {
        id,
        subject,
        label: text(&row.udf_type_label),
        data_type,
        source: source.to_source_ref(),
    };
    Some(kind)
}

fn resource(row: &typed::ResourceRow<'_>, report: &mut Diagnostics) -> Option<m::Resource> {
    let source = row.meta.source;
    let id = report
        .id(&row.rsrc_id, source, "rsrc_id")
        .map(m::ResourceId)?;
    let kind = report.code(&row.rsrc_type, source, "rsrc_type", enums::resource)?;
    let resource = m::Resource {
        id,
        short_name: text(&row.rsrc_short_name),
        name: text(&row.rsrc_name),
        kind,
        calendar: parent_id(&row.clndr_id).map(m::CalendarId),
        parent: parent_id(&row.parent_rsrc_id).map(m::ResourceId),
        default_units_per_hour: number(&row.def_qty_per_hr),
        default_cost_follows_units: row.def_cost_qty_link_flag.value().copied().unwrap_or(false),
        rates: Vec::new(),
        roles: Vec::new(),
        udfs: BTreeMap::new(),
        source: source.to_source_ref(),
    };
    Some(resource)
}
