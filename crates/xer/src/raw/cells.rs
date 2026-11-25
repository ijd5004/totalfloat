//! A row's cells, kept in one string. A schedule holds millions of cells, most
//! of them short, so a heap string for each costs several times the file.

use std::ops::{Index, Range};

/// Most text a row can hold, so its offsets fit in 32 bits. Reading can't get
/// near it: a line is limited to [`MAX_LINE_BYTES`](super::MAX_LINE_BYTES).
pub(super) const MAX_ROW_TEXT: usize = u32::MAX as usize;

/// The decoded cells of one row.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Cells {
    text: Box<str>,
    /// Where each cell ends in `text`.
    ends: Box<[u32]>,
}

/// Collects cells into [`Cells`].
pub(super) struct Builder {
    text: String,
    ends: Vec<u32>,
}

impl Builder {
    pub(super) fn with_capacity(bytes: usize, cells: usize) -> Self {
        Self {
            text: String::with_capacity(bytes),
            ends: Vec::with_capacity(cells),
        }
    }

    pub(super) fn push(&mut self, cell: &str) {
        self.text.push_str(cell);
        self.ends
            .push(u32::try_from(self.text.len()).unwrap_or(u32::MAX));
    }

    pub(super) fn finish(self) -> Cells {
        Cells {
            text: self.text.into_boxed_str(),
            ends: self.ends.into_boxed_slice(),
        }
    }
}

impl Cells {
    /// Cells from a list holding at most [`MAX_ROW_TEXT`] bytes of text.
    pub(super) fn new(cells: &[String]) -> Self {
        let text = cells.iter().map(String::len).sum();
        let mut built = Builder::with_capacity(text, cells.len());
        for cell in cells {
            built.push(cell);
        }
        built.finish()
    }

    /// Number of cells.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.ends.len()
    }

    /// True if the row has no cells.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.ends.is_empty()
    }

    /// Bytes of decoded text across all cells.
    #[must_use]
    pub const fn text_len(&self) -> usize {
        self.text.len()
    }

    fn span(&self, index: usize) -> Option<Range<usize>> {
        let end = *self.ends.get(index)? as usize;
        let start = match index.checked_sub(1) {
            Some(before) => self.ends[before] as usize,
            None => 0,
        };
        Some(start..end)
    }

    /// The cell at `index`, or `None` past the end.
    #[must_use]
    pub fn get(&self, index: usize) -> Option<&str> {
        self.text.get(self.span(index)?)
    }

    /// The cells in order.
    #[must_use]
    pub fn iter(&self) -> Iter<'_> {
        Iter {
            text: &self.text,
            ends: self.ends.iter(),
            start: 0,
        }
    }

    /// Replaces cells, adding empty ones up to the last if needed.
    pub(super) fn set(&mut self, changes: &[(usize, String)]) {
        let added: usize = changes.iter().map(|(_, value)| value.len()).sum();
        let last = changes.iter().map(|(index, _)| index + 1).max();
        let count = self.len().max(last.unwrap_or(0));
        let mut cells = Builder::with_capacity(self.text.len() + added, count);
        for at in 0..count {
            let change = changes.iter().rev().find(|(index, _)| *index == at);
            cells.push(
                change.map_or_else(|| self.get(at).unwrap_or(""), |(_, value)| value.as_str()),
            );
        }
        *self = cells.finish();
    }
}

impl Index<usize> for Cells {
    type Output = str;

    fn index(&self, index: usize) -> &str {
        match self.span(index) {
            Some(span) => &self.text[span],
            None => panic!("no cell {index} in a row of {}", self.len()),
        }
    }
}

/// Iterates a row's cells.
#[derive(Debug, Clone)]
pub struct Iter<'a> {
    text: &'a str,
    ends: std::slice::Iter<'a, u32>,
    start: usize,
}

impl<'a> Iterator for Iter<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<&'a str> {
        let end = *self.ends.next()? as usize;
        let cell = self.text.get(self.start..end);
        self.start = end;
        cell
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.ends.size_hint()
    }
}

impl<'a> IntoIterator for &'a Cells {
    type Item = &'a str;
    type IntoIter = Iter<'a>;

    fn into_iter(self) -> Iter<'a> {
        self.iter()
    }
}

impl<const N: usize> PartialEq<[&str; N]> for Cells {
    fn eq(&self, other: &[&str; N]) -> bool {
        self.len() == N && self.iter().eq(other.iter().copied())
    }
}

impl PartialEq<Vec<String>> for Cells {
    fn eq(&self, other: &Vec<String>) -> bool {
        self.len() == other.len() && self.iter().eq(other.iter().map(String::as_str))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cells(values: &[&str]) -> Cells {
        let values: Vec<String> = values.iter().map(|&v| v.to_owned()).collect();
        Cells::new(&values)
    }

    #[test]
    fn empty_cells_and_multibyte_text_keep_their_places() {
        let row = cells(&["", "é", "", "ab", ""]);
        assert_eq!(row.len(), 5);
        assert_eq!(row, ["", "é", "", "ab", ""]);
        assert_eq!(row.get(1), Some("é"));
        assert_eq!(&row[3], "ab");
        assert_eq!(row.get(5), None);
    }

    #[test]
    fn setting_a_cell_changes_only_that_cell() {
        let mut row = cells(&["a", "b", "c"]);
        row.set(&[(1, "long value".to_owned())]);
        assert_eq!(row, ["a", "long value", "c"]);
        row.set(&[(0, String::new())]);
        assert_eq!(row, ["", "long value", "c"]);
    }

    #[test]
    fn setting_past_the_end_extends_the_row_with_empty_cells() {
        let mut row = cells(&["a"]);
        row.set(&[(3, "d".to_owned())]);
        assert_eq!(row, ["a", "", "", "d"]);
    }

    #[test]
    fn the_last_change_to_a_cell_wins() {
        let mut row = cells(&["a", "b"]);
        row.set(&[
            (1, "x".to_owned()),
            (0, "y".to_owned()),
            (1, "z".to_owned()),
        ]);
        assert_eq!(row, ["y", "z"]);
    }
}
