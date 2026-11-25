use super::*;
use proptest::prelude::*;

const MINIMAL: &str = "ERMHDR\t23.12\t2026-01-05\tProject\tadmin\tAdmin\tdbx\tProject Management\tUSD\n\
%T\tPROJECT\n\
%F\tproj_id\tproj_short_name\n\
%R\t1\tDEMO\n\
%E\n";

fn ok(input: &[u8]) -> RawXer {
    let x = read(input).unwrap();
    let mut out = Vec::new();
    x.write(&mut out).unwrap();
    assert_eq!(out, input);
    x
}

/// Segments must tile the source exactly.
fn assert_covers(x: &RawXer) {
    let mut pos = 0;
    for s in x.segments() {
        assert_eq!(s.span.start, pos, "gap or overlap at {pos}: {:?}", s.kind);
        pos = s.span.end;
    }
    assert_eq!(pos, x.source().len());
    let joined: Vec<u8> = x
        .segments()
        .iter()
        .flat_map(|s| x.source()[s.span.clone()].iter().copied())
        .collect();
    assert_eq!(joined, x.source());
    let mut out = Vec::new();
    x.write(&mut out).unwrap();
    assert_eq!(out, x.source());
}

fn kinds(x: &RawXer) -> Vec<DiagnosticKind> {
    x.diagnostics.iter().map(|d| d.kind).collect()
}

#[test]
fn minimal_file() {
    let x = ok(MINIMAL.as_bytes());
    assert_eq!(x.encoding, Encoding::Windows1252);
    assert_eq!(x.line_ending, LineEnding::Lf);
    assert_eq!(x.header.version(), Some("23.12"));
    assert_eq!(x.header.fields.len(), 8);
    assert_eq!(x.tables.len(), 1);
    let t = x.table("PROJECT").unwrap();
    assert_eq!(t.fields, ["proj_id", "proj_short_name"]);
    assert_eq!(t.field_index("proj_short_name"), Some(1));
    assert_eq!(t.rows[0].cells, ["1", "DEMO"]);
    assert_eq!(t.rows[0].line, 4);
    assert_eq!(t.rows[0].get(5), None);
    assert_eq!(x.diagnostics, []);
    assert!(x.ends_with_newline());
    assert_covers(&x);
    let k: Vec<_> = x.segments().iter().map(|s| s.kind).collect();
    assert_eq!(
        k,
        [
            SegmentKind::Header,
            SegmentKind::TableName(0),
            SegmentKind::Fields(0),
            SegmentKind::Row(0, 0),
            SegmentKind::End
        ]
    );
}

#[test]
fn multiple_tables_and_empty_table() {
    let src = "ERMHDR\t8.2\n%T\tCURRTYPE\n%F\tcurr_id\n%R\t1\n%T\tOBS\n%F\tobs_id\tobs_name\n%T\tTASK\n%F\ttask_id\ttask_code\n%R\t10\tA1\n%R\t11\tA2\n%E\n";
    let x = ok(src.as_bytes());
    let names: Vec<_> = x.tables.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(names, ["CURRTYPE", "OBS", "TASK"]);
    assert_eq!(x.table("OBS").unwrap().rows, []);
    assert_eq!(x.table("TASK").unwrap().rows.len(), 2);
    assert_eq!(x.diagnostics, []);
    assert_covers(&x);
}

#[test]
fn crlf_and_missing_final_newline() {
    let src = MINIMAL.replace('\n', "\r\n");
    let x = ok(src.as_bytes());
    assert_eq!(x.line_ending, LineEnding::CrLf);
    assert_eq!(x.table("PROJECT").unwrap().rows[0].cells, ["1", "DEMO"]);
    assert_eq!(x.diagnostics, []);
    assert_covers(&x);

    let trimmed = src.trim_end_matches("\r\n");
    let x = ok(trimmed.as_bytes());
    assert!(!x.ends_with_newline());
    assert_eq!(x.segments().last().unwrap().kind, SegmentKind::End);
    assert_covers(&x);
}

#[test]
fn mixed_line_endings_are_reported() {
    let src = "ERMHDR\t8.2\r\n%T\tA\n%F\tx\r\n%R\t1\n%E\r\n";
    let x = ok(src.as_bytes());
    assert_eq!(x.line_ending, LineEnding::CrLf);
    assert_eq!(kinds(&x), [DiagnosticKind::MixedLineEndings]);
    assert_eq!(x.table("A").unwrap().rows[0].cells, ["1"]);
    assert_covers(&x);
}

