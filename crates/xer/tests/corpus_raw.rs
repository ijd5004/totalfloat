//! Reads every `*.xer` under `$TOTALFLOAT_CORPUS` with the raw reader and
//! prints a summary. Skips when the variable is unset (CI has no corpus).
//!
//! Run with `cargo test -p totalfloat-xer --test corpus_raw -- --nocapture`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use totalfloat_xer::raw::{self, Encoding, LineEnding};
use totalfloat_xer::registry::{self, FieldKind};

fn xer_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            xer_files(&path, out);
        } else if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("xer"))
        {
            out.push(path);
        }
    }
}

#[test]
fn corpus_files_parse_losslessly() {
    let Some(root) = std::env::var_os("TOTALFLOAT_CORPUS") else {
        eprintln!("TOTALFLOAT_CORPUS is not set; skipping corpus test");
        return;
    };
    let root = PathBuf::from(root);
    let mut files = Vec::new();
    xer_files(&root, &mut files);
    files.sort();

    let mut errors: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut summary: BTreeMap<String, usize> = BTreeMap::new();
    let mut diagnostics: BTreeMap<String, usize> = BTreeMap::new();
    let mut noisy: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
    let (mut tables, mut rows) = (0usize, 0usize);
    for path in &files {
        let rel = path
            .strip_prefix(&root)
            .unwrap_or(path)
            .display()
            .to_string();
        let bytes = std::fs::read(path).unwrap();
        let len = bytes.len();
        match raw::read(bytes) {
            Err(e) => errors.entry(e.to_string()).or_default().push(rel),
            Ok(x) => {
                let mut out = Vec::new();
                x.write(&mut out).unwrap();
                let first_difference = out
                    .iter()
                    .zip(x.source())
                    .position(|(a, b)| a != b)
                    .or_else(|| (out.len() != len).then_some(out.len().min(len)));
                assert!(
                    first_difference.is_none(),
                    "{rel}: first differing byte {first_difference:?}"
                );
                // The lossless invariant: segments tile the file exactly.
                let mut pos = 0;
                for s in x.segments() {
                    assert_eq!(
                        s.span.start, pos,
                        "{rel}: segments don't tile at byte {pos}"
                    );
                    pos = s.span.end;
                }
                assert_eq!(pos, len, "{rel}: segments stop short");

                let enc = match x.encoding {
                    Encoding::Utf8 { bom: true } => "utf-8 with BOM",
                    Encoding::Utf8 { bom: false } => "utf-8",
                    Encoding::Windows1252 => "windows-1252",
                };
                *summary.entry(format!("encoding: {enc}")).or_default() += 1;
                let eol = match x.line_ending {
                    LineEnding::CrLf => "CRLF",
                    LineEnding::Lf => "LF",
                };
                *summary.entry(format!("line ending: {eol}")).or_default() += 1;
                let version = x.header.version().unwrap_or("?").to_owned();
                *summary.entry(format!("version: {version}")).or_default() += 1;
                tables += x.tables().len();
                rows += x.tables().iter().map(|t| t.rows.len()).sum::<usize>();
                for d in &x.diagnostics {
                    let kind = format!("{:?}", d.kind);
                    *diagnostics.entry(kind.clone()).or_default() += 1;
                    *noisy
                        .entry(rel.clone())
                        .or_default()
                        .entry(kind)
                        .or_default() += 1;
                }
            }
        }
    }

    println!("corpus: {} files under {}", files.len(), root.display());
    println!(
        "parsed: {}, rejected: {}",
        files.len() - errors.values().map(Vec::len).sum::<usize>(),
        errors.values().map(Vec::len).sum::<usize>()
    );
    println!("tables: {tables}, rows: {rows}");
    for (k, v) in &summary {
        println!("  {k}: {v}");
    }
    println!("diagnostics by kind:");
    for (k, v) in &diagnostics {
        println!("  {k}: {v}");
    }
    println!("files with diagnostics:");
    for (path, kinds) in &noisy {
        println!("  {path}: {kinds:?}");
    }
    for (e, paths) in &errors {
        println!("rejected ({e}):");
        for p in paths {
            println!("  {p}");
        }
    }
}

#[test]
fn corpus_core_registry_covers_columns_and_enum_codes() {
    let Some(root) = std::env::var_os("TOTALFLOAT_CORPUS") else {
        eprintln!("TOTALFLOAT_CORPUS is not set; skipping registry corpus test");
        return;
    };
    let mut files = Vec::new();
    xer_files(Path::new(&root), &mut files);
    files.sort();
    assert_ne!(files.len(), 0, "no XER files under TOTALFLOAT_CORPUS");
    let mut unknown_core_fields = std::collections::BTreeSet::new();
    let mut unknown_enum_codes: BTreeMap<String, usize> = BTreeMap::new();
    let mut seen_core_fields = std::collections::BTreeSet::new();

    for file in files {
        let Ok(xer) = raw::read(std::fs::read(file).unwrap()) else {
            continue;
        };
        for table in xer
            .tables()
            .iter()
            .filter(|t| registry::table(&t.name).is_some())
        {
            for (index, field) in table.fields.iter().enumerate() {
                let key = format!("{}.{}", table.name, field);
                seen_core_fields.insert(key.clone());
                let Some(spec) = registry::field(&table.name, field) else {
                    unknown_core_fields.insert(key);
                    continue;
                };
                if let FieldKind::Enum(codes) = spec.kind {
                    for row in &table.rows {
                        if let Some(value) = row.get(index).filter(|v| !v.is_empty())
                            && !codes.contains(&value)
                        {
                            *unknown_enum_codes.entry(key.clone()).or_default() += 1;
                        }
                    }
                }
            }
        }
    }
    println!(
        "core fields: {}, unknown: {}",
        seen_core_fields.len(),
        unknown_core_fields.len()
    );
    println!("unknown core fields: {unknown_core_fields:?}");
    println!("unknown enum code counts (no values): {unknown_enum_codes:?}");
    // Registry coverage is asserted here; readers and inventory remain permissive.
    assert!(
        unknown_core_fields.is_empty(),
        "registry needs new column definitions"
    );
    assert!(
        unknown_enum_codes.is_empty(),
        "registry needs new enum codes"
    );
}

/// Codes the registry knows that no corpus file holds in their field: read
/// from MPXJ's code lists as format facts (docs/rules/sources.md#mpxj), never
/// copied. As table, field and code.
const CODES_FROM_MPXJ: [(&str, &str, &str); 5] = [
    ("TASK", "duration_type", "DT_FixedRate"),
    ("TASK", "cstr_type", "CS_MANDSTART"),
    ("ACTVTYPE", "actv_code_type_scope", "AS_EPS"),
    ("SCHEDOPTIONS", "sched_float_type", "FT_Min"),
    ("SCHEDOPTIONS", "sched_float_type", "FT_SS"),
];

/// Every table, field and code the registry knows is backed by a corpus file
/// in `$TOTALFLOAT_CORPUS/files`, or for a code, by MPXJ's code lists
/// (CONTRIBUTING.md, Calculation rules and their sources: `format-fact`). A
/// code is backed in its own field, or in a field the registry gives the same
/// code list, such as a project's default task type and a task's type. Prints
/// names only, never content.
#[test]
fn corpus_backs_every_registered_table_field_and_code() {
    let Some(root) = std::env::var_os("TOTALFLOAT_CORPUS") else {
        eprintln!("TOTALFLOAT_CORPUS is not set; skipping registry provenance test");
        return;
    };
    let mut files = Vec::new();
    xer_files(&Path::new(&root).join("files"), &mut files);
    assert_ne!(files.len(), 0, "no XER files under TOTALFLOAT_CORPUS/files");
    let mut fields = std::collections::BTreeSet::new();
    let mut codes = std::collections::BTreeSet::new();
    for file in files {
        let Ok(xer) = raw::read(std::fs::read(file).unwrap()) else {
            continue;
        };
        for table in xer.tables() {
            for (index, field) in table.fields.iter().enumerate() {
                fields.insert((table.name.clone(), field.clone()));
                if let Some(FieldKind::Enum(_)) =
                    registry::field(&table.name, field).map(|spec| spec.kind)
                {
                    for row in &table.rows {
                        if let Some(value) = row.get(index).filter(|v| !v.is_empty()) {
                            codes.insert((table.name.clone(), field.clone(), value.to_owned()));
                        }
                    }
                }
            }
        }
    }
    // The codes backed for each code list, from every field that uses it.
    let mut backed: std::collections::BTreeMap<Vec<&str>, std::collections::BTreeSet<&str>> =
        std::collections::BTreeMap::new();
    for table in registry::tables() {
        for field in table.fields {
            if let FieldKind::Enum(known) = field.kind {
                for code in known {
                    let seen = codes.contains(&(
                        table.name.to_owned(),
                        field.name.to_owned(),
                        (*code).to_owned(),
                    ));
                    if seen || CODES_FROM_MPXJ.contains(&(table.name, field.name, *code)) {
                        backed.entry(known.to_vec()).or_default().insert(code);
                    }
                }
            }
        }
    }
    let mut unbacked = Vec::new();
    for table in registry::tables() {
        for field in table.fields {
            if !fields.contains(&(table.name.to_owned(), field.name.to_owned())) {
                unbacked.push(format!("{}.{}", table.name, field.name));
            }
            if let FieldKind::Enum(known) = field.kind {
                for code in known {
                    if !backed.get(known).is_some_and(|b| b.contains(code)) {
                        unbacked.push(format!("{}.{} code {code}", table.name, field.name));
                    }
                }
            }
        }
    }
    assert!(
        unbacked.is_empty(),
        "registered but in no corpus file and no cited source: {unbacked:?}"
    );
}

