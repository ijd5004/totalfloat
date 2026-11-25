//! Line- and cell-level comparison of two XER byte streams, for tests and
//! review tools. Not used for writing.

/// The lines that differ between two files.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LineDiff {
    /// Lines only in the old file: 0-based line index and text (no terminator).
    pub removed: Vec<(usize, String)>,
    /// Lines only in the new file: 0-based line index and text.
    pub added: Vec<(usize, String)>,
}

impl LineDiff {
    /// True if the files have the same lines.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.removed.is_empty() && self.added.is_empty()
    }
}

/// Largest LCS table [`diff_lines`] builds (about 32 MB of `usize`).
const MAX_LCS_CELLS: usize = 4_000_000;

/// Lines as bytes, split on `\n` with any `\r` kept so line-ending changes show
/// up as differences. Bytes, because a lossy text form can make different lines equal.
fn lines(bytes: &[u8]) -> Vec<&[u8]> {
    bytes.split(|&b| b == b'\n').collect()
}

fn text(line: &[u8]) -> String {
    String::from_utf8_lossy(line).into_owned()
}

/// A minimal line diff (longest common subsequence after trimming the common
/// prefix and suffix). Meant for files that differ in a few places; the
/// differing middle is compared in `O(n·m)`; if that would exceed
/// [`MAX_LCS_CELLS`], the whole middle is reported as removed and re-added.
#[must_use]
pub fn diff_lines(old: &[u8], new: &[u8]) -> LineDiff {
    let (a, b) = (lines(old), lines(new));
    let prefix = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    let suffix = a[prefix..]
        .iter()
        .rev()
        .zip(b[prefix..].iter().rev())
        .take_while(|(x, y)| x == y)
        .count();
    let (am, bm) = (&a[prefix..a.len() - suffix], &b[prefix..b.len() - suffix]);
    let mut diff = LineDiff::default();
    // Bound memory: a huge differing middle is reported as fully replaced.
    if (am.len() + 1).saturating_mul(bm.len() + 1) > MAX_LCS_CELLS {
        diff.removed = am
            .iter()
            .enumerate()
            .map(|(i, l)| (prefix + i, text(l)))
            .collect();
        diff.added = bm
            .iter()
            .enumerate()
            .map(|(j, l)| (prefix + j, text(l)))
            .collect();
        return diff;
    }
    // lcs[i][j] = LCS length of am[i..] and bm[j..].
    let mut lcs = vec![vec![0usize; bm.len() + 1]; am.len() + 1];
    for i in (0..am.len()).rev() {
        for j in (0..bm.len()).rev() {
            lcs[i][j] = if am[i] == bm[j] {
                lcs[i + 1][j + 1] + 1
            } else {
                lcs[i + 1][j].max(lcs[i][j + 1])
            };
        }
    }
    let (mut i, mut j) = (0, 0);
    while i < am.len() || j < bm.len() {
        if i < am.len() && j < bm.len() && am[i] == bm[j] {
            i += 1;
            j += 1;
        } else if j < bm.len() && (i == am.len() || lcs[i][j + 1] >= lcs[i + 1][j]) {
            diff.added.push((prefix + j, text(bm[j])));
            j += 1;
        } else {
            diff.removed.push((prefix + i, text(am[i])));
            i += 1;
        }
    }
    diff
}

/// Indexes of tab-separated cells that differ between two lines (a cell
/// present in only one line counts as different).
#[must_use]
pub fn changed_cells(old: &str, new: &str) -> Vec<usize> {
    let (a, b): (Vec<&str>, Vec<&str>) = (old.split('\t').collect(), new.split('\t').collect());
    (0..a.len().max(b.len()))
        .filter(|&i| a.get(i) != b.get(i))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_changed_added_and_removed_lines() {
        let old = b"a\nb\nc\nd\n";
        assert!(diff_lines(old, old).is_empty());
        let d = diff_lines(old, b"a\nB\nc\nd\nE\n");
        assert_eq!(d.removed, [(1, "b".to_owned())]);
        assert_eq!(d.added, [(1, "B".to_owned()), (4, "E".to_owned())]);
        let d = diff_lines(old, b"a\nd\n");
        assert_eq!(d.removed, [(1, "b".to_owned()), (2, "c".to_owned())]);
        assert_eq!(d.added, []);
    }

    #[test]
    fn lines_differing_only_in_invalid_bytes_are_different() {
        let d = diff_lines(b"a\xff\n", b"a\xfe\n");
        assert_eq!(d.removed.len(), 1);
        assert_eq!(d.added.len(), 1);
    }

    #[test]
    fn a_huge_one_sided_middle_stays_inside_the_bound() {
        let old = "x\n".repeat(5_000_000);
        let d = diff_lines(old.as_bytes(), b"");
        assert_eq!(d.removed.len(), 5_000_000);
    }

    #[test]
    fn finds_changed_cells() {
        assert_eq!(changed_cells("%R\t1\tx\t3", "%R\t1\ty\t3"), [2]);
        assert_eq!(changed_cells("%R\t1", "%R\t1\t2"), [2]);
        assert_eq!(changed_cells("%R\t1", "%R\t1"), Vec::<usize>::new());
    }
}
