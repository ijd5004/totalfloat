//! Layer 1 of the XER codec: a lossless view of the file's bytes (ARCHITECTURE.md, Lossless round trips).
//!
//! An XER file is a header line (`ERMHDR`, tab-separated) followed by tables.
//! Each table is a `%T` line naming it, a `%F` line listing its fields, and
//! `%R` lines holding rows; the file ends with `%E`. Cells are separated by
//! tabs. A cell may contain line breaks (P6 writes multi-line calendar data
//! and text this way), so one row can span several physical lines: a line
//! that doesn't start with a record marker continues the row before it.
//!
//! [`read`] keeps the original bytes and records, in file order, which byte
//! range every part of the file came from. Concatenating those ranges
//! reproduces the input exactly, which is what lets the writer emit
//! untouched files byte for byte. Cells are also decoded, into [`Cells`], for
//! the typed layer.
//!
//! The reader never panics on any input. A file that isn't an XER at all is
//! an [`Err`]; anything odd inside an XER (ragged rows, a missing `%E`, stray
//! lines) is kept as-is and reported in [`RawXer::diagnostics`].

use std::borrow::Cow;
use std::fmt;
use std::ops::Range;
use std::sync::Arc;

use encoding_rs::WINDOWS_1252;

/// Longest line accepted, in bytes; also the limit for a row spanning several
/// lines. Real files stay far below this; the limit protects against hostile
/// input.
pub const MAX_LINE_BYTES: usize = 64 * 1024 * 1024;
/// Most tables accepted in one file.
pub const MAX_TABLES: usize = 10_000;
/// Most fields accepted in one table, and most cells in one row.
pub const MAX_FIELDS: usize = 10_000;

const BOM: &[u8] = b"\xEF\xBB\xBF";

/// A byte range in the original file.
pub type Span = Range<usize>;

/// Text encoding of an XER file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Encoding {
    /// UTF-8; `bom` records whether the file starts with a byte-order mark.
    Utf8 {
        /// The file starts with `EF BB BF`.
        bom: bool,
    },
    /// Windows-1252, the usual encoding of P6 exports. Chosen whenever the
    /// file is not valid UTF-8, or is pure ASCII.
    Windows1252,
}

/// Line terminator style.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineEnding {
    /// `\r\n`.
    CrLf,
    /// `\n`.
    Lf,
}

impl LineEnding {
    /// The terminator's bytes.
    #[must_use]
    pub const fn as_bytes(self) -> &'static [u8] {
        match self {
            Self::CrLf => b"\r\n",
            Self::Lf => b"\n",
        }
    }
}

/// The `ERMHDR` line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    /// Tab-separated fields after `ERMHDR`, decoded. Typically: export
    /// version, export date, `Project`, user name, user display name, database
    /// name, module, currency.
    pub fields: Vec<String>,
    /// 1-based line number.
    pub line: usize,
}

impl Header {
    /// The version in the file's header (first field, e.g. `23.12`), if present.
    #[must_use]
    pub fn version(&self) -> Option<&str> {
        self.fields.first().map(String::as_str)
    }
}

/// One table: its name, field names and rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawTable {
    /// Table name from the `%T` line, e.g. `TASK`.
    pub name: String,
    /// Field names from the `%F` line, in file order. Empty if the table has
    /// no `%F` line.
    pub fields: Vec<String>,
    /// Rows in file order.
    pub rows: Vec<RawRow>,
    /// 1-based line number of the `%T` line.
    pub line: usize,
}

impl RawTable {
    /// Position of `field` in [`RawTable::fields`].
    #[must_use]
    pub fn field_index(&self, field: &str) -> Option<usize> {
        self.fields.iter().position(|f| f == field)
    }
}

/// One `%R` row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawRow {
    /// Decoded cells. Usually as many as the table has fields; ragged rows are
    /// kept as they are and reported in the diagnostics.
    pub cells: Cells,
    /// 1-based line number of the `%R` line; 0 for rows added after reading.
    pub line: usize,
}

impl RawRow {
    /// The cell at `index`, or `None` past the end of a short row.
    #[must_use]
    pub fn get(&self, index: usize) -> Option<&str> {
        self.cells.get(index)
    }
}