#[test]
fn corpus_typed_cells_reencode_exactly_with_no_fatal_errors() {
    use totalfloat_xer::typed::{DiagnosticKind, Severity, TypedXer};
    let Some(root) = std::env::var_os("TOTALFLOAT_CORPUS") else {
        eprintln!("TOTALFLOAT_CORPUS is not set; skipping typed corpus test");
        return;
    };
    let mut files = Vec::new();
    xer_files(Path::new(&root), &mut files);
    files.sort();
    assert_ne!(files.len(), 0, "no XER files under TOTALFLOAT_CORPUS");
    let mut counts: BTreeMap<DiagnosticKind, usize> = BTreeMap::new();
    let (mut parsed, mut rejected, mut cells, mut rows) = (0usize, 0usize, 0usize, 0usize);
    for path in files {
        let Ok(raw) = raw::read(std::fs::read(&path).unwrap()) else {
            rejected += 1;
            continue;
        };
        parsed += 1;
        let typed = TypedXer::decode(&raw);
        assert!(
            typed
                .diagnostics
                .iter()
                .all(|d| d.severity != Severity::Error)
        );
        for (kind, count) in &typed.diagnostic_counts {
            *counts.entry(*kind).or_default() += count;
        }
        for table in &typed.tables {
            let raw_table = &raw.tables()[table.source_index()];
            let mut columns = BTreeMap::new();
            for (index, name) in raw_table.fields.iter().enumerate() {
                columns.entry(name.as_str()).or_insert(index);
            }
            table.visit_rows(&mut |row| {
                rows += 1;
                let source = row.meta().source;
                assert_eq!(source.table, table.source_index());
                let raw_row = &raw_table.rows[source.row];
                row.visit_fields(&mut |field, encoded| {
                    let expected = columns
                        .get(field)
                        .and_then(|&index| raw_row.get(index))
                        .unwrap_or("");
                    assert!(
                        encoded == expected,
                        "{}: table {}, row {}, field {field}: cell re-encoding differs",
                        path.display(),
                        source.table,
                        source.row
                    );
                    if columns.contains_key(field) {
                        cells += 1;
                    }
                });
            });
        }
    }
    println!(
        "typed corpus: parsed={parsed} rejected_non_xer={rejected} rows={rows} cells_checked={cells}"
    );
    println!("typed warnings by kind (no values): {counts:?}");
}

#[test]
fn corpus_calendars_decode_and_keep_original_text() {
    use totalfloat_xer::{
        calendar_data::{DiagnosticKind, decode_calendars},
        typed::TypedXer,
    };
    let Some(root) = std::env::var_os("TOTALFLOAT_CORPUS") else {
        eprintln!("TOTALFLOAT_CORPUS is not set; skipping calendar corpus test");
        return;
    };
    let mut files = Vec::new();
    xer_files(Path::new(&root), &mut files);
    files.sort();
    assert_ne!(files.len(), 0);
    let mut counts = BTreeMap::new();
    let (mut rows, mut resolved, mut consistent, mut mismatch) = (0usize, 0usize, 0usize, 0usize);
    let mut holidays: BTreeMap<i32, (usize, usize)> = BTreeMap::new();
    for path in files {
        let Ok(raw) = raw::read(std::fs::read(&path).unwrap()) else {
            continue;
        };
        let typed = TypedXer::decode(&raw);
        let calendars = decode_calendars(&typed);
        let expected_rows: usize = raw
            .tables()
            .iter()
            .filter(|t| t.name == "CALENDAR")
            .map(|t| t.rows.len())
            .sum();
        assert_eq!(calendars.entries.len(), expected_rows);
        for warning in &calendars.diagnostics {
            *counts.entry(warning.kind).or_insert(0usize) += 1;
        }
        for entry in &calendars.entries {
            rows += 1;
            let table = &raw.tables()[entry.source.table];
            let original = table
                .field_index("clndr_data")
                .and_then(|index| table.rows[entry.source.row].get(index))
                .unwrap_or("");
            assert!(
                entry.data.encode() == original,
                "calendar re-encoding differs in {}",
                path.display()
            );
            for date in entry.data.exceptions().keys() {
                for offset in -2..=2 {
                    let Some(shifted) = date.checked_add_days(offset) else {
                        continue;
                    };
                    let (_, month, day) = shifted.ymd();
                    let count = holidays.entry(offset).or_default();
                    if (month, day) == (1, 1) {
                        count.0 += 1;
                    }
                    if (month, day) == (12, 25) {
                        count.1 += 1;
                    }
                }
            }
            if let Some(calendar) = &entry.calendar {
                resolved += 1;
                let weekly: i64 = calendar
                    .work_week
                    .0
                    .iter()
                    .flatten()
                    .map(|i| i.duration().get())
                    .sum();
                if let Some(stored) = calendar.hours_per_period.week {
                    if stored.get() == weekly {
                        consistent += 1;
                    } else {
                        mismatch += 1;
                    }
                }
            }
        }
    }
    println!(
        "calendar corpus: rows={rows} resolved={resolved} consistent_week_hours={consistent} mismatched_week_hours={mismatch}"
    );
    println!("calendar warning counts: {counts:?}");
    println!("TF-011 holiday counts at candidate epoch offsets (Jan 1, Dec 25): {holidays:?}");
    assert!(resolved > 0 && consistent > 0);
    if let Some(&(jan, dec)) = holidays.get(&0) {
        for (offset, &(other_jan, other_dec)) in &holidays {
            if *offset != 0 {
                assert!(
                    jan > other_jan && dec > other_dec,
                    "TF-011 epoch requires investigation"
                );
            }
        }
    }
    assert_eq!(
        counts
            .get(&DiagnosticKind::WeeklyHoursMismatch)
            .copied()
            .unwrap_or(0),
        mismatch
    );
}

/// Tables whose every row must be represented or explicitly omitted.
const ACCOUNTED_TABLES: [&str; 3] = ["PROJECT", "TASK", "TASKPRED"];

