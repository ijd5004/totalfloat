//! P6 code facts already documented on model enum variants.
use totalfloat_model as m;

/// Each table generates both directions: `name(code)` for reading and
/// `name_code(value)` for writing, so the two can't drift apart.
macro_rules! codes {
    ($name:ident, $back:ident -> $type:ty { $( $code:literal => $value:path ),* $(,)? }) => {
        pub(crate) fn $name(source: &str) -> Option<$type> { match source { $( $code => Some($value), )* _ => None } }
        #[allow(dead_code, reason = "write direction is used by the patch writer for some tables only")]
        pub(crate) fn $back(value: $type) -> &'static str { match value { $( $value => $code, )* } }
    };
}

codes!(activity, activity_code -> m::ActivityType {
    "TT_Task" => m::ActivityType::TaskDependent,
    "TT_Rsrc" => m::ActivityType::ResourceDependent,
    "TT_LOE" => m::ActivityType::LevelOfEffort,
    "TT_Mile" => m::ActivityType::StartMilestone,
    "TT_FinMile" => m::ActivityType::FinishMilestone,
    "TT_WBS" => m::ActivityType::WbsSummary
});
codes!(duration, duration_code -> m::DurationType {
    "DT_FixedRate" => m::DurationType::FixedUnitsPerTime,
    "DT_FixedDrtn" => m::DurationType::FixedDurationAndUnitsPerTime,
    "DT_FixedQty" => m::DurationType::FixedUnits,
    "DT_FixedDUR2" => m::DurationType::FixedDurationAndUnits
});
codes!(percent, percent_code -> m::PercentCompleteType {
    "CP_Phys" => m::PercentCompleteType::Physical,
    "CP_Drtn" => m::PercentCompleteType::Duration,
    "CP_Units" => m::PercentCompleteType::Units
});
codes!(status, status_code -> m::ActivityStatus {
    "TK_NotStart" => m::ActivityStatus::NotStarted,
    "TK_Active" => m::ActivityStatus::InProgress,
    "TK_Complete" => m::ActivityStatus::Completed
});
codes!(constraint, constraint_code -> m::ConstraintType {
    "CS_MSO" => m::ConstraintType::StartOn,
    "CS_MSOA" => m::ConstraintType::StartOnOrAfter,
    "CS_MSOB" => m::ConstraintType::StartOnOrBefore,
    "CS_MEO" => m::ConstraintType::FinishOn,
    "CS_MEOA" => m::ConstraintType::FinishOnOrAfter,
    "CS_MEOB" => m::ConstraintType::FinishOnOrBefore,
    "CS_ALAP" => m::ConstraintType::AsLateAsPossible,
    "CS_MANDSTART" => m::ConstraintType::MandatoryStart,
    "CS_MANDFIN" => m::ConstraintType::MandatoryFinish
});
codes!(relationship, relationship_code -> m::RelationshipType {
    "PR_FS" => m::RelationshipType::FinishToStart,
    "PR_SS" => m::RelationshipType::StartToStart,
    "PR_FF" => m::RelationshipType::FinishToFinish,
    "PR_SF" => m::RelationshipType::StartToFinish
});
codes!(resource, resource_code -> m::ResourceType {
    "RT_Labor" => m::ResourceType::Labor,
    "RT_Equip" => m::ResourceType::Nonlabor,
    "RT_Mat" => m::ResourceType::Material
});
codes!(price_source, price_source_code -> m::PriceSource {
    "ST_Rsrc" => m::PriceSource::Resource,
    "ST_Role" => m::PriceSource::Role,
    "ST_Custom" => m::PriceSource::Custom
});
codes!(accrual, accrual_code -> m::Accrual {
    "CL_Start" => m::Accrual::Start,
    "CL_End" => m::Accrual::End,
    "CL_Uniform" => m::Accrual::Uniform
});
codes!(rate_type, rate_type_code -> m::RateType {
    "COST_PER_QTY" => m::RateType::Price1,
    "COST_PER_QTY2" => m::RateType::Price2,
    "COST_PER_QTY3" => m::RateType::Price3,
    "COST_PER_QTY4" => m::RateType::Price4,
    "COST_PER_QTY5" => m::RateType::Price5
});
codes!(scope, scope_code -> m::CodeScope {
    "AS_Global" => m::CodeScope::Global,
    "AS_Project" => m::CodeScope::Project,
    "AS_EPS" => m::CodeScope::Eps
});
codes!(float_type, float_type_code -> m::FloatType {
    "FT_FF" => m::FloatType::FinishFloat,
    "FT_SS" => m::FloatType::StartFloat,
    "FT_Min" => m::FloatType::SmallestFloat
});

codes!(lag_calendar, lag_calendar_code -> m::LagCalendar {
    "rcal_Predecessor" => m::LagCalendar::Predecessor,
    "rcal_Successor" => m::LagCalendar::Successor,
    "rcal_24Hour" => m::LagCalendar::TwentyFourHour,
    "rcal_ProjDefault" => m::LagCalendar::ProjectDefault
});
