//! Resources and assignments.

use std::collections::BTreeMap;

use crate::{
    ActivityId, AssignmentId, CalendarId, CostAccountId, DateTime, Minutes, PriceSource, ProjectId,
    RateType, ResourceCurveId, ResourceId, ResourceRateId, ResourceRoleId, ResourceType, RoleId,
    RoleRateId, SourceRef, UdfTypeId, UdfValue,
};

/// A labor, nonlabor or material resource. XER: an `RSRC` row.
#[derive(Debug, Clone, PartialEq)]
pub struct Resource {
    /// XER: `RSRC.rsrc_id`.
    pub id: ResourceId,
    /// Resource ID shown to users. XER: `RSRC.rsrc_short_name`.
    pub short_name: String,
    /// XER: `RSRC.rsrc_name`.
    pub name: String,
    /// XER: `RSRC.rsrc_type`.
    pub kind: ResourceType,
    /// XER: `RSRC.clndr_id`.
    pub calendar: Option<CalendarId>,
    /// Parent in the resource hierarchy. XER: `RSRC.parent_rsrc_id`.
    pub parent: Option<ResourceId>,
    /// Units per hour a new assignment starts with. XER: `RSRC.def_qty_per_hr`.
    pub default_units_per_hour: Option<f64>,
    /// Prices over time, earliest first as the reader leaves them.
    /// XER: `RSRCRATE` rows.
    pub rates: Vec<ResourceRate>,
    /// Whether a new assignment's cost follows its units. A file without the
    /// column reads as `false`; P6's own exports always fill it.
    /// XER: `RSRC.def_cost_qty_link_flag`.
    pub default_cost_follows_units: bool,
    /// The roles it can fill. XER: `RSRCROLE` rows.
    pub roles: Vec<ResourceRole>,
    /// User-defined field values. XER: `UDFVALUE` rows whose type applies to `RSRC`.
    pub udfs: BTreeMap<UdfTypeId, UdfValue>,
    /// Source row link (see [`SourceRef`]).
    pub source: Option<SourceRef>,
}

/// A role a resource can fill. XER: an `RSRCROLE` row.
#[derive(Debug, Clone, PartialEq)]
pub struct ResourceRole {
    /// XER: `RSRCROLE.rsrc_role_id`.
    pub id: ResourceRoleId,
    /// XER: `RSRCROLE.role_id`.
    pub role: RoleId,
    /// Proficiency, as the file gives it. XER: `RSRCROLE.skill_level`.
    pub skill_level: Option<i64>,
    /// Source row link (see [`SourceRef`]).
    pub source: Option<SourceRef>,
}

/// A kind of work, such as a trade, that activities can be staffed by before
/// or instead of a named resource. XER: a `ROLES` row.
#[derive(Debug, Clone, PartialEq)]
pub struct Role {
    /// XER: `ROLES.role_id`.
    pub id: RoleId,
    /// Role ID shown to users. XER: `ROLES.role_short_name`.
    pub short_name: String,
    /// XER: `ROLES.role_name`.
    pub name: String,
    /// Parent in the role hierarchy. XER: `ROLES.parent_role_id`.
    pub parent: Option<RoleId>,
    /// Prices over time. XER: `ROLERATE` rows.
    pub rates: Vec<RoleRate>,
    /// Whether a new assignment's cost follows its units. A file without the
    /// column reads as `false`. XER: `ROLES.def_cost_qty_link_flag`.
    pub default_cost_follows_units: bool,
    /// Source row link (see [`SourceRef`]).
    pub source: Option<SourceRef>,
}

/// A role's prices from a date on. XER: a `ROLERATE` row.
#[derive(Debug, Clone, PartialEq)]
pub struct RoleRate {
    /// XER: `ROLERATE.role_rate_id`.
    pub id: RoleRateId,
    /// When these prices take effect; absent in older files.
    /// XER: `ROLERATE.start_date`.
    pub effective: Option<DateTime>,
    /// Five prices per unit. XER: `ROLERATE.cost_per_qty` to `cost_per_qty5`.
    pub prices: [Option<f64>; 5],
    /// XER: `ROLERATE.max_qty_per_hr`.
    pub max_units_per_hour: Option<f64>,
    /// Source row link (see [`SourceRef`]).
    pub source: Option<SourceRef>,
}

/// A resource's prices from a date on. XER: an `RSRCRATE` row.
#[derive(Debug, Clone, PartialEq)]
pub struct ResourceRate {
    /// XER: `RSRCRATE.rsrc_rate_id`.
    pub id: ResourceRateId,
    /// When these prices take effect. XER: `RSRCRATE.start_date`.
    pub effective: Option<DateTime>,
    /// Five prices per unit, chosen by an assignment's rate type.
    /// XER: `RSRCRATE.cost_per_qty` to `cost_per_qty5`.
    pub prices: [Option<f64>; 5],
    /// XER: `RSRCRATE.max_qty_per_hr`.
    pub max_units_per_hour: Option<f64>,
    /// Source row link (see [`SourceRef`]).
    pub source: Option<SourceRef>,
}

