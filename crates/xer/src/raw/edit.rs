//! Editing a [`RawXer`] and writing it back out.
//!
//! The writer reproduces every byte it hasn't been told changed:
//!
//! - untouched segments are copied from the source;
//! - an edited row keeps the original bytes of each cell that wasn't edited
//!   (re-split from the row's source bytes), its separators and its line
//!   terminator, and encodes only the edited cells;
//! - added rows go after the last existing line of their table, and added
//!   tables go before `%E`, using the file's encoding and line ending.
//!
//! So an unedited file writes back byte-identical, and editing one cell
//! changes only that cell's bytes.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, Write};

use encoding_rs::WINDOWS_1252;

use super::{
    BOM, Cells, Encoding, LineEnding, MAX_FIELDS, MAX_TABLES, RawRow, RawTable, RawXer,
    SegmentKind, cells::MAX_ROW_TEXT, marker, read,
};

/// Edit tracking. Rows and tables at or past the `original_*` counts were
/// added after reading.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(super) struct Edits {
    /// Edited cells of rows that came from the file: (table, row) → fields.
    cells: BTreeMap<(usize, usize), BTreeSet<usize>>,
    /// Rows per table as read.
    original_rows: Vec<usize>,
    /// Tables as read.
    original_tables: usize,
    /// Set once the counts above are captured (on the first edit).
    started: bool,
}

/// Why an edit was rejected. Nothing changes when an edit fails.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EditError {
    /// No table at this index.
    #[error("no table at index {0}")]
    NoSuchTable(usize),
    /// No row at this index in the table.
    #[error("table {table} has no row {row}")]
    NoSuchRow {
        /// Table index.
        table: usize,
        /// Row index.
        row: usize,
    },
    /// The field index is past both the table's fields and the row's cells.
    #[error("table {table} has no field {field}")]
    NoSuchField {
        /// Table index.
        table: usize,
        /// Field index.
        field: usize,
    },
    /// The value would not read back as the same cell.
    #[error("{0}")]
    InvalidValue(&'static str),
    /// The value has characters the file's encoding can't represent.
    #[error("{value:?} can't be written in Windows-1252")]
    Unencodable {
        /// The value (truncated).
        value: String,
    },
    /// A row's text would pass 4 GiB.
    #[error("a row's text can't pass 4 GiB")]
    RowTooLong,
    /// Too many cells or fields.
    #[error("more than {MAX_FIELDS} cells")]
    TooManyCells,
}

impl RawXer {
    /// An empty XER with the given header fields (everything after `ERMHDR`),
    /// encoding and line ending, containing no tables.
    ///
    /// # Errors
    ///
    /// [`EditError`] if a header field contains a tab or line break, or can't
    /// be encoded, or the header exceeds reader limits.
    pub fn new(
        header_fields: &[&str],
        encoding: Encoding,
        line_ending: LineEnding,
    ) -> Result<Self, EditError> {
        let eol = line_ending.as_bytes();
        let mut bytes = Vec::new();
        if encoding == (Encoding::Utf8 { bom: true }) {
            bytes.extend_from_slice(BOM);
        }
        bytes.extend_from_slice(b"ERMHDR");
        for f in header_fields {
            check_name(f)?;
            bytes.push(b'\t');
            bytes.extend_from_slice(&encode(encoding, f)?);
        }
        bytes.extend_from_slice(eol);
        bytes.extend_from_slice(b"%E");
        bytes.extend_from_slice(eol);
        let mut xer =
            read(bytes).map_err(|_| EditError::InvalidValue("header exceeds reader limits"))?;
        // A Windows-1252 file that happens to be ASCII reads back as UTF-8.
        xer.encoding = encoding;
        Ok(xer)
    }

    /// Sets one cell, marking it edited. A field past the end of a short row
    /// extends the row with empty cells.
    ///
    /// # Errors
    ///
    /// [`EditError`] for bad indices, a row grown past 4 GiB, or a value that
    /// wouldn't read back identically (tabs, a trailing line break, an embedded
    /// line that looks like a record marker, or characters the file's encoding
    /// lacks).
    pub fn set_cell(
        &mut self,
        table: usize,
        row: usize,
        field: usize,
        value: impl Into<String>,
    ) -> Result<(), EditError> {
        self.set_cells(table, row, &[(field, value.into())])
    }

