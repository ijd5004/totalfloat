use super::*;
use crate::raw;
use proptest::prelude::*;
use totalfloat_model::DateTime;

#[test]
fn scalar_kinds_preserve_tokens_and_parse_real_values() {
    for kind in [FieldKind::Id, FieldKind::ForeignId("TASK"), FieldKind::Int] {
        let cell = Cell::<i64>::decode(kind, "+00042");
        assert_eq!(cell.value(), Some(&42));
        assert_eq!(cell.encode(), "+00042");
        assert_eq!(Cell::<i64>::decode(kind, "").state(), &CellState::Empty);
        assert_eq!(
            Cell::<i64>::decode(kind, "9223372036854775808").state(),
            &CellState::Invalid(ValueError::InvalidInteger)
        );
    }
    for (token, coefficient, scale) in [
        ("+0001.2300", 12300, 4),
        ("-.5", -5, 1),
        ("1.", 1, 0),
        ("1E+3", 1, -3),
        ("-0.000", 0, 3),
        ("1.20e-3", 120, 5),
    ] {
        let cell = Cell::<Decimal>::decode(FieldKind::Decimal, token);
        assert_eq!(
            cell.value(),
            Some(&Decimal::new(coefficient, scale).unwrap())
        );
        assert_eq!(cell.encode(), token);
    }
    let cell = Cell::<DateTime>::decode(FieldKind::DateTime, "2024-02-29 08:15");
    assert_eq!(cell.value(), Some(&"2024-02-29 08:15".parse().unwrap()));
    for (token, value) in [("Y", true), ("N", false)] {
        let cell = Cell::<bool>::decode(FieldKind::Flag, token);
        assert_eq!(cell.value(), Some(&value));
        assert_eq!(cell.encode(), token);
    }
    for kind in [
        FieldKind::Text,
        FieldKind::Blob,
        FieldKind::Enum(&["TT_Task"]),
        FieldKind::Guid,
    ] {
        let token = match kind {
            FieldKind::Guid => "0123456789abcdefABCDEF",
            FieldKind::Enum(_) => "TT_Task",
            _ => "text\nwith content",
        };
        let cell = Cell::<&str>::decode(kind, token);
        assert_eq!(cell.value(), Some(&token));
        assert_eq!(cell.encode(), token);
        assert_eq!(Cell::<&str>::decode(kind, "").state(), &CellState::Empty);
    }
    assert_eq!(
        Cell::<Decimal>::decode(FieldKind::Decimal, "").state(),
        &CellState::Empty
    );
    assert_eq!(
        Cell::<DateTime>::decode(FieldKind::DateTime, "").state(),
        &CellState::Empty
    );
    assert_eq!(
        Cell::<bool>::decode(FieldKind::Flag, "").state(),
        &CellState::Empty
    );
}

#[test]
fn invalid_tokens_are_not_silently_normalized() {
    for token in [
        "NaN",
        "inf",
        "1,234.5",
        " 1",
        ".",
        "1e9999999999999",
        "1.2.3",
        "--1",
        "1e1e1",
        "1e-65",
        "1e64",
        "170141183460469231731687303715884105728",
    ] {
        let cell = Cell::<Decimal>::decode(FieldKind::Decimal, token);
        assert_eq!(
            cell.state(),
            &CellState::Invalid(ValueError::InvalidDecimal),
            "{token}"
        );
        assert_eq!(cell.encode(), token);
    }
    for (token, error) in [
        ("2026-02-29 08:00", ValueError::InvalidDateTime),
        ("2026-01-01 08:00:00", ValueError::UnsupportedPrecision),
        ("2026-01-01 08:00:59", ValueError::UnsupportedPrecision),
    ] {
        let cell = Cell::<DateTime>::decode(FieldKind::DateTime, token);
        assert_eq!(cell.state(), &CellState::Invalid(error));
        assert_eq!(cell.encode(), token);
    }
    assert_eq!(
        Cell::<bool>::decode(FieldKind::Flag, "false").state(),
        &CellState::Invalid(ValueError::InvalidFlag)
    );
    assert_eq!(
        Cell::<&str>::decode(FieldKind::Enum(&["KNOWN"]), "FUTURE").state(),
        &CellState::Invalid(ValueError::UnknownEnum)
    );
    assert_eq!(
        Cell::<&str>::decode(FieldKind::Guid, "bad-guid").state(),
        &CellState::Invalid(ValueError::InvalidGuid)
    );
}