/// Which part of the file a [`Segment`] holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SegmentKind {
    /// The UTF-8 byte-order mark.
    Bom,
    /// The `ERMHDR` line.
    Header,
    /// A `%T` line; the index is into [`RawXer::tables`].
    TableName(usize),
    /// A `%F` line.
    Fields(usize),
    /// A `%R` row (with any continuation lines): table index, row index.
    Row(usize, usize),
    /// The `%E` line.
    End,
    /// A line the reader didn't recognize, kept verbatim.
    Stray,
    /// Bytes after `%E`.
    Trailing,
    /// A row removed by [`RawXer::remove_row`]; its bytes are not written.
    Removed,
}

/// A contiguous part of the original file. A file's segments, in order,
/// cover every byte exactly once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    /// What the bytes are.
    pub kind: SegmentKind,
    /// Where they are, including the line terminator.
    pub span: Span,
}

/// Something unusual in an XER that the reader kept and reported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// 1-based line number.
    pub line: usize,
    /// What kind of problem.
    pub kind: DiagnosticKind,
    /// A human-readable description.
    pub message: String,
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {} ({:?})", self.line, self.message, self.kind)
    }
}

/// Kinds of [`Diagnostic`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticKind {
    /// A row's cell count differs from its table's field count.
    RaggedRow,
    /// The file has no `%E` line (often a truncated export).
    MissingEnd,
    /// Content after `%E`.
    TrailingContent,
    /// A line that is not a record and doesn't continue a row.
    StrayLine,
    /// A `%F` or `%R` line before any `%T`, or a second `%F` in a table.
    OutOfPlace,
    /// Both `\r\n` and bare `\n` terminate lines.
    MixedLineEndings,
    /// Invalid UTF-8 after a UTF-8 byte-order mark; decoded with replacement
    /// characters.
    InvalidUtf8,
    /// No byte-order mark and not valid UTF-8, yet the file contains valid
    /// UTF-8 sequences. Real P6 exports do this (TF-009): the
    /// file is decoded as Windows-1252, so the UTF-8 parts read as mojibake.
    /// The original bytes are kept either way.
    MixedEncoding,
    /// More than the diagnostic limit; the last diagnostic counts the rest.
    TooManyDiagnostics,
}

/// Why a file could not be read as an XER at all.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ReadError {
    /// The input is empty.
    #[error("the file is empty")]
    Empty,
    /// The first line isn't an `ERMHDR` header.
    #[error("not an XER file: the first line does not start with ERMHDR")]
    NotXer,
    /// A line exceeds [`MAX_LINE_BYTES`].
    #[error("line {line} is longer than {MAX_LINE_BYTES} bytes")]
    LineTooLong {
        /// 1-based line number.
        line: usize,
    },
    /// More than [`MAX_TABLES`] tables.
    #[error("line {line}: more than {MAX_TABLES} tables")]
    TooManyTables {
        /// 1-based line number.
        line: usize,
    },
    /// A `%F` or `%R` record with more than [`MAX_FIELDS`] fields or cells.
    #[error("line {line}: more than {MAX_FIELDS} fields or cells")]
    TooManyFields {
        /// 1-based line number.
        line: usize,
    },
}

/// A parsed XER file that remembers its original bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawXer {
    /// The detected encoding.
    pub encoding: Encoding,
    /// The line ending of the first line; used for new rows.
    pub line_ending: LineEnding,
    /// The `ERMHDR` line.
    pub header: Header,
    /// Problems found while reading.
    pub diagnostics: Vec<Diagnostic>,
    tables: Vec<RawTable>,
    /// Shared, so patching a copy doesn't copy the file.
    source: Arc<Vec<u8>>,
    segments: Vec<Segment>,
    edits: edit::Edits,
}

impl RawXer {
    /// Tables in file order, followed by any added with
    /// [`RawXer::push_table`]. Read-only: change content through the edit
    /// methods so the writer knows what changed.
    #[must_use]
    pub fn tables(&self) -> &[RawTable] {
        &self.tables
    }

    /// The first table named `name`.
    #[must_use]
    pub fn table(&self, name: &str) -> Option<&RawTable> {
        self.tables.iter().find(|t| t.name == name)
    }