    /// Replaces several cells of a row at once, cheaper than one at a time.
    /// Nothing changes if any of them is rejected. A later value for the same
    /// field wins.
    ///
    /// # Errors
    ///
    /// As [`RawXer::set_cell`].
    pub fn set_cells(
        &mut self,
        table: usize,
        row: usize,
        changes: &[(usize, String)],
    ) -> Result<(), EditError> {
        let t = self
            .tables
            .get(table)
            .ok_or(EditError::NoSuchTable(table))?;
        let r = t.rows.get(row).ok_or(EditError::NoSuchRow { table, row })?;
        if changes.is_empty() {
            return Ok(());
        }
        let mut added = 0_usize;
        for (field, value) in changes {
            self.check_value(value)?;
            if *field >= t.fields.len().max(r.cells.len()) {
                return Err(EditError::NoSuchField {
                    table,
                    field: *field,
                });
            }
            added = added.saturating_add(value.len());
        }
        if r.cells.text_len().saturating_add(added) > MAX_ROW_TEXT {
            return Err(EditError::RowTooLong);
        }
        self.start_editing();
        self.tables[table].rows[row].cells.set(changes);
        for (field, _) in changes {
            self.mark(table, row, *field);
        }
        Ok(())
    }

    /// Replaces every cell of a row, marking all of them edited.
    ///
    /// # Errors
    ///
    /// As [`RawXer::set_cell`], plus [`EditError::TooManyCells`].
    pub fn set_row(&mut self, table: usize, row: usize, cells: &[String]) -> Result<(), EditError> {
        self.check_cells(cells)?;
        let t = self
            .tables
            .get(table)
            .ok_or(EditError::NoSuchTable(table))?;
        if row >= t.rows.len() {
            return Err(EditError::NoSuchRow { table, row });
        }
        self.start_editing();
        let n = cells.len();
        self.tables[table].rows[row].cells = Cells::new(cells);
        for field in 0..n {
            self.mark(table, row, field);
        }
        // Mark the row edited even when it now has no cells.
        if n == 0 {
            self.edits.cells.entry((table, row)).or_default();
        }
        Ok(())
    }

    /// Appends a row to a table and returns its index.
    ///
    /// # Errors
    ///
    /// As [`RawXer::set_row`].
    pub fn push_row(&mut self, table: usize, cells: &[String]) -> Result<usize, EditError> {
        self.check_cells(cells)?;
        if table >= self.tables.len() {
            return Err(EditError::NoSuchTable(table));
        }
        self.start_editing();
        let rows = &mut self.tables[table].rows;
        let cells = Cells::new(cells);
        rows.push(RawRow { cells, line: 0 });
        Ok(rows.len() - 1)
    }

    /// Appends a table (written before `%E`) and returns its index.
    ///
    /// # Errors
    ///
    /// [`EditError`] if the table name is empty, a name contains a tab,
    /// line break or unencodable character, or there are too many fields or tables.
    pub fn push_table(&mut self, name: &str, fields: Vec<String>) -> Result<usize, EditError> {
        if self.tables.len() >= MAX_TABLES {
            return Err(EditError::InvalidValue("too many tables"));
        }
        if name.is_empty() {
            return Err(EditError::InvalidValue("table name is empty"));
        }
        check_name(name)?;
        encode(self.encoding, name)?;
        if fields.len() > MAX_FIELDS {
            return Err(EditError::TooManyCells);
        }
        for f in &fields {
            check_name(f)?;
            encode(self.encoding, f)?;
        }
        self.start_editing();
        self.tables.push(RawTable {
            name: name.to_owned(),
            fields,
            rows: Vec::new(),
            line: 0,
        });
        Ok(self.tables.len() - 1)
    }

    /// Removes a row and returns it. Rows after it in the same table move up
    /// one index. A removed row's bytes are left out of the output; nothing
    /// else changes. To remove many rows, use [`RawXer::remove_rows`].
    ///
    /// # Errors
    ///
    /// [`EditError::NoSuchTable`] or [`EditError::NoSuchRow`].
    pub fn remove_row(&mut self, table: usize, row: usize) -> Result<RawRow, EditError> {
        let removed = self
            .tables
            .get(table)
            .ok_or(EditError::NoSuchTable(table))?
            .rows
            .get(row)
            .ok_or(EditError::NoSuchRow { table, row })?
            .clone();
        self.remove_rows([(table, row)])?;
        Ok(removed)
    }

