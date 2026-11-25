//! Problems found reading a schedule text file, and how they print.

use std::fmt::Write as _;

use crate::At;

/// One problem in a schedule text file, with where it is and what to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// Stable and machine-readable, such as `unknown-keyword`.
    pub code: &'static str,
    /// What's wrong.
    pub message: String,
    /// What to do, starting with `did you mean …?` when a close word exists.
    pub hint: String,
    /// The file name, as given to [`parse`](crate::parse).
    pub file: String,
    /// Where it is.
    pub at: At,
}

impl Diagnostic {
    /// Prints the problem as a compiler would: the code and message, the
    /// place, the line with the text underlined, and the hint.
    #[must_use]
    pub fn render(&self, source: &str) -> String {
        let At {
            line,
            column,
            length,
        } = self.at;
        let mut out = format!("error[{}]: {}\n", self.code, self.message);
        let number = line.to_string();
        let pad = " ".repeat(number.len());
        let _ = writeln!(out, "{pad}--> {}:{line}:{column}", self.file);
        if let Some(text) = source.lines().nth(line.saturating_sub(1)) {
            let text = text.trim_end();
            let _ = writeln!(out, "{pad} |");
            let _ = writeln!(out, "{number} | {text}");
            let lead: String = text
                .chars()
                .take(column.saturating_sub(1))
                .map(|c| if c == '\t' { '\t' } else { ' ' })
                .collect();
            let _ = writeln!(out, "{pad} | {lead}{}", "^".repeat(length.max(1)));
        }
        if !self.hint.is_empty() {
            let _ = writeln!(out, "{pad} = hint: {}", self.hint);
        }
        out
    }
}

/// Edits needed to turn one word into the other.
fn distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut previous = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let substitute = previous + usize::from(ca != *cb);
            previous = row[j + 1];
            row[j + 1] = substitute.min(row[j] + 1).min(row[j + 1] + 1);
        }
    }
    row[b.len()]
}

/// The closest of `known` to `word`, ignoring case, if it's close enough to
/// be a typo.
pub(crate) fn closest<'a>(word: &str, known: &[&'a str]) -> Option<&'a str> {
    let word = word.to_lowercase();
    let limit = (word.chars().count() / 3).max(1);
    known
        .iter()
        .map(|k| (distance(&word, &k.to_lowercase()), *k))
        .filter(|(d, _)| *d <= limit)
        .min()
        .map(|(_, k)| k)
}