/// Strict conversion's contract (maintainer decision): every raw
/// `PROJECT`, `TASK` and `TASKPRED` row is either represented in a schedule or
/// explicitly listed in `Diagnostics::omitted`. Unrepresentable values are
/// never rounded or defaulted to make counts match.
#[test]
#[allow(
    clippy::too_many_lines,
    reason = "one corpus pass that gathers and prints every aggregate it asserts on"
)]
fn corpus_model_accounts_for_every_raw_row() {
    use std::collections::BTreeSet;
    use totalfloat_xer::{to_model, typed::RowRef};
    let Some(root) = std::env::var_os("TOTALFLOAT_CORPUS") else {
        eprintln!("TOTALFLOAT_CORPUS is not set; skipping model mapping corpus test");
        return;
    };
    let mut files = Vec::new();
    xer_files(Path::new(&root), &mut files);
    files.sort();
    assert_ne!(files.len(), 0);
    // table name -> (raw rows, represented, omitted)
    let mut totals: BTreeMap<&str, (usize, usize, usize)> = BTreeMap::new();
    let mut nonwhole = BTreeMap::new();
    let mut outside_range = BTreeMap::new();
    let mut warnings = BTreeMap::new();
    let mut skipped_fields = BTreeMap::new();
    let mut violations = Vec::new();
    let mut relationship_instances = 0usize;
    let mut integrity: BTreeMap<String, usize> = BTreeMap::new();
    for (file_index, path) in files.iter().enumerate() {
        let Ok(raw) = raw::read(std::fs::read(path).unwrap()) else {
            continue;
        };
        let (schedules, report) = to_model(&raw);
        let mut represented: BTreeSet<RowRef> = BTreeSet::new();
        for schedule in &schedules {
            let sources = std::iter::once(schedule.project.source)
                .chain(schedule.activities.values().map(|a| a.source))
                .chain(schedule.relationships.values().map(|r| r.source));
            represented.extend(sources.flatten().filter_map(RowRef::from_source_ref));
            relationship_instances += schedule.relationships.len();
            for d in schedule.validate() {
                *integrity
                    .entry(format!("{}: {}", d.code, kind_of(&d.entity)))
                    .or_insert(0usize) += 1;
            }
        }
        for (table_index, table) in raw.tables().iter().enumerate() {
            let Some(name) = ACCOUNTED_TABLES.iter().copied().find(|n| *n == table.name) else {
                continue;
            };
            let total = totals.entry(name).or_default();
            for row in 0..table.rows.len() {
                let at = RowRef {
                    table: table_index,
                    row,
                };
                let mapped = represented.contains(&at);
                let omitted = report.omitted.contains(&at);
                total.0 += 1;
                total.1 += usize::from(mapped);
                total.2 += usize::from(omitted && !mapped);
                if !mapped && !omitted {
                    violations.push(format!(
                        "file {file_index}: {name} row {row} silently dropped"
                    ));
                }
                // A relationship can legitimately map into one project and be
                // a duplicate in another; projects and activities cannot.
                if mapped && omitted && name != "TASKPRED" {
                    violations.push(format!(
                        "file {file_index}: {name} row {row} both mapped and omitted"
                    ));
                }
            }
        }
        for (field, count) in report.skipped_fields {
            *skipped_fields.entry(field).or_insert(0usize) += count;
        }
        for (field, count) in report.out_of_range_hour_fields {
            *outside_range.entry(field).or_insert(0usize) += count;
        }
        for (field, count) in report.non_whole_minute_hour_fields {
            *nonwhole.entry(field).or_insert(0usize) += count;
        }
        for (kind, count) in report.counts {
            *warnings.entry(kind).or_insert(0usize) += count;
        }
    }
    for (name, (raw_rows, mapped, omitted)) in &totals {
        println!("model {name}: raw={raw_rows} represented={mapped} omitted={omitted}");
    }
    println!("relationship instances across schedules: {relationship_instances}");
    println!("Schedule::validate() findings: {integrity:?}");
    println!("model warning counts: {warnings:?}");
    println!("required field omissions: {skipped_fields:?}");
    println!("non-whole-minute hour fields: {nonwhole:?}");
    println!("whole-minute out-of-range fields: {outside_range:?}");
    // Relationships and assignments never dangle; whatever validate() still
    // finds is in the import report.
    assert!(
        !integrity
            .keys()
            .any(|k| k.ends_with("relationship") || k.ends_with("assignment")),
        "dangling relationships or assignments: {integrity:?}"
    );
    assert_eq!(
        integrity.values().sum::<usize>(),
        warnings
            .get(&totalfloat_xer::mapping::DiagnosticKind::Integrity)
            .copied()
            .unwrap_or(0),
        "every remaining integrity finding is reported"
    );
    assert!(
        violations.is_empty(),
        "{} raw rows are unaccounted for: {:?}",
        violations.len(),
        &violations[..violations.len().min(20)]
    );
}

fn kind_of(entity: &totalfloat_model::EntityRef) -> &'static str {
    use totalfloat_model::EntityRef as E;
    match entity {
        E::Project(_) => "project",
        E::Wbs(_) => "wbs",
        E::Activity(_) => "activity",
        E::Relationship(_) => "relationship",
        E::Calendar(_) => "calendar",
        E::Resource(_) => "resource",
        E::Role(_) => "role",
        E::Assignment(_) => "assignment",
        E::CostAccount(_) => "cost account",
        E::Expense(_) => "expense",
        E::ActivityCodeType(_) => "code type",
        E::ActivityCode(_) => "code",
    }
}

/// Patching an unmodified schedule reproduces every corpus file byte for
/// byte, and changing one activity's original duration changes exactly one
/// cell.
#[test]
fn corpus_patch_writer_is_minimal() {
    use totalfloat_xer::rawdiff::{changed_cells, diff_lines};
    use totalfloat_xer::{patch, to_model};
    let Some(root) = std::env::var_os("TOTALFLOAT_CORPUS") else {
        eprintln!("TOTALFLOAT_CORPUS is not set; skipping patch writer corpus test");
        return;
    };
    let mut files = Vec::new();
    xer_files(Path::new(&root), &mut files);
    files.sort();
    let (mut identical, mut one_cell, mut schedules_seen) = (0usize, 0usize, 0usize);
    let mut failures = Vec::new();
    for (file_index, path) in files.iter().enumerate() {
        let Ok(raw) = raw::read(std::fs::read(path).unwrap()) else {
            continue;
        };
        for schedule in to_model(&raw).0 {
            schedules_seen += 1;
            let mut out = Vec::new();
            match patch(&raw, &schedule) {
                Ok(p) => p.write(&mut out).unwrap(),
                Err(e) => {
                    failures.push(format!("file {file_index}: unmodified patch failed: {e}"));
                    continue;
                }
            }
            if out == raw.source() {
                identical += 1;
            } else {
                let offset = out
                    .iter()
                    .zip(raw.source())
                    .take_while(|(a, b)| a == b)
                    .count();
                failures.push(format!("file {file_index}: first differing byte {offset}"));
            }
            // One edit: lengthen the first activity by an hour.
            let mut edited = schedule.clone();
            let Some(activity) = edited.activities.values_mut().next() else {
                continue;
            };
            activity.original_duration =
                activity.original_duration + totalfloat_model::Minutes::new(60);
            let mut out = Vec::new();
            match patch(&raw, &edited) {
                Ok(p) => p.write(&mut out).unwrap(),
                Err(e) => {
                    failures.push(format!("file {file_index}: one-cell patch failed: {e}"));
                    continue;
                }
            }
            let d = diff_lines(raw.source(), &out);
            if d.removed.len() == 1
                && d.added.len() == 1
                && changed_cells(&d.removed[0].1, &d.added[0].1).len() == 1
            {
                one_cell += 1;
            } else {
                failures.push(format!(
                    "file {file_index}: duration edit changed {} lines out, {} in",
                    d.removed.len(),
                    d.added.len()
                ));
            }
        }
    }
    println!(
        "patch writer: {schedules_seen} schedules; {identical} byte-identical when unmodified; {one_cell} one-cell edits"
    );
    assert!(failures.is_empty(), "{failures:#?}");
}