impl Resource {
    /// The rate in force at `when`: the latest to take effect by then. Before
    /// any has, a rate with no readable date is in force, as one that has
    /// always been, or else the earliest. The order of `rates` doesn't matter.
    #[must_use]
    pub fn rate_at(&self, when: Option<DateTime>) -> Option<&ResourceRate> {
        rate_in_force(&self.rates, when, |r| (r.effective, r.id.0))
    }
}

impl Role {
    /// The rate in force at `when`, chosen as [`Resource::rate_at`] chooses.
    #[must_use]
    pub fn rate_at(&self, when: Option<DateTime>) -> Option<&RoleRate> {
        rate_in_force(&self.rates, when, |r| (r.effective, r.id.0))
    }
}

/// Of `rates`, the one in force at `when` (TF-001), each given its start
/// and ID by `key`: as [`Resource::rate_at`] chooses, for any kind of rate.
pub fn rate_in_force<T>(
    rates: &[T],
    when: Option<DateTime>,
    key: impl Fn(&T) -> (Option<DateTime>, u64),
) -> Option<&T> {
    let dated = || rates.iter().filter(|r| key(r).0.is_some());
    let started = dated()
        .filter(|r| when.is_some_and(|w| key(r).0.is_some_and(|e| e <= w)))
        .max_by_key(|r| key(r));
    started
        .or_else(|| {
            let undated = rates.iter().filter(|r| key(r).0.is_none());
            undated.min_by_key(|r| key(r).1)
        })
        .or_else(|| dated().min_by_key(|r| key(r)))
}

/// An assignment's units and costs, as stored. Units are hours for labor and
/// nonlabor resources, and the resource's own unit for material. An empty cell
/// is `None`, so a file reads back as it was.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Amounts {
    /// XER: `TASKRSRC.target_qty`.
    pub budgeted_units: Option<f64>,
    /// Regular-time actual units. XER: `TASKRSRC.act_reg_qty`.
    pub actual_units: Option<f64>,
    /// XER: `TASKRSRC.act_ot_qty`.
    pub actual_overtime_units: Option<f64>,
    /// XER: `TASKRSRC.remain_qty`.
    pub remaining_units: Option<f64>,
    /// XER: `TASKRSRC.target_qty_per_hr`.
    pub budgeted_units_per_hour: Option<f64>,
    /// XER: `TASKRSRC.remain_qty_per_hr`.
    pub remaining_units_per_hour: Option<f64>,
    /// XER: `TASKRSRC.target_cost`.
    pub budgeted_cost: Option<f64>,
    /// Regular-time actual cost. XER: `TASKRSRC.act_reg_cost`.
    pub actual_cost: Option<f64>,
    /// XER: `TASKRSRC.act_ot_cost`.
    pub actual_overtime_cost: Option<f64>,
    /// XER: `TASKRSRC.remain_cost`.
    pub remaining_cost: Option<f64>,
    /// XER: `TASKRSRC.cost_per_qty`.
    pub price_per_unit: Option<f64>,
    /// XER: `TASKRSRC.cost_per_qty_source_type`.
    pub price_source: Option<PriceSource>,
    /// XER: `TASKRSRC.rate_type`.
    pub rate_type: Option<RateType>,
    /// Whether cost follows units at the price. A file without the column
    /// reads as `false`; P6's own exports always fill it.
    /// XER: `TASKRSRC.cost_qty_link_flag`.
    pub cost_follows_units: bool,
}

/// The number of points in a [`ResourceCurve`].
pub const CURVE_POINTS: usize = 21;

/// How an assignment's units spread over its duration. XER: an
/// `RSRCCURVDATA` row.
///
/// Point 0 is the start and points 1 to 20 are the twentieths of the
/// duration, each the percent of the units that falls in it (TF-039).
#[derive(Debug, Clone, PartialEq)]
pub struct ResourceCurve {
    /// XER: `RSRCCURVDATA.curv_id`.
    pub id: ResourceCurveId,
    /// XER: `RSRCCURVDATA.curv_name`.
    pub name: String,
    /// Set on the curves with standard names and clear on a user's own, as far as the
    /// corpus shows. XER: `RSRCCURVDATA.default_flag`.
    pub default: bool,
    /// Percent of the units at each point; they usually sum to 100.
    /// XER: `RSRCCURVDATA.pct_usage_0` to `pct_usage_20`.
    pub points: [f64; CURVE_POINTS],
    /// Source row link (see [`SourceRef`]).
    pub source: Option<SourceRef>,
}

/// One stretch of a hand-typed spread: `units` over the next `work` of the
/// assignment's work time, at an even rate within it (TF-039). XER: one
/// `units:hours` pair of a `TASKRSRC` spread cell.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpreadSegment {
    /// Units in the stretch, as the file stores them.
    pub units: f64,
    /// The stretch's length in work time on the assignment's calendar.
    pub work: Minutes,
}

