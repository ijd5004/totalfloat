//! Deterministic field-aware text pseudonymization with verified cell coverage.
//!
//! IDs, GUIDs, costs and scheduling inputs/results remain sensitive metadata.
//! This is not a guarantee against reidentification. Unsupported content is
//! refused; callers must not publish an output until independent review.

use std::collections::{BTreeMap, BTreeSet};

use sha2::{Digest, Sha256};
use totalfloat_model::DateTime;

mod supplemental;

use crate::{
    RawXer,
    calendar_data::CalendarData,
    raw::{self, SegmentKind},
    registry::{self, FieldKind},
    typed::{Cell, CellState, Decimal, Value},
};

/// A field's scrubbed nonempty cell count. Never contains original values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScrubbedField {
    /// XER table, or `ERMHDR` for header metadata.
    pub table: String,
    /// Field name.
    pub field: String,
    /// Changed nonempty cells.
    pub count: usize,
}

/// Validated output bytes and a deterministic report without the private mapping.
#[derive(Debug)]
pub struct Anonymized {
    /// Reparsed, round-trip-stable output.
    pub xer: RawXer,
    /// Fields in table/field order, without original or replacement values.
    pub report: Vec<ScrubbedField>,
}

/// Why no anonymized output can be returned.
#[derive(Debug, thiserror::Error)]
pub enum AnonymizeError {
    /// Unsupported populated field or opaque structure.
    #[error("unclassified content in {table}.{field}; no output produced")]
    Unsupported {
        /// Table name; never a cell value.
        table: String,
        /// Field name; never a cell value.
        field: String,
    },
    /// Free bytes, malformed header or an unsupported row layout.
    #[error("unsupported header, stray bytes or row layout; no output produced")]
    Layout,
    /// Generated content did not match the field-aware scrubbing plan.
    #[error("scrubbed field verification failed; no output produced")]
    Verification,
    /// No unique labels fit the retained field-length limits.
    #[error("no unique labels fit the retained code-length limits; no output produced")]
    LabelSpace,
    /// A generated replacement was refused by the codec.
    #[error(transparent)]
    Edit(#[from] raw::EditError),
    /// In-memory serialization failed.
    #[error(transparent)]
    Write(#[from] std::io::Error),
    /// Re-reading output failed.
    #[error(transparent)]
    Read(#[from] raw::ReadError),
}

fn unsupported(table: &str, field: &str) -> AnonymizeError {
    AnonymizeError::Unsupported {
        table: table.to_owned(),
        field: field.to_owned(),
    }
}

// Text columns with operational meanings rather than personal/project labels.
fn operational_text(table: &str, field: &str) -> bool {
    match table {
        "ACTVCODE" => field == "color",
        "CURRTYPE" => matches!(
            field,
            "curr_short_name"
                | "curr_symbol"
                | "curr_type"
                | "decimal_symbol"
                | "digit_group_symbol"
                | "neg_curr_fmt_type"
                | "pos_curr_fmt_type"
        ),
        "PROJECT" => matches!(
            field,
            "baselines_to_export"
                | "def_qty_type"
                | "def_rate_type"
                | "hist_interval"
                | "hist_level"
                | "intg_proj_type"
                | "name_sep_char"
                | "publish_spread_assign_level"
                | "sched_wbs_heir_type"
                | "schedule_type"
                | "sum_assign_level"
        ),
        "PROJWBS" => matches!(
            field,
            "dscnt_period_type"
                | "ev_compute_type"
                | "ev_etc_compute_type"
                | "plan_open_state"
                | "status_code"
        ),
        "RSRC" | "ROLES" => field == "cost_qty_type",
        "TASK" => matches!(field, "priority_type" | "review_type"),
        "TASKRSRC" => matches!(field, "cost_per_qty_source_type" | "rate_type"),
        "UDFTYPE" => field == "table_name",
        _ => false,
    }
}

// Supplemental text fields in tables not yet covered by the typed registry.
// Unknown columns still fail closed, including unknown numeric-looking values.
fn identifying_field(field: &str) -> bool {
    [
        "_name",
        "_descr",
        "_description",
        "_notes",
        "_memo",
        "_text",
        "_url",
        "_path",
        "_user",
    ]
    .iter()
    .any(|suffix| field.ends_with(suffix))
        || matches!(
            field,
            "memo"
                | "notes"
                | "comments"
                | "user_name"
                | "email_addr"
                | "office_phone"
                | "other_phone"
                | "employee_code"
        )
}

fn parses_as(kind: FieldKind, value: &str) -> bool {
    fn parsed<'a, T: Value<'a>>(kind: FieldKind, value: &'a str) -> bool {
        matches!(Cell::<T>::decode(kind, value).state(), CellState::Parsed(_))
    }
    match kind {
        FieldKind::Id | FieldKind::ForeignId(_) | FieldKind::Int => parsed::<i64>(kind, value),
        FieldKind::Decimal => parsed::<Decimal>(kind, value),
        FieldKind::DateTime => parsed::<DateTime>(kind, value),
        FieldKind::Flag => parsed::<bool>(kind, value),
        FieldKind::Enum(_) | FieldKind::Guid => parsed::<&str>(kind, value),
        FieldKind::Text | FieldKind::Blob => false,
    }
}

/// The header fields kept as they are: version, export date, product, module and
/// currency. Each has a narrow form, so free text can't pass as one.
fn header_is_plain(fields: &[String]) -> bool {
    let [version, date, product, _, _, _, module, currency] = fields else {
        return false;
    };
    let number =
        |text: &str| !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit() || b == b'.');
    let day = date.len() == 10
        && date.bytes().enumerate().all(|(index, b)| match index {
            4 | 7 => b == b'-',
            _ => b.is_ascii_digit(),
        });
    number(version)
        && day
        && product == "Project"
        && module == "Project Management"
        && (1..=6).contains(&currency.len())
        && currency.bytes().all(|b| b.is_ascii_uppercase())
}

fn scrub(table: &str, field: &str, value: &str) -> Result<bool, AnonymizeError> {
    if value.is_empty() {
        return Ok(false);
    }
    if let Some(policy) = supplemental::classify(table, field, value) {
        return match policy {
            supplemental::Handling::Scrub => Ok(true),
            supplemental::Handling::Retain => Ok(false),
            supplemental::Handling::Refuse => Err(unsupported(table, field)),
        };
    }
    // Formula text may reference other labels; replacing it could corrupt its
    // meaning, while keeping it could leak confidential text. Refuse it.
    if table == "UDFTYPE" && field.ends_with("_expression") {
        return Err(unsupported(table, field));
    }
    match registry::field(table, field).map(|f| f.kind) {
        Some(FieldKind::Text) => Ok(!operational_text(table, field)),
        Some(FieldKind::Blob) if table == "CALENDAR" && field == "clndr_data" => {
            if CalendarData::decode(value).is_valid()
                && crate::calendar_data::contains_only_pattern_metadata(value)
            {
                Ok(false)
            } else {
                Err(unsupported(table, field))
            }
        }
        Some(FieldKind::Blob) => Err(unsupported(table, field)),
        // Kept as it is, so it has to be what the column says it is. Text in a
        // date or number column would otherwise reach the output.
        Some(kind) if parses_as(kind, value) => Ok(false),
        None if identifying_field(field)
            || (table == "MEMOTYPE" && field == "memo_type")
            || (table == "POBS" && field == "pobs_manager")
            || (table == "COSTTYPE" && field == "cost_type") =>
        {
            Ok(true)
        }
        None if field.ends_with("_id") && value.parse::<u64>().is_ok() => Ok(false),
        None if matches!(
            table,
            "COSTTYPE" | "MEMOTYPE" | "POBS" | "RCATTYPE" | "ROLES" | "UMEASURE"
        ) && field == "seq_num"
            && value.parse::<i64>().is_ok() =>
        {
            Ok(false)
        }
        None if ((table == "FINTMPL" && field == "default_flag")
            || (table == "MEMOTYPE" && field == "eps_flag"))
            && matches!(value, "Y" | "N") =>
        {
            Ok(false)
        }
        None if table == "RCATTYPE"
            && field == "rsrc_catg_short_len"
            && value.parse::<u32>().is_ok() =>
        {
            Ok(false)
        }
        _ => Err(unsupported(table, field)),
    }
}

fn mix(mut value: u64) -> u64 {
    // A permutation used for deterministic labels, not a hash of private text.
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

fn check_layout(xer: &RawXer) -> Result<(), AnonymizeError> {
    if !header_is_plain(&xer.header.fields) {
        return Err(AnonymizeError::Layout);
    }
    for segment in xer.segments() {
        if matches!(segment.kind, SegmentKind::Stray | SegmentKind::Trailing)
            && !xer.source()[segment.span.clone()]
                .iter()
                .all(u8::is_ascii_whitespace)
        {
            return Err(AnonymizeError::Layout);
        }
    }
    for table in xer.tables() {
        if table
            .rows
            .iter()
            .any(|row| row.cells.len() > table.fields.len())
        {
            return Err(AnonymizeError::Layout);
        }
    }
    Ok(())
}

type Counts = BTreeMap<(String, String), usize>;

fn note(counts: &mut Counts, table: &str, field: &str) {
    *counts
        .entry((table.to_owned(), field.to_owned()))
        .or_default() += 1;
}

fn scrub_header(
    bytes: &[u8],
    xer: &RawXer,
    names: &BTreeMap<String, String>,
    counts: &mut Counts,
) -> Vec<u8> {
    let end = bytes
        .iter()
        .position(|&b| b == b'\n')
        .unwrap_or(bytes.len());
    let cells: Vec<_> = bytes[..end].split(|&b| b == b'\t').collect();
    let mut output = Vec::with_capacity(bytes.len());
    for (index, cell) in cells.iter().enumerate() {
        if index > 0 {
            output.push(b'\t');
        }
        let replacement = index
            .checked_sub(1)
            .filter(|i| (3..=5).contains(i))
            .and_then(|i| xer.header.fields.get(i))
            .and_then(|value| names.get(value));
        if let Some(replacement) = replacement {
            output.extend_from_slice(replacement.as_bytes());
            let label = match index {
                4 => "user_name",
                5 => "user_full_name",
                _ => "database_name",
            };
            note(counts, "ERMHDR", label);
        } else {
            output.extend_from_slice(cell);
        }
    }
    output.extend_from_slice(&bytes[end..]);
    output
}

fn labels(
    xer: &RawXer,
    values: &BTreeSet<String>,
    seed: u64,
) -> Result<BTreeMap<String, String>, AnonymizeError> {
    let mut limits: BTreeMap<String, usize> =
        values.iter().map(|value| (value.clone(), 18)).collect();
    let mut type_limits = BTreeMap::new();
    for table in xer.tables().iter().filter(|table| table.name == "ACTVTYPE") {
        if let (Some(id), Some(limit)) = (
            table.field_index("actv_code_type_id"),
            table.field_index("actv_short_len"),
        ) {
            for row in &table.rows {
                if let Some((id, limit)) = row
                    .get(id)
                    .zip(row.get(limit))
                    .and_then(|(id, limit)| Some((id, limit.parse::<usize>().ok()?)))
                {
                    type_limits.insert(id.to_owned(), limit);
                }
            }
        }
    }
    for table in xer.tables().iter().filter(|table| table.name == "ACTVCODE") {
        if let (Some(kind), Some(value)) = (
            table.field_index("actv_code_type_id"),
            table.field_index("short_name"),
        ) {
            for row in &table.rows {
                if let Some((value, limit)) = row
                    .get(value)
                    .zip(row.get(kind).and_then(|id| type_limits.get(id)))
                    && let Some(current) = limits.get_mut(value)
                {
                    *current = (*current).min(*limit);
                }
            }
        }
    }
    supplemental::limit_labels(xer, &mut limits);
    let original: BTreeSet<_> = values
        .iter()
        .map(|value| value.to_ascii_lowercase())
        .collect();
    let mut used = BTreeSet::new();
    let mut result = BTreeMap::new();
    let mut counter = 0_u64;
    // Labels are handed out in hash order. In sorted order, undoing `mix` would
    // give each value's alphabetical rank.
    let mut order: Vec<&String> = values.iter().collect();
    order.sort_by_cached_key(|value| {
        let mut digest = Sha256::new();
        digest.update(b"totalfloat.anonymize-order/1\0");
        digest.update(seed.to_le_bytes());
        digest.update(value.as_bytes());
        digest.finalize()
    });
    for value in order {
        let limit = limits[value];
        if limit == 0 {
            return Err(AnonymizeError::LabelSpace);
        }
        let mut chosen = None;
        for _ in 0..65_536 {
            let token = token(mix(seed.wrapping_add(counter)), limit);
            counter = counter.checked_add(1).ok_or(AnonymizeError::LabelSpace)?;
            let folded = token.to_ascii_lowercase();
            if !original.contains(&folded) && used.insert(folded) {
                chosen = Some(token);
                break;
            }
        }
        result.insert(value.clone(), chosen.ok_or(AnonymizeError::LabelSpace)?);
    }
    Ok(result)
}

fn token(value: u64, limit: usize) -> String {
    const ALPHABET: &[u8; 36] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    if limit >= 18 {
        return format!("TF{value:016X}");
    }
    let mut value = value;
    let mut reversed = Vec::new();
    loop {
        reversed.push(char::from(ALPHABET[(value % 36) as usize]));
        value /= 36;
        if value == 0 || reversed.len() == limit {
            break;
        }
    }
    reversed.into_iter().rev().collect()
}

fn verify(
    original: &RawXer,
    output: &RawXer,
    names: &BTreeMap<String, String>,
) -> Result<(), AnonymizeError> {
    if original.tables().len() != output.tables().len()
        || original.header.fields.len() != output.header.fields.len()
    {
        return Err(AnonymizeError::Verification);
    }
    for (index, value) in original.header.fields.iter().enumerate() {
        let expected = if (3..=5).contains(&index) {
            names.get(value).unwrap_or(value)
        } else {
            value
        };
        if output.header.fields.get(index) != Some(expected) {
            return Err(AnonymizeError::Verification);
        }
    }
    for (before, after) in original.tables().iter().zip(output.tables()) {
        if before.name != after.name
            || before.fields != after.fields
            || before.rows.len() != after.rows.len()
        {
            return Err(AnonymizeError::Verification);
        }
        for (row, actual) in before.rows.iter().zip(&after.rows) {
            if row.cells.len() != actual.cells.len() {
                return Err(AnonymizeError::Verification);
            }
            for ((field, value), actual) in before.fields.iter().zip(&row.cells).zip(&actual.cells)
            {
                let expected = if scrub(&before.name, field, value)? {
                    names.get(value).ok_or(AnonymizeError::Verification)?
                } else {
                    value
                };
                if actual != expected {
                    return Err(AnonymizeError::Verification);
                }
            }
        }
    }
    Ok(())
}

/// Scrub recognized identifying text while preserving every other cell's bytes.
///
/// Equal source strings get equal labels across tables and header metadata.
/// Labels depend on the explicit seed and sorted vocabulary, never the clock.
/// Costs, IDs, GUIDs, dates, durations, options and calendar patterns are retained.
///
/// # Errors
///
/// Refuses populated unclassified fields/opaque blobs, stray bytes, unsupported
/// headers and any failed field-aware verification. Every designated cell is
/// replaced; fixed format tokens and structural metadata may retain coincidental
/// source substrings. Code labels respect retained dictionary length limits.
/// No partial output is returned.
pub fn anonymize(xer: &RawXer, seed: u64) -> Result<Anonymized, AnonymizeError> {
    check_layout(xer)?;
    let mut values = BTreeSet::new();
    for value in &xer.header.fields[3..=5] {
        if !value.is_empty() {
            values.insert(value.to_owned());
        }
    }
    for table in xer.tables() {
        for row in &table.rows {
            for (field, value) in table.fields.iter().zip(&row.cells) {
                if scrub(&table.name, field, value)? {
                    values.insert(value.to_owned());
                }
            }
        }
    }
    let names = labels(xer, &values, seed)?;
    let mut edited = xer.clone();
    let mut counts = Counts::new();
    for (table_index, table) in xer.tables().iter().enumerate() {
        for (row_index, row) in table.rows.iter().enumerate() {
            let mut changes = Vec::new();
            for (field_index, (field, value)) in table.fields.iter().zip(&row.cells).enumerate() {
                if scrub(&table.name, field, value)?
                    && let Some(replacement) = names.get(value)
                {
                    changes.push((field_index, replacement.clone()));
                    note(&mut counts, &table.name, field);
                }
            }
            edited.set_cells(table_index, row_index, &changes)?;
        }
    }
    let mut bytes = Vec::new();
    edited.write(&mut bytes)?;
    let output = scrub_header(&bytes, xer, &names, &mut counts);
    let output = raw::read(output)?;
    verify(xer, &output, &names)?;
    Ok(Anonymized {
        xer: output,
        report: counts
            .into_iter()
            .map(|((table, field), count)| ScrubbedField {
                table,
                field,
                count,
            })
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `mix` can be undone, so a label made from a rank gives the rank away.
    #[test]
    fn a_label_says_nothing_about_alphabetical_order() {
        let xer = crate::raw::read(b"ERMHDR\t23.12\r\n".as_slice()).unwrap();
        let values: BTreeSet<String> = (0..40).map(|n| format!("Value {n:02}")).collect();
        for seed in [0, 1, 42] {
            let names = labels(&xer, &values, seed).unwrap();
            let by_rank = values
                .iter()
                .enumerate()
                .filter(|(rank, value)| {
                    let rank = u64::try_from(*rank).unwrap();
                    names[*value] == token(mix(seed.wrapping_add(rank)), 18)
                })
                .count();
            assert!(by_rank < 5, "{by_rank} of 40 labels follow the sort order");
        }
    }

    fn fixture() -> RawXer {
        raw::read(include_bytes!("../../../fixtures/xer/model-mapping.xer").as_slice()).unwrap()
    }

    #[test]
    fn clean_application_fixture_matches_validated_output() {
        let input =
            raw::read(include_bytes!("../../../fixtures/xer/anonymize-clean.xer").as_slice())
                .unwrap();
        let output = anonymize(&input, 42).unwrap();
        // Exact bytes were loaded and checked in XER Tools 2.2b; never silently
        // replace this expectation when a new output needs external validation.
        assert_eq!(
            bytes(&output.xer),
            include_bytes!("../../../fixtures/xer/anonymize-clean.expected.xer").as_slice()
        );
    }

    #[test]
    fn field_aware_policy_allows_short_codes_that_overlap_fixed_format_tokens() {
        let original = fixture();
        let output = anonymize(&original, 0).unwrap();
        assert!(output.xer.table("CALENDAR").is_some());
        let table = original.table_index("ACTVCODE").unwrap();
        let column = original.tables()[table].field_index("short_name").unwrap();
        for (before, after) in original.tables()[table]
            .rows
            .iter()
            .zip(&output.xer.tables()[table].rows)
        {
            assert_ne!(before.cells[column], after.cells[column]);
        }
    }

    fn bytes(xer: &RawXer) -> Vec<u8> {
        let mut result = Vec::new();
        xer.write(&mut result).unwrap();
        result
    }

    #[test]
    fn fixture_scrubs_text_preserves_scheduling_cells_and_round_trips_exactly() {
        let original = fixture();
        let result = anonymize(&original, 0).unwrap();
        let output = bytes(&result.xer);
        for value in ["Synthetic", "synthetic", "SYN1", "SYN2", "Synthetic note"] {
            assert!(!String::from_utf8_lossy(&output).contains(value));
        }
        assert_eq!(bytes(&raw::read(output.clone()).unwrap()), output);
        let before = crate::to_model(&original).0;
        let after = crate::to_model(&result.xer).0;
        assert_eq!(before.len(), after.len());
        for (before, after) in before.iter().zip(&after) {
            assert_eq!(before.project.id, after.project.id);
            assert_eq!(before.project.data_date, after.project.data_date);
            assert_eq!(before.project.options, after.project.options);
            assert_eq!(before.project.stored, after.project.stored);
            assert_eq!(before.relationships, after.relationships);
            assert_eq!(before.activities.len(), after.activities.len());
            assert_eq!(before.assignments.len(), after.assignments.len());
            for (id, assignment) in &before.assignments {
                let other = &after.assignments[id];
                assert_eq!(assignment.activity, other.activity);
                assert_eq!(assignment.resource, other.resource);
                assert_eq!(assignment.project, other.project);
            }
            for (id, activity) in &before.activities {
                let other = &after.activities[id];
                assert_eq!(activity.stored, other.stored);
                assert_eq!(activity.original_duration, other.original_duration);
                assert_eq!(activity.remaining_duration, other.remaining_duration);
                assert_eq!(activity.actual_start, other.actual_start);
                assert_eq!(activity.actual_finish, other.actual_finish);
                assert_eq!(activity.calendar, other.calendar);
                assert_eq!(activity.codes, other.codes);
            }
            for (id, calendar) in &before.calendars {
                assert_eq!(calendar.work_week, after.calendars[id].work_week);
                assert_eq!(calendar.exceptions, after.calendars[id].exceptions);
            }
        }
        for (before, after) in original.tables().iter().zip(result.xer.tables()) {
            assert_eq!(before.fields, after.fields);
            assert_eq!(before.rows.len(), after.rows.len());
            for (row, changed) in before.rows.iter().zip(&after.rows) {
                for ((field, value), actual) in
                    before.fields.iter().zip(&row.cells).zip(&changed.cells)
                {
                    if !scrub(&before.name, field, value).unwrap() {
                        assert_eq!(value, actual);
                    }
                }
            }
        }
        assert!(result.report.iter().any(|field| field.table == "ERMHDR"));
        assert!(result.report.iter().any(|field| field.table == "UDFVALUE"));
    }

    #[test]
    fn short_code_limits_and_shared_mapping_are_respected() {
        let mut original = fixture();
        let table = original.table_index("ACTVTYPE").unwrap();
        let field = original.tables()[table]
            .field_index("actv_short_len")
            .unwrap();
        for row in 0..original.tables()[table].rows.len() {
            original.set_cell(table, row, field, "1").unwrap();
        }
        let output = anonymize(&original, 42).unwrap();
        let table = output.xer.table("ACTVCODE").unwrap();
        let field = table.field_index("short_name").unwrap();
        let values: BTreeSet<_> = table.rows.iter().map(|row| &row.cells[field]).collect();
        assert_eq!(values.len(), table.rows.len());
        assert!(values.iter().all(|value| value.len() <= 1));
        assert!(values.iter().all(|value| !["A", "B"].contains(value)));
        let table = original.table_index("ACTVTYPE").unwrap();
        let field = original.tables()[table]
            .field_index("actv_short_len")
            .unwrap();
        original.set_cell(table, 0, field, "0").unwrap();
        assert!(matches!(
            anonymize(&original, 42),
            Err(AnonymizeError::LabelSpace)
        ));
    }

    #[test]
    fn ignored_calendar_metadata_cannot_carry_free_text() {
        let mut original = fixture();
        let table = original.table_index("CALENDAR").unwrap();
        let field = original.tables()[table].field_index("clndr_data").unwrap();
        let source = "(0||CalendarData()((0||VIEW(PRIVATE_TEXT)())))";
        assert!(CalendarData::decode(source).is_valid());
        original.set_cell(table, 0, field, source).unwrap();
        assert!(matches!(
            anonymize(&original, 0),
            Err(AnonymizeError::Unsupported { .. })
        ));
    }

    #[test]
    fn text_in_a_typed_column_is_refused() {
        for (table, field, value) in [
            ("TASK", "act_start_date", "PRIVATE_DATE"),
            ("TASK", "target_drtn_hr_cnt", "PRIVATE_NUMBER"),
            ("TASK", "task_id", "PRIVATE_ID"),
            ("TASK", "status_code", "PRIVATE_STATUS"),
            ("TASK", "guid", "PRIVATE_GUID"),
        ] {
            assert!(
                matches!(
                    scrub(table, field, value),
                    Err(AnonymizeError::Unsupported { .. })
                ),
                "{table}.{field}"
            );
        }
    }

    #[test]
    fn valid_typed_values_are_kept() {
        for (table, field, value) in [
            ("TASK", "act_start_date", "2026-01-05 08:00"),
            ("TASK", "target_drtn_hr_cnt", "8"),
            ("TASK", "task_id", "1001"),
            ("TASK", "status_code", "TK_Active"),
            ("RSRC", "cost_qty_type", "QT_Hour"),
            ("ROLES", "cost_qty_type", "QT_Hour"),
        ] {
            assert!(
                matches!(scrub(table, field, value), Ok(false)),
                "{table}.{field}"
            );
        }
    }

    #[test]
    fn text_in_a_kept_header_field_is_refused() {
        let good = [
            "23.12",
            "2026-01-01",
            "Project",
            "a",
            "b",
            "c",
            "Project Management",
            "USD",
        ];
        let fields = |index: usize, value: &str| {
            let mut fields = good.map(str::to_owned);
            fields[index] = value.to_owned();
            fields
        };
        assert!(header_is_plain(&good.map(str::to_owned)));
        for index in [0, 1, 2, 6, 7] {
            assert!(!header_is_plain(&fields(index, "PRIVATE NAME")), "{index}");
        }
    }

    #[test]
    fn formula_text_is_refused_instead_of_corrupted_or_retained() {
        let mut original = fixture();
        let table = original
            .push_table("UDFTYPE", vec!["indicator_expression".to_owned()])
            .unwrap();
        original
            .push_row(table, &["PRIVATE_FORMULA".to_owned()])
            .unwrap();
        assert!(matches!(
            anonymize(&original, 0),
            Err(AnonymizeError::Unsupported { .. })
        ));
    }

    #[test]
    fn same_seed_is_identical_and_changes_do_not_mutate_the_input() {
        let original = fixture();
        let source = bytes(&original);
        let first = anonymize(&original, 42).unwrap();
        let second = anonymize(&original, 42).unwrap();
        assert_eq!(bytes(&first.xer), bytes(&second.xer));
        assert_eq!(first.report, second.report);
        assert_ne!(
            bytes(&first.xer),
            bytes(&anonymize(&original, 43).unwrap().xer)
        );
        assert_eq!(bytes(&original), source);
    }

    #[test]
    fn repeated_values_have_one_mapping_across_header_and_tables() {
        let mut original = fixture();
        let table = original.table_index("PROJECT").unwrap();
        let field = original.tables()[table]
            .field_index("proj_short_name")
            .unwrap();
        original.set_cell(table, 0, field, "synthetic").unwrap();
        let result = anonymize(&original, 0).unwrap();
        assert_eq!(
            result.xer.header.fields[3],
            result.xer.tables()[table].rows[0].cells[field]
        );
    }

    #[test]
    fn identifiers_overlapping_a_protected_version_do_not_change_the_version() {
        let mut original = fixture();
        let table = original.table_index("PROJECT").unwrap();
        let field = original.tables()[table]
            .field_index("proj_short_name")
            .unwrap();
        original.set_cell(table, 0, field, "23.12").unwrap();
        let result = anonymize(&original, 0).unwrap();
        assert_eq!(result.xer.header.version(), Some("23.12"));
        assert_ne!(&result.xer.tables()[table].rows[0].cells[field], "23.12");
    }

    #[test]
    fn unsupported_fields_opaque_data_and_stray_bytes_are_refused_without_values() {
        let mut original = fixture();
        original
            .push_table("FUTURE", vec!["opaque".to_owned()])
            .unwrap();
        let table = original.table_index("FUTURE").unwrap();
        original
            .push_row(table, &["PRIVATE_ACCOUNT_NAME".to_owned()])
            .unwrap();
        let error = anonymize(&original, 0).unwrap_err().to_string();
        assert!(error.contains("FUTURE.opaque"));
        assert!(!error.contains("PRIVATE_ACCOUNT_NAME"));
        let data = b"ERMHDR\t23.12\t2026-01-01\tProject\tAlice\tBob\tCompany\tProject Management\tUSD\nPRIVATE_TRAILING\n%E\n";
        assert!(matches!(
            anonymize(&raw::read(data.as_slice()).unwrap(), 0),
            Err(AnonymizeError::Layout)
        ));
        let mut original = fixture();
        let table = original.table_index("CALENDAR").unwrap();
        let field = original.tables()[table].field_index("clndr_data").unwrap();
        original
            .set_cell(table, 0, field, "PRIVATE_CALENDAR_BLOB")
            .unwrap();
        assert!(matches!(
            anonymize(&original, 0),
            Err(AnonymizeError::Unsupported { .. })
        ));
    }

    #[test]
    fn windows_1252_and_crlf_are_preserved_and_names_are_scrubbed() {
        let data = b"ERMHDR\t23.12\t2026-01-01\tProject\tAlice\tBob\tCompany\tProject Management\tUSD\r\n%T\tPROJECT\r\n%F\tproj_id\tproj_short_name\r\n%R\t1\tCaf\xe9\r\n%E\r\n";
        let result = anonymize(&raw::read(data.as_slice()).unwrap(), 12).unwrap();
        let output = bytes(&result.xer);
        assert!(!output.windows(4).any(|value| value == b"Caf\xe9"));
        assert_eq!(result.xer.line_ending, raw::LineEnding::CrLf);
        assert!(output.windows(2).any(|pair| pair == b"\r\n"));
    }

    #[test]
    fn memo_text_is_scrubbed_without_changing_its_foreign_ids() {
        let mut original = fixture();
        let table = original
            .push_table(
                "TASKMEMO",
                vec![
                    "memo_id".to_owned(),
                    "task_id".to_owned(),
                    "task_memo".to_owned(),
                ],
            )
            .unwrap();
        original
            .push_row(
                table,
                &[
                    "7".to_owned(),
                    "101".to_owned(),
                    "PRIVATE_MEMO\nsecond line".to_owned(),
                ],
            )
            .unwrap();
        let result = anonymize(&original, 0).unwrap();
        assert_eq!(
            result.xer.tables()[table].rows[0]
                .cells
                .iter()
                .take(2)
                .collect::<Vec<_>>(),
            ["7", "101"]
        );
        assert!(!String::from_utf8_lossy(&bytes(&result.xer)).contains("PRIVATE_MEMO"));
    }
}