/// Every role, role rate, resource role and assignment role a file
/// holds is read into the model, and a role added and assigned by role alone
/// writes back as two new rows and nothing else. Reports aggregates only.
#[test]
fn corpus_roles_read_and_round_trip() {
    use std::collections::BTreeSet;
    use totalfloat_xer::to_model;
    let Some(root) = std::env::var_os("TOTALFLOAT_CORPUS") else {
        eprintln!("TOTALFLOAT_CORPUS is not set; skipping roles corpus test");
        return;
    };
    let mut files = Vec::new();
    xer_files(Path::new(&root), &mut files);
    files.sort();
    let ids = |raw: &raw::RawXer, table: &str, field: &str| -> BTreeSet<u64> {
        raw.tables()
            .iter()
            .filter(|t| t.name == table)
            .filter_map(|t| t.field_index(field).map(|c| (t, c)))
            .flat_map(|(t, c)| t.rows.iter().filter_map(move |r| r.get(c)?.parse().ok()))
            .collect()
    };
    let (mut with_roles, mut roles_read, mut rates_read, mut filled_read, mut assigned) =
        (0, 0, 0, 0, 0);
    let mut failures = Vec::new();
    for (file_index, path) in files.iter().enumerate() {
        let Ok(raw) = raw::read(std::fs::read(path).unwrap()) else {
            continue;
        };
        let roles = ids(&raw, "ROLES", "role_id");
        if roles.is_empty() {
            continue;
        }
        with_roles += 1;
        let schedules = to_model(&raw).0;
        let Some(first) = schedules.first() else {
            continue;
        };
        let read: BTreeSet<u64> = first.roles.keys().map(|r| r.0).collect();
        if read != roles {
            failures.push(format!(
                "file {file_index}: roles read differ from ROLES rows"
            ));
        }
        roles_read += read.len();
        let rates: BTreeSet<u64> = first
            .roles
            .values()
            .flat_map(|r| r.rates.iter().map(|rate| rate.id.0))
            .collect();
        if rates != ids(&raw, "ROLERATE", "role_rate_id") {
            failures.push(format!(
                "file {file_index}: role rates read differ from ROLERATE rows"
            ));
        }
        rates_read += rates.len();
        // A resource no project uses still lands in every project, so the
        // resource roles of all of them together cover RSRCROLE.
        let filled: BTreeSet<u64> = schedules
            .iter()
            .flat_map(|s| s.resources.values())
            .flat_map(|r| r.roles.iter().map(|f| f.id.0))
            .collect();
        if filled != ids(&raw, "RSRCROLE", "rsrc_role_id") {
            failures.push(format!(
                "file {file_index}: resource roles differ from RSRCROLE rows"
            ));
        }
        filled_read += filled.len();
        assigned += schedules
            .iter()
            .flat_map(|s| s.assignments.values())
            .filter(|a| a.role.is_some())
            .count();

        if let Some(failure) = add_a_role(&raw, first, &roles) {
            failures.push(format!("file {file_index}: {failure}"));
        }
    }
    eprintln!(
        "roles: {with_roles} files; {roles_read} roles, {rates_read} rates, \
         {filled_read} resource roles, {assigned} assignments with a role"
    );
    assert!(failures.is_empty(), "{failures:#?}");
}

/// Adds a role to `first` and assigns it alone to the first activity that can
/// take one, and says what went wrong, if anything: the file must come back
/// with exactly two rows inserted, and the assignment must read back.
fn add_a_role(
    raw: &raw::RawXer,
    first: &totalfloat_model::Schedule,
    roles: &std::collections::BTreeSet<u64>,
) -> Option<String> {
    use totalfloat_model as m;
    use totalfloat_xer::{patch, to_model};
    let mut edited = first.clone();
    let activity = edited
        .activities
        .values()
        .find(|a| a.status != m::ActivityStatus::Completed && !a.activity_type.is_milestone())?
        .id;
    let role = m::RoleId(roles.last().copied().unwrap_or(0) + 1);
    let next = raw
        .tables()
        .iter()
        .filter(|t| t.name == "TASKRSRC")
        .filter_map(|t| t.field_index("taskrsrc_id").map(|c| (t, c)))
        .flat_map(|(t, c)| {
            t.rows
                .iter()
                .filter_map(move |r| r.get(c)?.parse::<u64>().ok())
        })
        .max()
        .unwrap_or(0)
        + 1;
    edited.roles.insert(
        role,
        m::Role {
            id: role,
            short_name: "TF-ROLE".to_owned(),
            name: "Added by the corpus test".to_owned(),
            parent: None,
            rates: Vec::new(),
            default_cost_follows_units: true,
            source: None,
        },
    );
    edited.assignments.insert(
        m::AssignmentId(next),
        m::Assignment {
            id: m::AssignmentId(next),
            activity,
            resource: None,
            role: Some(role),
            cost_account: None,
            project: edited.project.id,
            curve: None,
            typed_spread: None,
            amounts: m::Amounts::default(),
            udfs: BTreeMap::new(),
            source: None,
        },
    );
    let mut out = Vec::new();
    match patch(raw, &edited) {
        Ok(p) => {
            if let Err(e) = p.write(&mut out) {
                return Some(format!("writing the role patch failed: {e}"));
            }
        }
        Err(e) => return Some(format!("role patch failed: {e}")),
    }
    // Only insertions: every line of the file, in order, with the new rows
    // between them. (The rows are far apart, too far for `diff_lines`.)
    let (old, new): (Vec<&[u8]>, Vec<&[u8]>) = (
        raw.source().split(|b| *b == b'\n').collect(),
        out.split(|b| *b == b'\n').collect(),
    );
    let mut kept = 0;
    let mut inserted = 0;
    for line in &new {
        if old.get(kept) == Some(line) {
            kept += 1;
        } else {
            inserted += 1;
        }
    }
    if kept != old.len() || inserted != 2 {
        return Some(format!(
            "adding a role kept {kept} of {} lines and inserted {inserted}",
            old.len()
        ));
    }
    let back = match raw::read(out) {
        Ok(read) => to_model(&read).0,
        Err(e) => return Some(format!("the role patch doesn't read back: {e}")),
    };
    let held = back
        .iter()
        .find(|s| s.project.id == edited.project.id)
        .and_then(|s| s.assignments.get(&m::AssignmentId(next)));
    (held.map(|a| (a.resource, a.role)) != Some((None, Some(role))))
        .then(|| "the role assignment didn't read back".to_owned())
}

/// Every expense, cost account and expense category a file holds is
/// read into the model; changing an expense's remaining cost changes one
/// line, and an added expense writes back as one new row and nothing else.
/// Reports aggregates only.
#[test]
fn corpus_expenses_read_and_round_trip() {
    use std::collections::BTreeSet;
    use totalfloat_xer::to_model;
    let Some(root) = std::env::var_os("TOTALFLOAT_CORPUS") else {
        eprintln!("TOTALFLOAT_CORPUS is not set; skipping expenses corpus test");
        return;
    };
    let mut files = Vec::new();
    xer_files(Path::new(&root), &mut files);
    files.sort();
    let ids = |raw: &raw::RawXer, table: &str, field: &str| -> BTreeSet<u64> {
        raw.tables()
            .iter()
            .filter(|t| t.name == table)
            .filter_map(|t| t.field_index(field).map(|c| (t, c)))
            .flat_map(|(t, c)| t.rows.iter().filter_map(move |r| r.get(c)?.parse().ok()))
            .collect()
    };
    let (mut with_expenses, mut expenses_read, mut accounts_read, mut categories_read) =
        (0, 0, 0, 0);
    let mut accruals = BTreeMap::new();
    let mut failures = Vec::new();
    for (file_index, path) in files.iter().enumerate() {
        let Ok(raw) = raw::read(std::fs::read(path).unwrap()) else {
            continue;
        };
        let expenses = ids(&raw, "PROJCOST", "cost_item_id");
        let accounts = ids(&raw, "ACCOUNT", "acct_id");
        let categories = ids(&raw, "COSTTYPE", "cost_type_id");
        if expenses.is_empty() && accounts.is_empty() && categories.is_empty() {
            continue;
        }
        let schedules = to_model(&raw).0;
        let Some(first) = schedules.first() else {
            continue;
        };
        let read: BTreeSet<u64> = schedules
            .iter()
            .flat_map(|s| s.expenses.keys().map(|x| x.0))
            .collect();
        if read != expenses {
            failures.push(format!(
                "file {file_index}: expenses read differ from PROJCOST rows"
            ));
        }
        let held: BTreeSet<u64> = first.cost_accounts.keys().map(|a| a.0).collect();
        if held != accounts {
            failures.push(format!(
                "file {file_index}: cost accounts read differ from ACCOUNT rows"
            ));
        }
        let held: BTreeSet<u64> = first.expense_categories.keys().map(|c| c.0).collect();
        if held != categories {
            failures.push(format!(
                "file {file_index}: categories read differ from COSTTYPE rows"
            ));
        }
        accounts_read += accounts.len();
        categories_read += categories.len();
        if expenses.is_empty() {
            continue;
        }
        with_expenses += 1;
        expenses_read += read.len();
        for x in schedules.iter().flat_map(|s| s.expenses.values()) {
            *accruals.entry(format!("{:?}", x.accrual)).or_insert(0) += 1;
        }
        if let Some(failure) = edit_expenses(&raw, &schedules, &expenses) {
            failures.push(format!("file {file_index}: {failure}"));
        }
    }
    eprintln!(
        "expenses: {with_expenses} files; {expenses_read} expenses, {accounts_read} cost \
         accounts, {categories_read} categories; accruals {accruals:?}"
    );
    assert!(failures.is_empty(), "{failures:#?}");
}