#[test]
fn fresh_values_use_p6_scalar_formatting() {
    assert_eq!(Cell::from_value(true).encode(), "Y");
    assert_eq!(Cell::from_value(false).encode(), "N");
    assert_eq!(Cell::from_value(42i64).encode(), "42");
    assert_eq!(
        Cell::from_value("2026-01-01 08:00".parse::<DateTime>().unwrap()).encode(),
        "2026-01-01 08:00"
    );
    for (decimal, expected) in [
        (Decimal::new(12300, 4).unwrap(), "1.2300"),
        (Decimal::new(-5, 3).unwrap(), "-0.005"),
        (Decimal::new(12, -2).unwrap(), "1200"),
    ] {
        assert_eq!(Cell::from_value(decimal).encode(), expected);
    }
    assert_eq!(
        i128::MIN
            .to_string()
            .parse::<Decimal>()
            .unwrap()
            .coefficient(),
        i128::MIN
    );
    assert!(Decimal::new(i128::MAX, -1).is_err());
    assert_eq!(Decimal::new(0, -64).unwrap().to_string(), "0");
    assert_eq!(Cell::<i64>::missing().encode(), "");
    assert_eq!(Cell::<i64>::missing().state(), &CellState::Missing);
}

#[test]
fn row_binding_uses_names_keeps_extras_and_source_coordinates() {
    let raw = raw::read(b"ERMHDR\n%T\tTASK\n%F\ttask_name\tfuture\ttask_id\ttask_type\n%R\tname\textra\t0007\tTT_Task\n%R\tshort\n%T\tFUTURE\n%F\tx\n%R\topaque\n%E\n".as_slice()).unwrap();
    let typed = TypedXer::decode(&raw);
    let TypedTable::Task { source_index, rows } = typed.table("TASK").unwrap() else {
        panic!("expected TASK");
    };
    assert_eq!(*source_index, 0);
    assert_eq!(rows[0].task_id.value(), Some(&7));
    assert_eq!(rows[0].task_id.encode(), "0007");
    assert_eq!(rows[0].meta.source, RowRef { table: 0, row: 0 });
    assert_eq!(rows[0].meta.extras["future"], "extra");
    assert_eq!(rows[0].cstr_type.state(), &CellState::Missing);
    assert_eq!(rows[1].task_id.state(), &CellState::Empty);
    assert_eq!(rows[1].task_type.state(), &CellState::Empty);
    assert_eq!(typed.unknown_tables, [1]);
    assert_eq!(typed.diagnostic_counts[&DiagnosticKind::UnknownField], 1);
    assert_eq!(typed.diagnostic_counts[&DiagnosticKind::UnknownTable], 1);
    assert!(
        typed
            .diagnostics
            .iter()
            .all(|d| d.severity == Severity::Warning)
    );
    let mut bytes = Vec::new();
    raw.write(&mut bytes).unwrap();
    assert_eq!(bytes, raw.source());
}

#[test]
fn duplicate_columns_overflow_and_repeat_tables_remain_available() {
    let raw = raw::read(b"ERMHDR\n%T\tTASK\n%F\ttask_id\ttask_id\n%R\t1\t2\toverflow\n%T\tTASK\n%F\ttask_id\n%R\t3\n%E\n".as_slice()).unwrap();
    let typed = TypedXer::decode(&raw);
    assert_eq!(typed.tables.len(), 2);
    let TypedTable::Task { rows, .. } = &typed.tables[0] else {
        panic!("TASK");
    };
    assert_eq!(rows[0].task_id.value(), Some(&1));
    assert_eq!(rows[0].meta.duplicates, [(1, "2")]);
    assert_eq!(rows[0].meta.overflow, ["overflow"]);
    assert_eq!(typed.tables[1].source_index(), 1);
    assert_eq!(typed.diagnostic_counts[&DiagnosticKind::DuplicateField], 1);
    assert_eq!(typed.diagnostic_counts[&DiagnosticKind::ExtraCells], 1);
}

