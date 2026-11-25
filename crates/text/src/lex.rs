//! Splitting one line into words, commas and a comment.

use crate::{At, Diagnostic};

/// A word or a comma on a line.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Token {
    /// A bare word, or a quoted one with its quotes and escapes removed.
    Word { text: String, quoted: bool, at: At },
    /// A comma between predecessors.
    Comma(At),
}

impl Token {
    pub(crate) fn at(&self) -> At {
        match self {
            Self::Word { at, .. } | Self::Comma(at) => *at,
        }
    }
}

/// One line, split up.
#[derive(Debug, Default)]
pub(crate) struct Line {
    /// Whether the line starts with a space or tab.
    pub indented: bool,
    pub tokens: Vec<Token>,
    /// The text after `#`, trimmed, and where the `#` was.
    pub comment: Option<(String, At)>,
}

/// Splits a line. `number` is its 1-based line number.
pub(crate) fn line(text: &str, number: usize, file: &str) -> Result<Line, Diagnostic> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Line {
        indented: chars.first().is_some_and(|c| c.is_whitespace()),
        ..Line::default()
    };
    let at = |column: usize, length: usize| At {
        line: number,
        column: column + 1,
        length,
    };
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
        } else if c == '#' {
            let rest: String = chars[i + 1..].iter().collect();
            out.comment = Some((rest.trim().to_owned(), at(i, 1)));
            break;
        } else if c == ',' {
            out.tokens.push(Token::Comma(at(i, 1)));
            i += 1;
        } else if c == '"' {
            let start = i;
            let mut word = String::new();
            i += 1;
            loop {
                match chars.get(i) {
                    None => {
                        return Err(Diagnostic {
                            code: "unclosed-quote",
                            message: "this quote isn't closed".to_owned(),
                            hint: "end the name with \" on the same line; inside it, write \\\" for a quote, \\\\ for a backslash and \\n for a line break"
                                .to_owned(),
                            file: file.to_owned(),
                            at: at(start, chars.len() - start),
                        });
                    }
                    Some('"') => break,
                    Some('\\')
                        if matches!(chars.get(i + 1), Some('"' | '\\' | 'n' | 'r' | 't')) =>
                    {
                        word.push(match chars[i + 1] {
                            'n' => '\n',
                            'r' => '\r',
                            't' => '\t',
                            other => other,
                        });
                        i += 2;
                    }
                    Some(other) => {
                        word.push(*other);
                        i += 1;
                    }
                }
            }
            i += 1;
            out.tokens.push(Token::Word {
                text: word,
                quoted: true,
                at: at(start, i - start),
            });
        } else {
            let start = i;
            while i < chars.len()
                && !chars[i].is_whitespace()
                && !matches!(chars[i], ',' | '#' | '"')
            {
                i += 1;
            }
            out.tokens.push(Token::Word {
                text: chars[start..i].iter().collect(),
                quoted: false,
                at: at(start, i - start),
            });
        }
    }
    Ok(out)
}