#[test]
fn utf8_bom() {
    let mut src = BOM.to_vec();
    src.extend_from_slice("ERMHDR\t20.12\n%T\tTASK\n%F\ttask_name\n%R\tCafé ✓\n%E\n".as_bytes());
    let x = ok(&src);
    assert_eq!(x.encoding, Encoding::Utf8 { bom: true });
    assert_eq!(x.segments()[0].kind, SegmentKind::Bom);
    assert_eq!(x.table("TASK").unwrap().rows[0].cells, ["Café ✓"]);
    assert_covers(&x);
}

#[test]
fn invalid_utf8_after_bom_is_reported_once() {
    let mut src = BOM.to_vec();
    src.extend_from_slice(b"ERMHDR\t20.12\n%T\tTASK\n%F\ta\tb\n%R\t\xff\t\xfe\n%E\n");
    let x = ok(&src);
    assert_eq!(x.encoding, Encoding::Utf8 { bom: true });
    assert_eq!(kinds(&x), [DiagnosticKind::InvalidUtf8]);
    assert_eq!(
        x.table("TASK").unwrap().rows[0].cells,
        ["\u{FFFD}", "\u{FFFD}"]
    );
}

#[test]
fn windows_1252() {
    // 0xE9 = é, 0x96 = en dash, 0x80 = euro sign in Windows-1252.
    let src = b"ERMHDR\t6.0\n%T\tTASK\n%F\ttask_name\n%R\tCaf\xe9 \x96 \x80100\n%E\n";
    let x = ok(src);
    assert_eq!(x.encoding, Encoding::Windows1252);
    assert_eq!(x.table("TASK").unwrap().rows[0].cells, ["Café – €100"]);
    assert_covers(&x);
}

#[test]
fn ragged_rows_are_kept_and_reported() {
    let src = "ERMHDR\t8.2\n%T\tA\n%F\tx\ty\tz\n%R\t1\t2\n%R\t1\t2\t3\t4\n%R\t1\t2\t3\n%E\n";
    let x = ok(src.as_bytes());
    let rows = &x.table("A").unwrap().rows;
    assert_eq!(rows[0].cells, ["1", "2"]);
    assert_eq!(rows[1].cells, ["1", "2", "3", "4"]);
    assert_eq!(
        kinds(&x),
        [DiagnosticKind::RaggedRow, DiagnosticKind::RaggedRow]
    );
    assert_eq!(x.diagnostics[0].line, 4);
    assert_eq!(x.diagnostics[1].line, 5);
    assert_covers(&x);
}

#[test]
fn missing_end_is_reported() {
    let src = "ERMHDR\t8.2\n%T\tA\n%F\tx\n%R\t1\n";
    let x = ok(src.as_bytes());
    assert_eq!(kinds(&x), [DiagnosticKind::MissingEnd]);
    assert_eq!(x.table("A").unwrap().rows.len(), 1);
    assert_covers(&x);
}

#[test]
fn rows_may_span_lines() {
    // P6 writes multi-line cells (e.g. calendar data) with raw line breaks;
    // the following cells continue on the last physical line.
    let src = "ERMHDR\t24.12\r\n%T\tCALENDAR\r\n%F\tclndr_id\tclndr_data\tday_hr_cnt\r\n%R\t1\t(0||CalendarData()(\r\n  (0||DaysOfWeek()())))\t8\r\n%R\t2\t\t8\r\n%E\r\n";
    let x = ok(src.as_bytes());
    let rows = &x.table("CALENDAR").unwrap().rows;
    assert_eq!(rows.len(), 2);
    assert_eq!(
        rows[0].cells,
        ["1", "(0||CalendarData()(\r\n  (0||DaysOfWeek()())))", "8"]
    );
    assert_eq!(rows[1].cells, ["2", "", "8"]);
    assert_eq!(rows[1].line, 6);
    assert_eq!(x.diagnostics, []);
    assert_covers(&x);
}

#[test]
fn stray_out_of_place_and_trailing_lines() {
    let src = "ERMHDR\t8.2\nnot a record\n%R\torphan\n%F\torphan\n%T\tA\n%F\tx\n%F\ty\n%R\t1\n%E\n\nextra\n";
    let x = ok(src.as_bytes());
    assert_eq!(
        kinds(&x),
        [
            DiagnosticKind::StrayLine,
            DiagnosticKind::OutOfPlace,
            DiagnosticKind::OutOfPlace,
            DiagnosticKind::OutOfPlace,
            DiagnosticKind::TrailingContent,
        ]
    );
    assert_eq!(x.table("A").unwrap().fields, ["x"]);
    assert_eq!(x.segments().last().unwrap().kind, SegmentKind::Trailing);
    assert_covers(&x);
}

#[test]
fn trailing_blank_lines_are_not_reported() {
    let x = ok(format!("{MINIMAL}\n\n").as_bytes());
    assert_eq!(x.diagnostics, []);
    assert_covers(&x);
}