    /// Removes a set of `(table, row)` rows in one pass. Indices refer to the
    /// rows before any of them is removed; later rows move up. Removed rows'
    /// bytes are left out of the output; nothing else changes.
    ///
    /// # Errors
    ///
    /// [`EditError::NoSuchTable`] or [`EditError::NoSuchRow`]; nothing is
    /// removed then.
    pub fn remove_rows(
        &mut self,
        rows: impl IntoIterator<Item = (usize, usize)>,
    ) -> Result<(), EditError> {
        let rows: BTreeSet<(usize, usize)> = rows.into_iter().collect();
        for &(table, row) in &rows {
            let t = self
                .tables
                .get(table)
                .ok_or(EditError::NoSuchTable(table))?;
            if row >= t.rows.len() {
                return Err(EditError::NoSuchRow { table, row });
            }
        }
        if rows.is_empty() {
            return Ok(());
        }
        self.start_editing();
        // Original rows being removed, sorted, per table.
        let mut gone: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for &(table, row) in &rows {
            if table < self.edits.original_tables && row < self.edits.original_rows[table] {
                gone.entry(table).or_default().push(row);
            }
        }
        let shifted = |table: usize, row: usize| -> Option<usize> {
            match gone.get(&table) {
                None => Some(row),
                Some(g) => match g.binary_search(&row) {
                    Ok(_) => None,
                    Err(below) => Some(row - below),
                },
            }
        };
        for segment in &mut self.segments {
            if let SegmentKind::Row(t, r) = segment.kind {
                segment.kind =
                    shifted(t, r).map_or(SegmentKind::Removed, |r| SegmentKind::Row(t, r));
            }
        }
        let cells = std::mem::take(&mut self.edits.cells);
        self.edits.cells = cells
            .into_iter()
            .filter_map(|((t, r), v)| shifted(t, r).map(|r| ((t, r), v)))
            .collect();
        for (table, g) in &gone {
            self.edits.original_rows[*table] -= g.len();
        }
        for (table, kept) in self.tables.iter_mut().enumerate() {
            let first = rows.range((table, 0)..=(table, usize::MAX));
            let mut removed = first.map(|&(_, row)| row).peekable();
            let mut row = 0;
            kept.rows.retain(|_| {
                let gone = removed.next_if_eq(&row).is_some();
                row += 1;
                !gone
            });
        }
        Ok(())
    }

    /// True if anything has been edited since reading.
    #[must_use]
    pub fn is_edited(&self) -> bool {
        self.edits.started
    }

    /// Writes the file. With no edits, the output equals [`RawXer::source`].
    /// Cells are written one at a time, so put a buffer in front of a file.
    ///
    /// # Errors
    ///
    /// Any error from `out`.
    pub fn write(&self, mut out: impl Write) -> io::Result<()> {
        let mut w = Writer {
            out: &mut out,
            at_line_start: true,
        };
        if !self.edits.started {
            return w.raw(&self.source);
        }
        let original_tables = self.edits.original_tables;
        // The last segment of each original table: added rows go after it.
        let mut anchor = vec![None; original_tables];
        let mut current_table = None;
        for (i, s) in self.segments.iter().enumerate() {
            match s.kind {
                SegmentKind::TableName(t) | SegmentKind::Fields(t) | SegmentKind::Row(t, _) => {
                    current_table = Some(t);
                    anchor[t] = Some(i);
                }
                SegmentKind::Stray => {
                    if let Some(t) = current_table {
                        anchor[t] = Some(i);
                    }
                }
                _ => {}
            }
        }
        let anchors: BTreeMap<_, _> = anchor
            .into_iter()
            .enumerate()
            .filter_map(|(table, segment)| segment.map(|segment| (segment, table)))
            .collect();
        let mut wrote_new_tables = false;
        for (i, s) in self.segments.iter().enumerate() {
            let bytes = &self.source[s.span.clone()];
            match s.kind {
                SegmentKind::Row(t, r) if self.edits.cells.contains_key(&(t, r)) => {
                    self.write_edited_row(&mut w, t, r, bytes)?;
                }
                SegmentKind::End => {
                    self.write_new_tables(&mut w)?;
                    wrote_new_tables = true;
                    w.raw(bytes)?;
                }
                SegmentKind::Removed => {}
                _ => w.raw(bytes)?,
            }
            if let Some(&t) = anchors.get(&i) {
                for row in &self.tables[t].rows[self.edits.original_rows[t]..] {
                    self.write_new_row(&mut w, row)?;
                }
            }
        }
        if !wrote_new_tables {
            self.write_new_tables(&mut w)?;
        }
        Ok(())
    }

    fn write_edited_row(
        &self,
        w: &mut Writer<'_>,
        t: usize,
        r: usize,
        original: &[u8],
    ) -> io::Result<()> {
        let edited = &self.edits.cells[&(t, r)];
        let terminator_len = if original.ends_with(b"\r\n") {
            2
        } else {
            usize::from(original.ends_with(b"\n"))
        };
        let (content, terminator) = original.split_at(original.len() - terminator_len);
        // `content` is "%R" followed by "\t<cell>" for each cell.
        let old: Vec<&[u8]> = match content.get(2..) {
            Some([b'\t', rest @ ..]) => rest.split(|&b| b == b'\t').collect(),
            _ => Vec::new(),
        };
        w.raw(b"%R")?;
        for (i, cell) in self.tables[t].rows[r].cells.iter().enumerate() {
            w.raw(b"\t")?;
            match old.get(i) {
                Some(bytes) if !edited.contains(&i) => w.raw(bytes)?,
                _ => w.raw(&self.encoded(cell)?)?,
            }
        }
        if terminator.is_empty() {
            Ok(())
        } else {
            w.raw(terminator)
        }
    }

