//! Narrow policies for supplemental metadata, never a general pass-through.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Handling {
    Scrub,
    Retain,
    Refuse,
}
pub(super) fn classify(table: &str, field: &str, value: &str) -> Option<Handling> {
    use Handling::{Refuse, Retain, Scrub};
    match (table, field) {
        ("RCATTYPE", "rsrc_catg_type") | ("UMEASURE", "unit_abbrev") => Some(Scrub),
        ("MEMOTYPE", "eps_flag" | "proj_flag" | "wbs_flag" | "task_flag")
        | ("ROLES", "def_cost_qty_link_flag") => Some(if matches!(value, "Y" | "N") {
            Retain
        } else {
            Refuse
        }),
        ("SCHEDOPTIONS", "LevelPriorityList") => {
            Some(if priority_list(value) { Retain } else { Refuse })
        }
        _ => None,
    }
}
// Never widen unit abbreviations: their maximum width is not in the model.
// Shared vocabulary values also inherit this limit in their other fields.
pub(super) fn limit_labels(
    xer: &crate::RawXer,
    limits: &mut std::collections::BTreeMap<String, usize>,
) {
    for table in xer.tables().iter().filter(|t| t.name == "UMEASURE") {
        if let Some(column) = table.field_index("unit_abbrev") {
            for row in &table.rows {
                if let Some(value) = row.get(column)
                    && let Some(limit) = limits.get_mut(value)
                {
                    *limit = (*limit).min(value.chars().count());
                }
            }
        }
    }
}

fn priority_list(value: &str) -> bool {
    fn field(value: &str) -> bool {
        matches!(value, "priority_type" | "early_start_date")
    }
    fn direction(value: &str) -> bool {
        matches!(value, "ASC" | "DESC")
    }
    let mut any = false;
    for record in value.split('\x7f') {
        if record.is_empty() {
            continue;
        }
        any = true;
        if let Some((fields, order)) = record.split_once('/') {
            if !direction(order) {
                return false;
            }
            let fields = fields.strip_suffix(',').unwrap_or(fields);
            if !fields.split(',').all(field) {
                return false;
            }
        } else if let Some((name, order)) = record.split_once(',') {
            if !field(name) || !direction(order) {
                return false;
            }
        } else {
            return false;
        }
    }
    any
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        anonymize::{AnonymizeError, anonymize},
        raw,
    };
    #[test]
    fn supplemental_labels_are_scrubbed_and_validated_metadata_is_preserved() {
        let input=b"ERMHDR\t23.12\t2026-01-01\tProject\tSynthetic\tSynthetic\tSynthetic\tProject Management\tUSD\n%T\tMEMOTYPE\n%F\tmemo_type_id\tproj_flag\twbs_flag\ttask_flag\n%R\t1\tY\tN\tY\n%T\tRCATTYPE\n%F\trsrc_catg_type_id\trsrc_catg_type\n%R\t2\tPRIVATE_CATEGORY\n%T\tROLES\n%F\trole_id\tdef_cost_qty_link_flag\n%R\t3\tY\n%T\tUMEASURE\n%F\tunit_id\tunit_abbrev\n%R\t4\tPRIVATE_UNIT\n%T\tSCHEDOPTIONS\n%F\tLevelPriorityList\n%R\tpriority_type,/ASC\x7f\x7f\n%E\n";
        let original = raw::read(input.as_slice()).unwrap();
        let output = anonymize(&original, 42).unwrap();
        let mut bytes = Vec::new();
        output.xer.write(&mut bytes).unwrap();
        let text = String::from_utf8(bytes).unwrap();
        assert!(!text.contains("PRIVATE_CATEGORY"));
        assert!(!text.contains("PRIVATE_UNIT"));
        assert!(text.contains("priority_type,/ASC\x7f\x7f"));
        assert!(text.contains("%R\t1\tY\tN\tY"));
        assert!(
            output
                .report
                .iter()
                .any(|f| f.table == "RCATTYPE" && f.field == "rsrc_catg_type")
        );
        assert!(
            output
                .report
                .iter()
                .any(|f| f.table == "UMEASURE" && f.field == "unit_abbrev")
        );
    }
    #[test]
    fn invalid_flags_and_unclassified_priority_content_are_refused_without_values() {
        for (table, field) in [
            ("MEMOTYPE", "proj_flag"),
            ("MEMOTYPE", "wbs_flag"),
            ("MEMOTYPE", "task_flag"),
            ("ROLES", "def_cost_qty_link_flag"),
            ("SCHEDOPTIONS", "LevelPriorityList"),
        ] {
            for value in ["yes", "0", "PRIVATE_VALUE"] {
                let input = format!(
                    "ERMHDR\t23.12\t2026-01-01\tProject\tSynthetic\tSynthetic\tSynthetic\tProject Management\tUSD\n%T\t{table}\n%F\t{field}\n%R\t{value}\n%E\n"
                );
                let error = anonymize(&raw::read(input.as_bytes()).unwrap(), 0).unwrap_err();
                assert!(matches!(error, AnonymizeError::Unsupported { .. }));
                assert!(!error.to_string().contains(value));
            }
        }
    }
    #[test]
    fn priority_list_layouts_require_known_fields_and_directions() {
        for v in [
            "priority_type,ASC\x7f\x7f",
            "priority_type,/ASC",
            "early_start_date,/ASC\x7f\x7f",
            "priority_type,early_start_date/DESC",
            "priority_type,ASC\x7fearly_start_date,DESC",
        ] {
            assert!(priority_list(v), "{v:?}");
        }
        for v in [
            "\x7f\x7f",
            "priority_type,SECRET",
            "private_name/ASC",
            "priority_type,private_name/ASC",
            "priority_type,/ASC/SECRET",
            "priority_type,ASC\x7fPRIVATE_TEXT",
            "priority_type,,/ASC",
        ] {
            assert!(!priority_list(v), "{v:?}");
        }
    }
}

#[cfg(test)]
mod width_tests {
    #[test]
    fn unit_abbreviations_do_not_grow_and_shared_values_have_one_mapping() {
        let input=b"ERMHDR\t23.12\t2026-01-01\tProject\tSynthetic\tSynthetic\tSynthetic\tProject Management\tUSD\n%T\tUMEASURE\n%F\tunit_id\tunit_abbrev\tunit_name\n%R\t1\thr\thr\n%R\t2\tm\tm\n%E\n";
        let raw = crate::raw::read(input.as_slice()).unwrap();
        let output = crate::anonymize::anonymize(&raw, 42).unwrap();
        let table = output.xer.table("UMEASURE").unwrap();
        for row in &table.rows {
            assert_eq!(row.cells[1], row.cells[2]);
        }
        assert!(table.rows[0].cells[1].len() <= 2);
        assert_eq!(table.rows[1].cells[1].len(), 1);
    }
}