#[test]
fn markers_need_a_tab_or_line_end() {
    // "%Tx" and "%End" are not markers; inside a row they continue it.
    let src = "ERMHDR\t8.2\n%T\tA\n%F\tx\n%R\tone\n%Tx\n%End\n%E\n";
    let x = ok(src.as_bytes());
    assert_eq!(x.table("A").unwrap().rows[0].cells, ["one\n%Tx\n%End"]);
    assert_eq!(x.diagnostics, []);
    assert_covers(&x);
}

#[test]
fn garbage_input() {
    assert_eq!(read(b""), Err(ReadError::Empty));
    assert_eq!(read(b"hello world\n"), Err(ReadError::NotXer));
    assert_eq!(read([0u8; 512]), Err(ReadError::NotXer));
    assert_eq!(read(b"\xEF\xBB\xBF"), Err(ReadError::NotXer));
    assert_eq!(read(b"%T\tTASK\n"), Err(ReadError::NotXer));
    // A bare header is an XER with no tables and no %E.
    let x = ok(b"ERMHDR");
    assert_eq!(x.tables, []);
    assert_eq!(x.header.version(), None);
    assert_eq!(kinds(&x), [DiagnosticKind::MissingEnd]);
    assert_covers(&x);
}

#[test]
fn too_many_tables_is_an_error() {
    let tables = "%T\tT\n".repeat(MAX_TABLES + 1);
    let src = format!("ERMHDR\t8.2\n{tables}");
    assert!(matches!(
        read(src.as_bytes()),
        Err(ReadError::TooManyTables { .. })
    ));
}

#[test]
fn too_many_fields_is_an_error() {
    let fields = vec!["f"; MAX_FIELDS + 1].join("\t");
    let src = format!("ERMHDR\t8.2\n%T\tA\n%F\t{fields}\n%E\n");
    assert_eq!(
        read(src.as_bytes()),
        Err(ReadError::TooManyFields { line: 3 })
    );
}

#[test]
fn blank_lines_between_records_are_not_cell_content() {
    let src = "ERMHDR\t8.2\r\n%T\tA\r\n%F\tx\ty\r\n%R\t1\t2\r\n\r\n\r\n%T\tB\r\n%F\tz\r\n%R\t3\r\n\r\n%E\r\n";
    let x = ok(src.as_bytes());
    assert_eq!(x.table("A").unwrap().rows[0].cells, ["1", "2"]);
    assert_eq!(x.table("B").unwrap().rows[0].cells, ["3"]);
    assert_eq!(x.diagnostics, []);
    assert_covers(&x);
    // A blank line followed by more row content is part of the cell.
    let src = "ERMHDR\t8.2\n%T\tA\n%F\tmemo\tn\n%R\tpara one\n\npara two\t5\n%E\n";
    let x = ok(src.as_bytes());
    assert_eq!(
        x.table("A").unwrap().rows[0].cells,
        ["para one\n\npara two", "5"]
    );
    assert_covers(&x);
}

#[test]
fn end_marker_may_have_trailing_whitespace() {
    let src = "ERMHDR\t8.2\r\n%T\tA\r\n%F\tx\r\n%R\t1\r\n%E \t\r\n";
    let x = ok(src.as_bytes());
    assert_eq!(x.table("A").unwrap().rows[0].cells, ["1"]);
    assert_eq!(x.diagnostics, []);
    assert_eq!(x.segments().last().unwrap().kind, SegmentKind::End);
}

#[test]
fn line_breaks_inside_cells_do_not_count_as_mixed_endings() {
    let src = "ERMHDR\t24.12\r\n%T\tA\r\n%F\tdata\tn\r\n%R\t(a\n  b\n  c)\t8\r\n%E\r\n";
    let x = ok(src.as_bytes());
    assert_eq!(x.table("A").unwrap().rows[0].cells, ["(a\n  b\n  c)", "8"]);
    assert_eq!(x.diagnostics, []);
}

#[test]
fn invalid_utf8_is_reported_on_the_rows_first_line() {
    let mut src = BOM.to_vec();
    src.extend_from_slice(b"ERMHDR\t20.12\n%T\tA\n%F\ta\tb\n%R\t\xff\nmore\nmore\t1\n%E\n");
    let x = ok(&src);
    assert_eq!(x.diagnostics[0].kind, DiagnosticKind::InvalidUtf8);
    assert_eq!(x.diagnostics[0].line, 4);
}

