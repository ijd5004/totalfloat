use super::*;
use proptest::prelude::*;

fn output(x: &RawXer) -> Vec<u8> {
    let mut bytes = Vec::new();
    x.write(&mut bytes).unwrap();
    bytes
}

#[test]
fn patch_only_changes_one_cell_even_with_invalid_utf8_and_mixed_endings() {
    let source = b"\xef\xbb\xbfERMHDR\t23.12\r\n%T\tUNKNOWN\n%F\ta\tb\tc\r\n%R\tbad\xff\told\tline1\nline2\r\n%E\ntrailer";
    let mut x = read(source.as_slice()).unwrap();
    x.set_cell(0, 0, 1, "new value").unwrap();
    let expected = [
        source
            .split_at(source.windows(3).position(|b| b == b"old").unwrap())
            .0,
        b"new value",
        &source[source.windows(3).position(|b| b == b"old").unwrap() + 3..],
    ]
    .concat();
    assert_eq!(output(&x), expected);
    assert_eq!(
        read(output(&x)).unwrap().tables()[0].rows[0].get(1),
        Some("new value")
    );
}

#[test]
fn windows1252_edits_and_multiline_values() {
    let mut x = read(b"ERMHDR\t6.0\n%T\tT\n%F\ta\tb\n%R\t\xa3\told\n%E\n".as_slice()).unwrap();
    x.set_cell(0, 0, 1, "Café\nsecond line").unwrap();
    assert_eq!(
        output(&x),
        b"ERMHDR\t6.0\n%T\tT\n%F\ta\tb\n%R\t\xa3\tCaf\xe9\nsecond line\n%E\n"
    );
    assert_eq!(
        read(output(&x)).unwrap().tables()[0].rows[0].get(1),
        Some("Café\nsecond line")
    );
    let before = x.clone();
    assert!(x.set_cell(0, 0, 1, "猫").is_err());
    assert_eq!(x, before);
}

#[test]
fn extends_short_rows_replaces_and_appends() {
    let mut x = read(b"ERMHDR\n%T\tT\n%F\ta\tb\tc\n%R\told".as_slice()).unwrap();
    x.set_cell(0, 0, 2, "third").unwrap();
    assert_eq!(output(&x), b"ERMHDR\n%T\tT\n%F\ta\tb\tc\n%R\told\t\tthird");
    x.set_row(0, 0, &["replacement".into()]).unwrap();
    let row = x.push_row(0, &["added".into()]).unwrap();
    x.set_cell(0, row, 0, "changed").unwrap();
    let t = x.push_table("EXTRA", vec!["f".into()]).unwrap();
    x.push_row(t, &["extra".into()]).unwrap();
    let parsed = read(output(&x)).unwrap();
    assert_eq!(parsed.tables()[0].rows[0].cells, ["replacement"]);
    assert_eq!(parsed.tables()[0].rows[1].cells, ["changed"]);
    assert_eq!(parsed.tables()[1].rows[0].cells, ["extra"]);
}

#[test]
fn new_tables_go_before_end_and_preserve_trailing_bytes() {
    let mut x = read(b"ERMHDR\r\n%E\r\ntrailer".as_slice()).unwrap();
    let t = x.push_table("T", vec![]).unwrap();
    x.push_row(t, &[]).unwrap();
    assert_eq!(output(&x), b"ERMHDR\r\n%T\tT\r\n%F\r\n%R\r\n%E\r\ntrailer");
}

#[test]
fn invalid_edits_are_atomic() {
    let mut x = read(b"ERMHDR\n%T\tT\n%F\tf\n%R\tvalue\n%E\n".as_slice()).unwrap();
    let before = x.clone();
    for value in ["tab\there", "ends\n", "ends\r", "line\n%R\trow", "line\n%E"] {
        assert!(x.set_cell(0, 0, 0, value).is_err());
        assert_eq!(x, before);
    }
    assert!(x.set_cell(1, 0, 0, "x").is_err());
    assert!(x.set_cell(0, 1, 0, "x").is_err());
    assert!(x.set_cell(0, 0, 1, "x").is_err());
    assert!(x.push_table("bad\nname", vec![]).is_err());
    assert_eq!(x, before);
}

#[test]
fn writer_propagates_io_errors() {
    let x = read(b"ERMHDR\n%E\n".as_slice()).unwrap();
    assert!(x.write(&mut [][..]).is_err());
    let mut x = x;
    let t = x.push_table("T", vec![]).unwrap();
    x.push_row(t, &[]).unwrap();
    assert!(x.write(&mut [][..]).is_err());
}