    /// Index of the first table named `name`.
    #[must_use]
    pub fn table_index(&self, name: &str) -> Option<usize> {
        self.tables.iter().position(|t| t.name == name)
    }

    /// The bytes the file was read from (before any edits).
    #[must_use]
    pub fn source(&self) -> &[u8] {
        &self.source
    }

    /// The file's parts in order; their spans cover [`RawXer::source`] exactly.
    #[must_use]
    pub fn segments(&self) -> &[Segment] {
        &self.segments
    }

    /// True if the file ends with a line terminator.
    #[must_use]
    pub fn ends_with_newline(&self) -> bool {
        self.source.ends_with(b"\n")
    }
}

/// Parses an XER file, taking ownership of its bytes (pass a `Vec<u8>` to
/// avoid a copy).
///
/// # Errors
///
/// Returns [`ReadError`] if the input is empty, doesn't start with an
/// `ERMHDR` line (after an optional UTF-8 byte-order mark), or exceeds the
/// size limits. Everything else is accepted and reported in
/// [`RawXer::diagnostics`].
pub fn read(bytes: impl Into<Vec<u8>>) -> Result<RawXer, ReadError> {
    let bytes: Vec<u8> = bytes.into();
    if bytes.is_empty() {
        return Err(ReadError::Empty);
    }
    let bom = bytes.starts_with(BOM);
    let body_start = if bom { BOM.len() } else { 0 };
    if !bytes[body_start..].starts_with(b"ERMHDR") {
        return Err(ReadError::NotXer);
    }
    let utf8 = std::str::from_utf8(&bytes).is_ok();
    // Pure ASCII reads the same either way, so it takes the usual encoding of
    // P6 exports, which decides how an edit writes text beyond ASCII (TF-009).
    let encoding = if bom || (utf8 && !bytes.is_ascii()) {
        Encoding::Utf8 { bom }
    } else {
        Encoding::Windows1252
    };
    let mut reader = Reader::new(&bytes, encoding, body_start);
    if !bom && !utf8 && has_utf8_multibyte(&bytes) {
        reader.diag(
            1,
            DiagnosticKind::MixedEncoding,
            "the file mixes Windows-1252 bytes with valid UTF-8 sequences (P6 exports \
             sometimes do; see TF-009); decoded as Windows-1252, so the UTF-8 parts \
             read as mojibake"
                .to_owned(),
        );
    }
    let mut xer = reader.run()?;
    xer.source = Arc::new(bytes);
    Ok(xer)
}

/// True if `bytes` contains at least one valid multi-byte UTF-8 sequence.
fn has_utf8_multibyte(bytes: &[u8]) -> bool {
    bytes.utf8_chunks().any(|chunk| !chunk.valid().is_ascii())
}

/// Most diagnostics kept; further problems are counted in one final entry.
const MAX_DIAGNOSTICS: usize = 1000;

/// One physical line: content (without terminator) and full span.
struct Line {
    number: usize,
    content: Span,
    span: Span,
}

/// The tabs in a line, or an error if it holds too many fields. The leading
/// tab after a marker adds one empty cell.
#[allow(clippy::naive_bytecount, reason = "not worth a dependency")]
fn count_tabs(bytes: &[u8], line: usize) -> Result<usize, ReadError> {
    let tabs = bytes.iter().filter(|&&b| b == b'\t').count();
    if tabs > MAX_FIELDS {
        return Err(ReadError::TooManyFields { line });
    }
    Ok(tabs)
}