#[test]
fn mostly_utf8_with_a_stray_byte_is_flagged() {
    let mut src = "ERMHDR\t20.12\n%T\tA\n%F\tname\n%R\tCafé\n%R\tbad "
        .as_bytes()
        .to_vec();
    src.extend_from_slice(b"\x92\n%E\n");
    let x = ok(&src);
    assert_eq!(x.encoding, Encoding::Windows1252);
    assert_eq!(kinds(&x), [DiagnosticKind::MixedEncoding]);
    // A plain Windows-1252 file is not flagged.
    let x = ok(b"ERMHDR\t6.0\n%T\tA\n%F\tname\n%R\tCaf\xe9\n%E\n");
    assert_eq!(x.diagnostics, []);
}

#[test]
fn too_many_cells_in_a_row_is_an_error() {
    let cells = "\t".repeat(MAX_FIELDS + 1);
    let src = format!("ERMHDR\t8.2\n%T\tA\n%F\tx\n%R{cells}\n%E\n");
    assert_eq!(
        read(src.as_bytes()),
        Err(ReadError::TooManyFields { line: 4 })
    );
}

#[test]
fn diagnostics_are_capped_and_stray_lines_share_a_segment() {
    let strays = "junk\n".repeat(MAX_DIAGNOSTICS + 50);
    let src = format!("ERMHDR\t8.2\n{strays}%E\n");
    let x = ok(src.as_bytes());
    assert_eq!(x.diagnostics.len(), MAX_DIAGNOSTICS + 1);
    let last = x.diagnostics.last().unwrap();
    assert_eq!(last.kind, DiagnosticKind::TooManyDiagnostics);
    assert!(last.message.starts_with("50 more"), "{}", last.message);
    let k: Vec<_> = x.segments().iter().map(|s| s.kind).collect();
    assert_eq!(
        k,
        [SegmentKind::Header, SegmentKind::Stray, SegmentKind::End]
    );
    assert_covers(&x);
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 2000, ..ProptestConfig::default() })]

    #[test]
    fn arbitrary_bytes_never_panic(bytes in proptest::collection::vec(any::<u8>(), 0..512)) {
        if let Ok(x) = read(bytes.as_slice()) {
            assert_covers(&x);
        }
    }

    #[test]
    fn arbitrary_records_never_panic_and_tile(
        lines in proptest::collection::vec(
            prop_oneof![
                Just("%T\tT".to_owned()),
                Just("%F\ta\tb".to_owned()),
                Just("%E".to_owned()),
                Just(String::new()),
                "%R(\t[a-z\u{e9}\r]{0,3}){0,4}",
                "[%a-zA-Z\t\r]{0,6}",
            ],
            0..30,
        ),
        crlf in any::<bool>(),
        final_newline in any::<bool>(),
    ) {
        let eol = if crlf { "\r\n" } else { "\n" };
        let mut src = format!("ERMHDR\t9.9{eol}");
        src.push_str(&lines.join(eol));
        if final_newline {
            src.push_str(eol);
        }
        let x = read(src.as_bytes()).unwrap();
        assert_covers(&x);
    }
}

#[test]
fn a_long_blank_run_in_a_row_reads_in_linear_time() {
    let blanks = 100_000;
    let mut input = b"ERMHDR\t1\n%T\tT\n%F\ta\n%R\tx".to_vec();
    input.extend(std::iter::repeat_n(b'\n', blanks));
    input.extend(b"y\n%E\n");
    let started = std::time::Instant::now();
    let x = ok(&input);
    // Rescanning the run from each line took over 20 s at this size.
    assert!(started.elapsed() < std::time::Duration::from_secs(15));
    assert_eq!(x.tables()[0].rows.len(), 1);
    assert!(x.tables()[0].rows[0].cells[0].ends_with('y'));
}

#[test]
fn blank_runs_stay_in_their_row_whatever_the_line_ending() {
    for (input, cell) in [
        (
            &b"ERMHDR\t1\r\n%T\tT\r\n%F\ta\r\n%R\tx\r\n\r\n\r\ny\r\n%E\r\n"[..],
            "x\r\n\r\n\r\ny",
        ),
        (
            &b"ERMHDR\t1\n%T\tT\n%F\ta\n%R\tx\n\n\ny\n\n\nz\n%E\n"[..],
            "x\n\n\ny\n\n\nz",
        ),
    ] {
        let x = ok(input);
        assert_eq!(x.tables()[0].rows.len(), 1);
        assert_eq!(&x.tables()[0].rows[0].cells[0], cell);
    }
}

#[test]
fn invalid_utf8_before_a_rows_first_tab_is_reported() {
    let x = ok(b"\xEF\xBB\xBFERMHDR\t1\n%T\tT\n%F\ta\n%R\n\xFF\tx\n%E\n");
    assert!(kinds(&x).contains(&DiagnosticKind::InvalidUtf8));
}
