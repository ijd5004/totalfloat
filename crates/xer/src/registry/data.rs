//! Field names and kinds derived from the corpus; see docs/xer-format.md.
use super::{Field, FieldKind, Table};
use FieldKind::{Blob, DateTime, Decimal, Enum, Flag, ForeignId, Guid, Id, Int, Text};

const TASK_TYPE: &[&str] = &[
    "TT_Task",
    "TT_Rsrc",
    "TT_LOE",
    "TT_Mile",
    "TT_FinMile",
    "TT_WBS",
];
const DURATION_TYPE: &[&str] = &[
    "DT_FixedRate",
    "DT_FixedDrtn",
    "DT_FixedQty",
    "DT_FixedDUR2",
];
const COMPLETE_PCT_TYPE: &[&str] = &["CP_Phys", "CP_Drtn", "CP_Units"];
const STATUS_CODE: &[&str] = &["TK_NotStart", "TK_Active", "TK_Complete"];
const CSTR_TYPE: &[&str] = &[
    "CS_MSO",
    "CS_MSOA",
    "CS_MSOB",
    "CS_MEO",
    "CS_MEOA",
    "CS_MEOB",
    "CS_ALAP",
    "CS_MANDSTART",
    "CS_MANDFIN",
];
const PRED_TYPE: &[&str] = &["PR_FS", "PR_SS", "PR_FF", "PR_SF"];
const CLNDR_TYPE: &[&str] = &["CA_Base", "CA_Project", "CA_Rsrc"];
const COST_LOAD_TYPE: &[&str] = &["CL_Uniform", "CL_Start", "CL_End"];
const RSRC_TYPE: &[&str] = &["RT_Labor", "RT_Equip", "RT_Mat"];
const ACTV_CODE_TYPE_SCOPE: &[&str] = &["AS_Global", "AS_Project", "AS_EPS"];
const CRITICAL_PATH_TYPE: &[&str] = &["CT_TotFloat", "CT_DrivPath"];
const SCHED_CALENDAR_ON_RELATIONSHIP_LAG: &[&str] = &[
    "rcal_Predecessor",
    "rcal_Successor",
    "rcal_24Hour",
    "rcal_ProjDefault",
    "RCAL_SUCCESSOR",
];
const SCHED_FLOAT_TYPE: &[&str] = &["FT_FF", "FT_SS", "FT_Min"];
const SCHED_OUTER_DEPEND_TYPE: &[&str] = &["SD_Both"];
const LOGICAL_DATA_TYPE: &[&str] = &[
    "FT_TEXT",
    "FT_START_DATE",
    "FT_END_DATE",
    "FT_MONEY",
    "FT_FLOAT_2_DECIMALS",
    "FT_INT",
    "FT_STATICTYPE",
];
pub(super) const TABLES: &[Table] = &[
    Table {
        name: "ACCOUNT",
        notes: "Cost accounts. Column names observed in corpus files; missing and extra columns are allowed.",
        fields: &[
            Field {
                name: "acct_descr",
                kind: Text,
                notes: "Description, often HTML.",
            },
            Field {
                name: "acct_id",
                kind: Id,
                notes: "",
            },
            Field {
                name: "acct_name",
                kind: Text,
                notes: "",
            },
            Field {
                name: "acct_seq_num",
                kind: Int,
                notes: "Sort order among siblings.",
            },
            Field {
                name: "acct_short_name",
                kind: Text,
                notes: "The cost account's ID as shown.",
            },
            Field {
                name: "parent_acct_id",
                kind: ForeignId("ACCOUNT"),
                notes: "Parent in the account hierarchy; empty at the top.",
            },
        ],
    },
    Table {
        name: "ACTVCODE",
        notes: "Observed in the public sample corpus; missing and extra columns are allowed.",
        fields: &[
            Field {
                name: "actv_code_id",
                kind: Id,
                notes: "",
            },
            Field {
                name: "actv_code_name",
                kind: Text,
                notes: "Preserved as text; no stronger representation established.",
            },
            Field {
                name: "actv_code_type_id",
                kind: ForeignId("ACTVTYPE"),
                notes: "",
            },
            Field {
                name: "color",
                kind: Text,
                notes: "Observed text codes; vocabulary not established. Do not infer a number from the name.",
            },
            Field {
                name: "parent_actv_code_id",
                kind: ForeignId("ACTVCODE"),
                notes: "",
            },
            Field {
                name: "proj_id",
                kind: ForeignId("PROJECT"),
                notes: "Declared with no rows in one public sample; type inferred from the name.",
            },
            Field {
                name: "seq_num",
                kind: Int,
                notes: "",
            },
            Field {
                name: "short_name",
                kind: Text,
                notes: "Preserved as text; no stronger representation established.",
            },
            Field {
                name: "total_assignments",
                kind: Int,
                notes: "",
            },
        ],
    },
    Table {
        name: "ACTVTYPE",
        notes: "Observed in the public sample corpus; missing and extra columns are allowed.",
        fields: &[
            Field {
                name: "actv_code_type",
                kind: Text,
                notes: "Preserved as text; no stronger representation established.",
            },
            Field {
                name: "actv_code_type_id",
                kind: Id,
                notes: "",
            },
            Field {
                name: "actv_code_type_scope",
                kind: Enum(ACTV_CODE_TYPE_SCOPE),
                notes: "Known format codes; empty and future codes remain recoverable. See docs/xer-format.md for observed subsets.",
            },
            Field {
                name: "actv_short_len",
                kind: Int,
                notes: "",
            },
            Field {
                name: "export_flag",
                kind: Flag,
                notes: "",
            },
            Field {
                name: "proj_id",
                kind: ForeignId("PROJECT"),
                notes: "",
            },
            Field {
                name: "seq_num",
                kind: Int,
                notes: "",
            },
            Field {
                name: "wbs_id",
                kind: ForeignId("PROJWBS"),
                notes: "",
            },
        ],
    },
    Table {
        name: "CALENDAR",
        notes: "Observed in the public sample corpus; missing and extra columns are allowed.",
        fields: &[
            Field {
                name: "",
                kind: Text,
                notes: "Unnamed trailing column observed in synthetic edge cases; preserved positionally.",
            },
            Field {
                name: "base_clndr_id",
                kind: ForeignId("CALENDAR"),
                notes: "",
            },
            Field {
                name: "clndr_data",
                kind: Blob,
                notes: "",
            },
            Field {
                name: "clndr_id",
                kind: Id,
                notes: "",
            },
            Field {
                name: "clndr_name",
                kind: Text,
                notes: "Preserved as text; no stronger representation established.",
            },
            Field {
                name: "clndr_type",
                kind: Enum(CLNDR_TYPE),
                notes: "Known format codes; empty and future codes remain recoverable. See docs/xer-format.md for observed subsets.",
            },
            Field {
                name: "day_hr_cnt",
                kind: Decimal,
                notes: "",
            },
            Field {
                name: "default_flag",
                kind: Flag,
                notes: "",
            },
            Field {
                name: "last_chng_date",
                kind: DateTime,
                notes: "",
            },
            Field {
                name: "month_hr_cnt",
                kind: Decimal,
                notes: "",
            },
            Field {
                name: "proj_id",
                kind: ForeignId("PROJECT"),
                notes: "",
            },
            Field {
                name: "rsrc_private",
                kind: Flag,
                notes: "",
            },
            Field {
                name: "week_hr_cnt",
                kind: Decimal,
                notes: "",
            },
            Field {
                name: "year_hr_cnt",
                kind: Decimal,
                notes: "",
            },
        ],
    },
    Table {
        name: "COSTTYPE",
        notes: "Expense categories. Column names observed in corpus files.",
        fields: &[
            Field {
                name: "cost_type",
                kind: Text,
                notes: "The category's name.",
            },
            Field {
                name: "cost_type_id",
                kind: Id,
                notes: "",
            },
            Field {
                name: "seq_num",
                kind: Int,
                notes: "Sort order.",
            },
        ],
    },
    Table {
        name: "CURRTYPE",
        notes: "Observed in the public sample corpus; missing and extra columns are allowed.",
        fields: &[
            Field {
                name: "",
                kind: Text,
                notes: "Unnamed trailing column observed in synthetic edge cases; preserved positionally.",
            },
            Field {
                name: "base_exch_rate",
                kind: Decimal,
                notes: "",
            },
            Field {
                name: "curr_id",
                kind: Id,
                notes: "",
            },
            Field {
                name: "curr_short_name",
                kind: Text,
                notes: "Preserved as text; no stronger representation established.",
            },
            Field {
                name: "curr_symbol",
                kind: Text,
                notes: "Preserved as text; no stronger representation established.",
            },
            Field {
                name: "curr_type",
                kind: Text,
                notes: "Preserved as text; no stronger representation established.",
            },
            Field {
                name: "decimal_digit_cnt",
                kind: Int,
                notes: "",
            },
            Field {
                name: "decimal_symbol",
                kind: Text,
                notes: "Preserved as text; no stronger representation established.",
            },
            Field {
                name: "digit_group_symbol",
                kind: Text,
                notes: "Preserved as text; no stronger representation established.",
            },
            Field {
                name: "group_digit_cnt",
                kind: Int,
                notes: "",
            },
            Field {
                name: "neg_curr_fmt_type",
                kind: Text,
                notes: "Preserved as text; no stronger representation established.",
            },
            Field {
                name: "pos_curr_fmt_type",
                kind: Text,
                notes: "Preserved as text; no stronger representation established.",
            },
        ],
    },
    Table {
        name: "MEMOTYPE",
        notes: "Notebook topics, from the 13 corpus files holding them.",
        fields: &[
            Field {
                name: "eps_flag",
                kind: Flag,
                notes: "The topic applies to EPS nodes.",
            },
            Field {
                name: "memo_type",
                kind: Text,
                notes: "Topic name.",
            },
            Field {
                name: "memo_type_id",
                kind: Id,
                notes: "",
            },
            Field {
                name: "proj_flag",
                kind: Flag,
                notes: "The topic applies to projects.",
            },
            Field {
                name: "seq_num",
                kind: Int,
                notes: "",
            },
            Field {
                name: "task_flag",
                kind: Flag,
                notes: "The topic applies to activities.",
            },
            Field {
                name: "wbs_flag",
                kind: Flag,
                notes: "The topic applies to WBS nodes.",
            },
        ],
    },
    Table {
        name: "OBS",
        notes: "Observed in the public sample corpus; missing and extra columns are allowed.",
        fields: &[
            Field {
                name: "",
                kind: Text,
                notes: "Unnamed trailing column observed in synthetic edge cases; preserved positionally.",
            },
            Field {
                name: "guid",
                kind: Guid,
                notes: "",
            },
            Field {
                name: "obs_descr",
                kind: Text,
                notes: "Preserved as text; no stronger representation established.",
            },
            Field {
                name: "obs_id",
                kind: Id,
                notes: "",
            },
            Field {
                name: "obs_name",
                kind: Text,
                notes: "Preserved as text; no stronger representation established.",
            },
            Field {
                name: "parent_obs_id",
                kind: ForeignId("OBS"),
                notes: "",
            },
            Field {
                name: "seq_num",
                kind: Int,
                notes: "",
            },
        ],
    },
    Table {
        name: "PCATTYPE",
        notes: "Observed in the public sample corpus; missing and extra columns are allowed.",
        fields: &[
            Field {
                name: "export_flag",
                kind: Flag,
                notes: "",
            },
            Field {
                name: "proj_catg_short_len",
                kind: Int,
                notes: "",
            },
            Field {
                name: "proj_catg_type",
                kind: Text,
                notes: "Preserved as text; no stronger representation established.",
            },
            Field {
                name: "proj_catg_type_id",
                kind: Id,
                notes: "",
            },
            Field {
                name: "seq_num",
                kind: Int,
                notes: "",
            },
        ],
    },
    Table {
        name: "PCATVAL",
        notes: "Observed in the public sample corpus; missing and extra columns are allowed.",
        fields: &[
            Field {
                name: "parent_proj_catg_id",
                kind: ForeignId("PCATVAL"),
                notes: "",
            },
            Field {
                name: "proj_catg_id",
                kind: Id,
                notes: "",
            },
            Field {
                name: "proj_catg_name",
                kind: Text,
                notes: "Preserved as text; no stronger representation established.",
            },
            Field {
                name: "proj_catg_short_name",
                kind: Text,
                notes: "Preserved as text; no stronger representation established.",
            },
            Field {
                name: "proj_catg_type_id",
                kind: ForeignId("PCATTYPE"),
                notes: "",
            },
            Field {
                name: "seq_num",
                kind: Int,
                notes: "",
            },
        ],
    },
    Table {
        name: "PROJCOST",
        notes: "Expenses: costs on an activity that no resource carries. Column names observed in corpus files; some lack `cbs_id`.",
        fields: &[
            Field {
                name: "acct_id",
                kind: ForeignId("ACCOUNT"),
                notes: "Cost account; often empty.",
            },
            Field {
                name: "act_cost",
                kind: Decimal,
                notes: "Actual cost.",
            },
            Field {
                name: "auto_compute_act_flag",
                kind: Flag,
                notes: "Whether actual cost follows the activity's progress.",
            },
            Field {
                name: "cbs_id",
                kind: Text,
                notes: "The reference target is unresolved.",
            },
            Field {
                name: "contract_manager_import",
                kind: Text,
                notes: "",
            },
            Field {
                name: "cost_descr",
                kind: Text,
                notes: "Description, often HTML.",
            },
            Field {
                name: "cost_item_id",
                kind: Id,
                notes: "",
            },
            Field {
                name: "cost_load_type",
                kind: Enum(COST_LOAD_TYPE),
                notes: "How the cost accrues over the activity.",
            },
            Field {
                name: "cost_name",
                kind: Text,
                notes: "The expense item.",
            },
            Field {
                name: "cost_per_qty",
                kind: Decimal,
                notes: "Price per unit.",
            },
            Field {
                name: "cost_type_id",
                kind: ForeignId("COSTTYPE"),
                notes: "Expense category; often empty.",
            },
            Field {
                name: "po_number",
                kind: Text,
                notes: "Purchase order number.",
            },
            Field {
                name: "pobs_id",
                kind: Text,
                notes: "The reference target is unresolved.",
            },
            Field {
                name: "proj_id",
                kind: ForeignId("PROJECT"),
                notes: "",
            },
            Field {
                name: "qty_name",
                kind: Text,
                notes: "Unit of measure.",
            },
            Field {
                name: "remain_cost",
                kind: Decimal,
                notes: "Remaining cost.",
            },
            Field {
                name: "target_cost",
                kind: Decimal,
                notes: "Budgeted cost.",
            },
            Field {
                name: "target_qty",
                kind: Decimal,
                notes: "Budgeted units.",
            },
            Field {
                name: "task_id",
                kind: ForeignId("TASK"),
                notes: "",
            },
            Field {
                name: "vendor_name",
                kind: Text,
                notes: "",
            },
        ],
    },
    Table {
        name: "PROJECT",
        notes: "Union of public exports from P6 5.0–24.12; fields are optional, not version gates.",
        fields: &[
            Field {
                name: "",
                kind: Text,
                notes: "Unnamed trailing column observed in synthetic edge cases; preserved positionally.",
            },
            Field {
                name: "acct_id",
                kind: Int,
                notes: "Identity-like field; target table not established.",
            },
            Field {
                name: "act_pct_link_flag",
                kind: Flag,
                notes: "",
            },
            Field {
                name: "act_this_per_link_flag",
                kind: Flag,
                notes: "",
            },
            Field {
                name: "add_act_remain_flag",
                kind: Flag,
                notes: "",
            },
            Field {
                name: "add_by_name",
                kind: Text,
                notes: "Preserved as text; no stronger representation established.",
            },
            Field {
                name: "add_date",
                kind: DateTime,
                notes: "",
            },
            Field {
                name: "allow_complete_flag",
                kind: Flag,
                notes: "",
            },
            Field {
                name: "allow_neg_act_flag",
                kind: Flag,
                notes: "",
            },
            Field {
                name: "apply_actuals_date",
                kind: DateTime,
                notes: "",
            },
            Field {
                name: "base_type_id",
                kind: Int,
                notes: "Identity-like field; target table not established.",
            },
            Field {
                name: "baseline_names_to_export",
                kind: Text,
                notes: "Preserved as text; no stronger representation established.",
            },
            Field {
                name: "baselines_to_export",
                kind: Text,
                notes: "Preserved as text; no stronger representation established.",
            },
            Field {
                name: "batch_sum_flag",
                kind: Flag,
                notes: "",
            },
            Field {
                name: "checkout_flag",
                kind: Flag,
                notes: "",
            },
            Field {
                name: "chng_eff_cmp_pct_flag",
                kind: Flag,
                notes: "",
            },
            Field {
                name: "clndr_id",
                kind: ForeignId("CALENDAR"),
                notes: "",
            },
            Field {
                name: "close_period_flag",
                kind: Flag,
                notes: "",
            },
            Field {
                name: "control_updates_flag",
                kind: Flag,
                notes: "",
            },
            Field {
                name: "cost_qty_recalc_flag",
                kind: Flag,
                notes: "",
            },
            Field {
                name: "cr_external_key",
                kind: Text,
                notes: "Preserved as text; no stronger representation established.",
            },
            Field {
                name: "critical_drtn_hr_cnt",
                kind: Decimal,
                notes: "",
            },
            Field {
                name: "critical_path_type",
                kind: Enum(CRITICAL_PATH_TYPE),
                notes: "Known format codes; empty and future codes remain recoverable. See docs/xer-format.md for observed subsets.",
            },
            Field {
                name: "def_complete_pct_type",
                kind: Enum(COMPLETE_PCT_TYPE),
                notes: "Known format codes; empty and future codes remain recoverable. See docs/xer-format.md for observed subsets.",
            },
            Field {
                name: "def_cost_per_qty",
                kind: Decimal,
                notes: "",
            },
            Field {
                name: "def_duration_type",
                kind: Enum(DURATION_TYPE),
                notes: "Known format codes; empty and future codes remain recoverable. See docs/xer-format.md for observed subsets.",
            },
            Field {
                name: "def_qty_type",
                kind: Text,
                notes: "Legacy code retained as text; enum vocabulary not established.",
            },
            Field {
                name: "def_rate_type",
                kind: Text,
                notes: "Preserved as text; no stronger representation established.",
            },
            Field {
                name: "def_rollup_dates_flag",
                kind: Flag,
                notes: "",
            },
            Field {
                name: "def_task_type",
                kind: Enum(TASK_TYPE),
                notes: "Known format codes; empty and future codes remain recoverable. See docs/xer-format.md for observed subsets.",
            },
            Field {
                name: "export_flag",
                kind: Flag,
                notes: "",
            },
            Field {
                name: "fcst_start_date",
                kind: DateTime,
                notes: "",
            },
            Field {
                name: "fintmpl_id",
                kind: Int,
                notes: "Identity-like field; target table not established.",
            },
            Field {
                name: "fy_start_month_num",
                kind: Int,
                notes: "",
            },
            Field {
                name: "guid",
                kind: Guid,
                notes: "",
            },
            Field {
                name: "hist_interval",
                kind: Text,
                notes: "Observed text codes; vocabulary not established. Do not infer a number from the name.",
            },
            Field {
                name: "hist_level",
                kind: Text,
                notes: "Observed text codes; vocabulary not established. Do not infer a number from the name.",
            },
            Field {
                name: "intg_proj_type",
                kind: Text,
                notes: "Preserved as text; no stronger representation established.",
            },
            Field {
                name: "last_baseline_update_date",
                kind: DateTime,
                notes: "",
            },
            Field {
                name: "last_checksum",
                kind: Int,
                notes: "",
            },
            Field {
                name: "last_fin_dates_id",
                kind: Int,
                notes: "Identity-like field; target table not established.",
            },
            Field {
                name: "last_level_date",
                kind: DateTime,
                notes: "",
            },
            Field {
                name: "last_recalc_date",
                kind: DateTime,
                notes: "",
            },
            Field {
                name: "last_schedule_date",
                kind: DateTime,
                notes: "",
            },
            Field {
                name: "last_tasksum_date",
                kind: DateTime,
                notes: "",
            },
            Field {
                name: "loaded_scope_level",
                kind: Int,
                notes: "",
            },
            Field {
                name: "location_id",
                kind: Int,
                notes: "Identity-like field; target table not established.",
            },
            Field {
                name: "matrix_id",
                kind: Int,
                notes: "Identity-like field; target table not established.",
            },
            Field {
                name: "name_sep_char",
                kind: Text,
                notes: "Preserved as text; no stronger representation established.",
            },
            Field {
                name: "new_fin_dates_id",
                kind: Int,
                notes: "Identity-like field; target table not established.",
            },
            Field {
                name: "next_data_date",
                kind: DateTime,
                notes: "",
            },
            Field {
                name: "orig_proj_id",
                kind: ForeignId("PROJECT"),
                notes: "",
            },
            Field {
                name: "plan_end_date",
                kind: DateTime,
                notes: "",
            },
            Field {
                name: "plan_start_date",
                kind: DateTime,
                notes: "",
            },
            Field {
                name: "priority_num",
                kind: Int,
                notes: "",
            },
            Field {
                name: "proj_id",
                kind: Id,
                notes: "",
            },
            Field {
                name: "proj_short_name",
                kind: Text,
                notes: "Preserved as text; no stronger representation established.",
            },
            Field {
                name: "proj_url",
                kind: Text,
                notes: "Preserved as text; no stronger representation established.",
            },
            Field {
                name: "project_flag",
                kind: Flag,
                notes: "",
            },
            Field {
                name: "publish_spread_assign_level",
                kind: Text,
                notes: "Observed text codes; vocabulary not established. Do not infer a number from the name.",
            },
            Field {
                name: "px_enable_publication_flag",
                kind: Flag,
                notes: "",
            },
            Field {
                name: "px_last_update_date",
                kind: DateTime,
                notes: "",
            },
            Field {
                name: "px_priority",
                kind: Int,
                notes: "",
            },
            Field {
                name: "rem_target_link_flag",
                kind: Flag,
                notes: "",
            },
            Field {
                name: "reset_planned_flag",
                kind: Flag,
                notes: "",
            },
            Field {
                name: "risk_level",
                kind: Int,
                notes: "",
            },
            Field {
                name: "rsrc_multi_assign_flag",
                kind: Flag,
                notes: "",
            },
            Field {
                name: "rsrc_role_match_flag",
                kind: Flag,
                notes: "",
            },
            Field {
                name: "rsrc_self_add_flag",
                kind: Flag,
                notes: "",
            },
            Field {
                name: "scd_end_date",
                kind: DateTime,
                notes: "",
            },
            Field {
                name: "sched_wbs_heir_type",
                kind: Text,
                notes: "Preserved as text; no stronger representation established.",
            },
            Field {
                name: "schedule_type",
                kind: Text,
                notes: "Preserved as text; no stronger representation established.",
            },
            Field {
                name: "source_proj_id",
                kind: ForeignId("PROJECT"),
                notes: "",
            },
            Field {
                name: "step_complete_flag",
                kind: Flag,
                notes: "",
            },
            Field {
                name: "strgy_priority_num",
                kind: Int,
                notes: "",
            },
            Field {
                name: "sum_assign_level",
                kind: Text,
                notes: "Observed text codes; vocabulary not established. Do not infer a number from the name.",
            },
            Field {
                name: "sum_base_proj_id",
                kind: ForeignId("PROJECT"),
                notes: "",
            },
            Field {
                name: "sum_data_date",
                kind: DateTime,
                notes: "",
            },
            Field {
                name: "sum_only_flag",
                kind: Flag,
                notes: "",
            },
            Field {
                name: "sum_refresh_date",
                kind: DateTime,
                notes: "",
            },
            Field {
                name: "sumtask_loaded",
                kind: Flag,
                notes: "",
            },
            Field {
                name: "sync_wbs_heir_flag",
                kind: Flag,
                notes: "",
            },
            Field {
                name: "task_code_base",
                kind: Int,
                notes: "",
            },
            Field {
                name: "task_code_prefix",
                kind: Text,
                notes: "Preserved as text; no stronger representation established.",
            },
            Field {
                name: "task_code_prefix_flag",
                kind: Flag,
                notes: "",
            },
            Field {
                name: "task_code_step",
                kind: Int,
                notes: "",
            },
            Field {
                name: "trsrcsum_loaded",
                kind: Flag,
                notes: "",
            },
            Field {
                name: "ts_rsrc_mark_act_finish_flag",
                kind: Flag,
                notes: "",
            },
            Field {
                name: "ts_rsrc_vw_inact_actv_flag",
                kind: Flag,
                notes: "",
            },
            Field {
                name: "use_project_baseline_flag",
                kind: Flag,
                notes: "",
            },
            Field {
                name: "wbs_heir_levels",
                kind: Int,
                notes: "",
            },
            Field {
                name: "wbs_max_sum_level",
                kind: Int,
                notes: "",
            },
            Field {
                name: "web_local_root_path",
                kind: Text,
                notes: "Preserved as text; no stronger representation established.",
            },
        ],
    },
    Table {
        name: "PROJPCAT",
        notes: "Observed in the public sample corpus; missing and extra columns are allowed.",
        fields: &[
            Field {
                name: "proj_catg_id",
                kind: ForeignId("PCATVAL"),
                notes: "",
            },
            Field {
                name: "proj_catg_type_id",
                kind: ForeignId("PCATTYPE"),
                notes: "",
            },
            Field {
                name: "proj_id",
                kind: ForeignId("PROJECT"),
                notes: "",
            },
        ],
    },
    Table {
        name: "PROJWBS",
        notes: "Observed in the public sample corpus; missing and extra columns are allowed.",
        fields: &[
            Field {
                name: "",
                kind: Text,
                notes: "Unnamed trailing column observed in synthetic edge cases; preserved positionally.",
            },
            Field {
                name: "ann_dscnt_rate_pct",
                kind: Decimal,
                notes: "",
            },
            Field {
                name: "anticip_end_date",
                kind: DateTime,
                notes: "",
            },
            Field {
                name: "anticip_start_date",
                kind: DateTime,
                notes: "",
            },
            Field {
                name: "dscnt_period_type",
                kind: Text,
                notes: "Preserved as text; no stronger representation established.",
            },
            Field {
                name: "est_wt",
                kind: Decimal,
                notes: "",
            },
            Field {
                name: "ev_compute_type",
                kind: Text,
                notes: "Preserved as text; no stronger representation established.",
            },
            Field {
                name: "ev_etc_compute_type",
                kind: Text,
                notes: "Preserved as text; no stronger representation established.",
            },
            Field {
                name: "ev_etc_user_value",
                kind: Decimal,
                notes: "",
            },
            Field {
                name: "ev_user_pct",
                kind: Decimal,
                notes: "",
            },
            Field {
                name: "guid",
                kind: Guid,
                notes: "",
            },
            Field {
                name: "indep_remain_total_cost",
                kind: Decimal,
                notes: "",
            },
            Field {
                name: "indep_remain_work_qty",
                kind: Decimal,
                notes: "",
            },
            Field {
                name: "obs_id",
                kind: ForeignId("OBS"),
                notes: "",
            },
            Field {
                name: "orig_cost",
                kind: Decimal,
                notes: "",
            },
            Field {
                name: "parent_wbs_id",
                kind: ForeignId("PROJWBS"),
                notes: "",
            },
            Field {
                name: "phase_id",
                kind: Int,
                notes: "Identity-like field; target table not established.",
            },
            Field {
                name: "plan_open_state",
                kind: Text,
                notes: "Preserved as text; no stronger representation established.",
            },
            Field {
                name: "proj_id",
                kind: ForeignId("PROJECT"),
                notes: "",
            },
            Field {
                name: "proj_node_flag",
                kind: Flag,
                notes: "",
            },
            Field {
                name: "seq_num",
                kind: Int,
                notes: "",
            },
            Field {
                name: "status_code",
                kind: Text,
                notes: "Preserved as text; no stronger representation established.",
            },
            Field {
                name: "status_reviewer",
                kind: Text,
                notes: "Preserved as text; no stronger representation established.",
            },
            Field {
                name: "sum_data_flag",
                kind: Flag,
                notes: "",
            },
            Field {
                name: "tmpl_guid",
                kind: Guid,
                notes: "",
            },
            Field {
                name: "wbs_id",
                kind: Id,
                notes: "",
            },
            Field {
                name: "wbs_name",
                kind: Text,
                notes: "Preserved as text; no stronger representation established.",
            },
            Field {
                name: "wbs_short_name",
                kind: Text,
                notes: "Preserved as text; no stronger representation established.",
            },
        ],
    },
    Table {
        name: "RISKTYPE",
        notes: "Risk categories, a tree.",
        fields: &[
            Field {
                name: "parent_risk_type_id",
                kind: ForeignId("RISKTYPE"),
                notes: "",
            },
            Field {
                name: "risk_type",
                kind: Text,
                notes: "Category name.",
            },
            Field {
                name: "risk_type_id",
                kind: Id,
                notes: "",
            },
            Field {
                name: "seq_num",
                kind: Int,
                notes: "",
            },
        ],
    },
    Table {
        name: "ROLERATE",
        notes: "Role prices over time. Column names observed in corpus files; some lack `start_date` and `max_qty_per_hr`.",
        fields: &[
            Field {
                name: "cost_per_qty",
                kind: Decimal,
                notes: "The first of five prices per unit.",
            },
            Field {
                name: "cost_per_qty2",
                kind: Decimal,
                notes: "",
            },
            Field {
                name: "cost_per_qty3",
                kind: Decimal,
                notes: "",
            },
            Field {
                name: "cost_per_qty4",
                kind: Decimal,
                notes: "",
            },
            Field {
                name: "cost_per_qty5",
                kind: Decimal,
                notes: "",
            },
            Field {
                name: "max_qty_per_hr",
                kind: Decimal,
                notes: "",
            },
            Field {
                name: "role_id",
                kind: ForeignId("ROLES"),
                notes: "",
            },
            Field {
                name: "role_rate_id",
                kind: Id,
                notes: "",
            },
            Field {
                name: "start_date",
                kind: DateTime,
                notes: "When the rate takes effect.",
            },
        ],
    },
    Table {
        name: "ROLES",
        notes: "Roles: kinds of work assigned before, or instead of, a named resource. Column names observed in corpus files; missing and extra columns are allowed.",
        fields: &[
            Field {
                name: "cost_qty_type",
                kind: Text,
                notes: "Code whose values aren't established.",
            },
            Field {
                name: "def_cost_qty_link_flag",
                kind: Flag,
                notes: "Whether an assignment's cost follows its units by default.",
            },
            Field {
                name: "last_checksum",
                kind: Text,
                notes: "",
            },
            Field {
                name: "parent_role_id",
                kind: ForeignId("ROLES"),
                notes: "Parent in the role hierarchy; empty at the top.",
            },
            Field {
                name: "pobs_id",
                kind: Text,
                notes: "The reference target is unresolved.",
            },
            Field {
                name: "role_descr",
                kind: Text,
                notes: "Description, often HTML.",
            },
            Field {
                name: "role_id",
                kind: Id,
                notes: "",
            },
            Field {
                name: "role_name",
                kind: Text,
                notes: "",
            },
            Field {
                name: "role_short_name",
                kind: Text,
                notes: "The role's ID as shown.",
            },
            Field {
                name: "seq_num",
                kind: Int,
                notes: "Sort order among siblings.",
            },
        ],
    },
    Table {
        name: "RSRC",
        notes: "Observed in the public sample corpus; missing and extra columns are allowed.",
        fields: &[
            Field {
                name: "",
                kind: Text,
                notes: "Unnamed trailing column observed in synthetic edge cases; preserved positionally.",
            },
            Field {
                name: "active_flag",
                kind: Flag,
                notes: "",
            },
            Field {
                name: "auto_compute_act_flag",
                kind: Flag,
                notes: "",
            },
            Field {
                name: "auto_compute_acts_flag",
                kind: Flag,
                notes: "Declared with no rows in one public sample; type inferred from the name.",
            },
            Field {
                name: "clndr_id",
                kind: ForeignId("CALENDAR"),
                notes: "",
            },
            Field {
                name: "cost_qty_type",
                kind: Text,
                notes: "Legacy code retained as text; enum vocabulary not established.",
            },
            Field {
                name: "curr_id",
                kind: ForeignId("CURRTYPE"),
                notes: "",
            },
            Field {
                name: "def_cost_qty_link_flag",
                kind: Flag,
                notes: "",
            },
            Field {
                name: "def_qty_per_hour",
                kind: Decimal,
                notes: "Declared with no rows in one public sample; type inferred from the name.",
            },
            Field {
                name: "def_qty_per_hr",
                kind: Decimal,
                notes: "",
            },
            Field {
                name: "email_addr",
                kind: Text,
                notes: "Preserved as text; no stronger representation established.",
            },
            Field {
                name: "employee_code",
                kind: Text,
                notes: "Preserved as text; no stronger representation established.",
            },
            Field {
                name: "guid",
                kind: Guid,
                notes: "",
            },
            Field {
                name: "last_checksum",
                kind: Int,
                notes: "",
            },
            Field {
                name: "level_flag",
                kind: Flag,
                notes: "",
            },
            Field {
                name: "load_tasks_flag",
                kind: Flag,
                notes: "",
            },
            Field {
                name: "location_id",
                kind: Int,
                notes: "Identity-like field; target table not established.",
            },
            Field {
                name: "office_phone",
                kind: Text,
                notes: "Preserved as text; no stronger representation established.",
            },
            Field {
                name: "ot_factor",
                kind: Decimal,
                notes: "",
            },
            Field {
                name: "ot_flag",
                kind: Flag,
                notes: "",
            },
            Field {
                name: "other_phone",
                kind: Text,
                notes: "Preserved as text; no stronger representation established.",
            },
            Field {
                name: "parent_rsrc_id",
                kind: ForeignId("RSRC"),
                notes: "",
            },
            Field {
                name: "pobs_id",
                kind: Int,
                notes: "Identity-like field; target table not established.",
            },
            Field {
                name: "role_id",
                kind: Int,
                notes: "Identity-like field; target table not established.",
            },
            Field {
                name: "rsrc_id",
                kind: Id,
                notes: "",
            },
            Field {
                name: "rsrc_name",
                kind: Text,
                notes: "Preserved as text; no stronger representation established.",
            },
            Field {
                name: "rsrc_notes",
                kind: Text,
                notes: "Preserved as text; no stronger representation established.",
            },
            Field {
                name: "rsrc_seq_num",
                kind: Int,
                notes: "",
            },
            Field {
                name: "rsrc_short_name",
                kind: Text,
                notes: "Preserved as text; no stronger representation established.",
            },
            Field {
                name: "rsrc_title_name",
                kind: Text,
                notes: "Preserved as text; no stronger representation established.",
            },
            Field {
                name: "rsrc_type",
                kind: Enum(RSRC_TYPE),
                notes: "Known format codes; empty and future codes remain recoverable. See docs/xer-format.md for observed subsets.",
            },
            Field {
                name: "shift_id",
                kind: Int,
                notes: "Identity-like field; target table not established.",
            },
            Field {
                name: "timesheet_flag",
                kind: Flag,
                notes: "",
            },
            Field {
                name: "ts_approve_user_id",
                kind: Int,
                notes: "Identity-like field; target table not established.",
            },
            Field {
                name: "unit_id",
                kind: Int,
                notes: "Identity-like field; target table not established.",
            },
            Field {
                name: "user_id",
                kind: Int,
                notes: "Identity-like field; target table not established.",
            },
            Field {
                name: "xfer_complete_day_cnt",
                kind: Int,
                notes: "Observed integer day counts.",
            },
            Field {
                name: "xfer_notstart_day_cnt",
                kind: Int,
                notes: "Observed integer day counts.",
            },
        ],
    },
    Table {
        name: "RSRCCURVDATA",
        notes: "Resource curves. Column names observed in corpus files.",
        fields: &[
            Field {
                name: "curv_id",
                kind: Id,
                notes: "",
            },
            Field {
                name: "curv_name",
                kind: Text,
                notes: "The curve's name.",
            },
            Field {
                name: "default_flag",
                kind: Flag,
                notes: "Y on curves with P6's standard names, N on a user's own curve (corpus).",
            },
            Field {
                name: "pct_usage_0",
                kind: Decimal,
                notes: "Percent of units at 0% of the duration; 0 in every corpus curve.",
            },
            Field {
                name: "pct_usage_1",
                kind: Decimal,
                notes: "Percent of units in twentieth 1 of the duration.",
            },
            Field {
                name: "pct_usage_10",
                kind: Decimal,
                notes: "Percent of units in twentieth 10 of the duration.",
            },
            Field {
                name: "pct_usage_11",
                kind: Decimal,
                notes: "Percent of units in twentieth 11 of the duration.",
            },
            Field {
                name: "pct_usage_12",
                kind: Decimal,
                notes: "Percent of units in twentieth 12 of the duration.",
            },
            Field {
                name: "pct_usage_13",
                kind: Decimal,
                notes: "Percent of units in twentieth 13 of the duration.",
            },
            Field {
                name: "pct_usage_14",
                kind: Decimal,
                notes: "Percent of units in twentieth 14 of the duration.",
            },
            Field {
                name: "pct_usage_15",
                kind: Decimal,
                notes: "Percent of units in twentieth 15 of the duration.",
            },
            Field {
                name: "pct_usage_16",
                kind: Decimal,
                notes: "Percent of units in twentieth 16 of the duration.",
            },
            Field {
                name: "pct_usage_17",
                kind: Decimal,
                notes: "Percent of units in twentieth 17 of the duration.",
            },
            Field {
                name: "pct_usage_18",
                kind: Decimal,
                notes: "Percent of units in twentieth 18 of the duration.",
            },
            Field {
                name: "pct_usage_19",
                kind: Decimal,
                notes: "Percent of units in twentieth 19 of the duration.",
            },
            Field {
                name: "pct_usage_2",
                kind: Decimal,
                notes: "Percent of units in twentieth 2 of the duration.",
            },
            Field {
                name: "pct_usage_20",
                kind: Decimal,
                notes: "Percent of units in twentieth 20 of the duration.",
            },
            Field {
                name: "pct_usage_3",
                kind: Decimal,
                notes: "Percent of units in twentieth 3 of the duration.",
            },
            Field {
                name: "pct_usage_4",
                kind: Decimal,
                notes: "Percent of units in twentieth 4 of the duration.",
            },
            Field {
                name: "pct_usage_5",
                kind: Decimal,
                notes: "Percent of units in twentieth 5 of the duration.",
            },
            Field {
                name: "pct_usage_6",
                kind: Decimal,
                notes: "Percent of units in twentieth 6 of the duration.",
            },
            Field {
                name: "pct_usage_7",
                kind: Decimal,
                notes: "Percent of units in twentieth 7 of the duration.",
            },
            Field {
                name: "pct_usage_8",
                kind: Decimal,
                notes: "Percent of units in twentieth 8 of the duration.",
            },
            Field {
                name: "pct_usage_9",
                kind: Decimal,
                notes: "Percent of units in twentieth 9 of the duration.",
            },
        ],
    },
    Table {
        name: "RSRCRATE",
        notes: "Resource prices over time. Observed in the public sample corpus; missing and extra columns are allowed.",
        fields: &[
            Field {
                name: "",
                kind: Text,
                notes: "Unnamed trailing column observed in synthetic edge cases; preserved positionally.",
            },
            Field {
                name: "cost_per_qty",
                kind: Decimal,
                notes: "The first of five prices per unit.",
            },
            Field {
                name: "cost_per_qty2",
                kind: Decimal,
                notes: "",
            },
            Field {
                name: "cost_per_qty3",
                kind: Decimal,
                notes: "",
            },
            Field {
                name: "cost_per_qty4",
                kind: Decimal,
                notes: "",
            },
            Field {
                name: "cost_per_qty5",
                kind: Decimal,
                notes: "",
            },
            Field {
                name: "max_qty_per_hr",
                kind: Decimal,
                notes: "Observed integer and decimal shapes.",
            },
            Field {
                name: "rsrc_id",
                kind: ForeignId("RSRC"),
                notes: "",
            },
            Field {
                name: "rsrc_rate_id",
                kind: Id,
                notes: "",
            },
            Field {
                name: "shift_period_id",
                kind: Text,
                notes: "Always empty in the corpus; the reference target is unresolved.",
            },
            Field {
                name: "start_date",
                kind: DateTime,
                notes: "When the rate takes effect. Nine corpus cells carry seconds, which the typed layer reports as unsupported precision.",
            },
        ],
    },
    Table {
        name: "RSRCROLE",
        notes: "The roles a resource can fill, with a skill level. Column names observed in corpus files; the name columns repeat the resource's and role's.",
        fields: &[
            Field {
                name: "role_id",
                kind: ForeignId("ROLES"),
                notes: "",
            },
            Field {
                name: "role_name",
                kind: Text,
                notes: "Repeats `ROLES.role_name`.",
            },
            Field {
                name: "role_short_name",
                kind: Text,
                notes: "Repeats `ROLES.role_short_name`.",
            },
            Field {
                name: "rsrc_id",
                kind: ForeignId("RSRC"),
                notes: "",
            },
            Field {
                name: "rsrc_name",
                kind: Text,
                notes: "Repeats `RSRC.rsrc_name`.",
            },
            Field {
                name: "rsrc_role_id",
                kind: Id,
                notes: "",
            },
            Field {
                name: "rsrc_short_name",
                kind: Text,
                notes: "Repeats `RSRC.rsrc_short_name`.",
            },
            Field {
                name: "rsrc_type",
                kind: Enum(RSRC_TYPE),
                notes: "Repeats `RSRC.rsrc_type`.",
            },
            Field {
                name: "skill_level",
                kind: Int,
                notes: "Proficiency, a small integer; its scale isn't established.",
            },
        ],
    },
    Table {
        name: "SCHEDOPTIONS",
        notes: "Absent from some old exports; see TF-008. Meanings of several options remain hypotheses.",
        fields: &[
            Field {
                name: "LevelPriorityList",
                kind: Blob,
                notes: "",
            },
            Field {
                name: "enable_multiple_longest_path_calc",
                kind: Flag,
                notes: "",
            },
            Field {
                name: "key_activity_for_multiple_longest_paths",
                kind: ForeignId("TASK"),
                notes: "",
            },
            Field {
                name: "level_all_rsrc_flag",
                kind: Flag,
                notes: "",
            },
            Field {
                name: "level_float_thrs_cnt",
                kind: Decimal,
                notes: "",
            },
            Field {
                name: "level_keep_sched_date_flag",
                kind: Flag,
                notes: "",
            },
            Field {
                name: "level_outer_assign_flag",
                kind: Flag,
                notes: "",
            },
            Field {
                name: "level_outer_assign_priority",
                kind: Int,
                notes: "",
            },
            Field {
                name: "level_over_alloc_pct",
                kind: Decimal,
                notes: "",
            },
            Field {
                name: "level_within_float_flag",
                kind: Flag,
                notes: "",
            },
            Field {
                name: "limit_multiple_longest_path_calc",
                kind: Flag,
                notes: "",
            },
            Field {
                name: "max_multiple_longest_path",
                kind: Int,
                notes: "",
            },
            Field {
                name: "proj_id",
                kind: ForeignId("PROJECT"),
                notes: "",
            },
            Field {
                name: "sched_calendar_on_relationship_lag",
                kind: Enum(SCHED_CALENDAR_ON_RELATIONSHIP_LAG),
                notes: "Known format codes; empty and future codes remain recoverable. See docs/xer-format.md for observed subsets.",
            },
            Field {
                name: "sched_float_type",
                kind: Enum(SCHED_FLOAT_TYPE),
                notes: "Known format codes; empty and future codes remain recoverable. See docs/xer-format.md for observed subsets.",
            },
            Field {
                name: "sched_lag_early_start_flag",
                kind: Flag,
                notes: "",
            },
            Field {
                name: "sched_open_critical_flag",
                kind: Flag,
                notes: "",
            },
            Field {
                name: "sched_outer_depend_type",
                kind: Enum(SCHED_OUTER_DEPEND_TYPE),
                notes: "Known format codes; empty and future codes remain recoverable. See docs/xer-format.md for observed subsets.",
            },
            Field {
                name: "sched_progress_override",
                kind: Flag,
                notes: "",
            },
            Field {
                name: "sched_retained_logic",
                kind: Flag,
                notes: "",
            },
            Field {
                name: "sched_setplantoforecast",
                kind: Flag,
                notes: "",
            },
            Field {
                name: "sched_use_expect_end_flag",
                kind: Flag,
                notes: "",
            },
            Field {
                name: "sched_use_project_end_date_for_float",
                kind: Flag,
                notes: "",
            },
            Field {
                name: "schedoptions_id",
                kind: Id,
                notes: "",
            },
            Field {
                name: "use_total_float_multiple_longest_paths",
                kind: Flag,
                notes: "",
            },
        ],
    },
    Table {
        name: "TASK",
        notes: "Observed in the public sample corpus; missing and extra columns are allowed.",
        fields: &[
            Field {
                name: "",
                kind: Text,
                notes: "Unnamed trailing column observed in synthetic edge cases; preserved positionally.",
            },
            Field {
                name: "act_end_date",
                kind: DateTime,
                notes: "",
            },
            Field {
                name: "act_equip_qty",
                kind: Decimal,
                notes: "",
            },
            Field {
                name: "act_start_date",
                kind: DateTime,
                notes: "",
            },
            Field {
                name: "act_this_per_equip_qty",
                kind: Decimal,
                notes: "",
            },
            Field {
                name: "act_this_per_work_qty",
                kind: Decimal,
                notes: "",
            },
            Field {
                name: "act_work_qty",
                kind: Decimal,
                notes: "",
            },
            Field {
                name: "auto_compute_act_flag",
                kind: Flag,
                notes: "",
            },
            Field {
                name: "cbs_id",
                kind: Int,
                notes: "Identity-like field; target table not established.",
            },
            Field {
                name: "clndr_id",
                kind: ForeignId("CALENDAR"),
                notes: "",
            },
            Field {
                name: "complete_pct_type",
                kind: Enum(COMPLETE_PCT_TYPE),
                notes: "Known format codes; empty and future codes remain recoverable. See docs/xer-format.md for observed subsets.",
            },
            Field {
                name: "control_updates_flag",
                kind: Flag,
                notes: "",
            },
            Field {
                name: "create_date",
                kind: DateTime,
                notes: "",
            },
            Field {
                name: "create_user",
                kind: Text,
                notes: "Preserved as text; no stronger representation established.",
            },
            Field {
                name: "crt_path_num",
                kind: Int,
                notes: "",
            },
            Field {
                name: "cstr_date",
                kind: DateTime,
                notes: "",
            },
            Field {
                name: "cstr_date2",
                kind: DateTime,
                notes: "Secondary constraint datetime; paired with cstr_type2. Empty in the current corpus.",
            },
            Field {
                name: "cstr_type",
                kind: Enum(CSTR_TYPE),
                notes: "Known format codes; empty and future codes remain recoverable. See docs/xer-format.md for observed subsets.",
            },
            Field {
                name: "cstr_type2",
                kind: Enum(CSTR_TYPE),
                notes: "Known format codes; empty and future codes remain recoverable. See docs/xer-format.md for observed subsets.",
            },
            Field {
                name: "custom_field_x",
                kind: Text,
                notes: "Preserved as text; no stronger representation established.",
            },
            Field {
                name: "driving_path_flag",
                kind: Flag,
                notes: "",
            },
            Field {
                name: "duration_type",
                kind: Enum(DURATION_TYPE),
                notes: "Known format codes; empty and future codes remain recoverable. See docs/xer-format.md for observed subsets.",
            },
            Field {
                name: "early_end_date",
                kind: DateTime,
                notes: "",
            },
            Field {
                name: "early_start_date",
                kind: DateTime,
                notes: "",
            },
            Field {
                name: "est_wt",
                kind: Decimal,
                notes: "",
            },
            Field {
                name: "expect_end_date",
                kind: DateTime,
                notes: "",
            },
            Field {
                name: "external_early_start_date",
                kind: DateTime,
                notes: "",
            },
            Field {
                name: "external_late_end_date",
                kind: DateTime,
                notes: "",
            },
            Field {
                name: "float_path",
                kind: Int,
                notes: "",
            },
            Field {
                name: "float_path_order",
                kind: Int,
                notes: "",
            },
            Field {
                name: "free_float_hr_cnt",
                kind: Decimal,
                notes: "",
            },
            Field {
                name: "guid",
                kind: Guid,
                notes: "",
            },
            Field {
                name: "late_end_date",
                kind: DateTime,
                notes: "",
            },
            Field {
                name: "late_start_date",
                kind: DateTime,
                notes: "",
            },
            Field {
                name: "location_id",
                kind: Int,
                notes: "Identity-like field; target table not established.",
            },
            Field {
                name: "lock_plan_flag",
                kind: Flag,
                notes: "",
            },
            Field {
                name: "old_reend_date",
                kind: DateTime,
                notes: "",
            },
            Field {
                name: "old_remain_drtn_hr_cnt",
                kind: Decimal,
                notes: "",
            },
            Field {
                name: "old_restart_date",
                kind: DateTime,
                notes: "",
            },
            Field {
                name: "phys_complete_pct",
                kind: Decimal,
                notes: "",
            },
            Field {
                name: "post_pess_finish_date",
                kind: DateTime,
                notes: "",
            },
            Field {
                name: "post_pess_start_date",
                kind: DateTime,
                notes: "",
            },
            Field {
                name: "pre_pess_finish_date",
                kind: DateTime,
                notes: "",
            },
            Field {
                name: "pre_pess_start_date",
                kind: DateTime,
                notes: "",
            },
            Field {
                name: "priority_type",
                kind: Text,
                notes: "Preserved as text; no stronger representation established.",
            },
            Field {
                name: "proj_id",
                kind: ForeignId("PROJECT"),
                notes: "",
            },
            Field {
                name: "reend_date",
                kind: DateTime,
                notes: "",
            },
            Field {
                name: "rem_late_end_date",
                kind: DateTime,
                notes: "",
            },
            Field {
                name: "rem_late_start_date",
                kind: DateTime,
                notes: "",
            },
            Field {
                name: "remain_drtn_hr_cnt",
                kind: Decimal,
                notes: "",
            },
            Field {
                name: "remain_equip_qty",
                kind: Decimal,
                notes: "",
            },
            Field {
                name: "remain_work_qty",
                kind: Decimal,
                notes: "",
            },
            Field {
                name: "restart_date",
                kind: DateTime,
                notes: "",
            },
            Field {
                name: "resume_date",
                kind: DateTime,
                notes: "",
            },
            Field {
                name: "rev_fdbk_flag",
                kind: Flag,
                notes: "",
            },
            Field {
                name: "review_end_date",
                kind: DateTime,
                notes: "",
            },
            Field {
                name: "review_type",
                kind: Text,
                notes: "Preserved as text; no stronger representation established.",
            },
            Field {
                name: "rsrc_id",
                kind: ForeignId("RSRC"),
                notes: "",
            },
            Field {
                name: "status_code",
                kind: Enum(STATUS_CODE),
                notes: "Known format codes; empty and future codes remain recoverable. See docs/xer-format.md for observed subsets.",
            },
            Field {
                name: "suspend_date",
                kind: DateTime,
                notes: "",
            },
            Field {
                name: "target_drtn_hr_cnt",
                kind: Decimal,
                notes: "",
            },
            Field {
                name: "target_end_date",
                kind: DateTime,
                notes: "",
            },
            Field {
                name: "target_equip_qty",
                kind: Decimal,
                notes: "",
            },
            Field {
                name: "target_start_date",
                kind: DateTime,
                notes: "",
            },
            Field {
                name: "target_work_qty",
                kind: Decimal,
                notes: "",
            },
            Field {
                name: "task_code",
                kind: Text,
                notes: "Preserved as text; no stronger representation established.",
            },
            Field {
                name: "task_id",
                kind: Id,
                notes: "",
            },
            Field {
                name: "task_name",
                kind: Text,
                notes: "Preserved as text; no stronger representation established.",
            },
            Field {
                name: "task_type",
                kind: Enum(TASK_TYPE),
                notes: "Known format codes; empty and future codes remain recoverable. See docs/xer-format.md for observed subsets.",
            },
            Field {
                name: "tmpl_guid",
                kind: Guid,
                notes: "",
            },
            Field {
                name: "total_float_hr_cnt",
                kind: Decimal,
                notes: "",
            },
            Field {
                name: "update_date",
                kind: DateTime,
                notes: "",
            },
            Field {
                name: "update_user",
                kind: Text,
                notes: "Preserved as text; no stronger representation established.",
            },
            Field {
                name: "wbs_id",
                kind: ForeignId("PROJWBS"),
                notes: "",
            },
        ],
    },
    Table {
        name: "TASKACTV",
        notes: "Observed in the public sample corpus; missing and extra columns are allowed.",
        fields: &[
            Field {
                name: "actv_code_id",
                kind: ForeignId("ACTVCODE"),
                notes: "",
            },
            Field {
                name: "actv_code_type_id",
                kind: ForeignId("ACTVTYPE"),
                notes: "",
            },
            Field {
                name: "proj_id",
                kind: ForeignId("PROJECT"),
                notes: "",
            },
            Field {
                name: "task_id",
                kind: ForeignId("TASK"),
                notes: "",
            },
        ],
    },
    Table {
        name: "TASKMEMO",
        notes: "Activity notes, from the 12 corpus files holding them.",
        fields: &[
            Field {
                name: "memo_id",
                kind: Id,
                notes: "",
            },
            Field {
                name: "memo_type_id",
                kind: ForeignId("MEMOTYPE"),
                notes: "",
            },
            Field {
                name: "proj_id",
                kind: ForeignId("PROJECT"),
                notes: "",
            },
            Field {
                name: "task_id",
                kind: ForeignId("TASK"),
                notes: "",
            },
            Field {
                name: "task_memo",
                kind: Text,
                notes: "HTML with quotes doubled and each line break as two DEL (0x7F) characters.",
            },
        ],
    },
    Table {
        name: "TASKPRED",
        notes: "Observed in the public sample corpus; missing and extra columns are allowed.",
        fields: &[
            Field {
                name: "",
                kind: Text,
                notes: "Unnamed trailing column observed in synthetic edge cases; preserved positionally.",
            },
            Field {
                name: "aref",
                kind: DateTime,
                notes: "Observed minute-shaped timestamps; relationship-field meaning not yet established.",
            },
            Field {
                name: "arls",
                kind: DateTime,
                notes: "Observed minute-shaped timestamps; relationship-field meaning not yet established.",
            },
            Field {
                name: "comments",
                kind: Text,
                notes: "Preserved as text; no stronger representation established.",
            },
            Field {
                name: "float_path",
                kind: Int,
                notes: "",
            },
            Field {
                name: "lag_hr_cnt",
                kind: Decimal,
                notes: "",
            },
            Field {
                name: "pred_proj_id",
                kind: ForeignId("PROJECT"),
                notes: "",
            },
            Field {
                name: "pred_task_id",
                kind: ForeignId("TASK"),
                notes: "",
            },
            Field {
                name: "pred_type",
                kind: Enum(PRED_TYPE),
                notes: "Known format codes; empty and future codes remain recoverable. See docs/xer-format.md for observed subsets.",
            },
            Field {
                name: "proj_id",
                kind: ForeignId("PROJECT"),
                notes: "",
            },
            Field {
                name: "task_id",
                kind: ForeignId("TASK"),
                notes: "",
            },
            Field {
                name: "task_pred_id",
                kind: Id,
                notes: "",
            },
        ],
    },
    Table {
        name: "TASKPROC",
        notes: "Activity steps, from the 4 corpus files holding them.",
        fields: &[
            Field {
                name: "complete_flag",
                kind: Flag,
                notes: "",
            },
            Field {
                name: "complete_pct",
                kind: Decimal,
                notes: "Percent complete, 0 to 100.",
            },
            Field {
                name: "proc_descr",
                kind: Text,
                notes: "HTML, as notes are.",
            },
            Field {
                name: "proc_id",
                kind: Id,
                notes: "",
            },
            Field {
                name: "proc_name",
                kind: Text,
                notes: "Step name.",
            },
            Field {
                name: "proc_wt",
                kind: Decimal,
                notes: "Step weight.",
            },
            Field {
                name: "proj_id",
                kind: ForeignId("PROJECT"),
                notes: "",
            },
            Field {
                name: "seq_num",
                kind: Int,
                notes: "",
            },
            Field {
                name: "task_id",
                kind: ForeignId("TASK"),
                notes: "",
            },
        ],
    },
    Table {
        name: "TASKRSRC",
        notes: "Observed in the public sample corpus; missing and extra columns are allowed.",
        fields: &[
            Field {
                name: "",
                kind: Text,
                notes: "Unnamed trailing column observed in synthetic edge cases; preserved positionally.",
            },
            Field {
                name: "acct_id",
                kind: Int,
                notes: "Identity-like field; target table not established.",
            },
            Field {
                name: "act_end_date",
                kind: DateTime,
                notes: "",
            },
            Field {
                name: "act_ot_cost",
                kind: Decimal,
                notes: "",
            },
            Field {
                name: "act_ot_qty",
                kind: Decimal,
                notes: "",
            },
            Field {
                name: "act_reg_cost",
                kind: Decimal,
                notes: "",
            },
            Field {
                name: "act_reg_qty",
                kind: Decimal,
                notes: "",
            },
            Field {
                name: "act_start_date",
                kind: DateTime,
                notes: "",
            },
            Field {
                name: "act_this_per_cost",
                kind: Decimal,
                notes: "",
            },
            Field {
                name: "act_this_per_qty",
                kind: Decimal,
                notes: "",
            },
            Field {
                name: "actual_crv",
                kind: Blob,
                notes: "",
            },
            Field {
                name: "cbs_id",
                kind: Int,
                notes: "Identity-like field; target table not established.",
            },
            Field {
                name: "cost_per_qty",
                kind: Decimal,
                notes: "",
            },
            Field {
                name: "cost_per_qty_source_type",
                kind: Text,
                notes: "Observed text codes; vocabulary not established. Do not infer a number from the name.",
            },
            Field {
                name: "cost_qty_link_flag",
                kind: Flag,
                notes: "",
            },
            Field {
                name: "create_date",
                kind: DateTime,
                notes: "",
            },
            Field {
                name: "create_user",
                kind: Text,
                notes: "Preserved as text; no stronger representation established.",
            },
            Field {
                name: "curv_id",
                kind: ForeignId("RSRCCURVDATA"),
                notes: "Resource curve; empty for a linear spread. Often names a curve the file doesn't hold.",
            },
            Field {
                name: "guid",
                kind: Guid,
                notes: "",
            },
            Field {
                name: "has_rsrchours",
                kind: Flag,
                notes: "",
            },
            Field {
                name: "old_reend_date",
                kind: DateTime,
                notes: "",
            },
            Field {
                name: "old_remain_drtn_hr_cnt",
                kind: Decimal,
                notes: "",
            },
            Field {
                name: "old_restart_date",
                kind: DateTime,
                notes: "",
            },
            Field {
                name: "ot_factor",
                kind: Decimal,
                notes: "",
            },
            Field {
                name: "pend_act_ot_qty",
                kind: Decimal,
                notes: "",
            },
            Field {
                name: "pend_act_reg_qty",
                kind: Decimal,
                notes: "",
            },
            Field {
                name: "pend_complete_pct",
                kind: Decimal,
                notes: "",
            },
            Field {
                name: "pend_remain_qty",
                kind: Decimal,
                notes: "",
            },
            Field {
                name: "pobs_id",
                kind: Int,
                notes: "Identity-like field; target table not established.",
            },
            Field {
                name: "proj_id",
                kind: ForeignId("PROJECT"),
                notes: "",
            },
            Field {
                name: "rate_type",
                kind: Text,
                notes: "Preserved as text; no stronger representation established.",
            },
            Field {
                name: "reend_date",
                kind: DateTime,
                notes: "",
            },
            Field {
                name: "relag_drtn_hr_cnt",
                kind: Decimal,
                notes: "",
            },
            Field {
                name: "rem_late_end_date",
                kind: DateTime,
                notes: "",
            },
            Field {
                name: "rem_late_start_date",
                kind: DateTime,
                notes: "",
            },
            Field {
                name: "remain_cost",
                kind: Decimal,
                notes: "",
            },
            Field {
                name: "remain_crv",
                kind: Blob,
                notes: "",
            },
            Field {
                name: "remain_qty",
                kind: Decimal,
                notes: "",
            },
            Field {
                name: "remain_qty_per_hr",
                kind: Decimal,
                notes: "",
            },
            Field {
                name: "restart_date",
                kind: DateTime,
                notes: "",
            },
            Field {
                name: "role_id",
                kind: Int,
                notes: "Identity-like field; target table not established.",
            },
            Field {
                name: "rollup_dates_flag",
                kind: Flag,
                notes: "",
            },
            Field {
                name: "rsrc_id",
                kind: ForeignId("RSRC"),
                notes: "",
            },
            Field {
                name: "rsrc_request_data",
                kind: Blob,
                notes: "",
            },
            Field {
                name: "rsrc_type",
                kind: Enum(RSRC_TYPE),
                notes: "Known format codes; empty and future codes remain recoverable. See docs/xer-format.md for observed subsets.",
            },
            Field {
                name: "skill_level",
                kind: Int,
                notes: "",
            },
            Field {
                name: "target_cost",
                kind: Decimal,
                notes: "",
            },
            Field {
                name: "target_crv",
                kind: Blob,
                notes: "",
            },
            Field {
                name: "target_end_date",
                kind: DateTime,
                notes: "",
            },
            Field {
                name: "target_lag_drtn_hr_cnt",
                kind: Decimal,
                notes: "",
            },
            Field {
                name: "target_qty",
                kind: Decimal,
                notes: "",
            },
            Field {
                name: "target_qty_per_hr",
                kind: Decimal,
                notes: "",
            },
            Field {
                name: "target_start_date",
                kind: DateTime,
                notes: "",
            },
            Field {
                name: "task_id",
                kind: ForeignId("TASK"),
                notes: "",
            },
            Field {
                name: "taskrsrc_id",
                kind: Id,
                notes: "",
            },
            Field {
                name: "taskrsrc_sum_id",
                kind: Int,
                notes: "Identity-like field; target table not established.",
            },
            Field {
                name: "ts_pend_act_end_flag",
                kind: Flag,
                notes: "",
            },
        ],
    },
    Table {
        name: "UDFTYPE",
        notes: "Observed in the public sample corpus; missing and extra columns are allowed.",
        fields: &[
            Field {
                name: "export_flag",
                kind: Flag,
                notes: "",
            },
            Field {
                name: "indicator_expression",
                kind: Text,
                notes: "Preserved as text; no stronger representation established.",
            },
            Field {
                name: "logical_data_type",
                kind: Enum(LOGICAL_DATA_TYPE),
                notes: "Known format codes; empty and future codes remain recoverable. See docs/xer-format.md for observed subsets.",
            },
            Field {
                name: "summary_indicator_expression",
                kind: Text,
                notes: "Preserved as text; no stronger representation established.",
            },
            Field {
                name: "super_flag",
                kind: Flag,
                notes: "",
            },
            Field {
                name: "table_name",
                kind: Text,
                notes: "Preserved as text; no stronger representation established.",
            },
            Field {
                name: "udf_type_id",
                kind: Id,
                notes: "",
            },
            Field {
                name: "udf_type_label",
                kind: Text,
                notes: "Preserved as text; no stronger representation established.",
            },
            Field {
                name: "udf_type_name",
                kind: Text,
                notes: "Preserved as text; no stronger representation established.",
            },
        ],
    },
    Table {
        name: "UDFVALUE",
        notes: "Observed in the public sample corpus; missing and extra columns are allowed.",
        fields: &[
            Field {
                name: "fk_id",
                kind: Int,
                notes: "Polymorphic identity; interpretation depends on UDFTYPE.",
            },
            Field {
                name: "proj_id",
                kind: ForeignId("PROJECT"),
                notes: "",
            },
            Field {
                name: "udf_code_id",
                kind: Int,
                notes: "Polymorphic identity; interpretation depends on UDFTYPE.",
            },
            Field {
                name: "udf_date",
                kind: DateTime,
                notes: "",
            },
            Field {
                name: "udf_number",
                kind: Decimal,
                notes: "",
            },
            Field {
                name: "udf_text",
                kind: Text,
                notes: "Preserved as text; no stronger representation established.",
            },
            Field {
                name: "udf_type_id",
                kind: ForeignId("UDFTYPE"),
                notes: "",
            },
        ],
    },
    Table {
        name: "WBSMEMO",
        notes: "WBS notes, from the 4 corpus files holding them.",
        fields: &[
            Field {
                name: "memo_type_id",
                kind: ForeignId("MEMOTYPE"),
                notes: "",
            },
            Field {
                name: "proj_id",
                kind: ForeignId("PROJECT"),
                notes: "",
            },
            Field {
                name: "wbs_id",
                kind: ForeignId("PROJWBS"),
                notes: "",
            },
            Field {
                name: "wbs_memo",
                kind: Text,
                notes: "HTML with quotes doubled and each line break as two DEL (0x7F) characters.",
            },
            Field {
                name: "wbs_memo_id",
                kind: Id,
                notes: "",
            },
        ],
    },
];
