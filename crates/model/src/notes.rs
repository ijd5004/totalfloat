//! Notebooks: notes on activities and WBS nodes, each under a topic.

use crate::{NotebookTopicId, SourceRef};

/// A notebook topic: a heading notes are filed under, such as "Basis of
/// schedule". Topics belong to the whole database, so every project in a
/// file holds every one. XER: a `MEMOTYPE` row.
#[allow(
    clippy::struct_excessive_bools,
    reason = "mirrors the file's independent applies-to flags one to one"
)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotebookTopic {
    /// XER: `MEMOTYPE.memo_type_id`.
    pub id: NotebookTopicId,
    /// XER: `MEMOTYPE.memo_type`.
    pub name: String,
    /// Sort order. XER: `MEMOTYPE.seq_num`.
    pub sequence: i64,
    /// Notes on EPS nodes can use it. XER: `MEMOTYPE.eps_flag`.
    pub for_eps: bool,
    /// Notes on projects can use it. XER: `MEMOTYPE.proj_flag`.
    pub for_projects: bool,
    /// Notes on WBS nodes can use it. XER: `MEMOTYPE.wbs_flag`.
    pub for_wbs: bool,
    /// Notes on activities can use it. XER: `MEMOTYPE.task_flag`.
    pub for_activities: bool,
    /// Source row link (see [`SourceRef`]).
    pub source: Option<SourceRef>,
}

/// One note: an activity's or WBS node's text under one topic. XER: a
/// `TASKMEMO` or `WBSMEMO` row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    /// The row's own ID. XER: `TASKMEMO.memo_id` or `WBSMEMO.wbs_memo_id`.
    pub id: u64,
    /// The note as P6 keeps it: HTML. XER: `TASKMEMO.task_memo` or
    /// `WBSMEMO.wbs_memo`, which double each quote and write each line break
    /// as two DEL characters; the codec undoes both.
    pub html: String,
    /// Source row link (see [`SourceRef`]).
    pub source: Option<SourceRef>,
}

impl Note {
    /// The note as plain text: tags dropped, paragraphs and line breaks as
    /// new lines, entities decoded, runs of spaces as one, and blank lines
    /// left out.
    #[must_use]
    pub fn text(&self) -> String {
        html_to_text(&self.html)
    }

    /// A note holding `text` as simple HTML: one paragraph a line, with
    /// `&`, `<` and `>` escaped.
    #[must_use]
    pub fn html_of(text: &str) -> String {
        let mut html = String::from("<html><body>");
        for line in text.lines() {
            html.push_str("<p>");
            for c in line.chars() {
                match c {
                    '&' => html.push_str("&amp;"),
                    '<' => html.push_str("&lt;"),
                    '>' => html.push_str("&gt;"),
                    _ => html.push(c),
                }
            }
            html.push_str("</p>");
        }
        html.push_str("</body></html>");
        html
    }
}

/// Tags whose content isn't text a reader sees.
const HIDDEN: [&str; 4] = ["head", "title", "style", "script"];
/// Tags that end a line of text.
const BREAKS: [&str; 13] = [
    "br", "p", "div", "li", "tr", "h1", "h2", "h3", "h4", "h5", "h6", "table", "ul",
];