    fn write_new_row(&self, w: &mut Writer<'_>, row: &RawRow) -> io::Result<()> {
        w.start_line(self.line_ending)?;
        w.raw(b"%R")?;
        for cell in &row.cells {
            w.raw(b"\t")?;
            w.raw(&self.encoded(cell)?)?;
        }
        w.raw(self.line_ending.as_bytes())
    }

    fn write_new_tables(&self, w: &mut Writer<'_>) -> io::Result<()> {
        let eol = self.line_ending.as_bytes();
        for table in &self.tables[self.edits.original_tables..] {
            w.start_line(self.line_ending)?;
            w.raw(b"%T\t")?;
            w.raw(&self.encoded(&table.name)?)?;
            w.raw(eol)?;
            w.raw(b"%F")?;
            for f in &table.fields {
                w.raw(b"\t")?;
                w.raw(&self.encoded(f)?)?;
            }
            w.raw(eol)?;
            for row in &table.rows {
                self.write_new_row(w, row)?;
            }
        }
        Ok(())
    }

    /// Encode without a lossy fallback, even if encoding changed after editing.
    fn encoded(&self, text: &str) -> io::Result<Vec<u8>> {
        encode(self.encoding, text).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    fn start_editing(&mut self) {
        if !self.edits.started {
            self.edits.started = true;
            self.edits.original_tables = self.tables.len();
            self.edits.original_rows = self.tables.iter().map(|t| t.rows.len()).collect();
        }
    }

    fn mark(&mut self, table: usize, row: usize, field: usize) {
        // Rows added after reading are written whole, so they need no marks.
        let original = table < self.edits.original_tables && row < self.edits.original_rows[table];
        if original {
            self.edits
                .cells
                .entry((table, row))
                .or_default()
                .insert(field);
        }
    }

    fn check_cells(&self, cells: &[String]) -> Result<(), EditError> {
        if cells.len() > MAX_FIELDS {
            return Err(EditError::TooManyCells);
        }
        if cells.iter().map(String::len).sum::<usize>() > MAX_ROW_TEXT {
            return Err(EditError::RowTooLong);
        }
        cells.iter().try_for_each(|c| self.check_value(c))
    }

    /// A cell value must read back as exactly itself.
    fn check_value(&self, value: &str) -> Result<(), EditError> {
        if value.contains('\t') {
            return Err(EditError::InvalidValue("cell values can't contain tabs"));
        }
        if value.ends_with(['\n', '\r']) {
            return Err(EditError::InvalidValue(
                "cell values can't end with a line break",
            ));
        }
        if value
            .split('\n')
            .skip(1)
            .any(|line| marker(line.as_bytes()).is_some())
        {
            return Err(EditError::InvalidValue(
                "a line inside a cell value can't start with an XER record marker",
            ));
        }
        encode(self.encoding, value).map(|_| ())
    }
}

/// Table and field names, and header fields, are single-line and tab-free.
fn check_name(name: &str) -> Result<(), EditError> {
    if name.contains(['\t', '\n', '\r']) {
        Err(EditError::InvalidValue(
            "names can't contain tabs or line breaks",
        ))
    } else {
        Ok(())
    }
}

fn encode(encoding: Encoding, text: &str) -> Result<Vec<u8>, EditError> {
    match encoding {
        Encoding::Utf8 { .. } => Ok(text.as_bytes().to_vec()),
        Encoding::Windows1252 => {
            let (bytes, _, unmappable) = WINDOWS_1252.encode(text);
            if unmappable {
                Err(EditError::Unencodable {
                    value: text.chars().take(40).collect(),
                })
            } else {
                Ok(bytes.into_owned())
            }
        }
    }
}

struct Writer<'a> {
    out: &'a mut dyn Write,
    at_line_start: bool,
}

impl Writer<'_> {
    fn raw(&mut self, bytes: &[u8]) -> io::Result<()> {
        if let Some(&last) = bytes.last() {
            self.at_line_start = last == b'\n';
        }
        self.out.write_all(bytes)
    }

    /// Ends the current line first if the output stopped mid-line (a file
    /// with no final newline).
    fn start_line(&mut self, eol: LineEnding) -> io::Result<()> {
        if self.at_line_start {
            Ok(())
        } else {
            self.raw(eol.as_bytes())
        }
    }
}