proptest! {
    #[test]
    fn generated_tables_write_read_equal(
        rows in prop::collection::vec(prop::collection::vec("[a-zA-Z0-9 é£]{0,25}", 0..6), 0..10),
        bom in any::<bool>(), crlf in any::<bool>(),
    ) {
        let mut x = RawXer::new(&["23.12", "synthetic"], Encoding::Utf8 { bom },
            if crlf { LineEnding::CrLf } else { LineEnding::Lf }).unwrap();
        let t = x.push_table("T", vec!["a".into(), "b".into()]).unwrap();
        for row in &rows { x.push_row(t, row).unwrap(); }
        let parsed = read(output(&x)).unwrap();
        prop_assert_eq!(&parsed.header.fields, &x.header.fields);
        // Pure ASCII reads as Windows-1252, whatever wrote it.
        let ascii = !bom && output(&x).is_ascii();
        let expected = if ascii { Encoding::Windows1252 } else { x.encoding };
        prop_assert_eq!(parsed.encoding, expected);
        prop_assert_eq!(parsed.line_ending, x.line_ending);
        prop_assert_eq!(&parsed.tables()[0].fields, &x.tables()[0].fields);
        let actual: Vec<_> = parsed.tables()[0].rows.iter().map(|r| r.cells.clone()).collect();
        prop_assert_eq!(actual, rows);
        prop_assert_eq!(output(&parsed), output(&x));
    }
}

#[test]
fn oversized_header_returns_error_without_panicking() {
    assert!(
        RawXer::new(
            &vec!["x"; MAX_FIELDS + 1],
            Encoding::Utf8 { bom: false },
            LineEnding::Lf
        )
        .is_err()
    );
}

#[test]
fn appended_row_follows_stray_content_of_empty_table() {
    let mut x = read(b"ERMHDR\n%T\tT\n%F\ta\njunk\n%E\n".as_slice()).unwrap();
    x.push_row(0, &["added".into()]).unwrap();
    assert_eq!(output(&x), b"ERMHDR\n%T\tT\n%F\ta\njunk\n%R\tadded\n%E\n");
    assert_eq!(
        read(output(&x)).unwrap().tables()[0].rows[0].cells,
        ["added"]
    );
}

#[test]
fn removing_rows_drops_only_their_bytes_and_shifts_indices() {
    let src = "ERMHDR\t8.2\r\n%T\tA\r\n%F\tid\tv\r\n%R\t1\ta\r\n%R\t2\tb\r\n%R\t3\tc\r\n%T\tB\r\n%F\tx\r\n%R\t9\r\n%E\r\n";
    let mut x = read(src.as_bytes()).unwrap();
    // Edit row 2 ("3"), then remove row 0: the edit must follow the row to index 1.
    x.set_cell(0, 2, 1, "C").unwrap();
    assert_eq!(x.remove_row(0, 0).unwrap().cells, ["1", "a"]);
    assert_eq!(x.tables()[0].rows[1].cells, ["3", "C"]);
    // An added row can be removed too, and doesn't affect original rows.
    let added = x.push_row(0, &["4".into(), "d".into()]).unwrap();
    x.push_row(0, &["5".into(), "e".into()]).unwrap();
    x.remove_row(0, added).unwrap();
    assert_eq!(
        String::from_utf8(output(&x)).unwrap(),
        "ERMHDR\t8.2\r\n%T\tA\r\n%F\tid\tv\r\n%R\t2\tb\r\n%R\t3\tC\r\n%R\t5\te\r\n%T\tB\r\n%F\tx\r\n%R\t9\r\n%E\r\n"
    );
    assert!(matches!(
        x.remove_row(0, 9),
        Err(EditError::NoSuchRow { .. })
    ));
    assert!(matches!(x.remove_row(7, 0), Err(EditError::NoSuchTable(7))));
    // Removing every row of a table keeps its header lines.
    let mut y = read(src.as_bytes()).unwrap();
    y.remove_row(1, 0).unwrap();
    assert_eq!(
        String::from_utf8(output(&y)).unwrap(),
        src.replace("%R\t9\r\n", "")
    );
}

#[test]
fn setting_no_cells_edits_nothing() {
    let mut x = read(b"ERMHDR\t1\n%T\tT\n%F\ta\n%R\t1\n%E\n".as_slice()).unwrap();
    x.set_cells(0, 0, &[]).unwrap();
    assert!(!x.is_edited());
}

#[test]
fn set_cells_replaces_together_and_changes_nothing_on_a_bad_value() {
    let mut x = read(b"ERMHDR\t1\n%T\tT\n%F\ta\tb\n%R\t1\t2\n%E\n".as_slice()).unwrap();
    let bad = [(0, "x".to_owned()), (1, "tab\there".to_owned())];
    assert!(x.set_cells(0, 0, &bad).is_err());
    assert_eq!(x.tables()[0].rows[0].cells, ["1", "2"]);
    x.set_cells(0, 0, &[(1, "y".to_owned()), (0, "x".to_owned())])
        .unwrap();
    assert_eq!(output(&x), b"ERMHDR\t1\n%T\tT\n%F\ta\tb\n%R\tx\ty\n%E\n");
}