/// Changes one expense's remaining cost, then adds an expense beside it, and
/// says what went wrong, if anything.
fn edit_expenses(
    raw: &raw::RawXer,
    schedules: &[totalfloat_model::Schedule],
    expenses: &std::collections::BTreeSet<u64>,
) -> Option<String> {
    use totalfloat_model as m;
    use totalfloat_xer::rawdiff::{changed_cells, diff_lines};
    use totalfloat_xer::{patch, to_model};
    let base = schedules.iter().find(|s| !s.expenses.is_empty())?;
    let mut edited = base.clone();
    let changed = edited.expenses.values_mut().next()?;
    let id = changed.id;
    changed.remaining_cost = Some(changed.remaining_cost.unwrap_or(0.0) + 1.0);
    let mut out = Vec::new();
    match patch(raw, &edited) {
        Ok(p) => p.write(&mut out).ok()?,
        Err(e) => return Some(format!("expense cost patch failed: {e}")),
    }
    let d = diff_lines(raw.source(), &out);
    if (d.removed.len(), d.added.len()) != (1, 1)
        || changed_cells(&d.removed[0].1, &d.added[0].1).len() != 1
    {
        return Some(format!(
            "changing an expense's remaining cost changed {} lines for {}",
            d.removed.len(),
            d.added.len()
        ));
    }
    let mut edited = base.clone();
    let mut added = edited.expenses[&id].clone();
    added.id = m::ExpenseId(expenses.last().copied().unwrap_or(0) + 1);
    "Added by the corpus test".clone_into(&mut added.name);
    added.source = None;
    edited.expenses.insert(added.id, added.clone());
    let mut out = Vec::new();
    match patch(raw, &edited) {
        Ok(p) => p.write(&mut out).ok()?,
        Err(e) => return Some(format!("new expense patch failed: {e}")),
    }
    let (old, new): (Vec<&[u8]>, Vec<&[u8]>) = (
        raw.source().split(|b| *b == b'\n').collect(),
        out.split(|b| *b == b'\n').collect(),
    );
    let mut kept = 0;
    let mut inserted = 0;
    for line in &new {
        if old.get(kept) == Some(line) {
            kept += 1;
        } else {
            inserted += 1;
        }
    }
    if kept != old.len() || inserted != 1 {
        return Some(format!(
            "adding an expense kept {kept} of {} lines and inserted {inserted}",
            old.len()
        ));
    }
    let back = to_model(&raw::read(out).ok()?).0;
    let mut read = back
        .iter()
        .find(|s| s.project.id == base.project.id)
        .and_then(|s| s.expenses.get(&added.id))?
        .clone();
    read.source = None;
    (read != added).then(|| "the new expense didn't read back the same".to_owned())
}

/// Every `RSRCCURVDATA` row reads as a curve, and changing an
/// assignment's curve changes one cell and reads back.
#[test]
fn corpus_curves_read_and_round_trip() {
    use std::collections::BTreeSet;
    use totalfloat_xer::rawdiff::{changed_cells, diff_lines};
    use totalfloat_xer::{patch, to_model};
    let Some(root) = std::env::var_os("TOTALFLOAT_CORPUS") else {
        eprintln!("TOTALFLOAT_CORPUS is not set; skipping curves corpus test");
        return;
    };
    let mut files = Vec::new();
    xer_files(Path::new(&root), &mut files);
    files.sort();
    let (mut with_curves, mut curves_read, mut hundred, mut curved, mut edited) = (0, 0, 0, 0, 0);
    let mut failures = Vec::new();
    for (file_index, path) in files.iter().enumerate() {
        let Ok(raw) = raw::read(std::fs::read(path).unwrap()) else {
            continue;
        };
        let rows: BTreeSet<u64> = raw
            .tables()
            .iter()
            .filter(|t| t.name == "RSRCCURVDATA")
            .filter_map(|t| t.field_index("curv_id").map(|c| (t, c)))
            .flat_map(|(t, c)| t.rows.iter().filter_map(move |r| r.get(c)?.parse().ok()))
            .collect();
        if rows.is_empty() {
            continue;
        }
        with_curves += 1;
        let schedules = to_model(&raw).0;
        let Some(first) = schedules.first() else {
            continue;
        };
        let read: BTreeSet<u64> = first.resource_curves.keys().map(|c| c.0).collect();
        if read != rows {
            failures.push(format!(
                "file {file_index}: curves read differ from RSRCCURVDATA rows"
            ));
        }
        curves_read += read.len();
        hundred += first
            .resource_curves
            .values()
            .filter(|c| (c.points.iter().sum::<f64>() - 100.0).abs() < 1e-9)
            .count();
        curved += schedules
            .iter()
            .flat_map(|s| s.assignments.values())
            .filter(|a| a.curve.is_some())
            .count();
        // Move one assignment to another curve, or off its curve.
        for schedule in &schedules {
            let Some(on) = schedule.assignments.values().find(|a| a.curve.is_some()) else {
                continue;
            };
            let other = schedule
                .resource_curves
                .keys()
                .copied()
                .find(|c| Some(*c) != on.curve);
            let mut changed = schedule.clone();
            changed.assignments.get_mut(&on.id).unwrap().curve = other;
            let out = match patch(&raw, &changed) {
                Ok(out) => out,
                Err(error) => {
                    failures.push(format!("file {file_index}: curve edit refused: {error}"));
                    break;
                }
            };
            let mut written = Vec::new();
            out.write(&mut written).unwrap();
            let d = diff_lines(raw.source(), &written);
            let one_cell = d.removed.len() == 1
                && d.added.len() == 1
                && changed_cells(&d.removed[0].1, &d.added[0].1).len() == 1;
            let back = to_model(&out)
                .0
                .into_iter()
                .find(|s| s.project.id == schedule.project.id)
                .and_then(|s| s.assignments.get(&on.id).map(|a| a.curve));
            if !one_cell || back != Some(other) {
                failures.push(format!(
                    "file {file_index}: a curve edit didn't change one cell and read back"
                ));
            }
            edited += 1;
            break;
        }
    }
    eprintln!(
        "curves: {with_curves} files; {curves_read} curves ({hundred} summing to 100), \
         {curved} assignments with a curve, {edited} curve edits patched"
    );
    assert!(failures.is_empty(), "{failures:#?}");
}