#[test]
fn warnings_are_bounded_but_aggregate_counts_are_complete() {
    let source = format!(
        "ERMHDR\n%T\tTASK\n%F\ttask_id\n{}%E\n",
        "%R\tbad\n".repeat(MAX_DIAGNOSTICS + 5)
    );
    let raw = raw::read(source.as_bytes()).unwrap();
    let typed = TypedXer::decode(&raw);
    assert_eq!(typed.tables[0].len(), MAX_DIAGNOSTICS + 5);
    assert_eq!(typed.diagnostics.len(), MAX_DIAGNOSTICS);
    assert_eq!(typed.suppressed_diagnostics, 5);
    assert_eq!(
        typed.diagnostic_counts[&DiagnosticKind::Value(ValueError::InvalidInteger)],
        MAX_DIAGNOSTICS + 5
    );
    assert_eq!(typed.diagnostics[0].table, "TASK");
    assert_eq!(typed.diagnostics[0].row, Some(0));
    assert_eq!(typed.diagnostics[0].field, "task_id");
    assert!(typed.diagnostics.iter().all(|d| !d.message.contains("bad")));
}

#[test]
fn every_registered_field_has_a_typed_binding() {
    let mut raw = RawXer::new(
        &["synthetic"],
        raw::Encoding::Utf8 { bom: false },
        raw::LineEnding::Lf,
    )
    .unwrap();
    for table in registry::tables() {
        let index = raw
            .push_table(
                table.name,
                table.fields.iter().map(|f| f.name.to_owned()).collect(),
            )
            .unwrap();
        let row: Vec<_> = table.fields.iter().map(|f| sample(f.kind)).collect();
        raw.push_row(index, &row).unwrap();
    }
    let typed = TypedXer::decode(&raw);
    assert_eq!(typed.diagnostics.len(), 0);
    assert_eq!(typed.tables.len(), registry::tables().len());
    assert_eq!(typed.unknown_tables.len(), 0);
    for table in &typed.tables {
        let mut fields = Vec::new();
        table.visit_rows(&mut |row| row.visit_fields(&mut |name, _| fields.push(name)));
        assert_eq!(
            fields,
            registry::table(table.name())
                .unwrap()
                .fields
                .iter()
                .map(|f| f.name)
                .collect::<Vec<_>>()
        );
    }
}

fn sample(kind: FieldKind) -> String {
    match kind {
        FieldKind::Id | FieldKind::ForeignId(_) | FieldKind::Int => "42",
        FieldKind::Decimal => "1.25",
        FieldKind::DateTime => "2024-01-01 08:00",
        FieldKind::Flag => "Y",
        FieldKind::Enum(codes) => codes[0],
        FieldKind::Guid => "0123456789abcdefABCDEF",
        FieldKind::Text | FieldKind::Blob => "text",
    }
    .to_owned()
}

proptest! {
    #[test]
    fn decimal_formatting_is_exact(coefficient in any::<i64>(), scale in -12i32..12) {
        let value = Decimal::new(i128::from(coefficient), scale).unwrap();
        let reparsed: Decimal = value.to_string().parse().unwrap();
        // Fixed-point formatting expands negative scales, preserving numeric value.
        let expected_coefficient = if scale < 0 { i128::from(coefficient) * 10i128.pow(scale.unsigned_abs()) } else { i128::from(coefficient) };
        prop_assert_eq!(reparsed.coefficient(), expected_coefficient);
        prop_assert_eq!(reparsed.scale(), scale.max(0));
    }

    #[test]
    fn arbitrary_tokens_never_panic_or_lose_spelling(token in ".{0,80}") {
        let id = Cell::<i64>::decode(FieldKind::Id, &token);
        prop_assert_eq!(id.encode(), token.as_str());
        let decimal = Cell::<Decimal>::decode(FieldKind::Decimal, &token);
        prop_assert_eq!(decimal.encode(), token.as_str());
        let datetime = Cell::<DateTime>::decode(FieldKind::DateTime, &token);
        prop_assert_eq!(datetime.encode(), token.as_str());
        let flag = Cell::<bool>::decode(FieldKind::Flag, &token);
        prop_assert_eq!(flag.encode(), token.as_str());
        let guid = Cell::<&str>::decode(FieldKind::Guid, &token);
        prop_assert_eq!(guid.encode(), token.as_str());
    }
}