fn html_to_text(html: &str) -> String {
    // In HTML a line break in the source is a space; only tags break lines.
    // Text with no tags at all keeps its own lines.
    let joined;
    let html = if html.contains('<') {
        joined = html.replace(['\r', '\n'], " ");
        joined.as_str()
    } else {
        html
    };
    let mut text = String::new();
    let mut rest = html;
    let mut hidden: Option<String> = None;
    while let Some(at) = rest.find('<') {
        if hidden.is_none() {
            text.push_str(&rest[..at]);
        }
        let starts_tag = rest[at + 1..]
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '/' || c == '!');
        let end = rest[at..].find('>').filter(|_| starts_tag);
        let Some(end) = end else {
            // A `<` that opens no tag is text.
            if hidden.is_none() {
                text.push('<');
            }
            rest = &rest[at + 1..];
            continue;
        };
        let tag = &rest[at + 1..at + end];
        rest = &rest[at + end + 1..];
        let closing = tag.starts_with('/');
        let name: String = tag
            .trim_start_matches('/')
            .chars()
            .take_while(char::is_ascii_alphanumeric)
            .collect::<String>()
            .to_ascii_lowercase();
        if let Some(open) = &hidden {
            if closing && *open == name {
                hidden = None;
            }
            continue;
        }
        if !closing && HIDDEN.contains(&name.as_str()) && !tag.ends_with('/') {
            hidden = Some(name);
        } else if BREAKS.contains(&name.as_str()) {
            text.push('\n');
        }
    }
    if hidden.is_none() {
        text.push_str(rest);
    }
    let decoded = decode_entities(&text);
    let lines: Vec<String> = decoded
        .lines()
        .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
        .collect();
    let out: Vec<&str> = lines
        .iter()
        .map(String::as_str)
        .filter(|line| !line.is_empty())
        .collect();
    out.join("\n")
}

fn decode_entities(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        // A byte search: `;` is ASCII, so where it's found is a character
        // boundary, wherever the bound falls.
        let Some(end) = rest.as_bytes()[..rest.len().min(12)]
            .iter()
            .position(|b| *b == b';')
        else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let entity = &rest[1..end];
        let decoded = match entity {
            "nbsp" => Some(' '),
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ => entity.strip_prefix('#').and_then(|number| {
                let value = match number.strip_prefix(['x', 'X']) {
                    Some(hex) => u32::from_str_radix(hex, 16).ok(),
                    None => number.parse().ok(),
                };
                value.and_then(char::from_u32)
            }),
        };
        if let Some(c) = decoded {
            out.push(if c == '\u{a0}' { ' ' } else { c });
            rest = &rest[end + 1..];
        } else {
            out.push('&');
            rest = &rest[1..];
        }
    }
    out.push_str(rest);
    out.replace('\u{a0}', " ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(html: &str) -> Note {
        Note {
            id: 1,
            html: html.to_owned(),
            source: None,
        }
    }

    #[test]
    fn html_reads_as_text() {
        let html = "<!DOCTYPE HTML PUBLIC \"-//W3C//DTD HTML 4.0 Transitional//EN\">\n<HTML><HEAD><TITLE>Note</TITLE>\n<META http-equiv=Content-Type content=\"text/html\"></HEAD>\n<BODY><P>Pour   after <B>cure</B>&nbsp;test.</P>\n<P>Crane &amp; crew: 3 &lt; 4&#33;</P><DIV>Line<BR>break</DIV></BODY></HTML>";
        assert_eq!(
            note(html).text(),
            "Pour after cure test.\nCrane & crew: 3 < 4!\nLine\nbreak"
        );
    }

    #[test]
    fn plain_text_and_odd_input_survive() {
        assert_eq!(note("just words").text(), "just words");
        assert_eq!(note("a < b & c").text(), "a < b & c");
        assert_eq!(note("<p>unclosed").text(), "unclosed");
        assert_eq!(note("").text(), "");
        assert_eq!(note("&#x41;&#66;&bogus;").text(), "AB&bogus;");
        // Bounding the search for `;` never splits a character.
        assert_eq!(note("<p>&amp;éééééé</p>").text(), "&éééééé");
        assert_eq!(note("&éééééé").text(), "&éééééé");
        // A soft line break in the HTML is a space; plain text keeps lines.
        assert_eq!(
            note("<P>Pour after\ncure test</P>").text(),
            "Pour after cure test"
        );
        assert_eq!(note("one\ntwo").text(), "one\ntwo");
    }

    #[test]
    fn new_notes_read_back_as_written() {
        let text = "Basis: 3 crews <max> & overtime\nSecond line";
        let html = Note::html_of(text);
        assert_eq!(
            html,
            "<html><body><p>Basis: 3 crews &lt;max&gt; &amp; overtime</p><p>Second line</p></body></html>"
        );
        assert_eq!(note(&html).text(), text);
    }
}