/// Every stored spread (`target_crv`, `remain_crv`, `actual_crv`)
/// decodes, the assignments that name a curve their file lacks read as typed
/// instead of being warned about, each file writes back byte for byte, and
/// changing one typed stretch changes one cell. Aggregates only.
#[test]
#[allow(clippy::too_many_lines, reason = "one corpus pass, counted as it goes")]
fn corpus_typed_spreads_read_and_round_trip() {
    use std::collections::BTreeSet;
    use totalfloat_xer::mapping::DiagnosticKind;
    use totalfloat_xer::rawdiff::{changed_cells, diff_lines};
    use totalfloat_xer::spread_data::{decode, encode};
    use totalfloat_xer::{patch, to_model};
    let Some(root) = std::env::var_os("TOTALFLOAT_CORPUS") else {
        eprintln!("TOTALFLOAT_CORPUS is not set; skipping typed spreads corpus test");
        return;
    };
    let mut files = Vec::new();
    xer_files(Path::new(&root), &mut files);
    files.sort();
    let mut stored: BTreeMap<&str, (usize, usize, usize)> = BTreeMap::new();
    let (mut with_spreads, mut marked, mut typed, mut warned, mut mismatched) = (0, 0, 0, 0, 0);
    let (mut identical, mut edited, mut suppressed) = (0, 0, 0);
    let mut failures = Vec::new();
    for (file_index, path) in files.iter().enumerate() {
        let Ok(raw) = raw::read(std::fs::read(path).unwrap()) else {
            continue;
        };
        let mut any = false;
        let mut held = BTreeSet::new();
        for table in raw.tables().iter().filter(|t| t.name == "RSRCCURVDATA") {
            if let Some(c) = table.field_index("curv_id") {
                held.extend(
                    table
                        .rows
                        .iter()
                        .filter_map(|r| r.get(c)?.parse::<u64>().ok()),
                );
            }
        }
        for table in raw.tables().iter().filter(|t| t.name == "TASKRSRC") {
            for row in &table.rows {
                for field in ["target_crv", "remain_crv", "actual_crv"] {
                    let Some(cell) = table.field_index(field).and_then(|c| row.get(c)) else {
                        continue;
                    };
                    if cell.is_empty() {
                        continue;
                    }
                    any = true;
                    let counts = stored.entry(field).or_default();
                    counts.0 += 1;
                    if let Ok(segments) = decode(cell) {
                        counts.1 += 1;
                        if encode(&segments).as_deref() == Some(cell) {
                            counts.2 += 1;
                        }
                    }
                }
                let curve = table
                    .field_index("curv_id")
                    .and_then(|c| row.get(c)?.parse::<u64>().ok());
                if curve.is_some_and(|c| !held.contains(&c)) {
                    marked += 1;
                }
            }
        }
        if !any {
            continue;
        }
        with_spreads += 1;
        let (schedules, report) = to_model(&raw);
        // Details stop at 1,000, so a warning could hide; the check below
        // that every marked assignment is typed or warned would then fail.
        suppressed += report.suppressed;
        warned += report
            .details
            .iter()
            .filter(|d| d.kind == DiagnosticKind::UnrepresentedValue && d.field == "curv_id")
            .count();
        mismatched += report
            .counts
            .get(&DiagnosticKind::InconsistentValues)
            .copied()
            .unwrap_or(0);
        typed += schedules
            .iter()
            .flat_map(|s| s.assignments.values())
            .filter(|a| a.typed_spread.is_some())
            .count();
        let mut file_identical = true;
        for schedule in &schedules {
            let mut out = Vec::new();
            match patch(&raw, schedule) {
                Ok(written) => written.write(&mut out).unwrap(),
                Err(error) => {
                    failures.push(format!(
                        "file {file_index}: unchanged patch refused: {error}"
                    ));
                    file_identical = false;
                    continue;
                }
            }
            file_identical &= out == raw.source();
        }
        if file_identical {
            identical += 1;
        } else {
            failures.push(format!("file {file_index}: not byte-identical"));
        }
        // Change one remaining stretch of the first typed assignment.
        for schedule in &schedules {
            let Some(on) = schedule.assignments.values().find(|a| {
                a.typed_spread
                    .as_ref()
                    .is_some_and(|t| !t.remaining.is_empty())
            }) else {
                continue;
            };
            let mut changed = schedule.clone();
            let spread = changed
                .assignments
                .get_mut(&on.id)
                .and_then(|a| a.typed_spread.as_mut())
                .unwrap();
            spread.remaining[0].units += 1.0;
            let expected = spread.clone();
            let out = match patch(&raw, &changed) {
                Ok(out) => out,
                Err(error) => {
                    failures.push(format!("file {file_index}: spread edit refused: {error}"));
                    break;
                }
            };
            let mut written = Vec::new();
            out.write(&mut written).unwrap();
            let d = diff_lines(raw.source(), &written);
            let one_cell = d.removed.len() == 1
                && d.added.len() == 1
                && changed_cells(&d.removed[0].1, &d.added[0].1).len() == 1;
            let back = to_model(&out)
                .0
                .into_iter()
                .find(|s| s.project.id == schedule.project.id)
                .and_then(|s| s.assignments.get(&on.id)?.typed_spread.clone());
            if !one_cell || back.as_ref() != Some(&expected) {
                failures.push(format!(
                    "file {file_index}: a spread edit didn't change one cell and read back"
                ));
            }
            edited += 1;
            break;
        }
    }
    for (field, (count, decoded, spelled)) in &stored {
        eprintln!(
            "typed spreads: {field}: {count} stored, {decoded} decode, {spelled} re-encode identically"
        );
    }
    eprintln!(
        "typed spreads: {with_spreads} files; {marked} assignments name a curve their file lacks, \
         {typed} read as typed, {warned} curv_id warnings left ({suppressed} details past the limit); \
         {mismatched} parts whose units \
         differ from the assignment's; {identical} files byte-identical; {edited} spread edits patched"
    );
    for (field, (count, decoded, _)) in &stored {
        if count != decoded {
            failures.push(format!(
                "{field}: {} stored spreads don't decode",
                count - decoded
            ));
        }
    }
    if typed + warned != marked {
        failures.push(format!(
            "{marked} marked assignments, but {typed} typed and {warned} warned"
        ));
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// Every corpus file's project code types, values and assignments read,
/// an unchanged schedule writes back byte-identical, and changing a project's
/// value changes one cell. Reports aggregates only.
#[test]
fn corpus_project_codes_read_and_round_trip() {
    use std::collections::BTreeSet;
    use totalfloat_xer::{patch, to_model};
    let Some(root) = std::env::var_os("TOTALFLOAT_CORPUS") else {
        eprintln!("TOTALFLOAT_CORPUS is not set; skipping project codes corpus test");
        return;
    };
    let mut files = Vec::new();
    xer_files(Path::new(&root), &mut files);
    files.sort();
    let ids = |raw: &raw::RawXer, table: &str, field: &str| -> BTreeSet<u64> {
        raw.tables()
            .iter()
            .filter(|t| t.name == table)
            .filter_map(|t| t.field_index(field).map(|c| (t, c)))
            .flat_map(|(t, c)| t.rows.iter().filter_map(move |r| r.get(c)?.parse().ok()))
            .collect()
    };
    let (mut files_read, mut types_read, mut values_read, mut assigned, mut changed) =
        (0, 0, 0, 0, 0);
    let mut failures = Vec::new();
    for (file_index, path) in files.iter().enumerate() {
        let Ok(raw) = raw::read(std::fs::read(path).unwrap()) else {
            continue;
        };
        let types = ids(&raw, "PCATTYPE", "proj_catg_type_id");
        if types.is_empty() {
            continue;
        }
        let schedules = to_model(&raw).0;
        let Some(first) = schedules.first() else {
            continue;
        };
        files_read += 1;
        let held: BTreeSet<u64> = first.project_code_types.keys().map(|t| t.0).collect();
        if held != types {
            failures.push(format!(
                "file {file_index}: types read differ from PCATTYPE rows"
            ));
        }
        let values = ids(&raw, "PCATVAL", "proj_catg_id");
        let held: BTreeSet<u64> = first.project_codes.keys().map(|c| c.0).collect();
        if held != values {
            failures.push(format!(
                "file {file_index}: values read differ from PCATVAL rows"
            ));
        }
        types_read += types.len();
        values_read += values.len();
        for schedule in &schedules {
            assigned += schedule.project.codes.len();
            if schedule
                .validate()
                .iter()
                .any(|d| d.entity == totalfloat_model::EntityRef::Project(schedule.project.id))
            {
                failures.push(format!(
                    "file {file_index}: a project's codes don't validate"
                ));
            }
            match patch(&raw, schedule) {
                Ok(out) => {
                    let mut bytes = Vec::new();
                    out.write(&mut bytes).unwrap();
                    if bytes != raw.source() {
                        failures.push(format!(
                            "file {file_index}: unchanged project rewrote bytes"
                        ));
                    }
                }
                Err(error) => failures.push(format!("file {file_index}: {error}")),
            }
            match change_a_project_code(&raw, schedule) {
                Some(Ok(())) => changed += 1,
                Some(Err(failure)) => failures.push(format!("file {file_index}: {failure}")),
                None => {}
            }
        }
    }
    eprintln!(
        "project codes: {files_read} files, {types_read} types, {values_read} values, \
         {assigned} assignments read, {changed} changed and read back"
    );
    assert!(failures.is_empty(), "{failures:#?}");
}

/// Moves one of the project's codes to another value of its type, if it has
/// one, and reads it back. `None` if no code has another value to take.
fn change_a_project_code(
    raw: &raw::RawXer,
    schedule: &totalfloat_model::Schedule,
) -> Option<Result<(), String>> {
    let (kind, value) = schedule.project.codes.iter().find_map(|(kind, code)| {
        schedule
            .project_codes
            .values()
            .find(|c| c.code_type == *kind && c.id != *code)
            .map(|c| (*kind, c.id))
    })?;
    let mut edited = schedule.clone();
    edited.project.codes.insert(kind, value);
    Some(match totalfloat_xer::patch(raw, &edited) {
        Ok(out) => {
            let back = totalfloat_xer::to_model(&out).0;
            let again = back.iter().find(|s| s.project.id == schedule.project.id);
            if again.and_then(|s| s.project.codes.get(&kind)) == Some(&value) {
                Ok(())
            } else {
                Err("changed code didn't read back".to_owned())
            }
        }
        Err(error) => Err(error.to_string()),
    })
}

/// Every corpus file's OBS and WBS managers read, an unchanged schedule
/// writes back byte-identical, and moving a WBS node to another OBS node
/// changes one cell. Reports aggregates only.
#[test]
fn corpus_obs_read_and_round_trip() {
    use std::collections::BTreeSet;
    use totalfloat_xer::rawdiff::{changed_cells, diff_lines};
    use totalfloat_xer::{patch, to_model};
    let Some(root) = std::env::var_os("TOTALFLOAT_CORPUS") else {
        eprintln!("TOTALFLOAT_CORPUS is not set; skipping OBS corpus test");
        return;
    };
    let mut files = Vec::new();
    xer_files(Path::new(&root), &mut files);
    files.sort();
    let (mut files_read, mut nodes_read, mut managed, mut unmanaged, mut moved) = (0, 0, 0, 0, 0);
    let mut failures = Vec::new();
    for (file_index, path) in files.iter().enumerate() {
        let Ok(raw) = raw::read(std::fs::read(path).unwrap()) else {
            continue;
        };
        let rows: BTreeSet<u64> = raw
            .tables()
            .iter()
            .filter(|t| t.name == "OBS")
            .filter_map(|t| t.field_index("obs_id").map(|c| (t, c)))
            .flat_map(|(t, c)| t.rows.iter().filter_map(move |r| r.get(c)?.parse().ok()))
            .collect();
        if rows.is_empty() {
            continue;
        }
        let schedules = to_model(&raw).0;
        let Some(first) = schedules.first() else {
            continue;
        };
        files_read += 1;
        nodes_read += rows.len();
        if first.obs.keys().map(|o| o.0).collect::<BTreeSet<_>>() != rows {
            failures.push(format!("file {file_index}: OBS read differs from OBS rows"));
        }
        for schedule in &schedules {
            for node in schedule.wbs.values() {
                if node.obs.is_some() {
                    managed += 1;
                } else {
                    unmanaged += 1;
                }
            }
            let mut out = Vec::new();
            match patch(&raw, schedule) {
                Ok(xer) => xer.write(&mut out).unwrap(),
                Err(error) => {
                    failures.push(format!("file {file_index}: {error}"));
                    continue;
                }
            }
            if out != raw.source() {
                failures.push(format!(
                    "file {file_index}: unchanged project rewrote bytes"
                ));
            }
            let target = schedule.wbs.values().find_map(|node| {
                let other = schedule
                    .obs
                    .keys()
                    .find(|o| node.obs.is_some_and(|n| n != **o))?;
                Some((node.id, *other))
            });
            let Some((wbs, obs)) = target else {
                continue;
            };
            let mut edited = schedule.clone();
            edited.wbs.get_mut(&wbs).unwrap().obs = Some(obs);
            match patch(&raw, &edited) {
                Ok(xer) => {
                    let mut bytes = Vec::new();
                    xer.write(&mut bytes).unwrap();
                    let d = diff_lines(raw.source(), &bytes);
                    let one = d.removed.len() == 1
                        && d.added.len() == 1
                        && changed_cells(&d.removed[0].1, &d.added[0].1).len() == 1;
                    if one {
                        moved += 1;
                    } else {
                        failures.push(format!("file {file_index}: a moved manager changed more"));
                    }
                }
                Err(error) => failures.push(format!("file {file_index}: {error}")),
            }
        }
    }
    eprintln!(
        "OBS: {files_read} files, {nodes_read} nodes, {managed} WBS nodes managed, \
         {unmanaged} not, {moved} moved to one changed cell"
    );
    assert!(failures.is_empty(), "{failures:#?}");
}

/// Every corpus file's notebook topics and notes read, every note
/// gives some text, an unchanged schedule writes back byte-identical, and a
/// changed note changes one cell. Reports aggregates only.
#[test]
fn corpus_notebooks_read_and_round_trip() {
    use totalfloat_xer::rawdiff::{changed_cells, diff_lines};
    use totalfloat_xer::{patch, to_model};
    let Some(root) = std::env::var_os("TOTALFLOAT_CORPUS") else {
        eprintln!("TOTALFLOAT_CORPUS is not set; skipping notebooks corpus test");
        return;
    };
    let mut files = Vec::new();
    xer_files(Path::new(&root), &mut files);
    files.sort();
    let rows = |raw: &raw::RawXer, table: &str| -> usize {
        raw.tables()
            .iter()
            .filter(|t| t.name == table)
            .map(|t| t.rows.len())
            .sum()
    };
    let (mut files_read, mut topics, mut stored, mut read, mut texts, mut changed) =
        (0, 0, 0, 0, 0, 0);
    let mut failures = Vec::new();
    for (file_index, path) in files.iter().enumerate() {
        let Ok(raw) = raw::read(std::fs::read(path).unwrap()) else {
            continue;
        };
        let held = rows(&raw, "TASKMEMO") + rows(&raw, "WBSMEMO");
        if rows(&raw, "MEMOTYPE") == 0 {
            continue;
        }
        let schedules = to_model(&raw).0;
        let Some(first) = schedules.first() else {
            continue;
        };
        files_read += 1;
        topics += first.notebook_topics.len();
        stored += held;
        for schedule in &schedules {
            let notes = schedule
                .activities
                .values()
                .flat_map(|a| a.notes.values())
                .chain(schedule.wbs.values().flat_map(|w| w.notes.values()));
            for note in notes {
                read += 1;
                if !note.text().is_empty() {
                    texts += 1;
                }
            }
            let mut out = Vec::new();
            match patch(&raw, schedule) {
                Ok(xer) => xer.write(&mut out).unwrap(),
                Err(error) => {
                    failures.push(format!("file {file_index}: {error}"));
                    continue;
                }
            }
            if out != raw.source() {
                failures.push(format!(
                    "file {file_index}: unchanged project rewrote bytes"
                ));
            }
            let Some(id) = schedule
                .activities
                .values()
                .find(|a| !a.notes.is_empty())
                .map(|a| a.id)
            else {
                continue;
            };
            let mut edited = schedule.clone();
            let note = edited
                .activities
                .get_mut(&id)
                .and_then(|a| a.notes.values_mut().next())
                .unwrap();
            note.html = totalfloat_model::Note::html_of("Changed by a test");
            match patch(&raw, &edited) {
                Ok(xer) => {
                    let mut bytes = Vec::new();
                    xer.write(&mut bytes).unwrap();
                    let d = diff_lines(raw.source(), &bytes);
                    let one = d.removed.len() == 1
                        && d.added.len() == 1
                        && changed_cells(&d.removed[0].1, &d.added[0].1).len() == 1;
                    if one {
                        changed += 1;
                    } else {
                        failures.push(format!("file {file_index}: a changed note changed more"));
                    }
                }
                Err(error) => failures.push(format!("file {file_index}: {error}")),
            }
        }
    }
    eprintln!(
        "notebooks: {files_read} files, {topics} topics, {stored} note rows, {read} notes read, \
         {texts} with text, {changed} changed to one cell"
    );
    assert!(failures.is_empty(), "{failures:#?}");
}

/// Every corpus file's steps read, an unchanged schedule writes back
/// byte-identical, and a step's progress changes its own row. Reports
/// aggregates only.
#[test]
fn corpus_steps_read_and_round_trip() {
    use totalfloat_xer::rawdiff::diff_lines;
    use totalfloat_xer::{patch, to_model};
    let Some(root) = std::env::var_os("TOTALFLOAT_CORPUS") else {
        eprintln!("TOTALFLOAT_CORPUS is not set; skipping steps corpus test");
        return;
    };
    let mut files = Vec::new();
    xer_files(Path::new(&root), &mut files);
    files.sort();
    let (mut files_read, mut stored, mut read, mut measured, mut changed) = (0, 0, 0, 0, 0);
    let mut failures = Vec::new();
    for (file_index, path) in files.iter().enumerate() {
        let Ok(raw) = raw::read(std::fs::read(path).unwrap()) else {
            continue;
        };
        let rows: usize = raw
            .tables()
            .iter()
            .filter(|t| t.name == "TASKPROC")
            .map(|t| t.rows.len())
            .sum();
        if rows == 0 {
            continue;
        }
        files_read += 1;
        stored += rows;
        for schedule in &to_model(&raw).0 {
            read += schedule
                .activities
                .values()
                .map(|a| a.steps.len())
                .sum::<usize>();
            measured += usize::from(schedule.project.percent_from_steps);
            let mut out = Vec::new();
            match patch(&raw, schedule) {
                Ok(xer) => xer.write(&mut out).unwrap(),
                Err(error) => {
                    failures.push(format!("file {file_index}: {error}"));
                    continue;
                }
            }
            if out != raw.source() {
                failures.push(format!(
                    "file {file_index}: unchanged project rewrote bytes"
                ));
            }
            let Some(id) = schedule
                .activities
                .values()
                .find(|a| !a.steps.is_empty())
                .map(|a| a.id)
            else {
                continue;
            };
            let mut edited = schedule.clone();
            let step = edited
                .activities
                .get_mut(&id)
                .and_then(|a| a.steps.values_mut().next())
                .unwrap();
            step.percent = Some(40.0);
            match patch(&raw, &edited) {
                Ok(xer) => {
                    let mut bytes = Vec::new();
                    xer.write(&mut bytes).unwrap();
                    let d = diff_lines(raw.source(), &bytes);
                    if (d.removed.len(), d.added.len()) == (1, 1) {
                        changed += 1;
                    } else {
                        failures.push(format!("file {file_index}: a step change moved more rows"));
                    }
                }
                Err(error) => failures.push(format!("file {file_index}: {error}")),
            }
        }
    }
    eprintln!(
        "steps: {files_read} files, {stored} step rows, {read} read, \
         {measured} projects measure by steps, {changed} changed to one row"
    );
    assert!(failures.is_empty(), "{failures:#?}");
}

/// In every corpus file with activity UDF values, changing one value
/// changes one cell and reads back. Reports aggregates only.
#[test]
fn corpus_udf_value_edits_change_one_cell() {
    use totalfloat_xer::rawdiff::{changed_cells, diff_lines};
    use totalfloat_xer::{patch, to_model};
    let Some(root) = std::env::var_os("TOTALFLOAT_CORPUS") else {
        eprintln!("TOTALFLOAT_CORPUS is not set; skipping UDF corpus test");
        return;
    };
    let mut files = Vec::new();
    xer_files(Path::new(&root), &mut files);
    files.sort();
    let (mut tried, mut changed) = (0, 0);
    let mut failures = Vec::new();
    for (file_index, path) in files.iter().enumerate() {
        let Ok(raw) = raw::read(std::fs::read(path).unwrap()) else {
            continue;
        };
        let schedules = to_model(&raw).0;
        let Some(schedule) = schedules
            .iter()
            .find(|s| s.activities.values().any(|a| !a.udfs.is_empty()))
        else {
            continue;
        };
        let (id, kind, value) = schedule
            .activities
            .values()
            .find_map(|a| {
                a.udfs.iter().find_map(|(k, v)| match v {
                    totalfloat_model::UdfValue::Text(_) => Some((
                        a.id,
                        *k,
                        totalfloat_model::UdfValue::Text("Changed".to_owned()),
                    )),
                    totalfloat_model::UdfValue::Number(n) => {
                        Some((a.id, *k, totalfloat_model::UdfValue::Number(n + 1.0)))
                    }
                    _ => None,
                })
            })
            .unwrap_or_else(|| unreachable!());
        tried += 1;
        let mut edited = schedule.clone();
        edited
            .activities
            .get_mut(&id)
            .unwrap()
            .udfs
            .insert(kind, value.clone());
        match patch(&raw, &edited) {
            Ok(xer) => {
                let mut bytes = Vec::new();
                xer.write(&mut bytes).unwrap();
                let d = diff_lines(raw.source(), &bytes);
                let one = d.removed.len() == 1
                    && d.added.len() == 1
                    && changed_cells(&d.removed[0].1, &d.added[0].1).len() == 1;
                let back = to_model(&raw::read(bytes).unwrap()).0;
                let read = back
                    .iter()
                    .find(|s| s.project.id == schedule.project.id)
                    .and_then(|s| s.activities.get(&id))
                    .and_then(|a| a.udfs.get(&kind));
                if one && read == Some(&value) {
                    changed += 1;
                } else {
                    failures.push(format!(
                        "file {file_index}: a UDF edit changed more or read back wrong"
                    ));
                }
            }
            Err(error) => failures.push(format!("file {file_index}: {error}")),
        }
    }
    eprintln!("UDF values: {tried} files tried, {changed} changed one cell and read back");
    assert!(failures.is_empty(), "{failures:#?}");
}

/// Every assignment's cost account reads, and booking one to another
/// account changes one cell.
#[test]
fn corpus_assignment_accounts_read_and_round_trip() {
    use std::collections::BTreeSet;
    use totalfloat_model as m;
    use totalfloat_xer::rawdiff::{changed_cells, diff_lines};
    use totalfloat_xer::{patch, to_model};
    let Some(root) = std::env::var_os("TOTALFLOAT_CORPUS") else {
        eprintln!("TOTALFLOAT_CORPUS is not set; skipping assignment accounts corpus test");
        return;
    };
    let mut files = Vec::new();
    xer_files(Path::new(&root), &mut files);
    files.sort();
    let (mut with_accounts, mut booked) = (0, 0);
    let mut failures = Vec::new();
    for (file_index, path) in files.iter().enumerate() {
        let Ok(raw) = raw::read(std::fs::read(path).unwrap()) else {
            continue;
        };
        let rows: BTreeSet<u64> = raw
            .tables()
            .iter()
            .filter(|t| t.name == "TASKRSRC")
            .filter_map(|t| Some((t, t.field_index("acct_id")?, t.field_index("taskrsrc_id")?)))
            .flat_map(|(t, account, id)| {
                t.rows
                    .iter()
                    .filter(move |r| r.get(account).is_some_and(|a| !a.is_empty()))
                    .filter_map(move |r| r.get(id)?.parse().ok())
            })
            .collect();
        if rows.is_empty() {
            continue;
        }
        let schedules = to_model(&raw).0;
        let read: BTreeSet<u64> = schedules
            .iter()
            .flat_map(|s| s.assignments.values())
            .filter(|a| a.cost_account.is_some())
            .map(|a| a.id.0)
            .collect();
        if read != rows {
            failures.push(format!(
                "file {file_index}: {} assignments booked to accounts, {} read",
                rows.len(),
                read.len()
            ));
        }
        with_accounts += 1;
        booked += read.len();
        // Rebook one assignment to another account, or to none.
        let Some((base, id, other)) = schedules.iter().find_map(|s| {
            let a = s.assignments.values().find(|a| a.cost_account.is_some())?;
            let other = s
                .cost_accounts
                .keys()
                .copied()
                .find(|c| Some(*c) != a.cost_account);
            Some((s, a.id, other))
        }) else {
            continue;
        };
        let mut edited = base.clone();
        if let Some(a) = edited.assignments.get_mut(&id) {
            a.cost_account = other;
        }
        let mut out = Vec::new();
        match patch(&raw, &edited) {
            Ok(p) => p.write(&mut out).unwrap(),
            Err(e) => {
                failures.push(format!("file {file_index}: rebooking failed: {e}"));
                continue;
            }
        }
        let d = diff_lines(raw.source(), &out);
        if (d.removed.len(), d.added.len()) != (1, 1)
            || changed_cells(&d.removed[0].1, &d.added[0].1).len() != 1
        {
            failures.push(format!(
                "file {file_index}: rebooking changed {} lines for {}",
                d.removed.len(),
                d.added.len()
            ));
        }
        let back = to_model(&raw::read(out).unwrap()).0;
        let held = back
            .iter()
            .find(|s| s.project.id == base.project.id)
            .and_then(|s| s.assignments.get(&m::AssignmentId(id.0)))
            .map(|a| a.cost_account);
        if held != Some(other) {
            failures.push(format!("file {file_index}: the rebooking didn't read back"));
        }
    }
    eprintln!("assignment accounts: {with_accounts} files; {booked} assignments booked");
    assert!(failures.is_empty(), "{failures:#?}");
}