/// An assignment's units typed in by hand, as stretches of work time in
/// order from its start (TF-039). An empty list is an empty cell.
///
/// The units are kept as the file stores them; they can disagree with the
/// assignment's, and then the stretches give the shape the units follow.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TypedSpread {
    /// The planned spread, from the planned start. XER: `TASKRSRC.target_crv`.
    pub planned: Vec<SpreadSegment>,
    /// The remaining spread, from the remaining start. XER:
    /// `TASKRSRC.remain_crv`.
    pub remaining: Vec<SpreadSegment>,
    /// The actual spread, from the actual start. XER: `TASKRSRC.actual_crv`.
    pub actual: Vec<SpreadSegment>,
}

impl TypedSpread {
    /// Whether no part has a stretch.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.planned.is_empty() && self.remaining.is_empty() && self.actual.is_empty()
    }
}

/// A resource assigned to an activity. XER: a `TASKRSRC` row.
///
/// Dates ride along in the source row until resource scheduling (M9) needs
/// them.
#[derive(Debug, Clone, PartialEq)]
pub struct Assignment {
    /// XER: `TASKRSRC.taskrsrc_id`.
    pub id: AssignmentId,
    /// XER: `TASKRSRC.task_id`.
    pub activity: ActivityId,
    /// The resource, or none for an assignment by role alone. XER: `TASKRSRC.rsrc_id`.
    pub resource: Option<ResourceId>,
    /// The role the resource fills, or the role wanted when there's no
    /// resource. XER: `TASKRSRC.role_id`.
    pub role: Option<RoleId>,
    /// The cost account its costs are booked to. XER: `TASKRSRC.acct_id`.
    pub cost_account: Option<CostAccountId>,
    /// XER: `TASKRSRC.proj_id`.
    pub project: ProjectId,
    /// The curve its units spread by, or none for an even spread.
    /// XER: `TASKRSRC.curv_id`.
    pub curve: Option<ResourceCurveId>,
    /// Units typed in by hand for each stretch of its work, in place of a
    /// curve. An assignment has a curve or a typed spread, not both.
    /// XER: `TASKRSRC.target_crv`, `remain_crv` and `actual_crv`, with
    /// `curv_id` naming a curve the file doesn't hold (TF-039).
    pub typed_spread: Option<TypedSpread>,
    /// Units and costs.
    pub amounts: Amounts,
    /// User-defined field values. XER: `UDFVALUE` rows whose type applies to `TASKRSRC`.
    pub udfs: BTreeMap<UdfTypeId, UdfValue>,
    /// Source row link (see [`SourceRef`]).
    pub source: Option<SourceRef>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rate(id: u64, from: Option<&str>, price: f64) -> ResourceRate {
        ResourceRate {
            id: ResourceRateId(id),
            effective: from.and_then(|f| f.parse().ok()),
            prices: [Some(price), None, None, None, None],
            max_units_per_hour: None,
            source: None,
        }
    }

    fn crew(rates: Vec<ResourceRate>) -> Resource {
        Resource {
            id: ResourceId(1),
            short_name: "CREW".to_owned(),
            name: "Crew".to_owned(),
            kind: ResourceType::Labor,
            calendar: None,
            parent: None,
            default_units_per_hour: None,
            rates,
            roles: Vec::new(),
            default_cost_follows_units: false,
            udfs: BTreeMap::new(),
            source: None,
        }
    }

    #[test]
    fn the_rate_in_force_doesn_t_depend_on_order() {
        let when = "2026-07-01 08:00".parse().ok();
        let held = crew(vec![
            rate(2, Some("2026-06-01 00:00"), 80.0),
            rate(1, Some("2026-01-01 00:00"), 50.0),
        ]);
        assert_eq!(held.rate_at(when).map(|r| r.id), Some(ResourceRateId(2)));
        let early = "2025-01-01 08:00".parse().ok();
        assert_eq!(held.rate_at(early).map(|r| r.id), Some(ResourceRateId(1)));
        assert_eq!(held.rate_at(None).map(|r| r.id), Some(ResourceRateId(1)));
        let undated = crew(vec![rate(3, None, 99.0)]);
        assert_eq!(undated.rate_at(when).map(|r| r.id), Some(ResourceRateId(3)));
    }

    /// An undated rate has always been in force, so a dated one takes over
    /// from it on its own date and not before.
    #[test]
    fn an_undated_rate_holds_until_the_first_dated_one() {
        let held = crew(vec![
            rate(2, Some("2026-06-01 00:00"), 80.0),
            rate(3, None, 99.0),
            rate(1, Some("2026-01-01 00:00"), 50.0),
        ]);
        let at = |when: &str| held.rate_at(when.parse().ok()).map(|r| r.id);
        assert_eq!(at("2026-07-01 08:00"), Some(ResourceRateId(2)));
        assert_eq!(at("2026-03-01 08:00"), Some(ResourceRateId(1)));
        assert_eq!(at("2025-01-01 08:00"), Some(ResourceRateId(3)));
        assert_eq!(held.rate_at(None).map(|r| r.id), Some(ResourceRateId(3)));
    }
}