struct Reader<'a> {
    bytes: &'a [u8],
    encoding: Encoding,
    pos: usize,
    line_number: usize,
    tables: Vec<RawTable>,
    segments: Vec<Segment>,
    diagnostics: Vec<Diagnostic>,
    suppressed: usize,
    has_fields: Vec<bool>,
    reported_invalid_utf8: bool,
    reported_trailing: bool,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8], encoding: Encoding, body_start: usize) -> Self {
        let mut segments = Vec::new();
        if body_start > 0 {
            segments.push(Segment {
                kind: SegmentKind::Bom,
                span: 0..body_start,
            });
        }
        Self {
            bytes,
            encoding,
            pos: body_start,
            line_number: 0,
            tables: Vec::new(),
            segments,
            diagnostics: Vec::new(),
            suppressed: 0,
            has_fields: Vec::new(),
            reported_invalid_utf8: false,
            reported_trailing: false,
        }
    }

    /// Parses everything after the BOM. The returned `source` is empty; the
    /// caller moves the bytes in.
    fn run(mut self) -> Result<RawXer, ReadError> {
        let first = self.next_line()?.ok_or(ReadError::NotXer)?;
        let line_ending = if self.bytes[first.content.end..first.span.end].starts_with(b"\r") {
            LineEnding::CrLf
        } else {
            LineEnding::Lf
        };
        let header = Header {
            fields: self
                .cells(first.content.start..first.content.end, first.number)?
                .into_iter()
                .skip(1)
                .collect(),
            line: first.number,
        };
        self.segments.push(Segment {
            kind: SegmentKind::Header,
            span: first.span,
        });

        let mut ended = false;
        while let Some(line) = self.next_line()? {
            if ended {
                self.trailing(&line);
                continue;
            }
            match marker(&self.bytes[line.span.clone()]) {
                Some(Marker::Table) => self.table(&line)?,
                Some(Marker::Fields) => self.fields(&line)?,
                Some(Marker::Row) => self.row(&line)?,
                Some(Marker::End) => {
                    self.segments.push(Segment {
                        kind: SegmentKind::End,
                        span: line.span,
                    });
                    ended = true;
                }
                None => self.stray(&line),
            }
        }
        if !ended {
            self.diag(
                self.line_number,
                DiagnosticKind::MissingEnd,
                "the file has no %E line; it may be truncated".to_owned(),
            );
        }
        self.check_line_endings();
        self.diagnostics.sort_by_key(|d| d.line);
        if self.suppressed > 0 {
            self.diagnostics.push(Diagnostic {
                line: self.line_number,
                kind: DiagnosticKind::TooManyDiagnostics,
                message: format!("{} more problems were not reported", self.suppressed),
            });
        }
        Ok(RawXer {
            encoding: self.encoding,
            line_ending,
            header,
            tables: self.tables,
            diagnostics: self.diagnostics,
            source: Arc::default(),
            segments: self.segments,
            edits: edit::Edits::default(),
        })
    }

    /// Reports mixed terminators. Only record terminators count: line breaks
    /// inside multi-line cells are content.
    fn check_line_endings(&mut self) {
        let (mut crlf, mut lf) = (0usize, 0usize);
        for s in &self.segments {
            let bytes = &self.bytes[s.span.clone()];
            if bytes.ends_with(b"\r\n") {
                crlf += 1;
            } else if bytes.ends_with(b"\n") {
                lf += 1;
            }
        }
        if crlf > 0 && lf > 0 {
            self.diag(
                1,
                DiagnosticKind::MixedLineEndings,
                format!("{crlf} records end with CRLF and {lf} with LF"),
            );
        }
    }

    /// The next physical line, or `None` at end of input.
    fn next_line(&mut self) -> Result<Option<Line>, ReadError> {
        if self.pos >= self.bytes.len() {
            return Ok(None);
        }
        self.line_number += 1;
        let start = self.pos;
        let rest = &self.bytes[start..];
        let (content_end, end) = match rest.iter().position(|&b| b == b'\n') {
            Some(i) => {
                let crlf = i > 0 && rest[i - 1] == b'\r';
                (start + i - usize::from(crlf), start + i + 1)
            }
            None => (self.bytes.len(), self.bytes.len()),
        };
        if end - start > MAX_LINE_BYTES {
            return Err(ReadError::LineTooLong {
                line: self.line_number,
            });
        }
        self.pos = end;
        Ok(Some(Line {
            number: self.line_number,
            content: start..content_end,
            span: start..end,
        }))
    }

    fn table(&mut self, line: &Line) -> Result<(), ReadError> {
        if self.tables.len() >= MAX_TABLES {
            return Err(ReadError::TooManyTables { line: line.number });
        }
        let name = self
            .cells(line.content.start + 2..line.content.end, line.number)?
            .into_iter()
            .find(|c| !c.is_empty())
            .unwrap_or_default();
        self.segments.push(Segment {
            kind: SegmentKind::TableName(self.tables.len()),
            span: line.span.clone(),
        });
        self.tables.push(RawTable {
            name,
            fields: Vec::new(),
            rows: Vec::new(),
            line: line.number,
        });
        self.has_fields.push(false);
        Ok(())
    }

    fn fields(&mut self, line: &Line) -> Result<(), ReadError> {
        let Some(t) = self.tables.len().checked_sub(1) else {
            self.out_of_place(line, "%F line before any %T line");
            return Ok(());
        };
        if self.has_fields[t] {
            self.out_of_place(line, "second %F line in a table");
            return Ok(());
        }
        let fields = self.cells(line.content.start + 2..line.content.end, line.number)?;
        self.tables[t].fields = fields.into_iter().skip(1).collect();
        self.has_fields[t] = true;
        self.segments.push(Segment {
            kind: SegmentKind::Fields(t),
            span: line.span.clone(),
        });
        Ok(())
    }

    fn row(&mut self, first: &Line) -> Result<(), ReadError> {
        let Some(t) = self.tables.len().checked_sub(1) else {
            self.out_of_place(first, "%R line before any %T line");
            return Ok(());
        };
        // Absorb continuation lines (multi-line cells) up to the next record.
        let mut content_end = first.content.end;
        let mut span_end = first.span.end;
        // Blank lines before this offset are known to be inside the row, so a
        // long run is scanned once.
        let mut content_at = 0;
        loop {
            if span_end >= content_at {
                let Some(at) = self.row_content_after(span_end) else {
                    break;
                };
                content_at = at;
            }
            let Some(next) = self.next_line()? else {
                break;
            };
            content_end = next.content.end;
            span_end = next.span.end;
            if span_end - first.span.start > MAX_LINE_BYTES {
                return Err(ReadError::LineTooLong { line: first.number });
            }
        }
        let cells = self.row_cells(first.content.start + 2..content_end, first.number)?;
        let expected = self.tables[t].fields.len();
        if cells.len() != expected {
            let name = self.tables[t].name.clone();
            self.diag(
                first.number,
                DiagnosticKind::RaggedRow,
                format!(
                    "{name} row has {} cells but the table has {expected} fields",
                    cells.len()
                ),
            );
        }
        let r = self.tables[t].rows.len();
        self.tables[t].rows.push(RawRow {
            cells,
            line: first.number,
        });
        self.segments.push(Segment {
            kind: SegmentKind::Row(t, r),
            span: first.span.start..span_end,
        });
        Ok(())
    }

    /// Where the current row continues, if the line starting at `at` is part
    /// of it: the offset of the first nonblank line, provided it isn't a record
    /// marker. Blank lines just before a record or the end of the file are
    /// separators, not cell content.
    fn row_content_after(&self, mut at: usize) -> Option<usize> {
        loop {
            let rest = &self.bytes[at..];
            if rest.is_empty() || marker(rest).is_some() {
                return None;
            }
            at += if rest.starts_with(b"\r\n") {
                2
            } else if rest.starts_with(b"\n") {
                1
            } else {
                return Some(at);
            };
        }
    }

    fn stray(&mut self, line: &Line) {
        if !line.content.is_empty() {
            self.diag(
                line.number,
                DiagnosticKind::StrayLine,
                "line is not an XER record; kept as-is".to_owned(),
            );
        }
        self.push_stray(line.span.clone());
    }

    fn out_of_place(&mut self, line: &Line, what: &str) {
        self.diag(
            line.number,
            DiagnosticKind::OutOfPlace,
            format!("{what}; kept as-is"),
        );
        self.push_stray(line.span.clone());
    }

    /// Consecutive stray lines share one segment.
    fn push_stray(&mut self, span: Span) {
        match self.segments.last_mut() {
            Some(last) if last.kind == SegmentKind::Stray => last.span.end = span.end,
            _ => self.segments.push(Segment {
                kind: SegmentKind::Stray,
                span,
            }),
        }
    }

    /// Bytes after `%E` form one trailing segment. Blank lines are normal;
    /// the first line with content is reported.
    fn trailing(&mut self, line: &Line) {
        let blank = self.bytes[line.span.clone()]
            .iter()
            .all(u8::is_ascii_whitespace);
        if !blank && !self.reported_trailing {
            self.reported_trailing = true;
            self.diag(
                line.number,
                DiagnosticKind::TrailingContent,
                "content after %E; kept as-is".to_owned(),
            );
        }
        match self.segments.last_mut() {
            Some(last) if last.kind == SegmentKind::Trailing => last.span.end = line.span.end,
            _ => self.segments.push(Segment {
                kind: SegmentKind::Trailing,
                span: line.span.clone(),
            }),
        }
    }

    /// Splits `range` on tabs and decodes each cell. Fails before allocating
    /// if there are more than [`MAX_FIELDS`] cells.
    fn cells(&mut self, range: Span, line: usize) -> Result<Vec<String>, ReadError> {
        let bytes = &self.bytes[range];
        count_tabs(bytes, line)?;
        Ok(bytes
            .split(|&b| b == b'\t')
            .map(|c| self.decode(c, line).into_owned())
            .collect())
    }

    /// A `%R` row's cells, leaving out what comes before its first tab.
    fn row_cells(&mut self, range: Span, line: usize) -> Result<Cells, ReadError> {
        let bytes = &self.bytes[range];
        let tabs = count_tabs(bytes, line)?;
        let mut cells = cells::Builder::with_capacity(bytes.len() - tabs, tabs);
        // A tab can't be part of a multibyte character, so splitting decoded
        // text gives the same cells as decoding each. Only invalid UTF-8 needs
        // the slower way, which says where the problem is.
        let text = match self.encoding {
            Encoding::Windows1252 => Some(WINDOWS_1252.decode_without_bom_handling(bytes).0),
            Encoding::Utf8 { .. } => std::str::from_utf8(bytes).ok().map(Cow::Borrowed),
        };
        if let Some(text) = text {
            text.split('\t').skip(1).for_each(|cell| cells.push(cell));
        } else {
            for (index, cell) in bytes.split(|&b| b == b'\t').enumerate() {
                let cell = self.decode(cell, line);
                if index > 0 {
                    cells.push(&cell);
                }
            }
        }
        Ok(cells.finish())
    }

    fn decode<'b>(&mut self, bytes: &'b [u8], line: usize) -> Cow<'b, str> {
        if self.encoding == Encoding::Windows1252 {
            return WINDOWS_1252.decode_without_bom_handling(bytes).0;
        }
        if let Ok(s) = std::str::from_utf8(bytes) {
            return Cow::Borrowed(s);
        }
        // Only reachable after a BOM: without one, invalid UTF-8 selects
        // Windows-1252 for the whole file. Reported once.
        if !self.reported_invalid_utf8 {
            self.reported_invalid_utf8 = true;
            self.diag(
                line,
                DiagnosticKind::InvalidUtf8,
                "invalid UTF-8 after a UTF-8 byte-order mark; decoded with replacement characters"
                    .to_owned(),
            );
        }
        String::from_utf8_lossy(bytes)
    }

    fn diag(&mut self, line: usize, kind: DiagnosticKind, message: String) {
        if self.diagnostics.len() >= MAX_DIAGNOSTICS {
            self.suppressed += 1;
            return;
        }
        self.diagnostics.push(Diagnostic {
            line,
            kind,
            message,
        });
    }
}

enum Marker {
    Table,
    Fields,
    Row,
    End,
}

/// The record marker at the start of `text`, which may run past the end of
/// the line. `%T`, `%F` and `%R` must be followed by a tab or the end of the
/// line; `%E` by nothing but whitespace on its line.
fn marker(text: &[u8]) -> Option<Marker> {
    let tag = text.get(..2)?;
    let line_end = text.iter().position(|&b| b == b'\n').unwrap_or(text.len());
    let tail = text.get(2..line_end).unwrap_or_default();
    let ends = tail.is_empty() || tail == b"\r";
    match tag {
        b"%T" if ends || tail.starts_with(b"\t") => Some(Marker::Table),
        b"%F" if ends || tail.starts_with(b"\t") => Some(Marker::Fields),
        b"%R" if ends || tail.starts_with(b"\t") => Some(Marker::Row),
        b"%E" if tail.iter().all(|b| matches!(b, b' ' | b'\t' | b'\r')) => Some(Marker::End),
        _ => None,
    }
}

mod cells;
mod edit;

pub use cells::{Cells, Iter as CellsIter};
pub use edit::EditError;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod writer_tests;
