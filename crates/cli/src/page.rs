//! A report as a document of headings, paragraphs and tables, written as one
//! self-contained HTML page or as Markdown. `report`, `check` and
//! `variance` build their pages here, so they look the same and escape the
//! same way. A page has an inline style and nothing else: no scripts, no
//! external resources and no time of writing.

use std::fmt::Write as _;

use serde::Deserialize;

use crate::report::escape_markdown;

/// How `check` and `variance` write a document for MCP.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Format {
    /// A Markdown document.
    Markdown,
    /// One self-contained HTML page.
    Html,
}

/// Text as HTML shows it: `&`, `<`, `>` and `"` escaped.
pub fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

const STYLE: &str = "body{font-family:system-ui,sans-serif;margin:2rem;color:#1d2433}\
table{border-collapse:collapse;margin:0.5rem 0 1.5rem}\
th,td{border:1px solid #c9ced8;padding:0.25rem 0.6rem;text-align:left}\
th{background:#eef1f5}p{color:#4b5465}";

/// What a marked page adds: failures and late dates in red on a tinted row,
/// passes in green, and what can't be measured in grey.
const MARKS: &str = ".fail,.late{color:#a4161a;font-weight:600}\
tr.fail td{background:#fdf0f0}.pass{color:#1e6b34}.na{color:#6b7280}";

/// A table cell: its text, and a class that marks it.
#[derive(Debug, Clone)]
pub struct Cell {
    text: String,
    class: Option<&'static str>,
}

impl Cell {
    /// A cell marked with `class`, or plain.
    pub fn marked(text: impl Into<String>, class: Option<&'static str>) -> Self {
        Self {
            text: text.into(),
            class,
        }
    }
}

impl From<String> for Cell {
    fn from(text: String) -> Self {
        Self::marked(text, None)
    }
}

impl From<&str> for Cell {
    fn from(text: &str) -> Self {
        Self::marked(text, None)
    }
}

/// A table row: its cells, and a class that marks the whole row.
#[derive(Debug, Clone)]
pub struct Row {
    cells: Vec<Cell>,
    class: Option<&'static str>,
}

impl Row {
    /// A row marked with `class`, or plain.
    pub fn marked(cells: Vec<Cell>, class: Option<&'static str>) -> Self {
        Self { cells, class }
    }
}

impl<T: Into<Cell>> From<Vec<T>> for Row {
    fn from(cells: Vec<T>) -> Self {
        Self::marked(cells.into_iter().map(Into::into).collect(), None)
    }
}

/// One part of a document.
#[derive(Debug, Clone)]
enum Block {
    Heading(u8, String),
    Paragraph(String),
    Strong(String),
    Table {
        headings: Vec<String>,
        rows: Vec<Row>,
        foot: Option<Vec<String>>,
    },
}

/// A document: a title, then headings, paragraphs and tables in order.
#[derive(Debug, Clone)]
pub struct Page {
    title: String,
    marked: bool,
    blocks: Vec<Block>,
}

impl Page {
    /// An empty document with this title.
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            marked: false,
            blocks: Vec::new(),
        }
    }

    /// The same, with the style that shows marked rows and cells.
    pub fn marked(title: impl Into<String>) -> Self {
        Self {
            marked: true,
            ..Self::new(title)
        }
    }

    /// A heading under the title, at `level` 2 or deeper.
    pub fn heading(&mut self, level: u8, text: impl Into<String>) {
        self.blocks
            .push(Block::Heading(level.clamp(2, 6), text.into()));
    }

    /// A paragraph of plain text.
    pub fn paragraph(&mut self, text: impl Into<String>) {
        self.blocks.push(Block::Paragraph(text.into()));
    }

    /// A paragraph in bold.
    pub fn strong(&mut self, text: impl Into<String>) {
        self.blocks.push(Block::Strong(text.into()));
    }

    /// A table with a header row and, if given, a footer row.
    pub fn table(&mut self, headings: Vec<String>, rows: Vec<Row>, foot: Option<Vec<String>>) {
        self.blocks.push(Block::Table {
            headings,
            rows,
            foot,
        });
    }

    /// The document as one HTML page.
    #[must_use]
    pub fn html(&self) -> String {
        let marks = if self.marked { MARKS } else { "" };
        let title = escape_html(&self.title);
        let mut out = format!(
            "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n<title>{title}</title>\n<style>{STYLE}{marks}</style>\n</head>\n<body>\n<h1>{title}</h1>\n"
        );
        let class =
            |class: Option<&str>| class.map_or_else(String::new, |c| format!(" class=\"{c}\""));
        for block in &self.blocks {
            match block {
                Block::Heading(level, text) => {
                    let _ = writeln!(out, "<h{level}>{}</h{level}>", escape_html(text));
                }
                Block::Paragraph(text) => {
                    let _ = writeln!(out, "<p>{}</p>", escape_html(text));
                }
                Block::Strong(text) => {
                    let _ = writeln!(out, "<p><strong>{}</strong></p>", escape_html(text));
                }
                Block::Table {
                    headings,
                    rows,
                    foot,
                } => {
                    out.push_str("<table>\n<thead><tr>");
                    for text in headings {
                        let _ = write!(out, "<th>{}</th>", escape_html(text));
                    }
                    out.push_str("</tr></thead>\n<tbody>\n");
                    for row in rows {
                        let _ = write!(out, "<tr{}>", class(row.class));
                        for cell in &row.cells {
                            let _ = write!(
                                out,
                                "<td{}>{}</td>",
                                class(cell.class),
                                escape_html(&cell.text)
                            );
                        }
                        out.push_str("</tr>\n");
                    }
                    out.push_str("</tbody>\n");
                    if let Some(foot) = foot {
                        out.push_str("<tfoot><tr>");
                        for text in foot {
                            let _ = write!(out, "<th>{}</th>", escape_html(text));
                        }
                        out.push_str("</tr></tfoot>\n");
                    }
                    out.push_str("</table>\n");
                }
            }
        }
        out.push_str("</body>\n</html>\n");
        out
    }

    /// The document as Markdown. Marks don't show.
    #[must_use]
    pub fn markdown(&self) -> String {
        let mut out = format!("# {}\n", escape_markdown(&self.title));
        let line = |cells: &mut dyn Iterator<Item = &str>| {
            let cells: Vec<String> = cells.map(escape_markdown).collect();
            format!("| {} |\n", cells.join(" | "))
        };
        for block in &self.blocks {
            out.push('\n');
            match block {
                Block::Heading(level, text) => {
                    let _ = writeln!(
                        out,
                        "{} {}",
                        "#".repeat(usize::from(*level)),
                        escape_markdown(text)
                    );
                }
                Block::Paragraph(text) => {
                    let _ = writeln!(out, "{}", escape_markdown(text));
                }
                Block::Strong(text) => {
                    let _ = writeln!(out, "**{}**", escape_markdown(text));
                }
                Block::Table {
                    headings,
                    rows,
                    foot,
                } => {
                    out.push_str(&line(&mut headings.iter().map(String::as_str)));
                    let _ = writeln!(out, "|{}", " --- |".repeat(headings.len()));
                    for row in rows {
                        out.push_str(&line(&mut row.cells.iter().map(|c| c.text.as_str())));
                    }
                    if let Some(foot) = foot {
                        out.push_str(&line(&mut foot.iter().map(String::as_str)));
                    }
                }
            }
        }
        out
    }

    /// The document in `format`.
    #[must_use]
    pub fn render(&self, format: Format) -> String {
        match format {
            Format::Markdown => self.markdown(),
            Format::Html => self.html(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Page {
        let mut page = Page::marked("Pour & <cure> \"slab\"");
        page.heading(2, "A <b> & \"c\"");
        page.paragraph("x < y & z > w");
        page.table(
            vec!["Code".to_owned(), "Name".to_owned()],
            vec![
                Row::marked(
                    vec![Cell::from("A1"), Cell::marked("<script>", Some("fail"))],
                    Some("fail"),
                ),
                Row::from(vec!["A2", "Tom's \"rig\" & crane"]),
            ],
            None,
        );
        page
    }

    #[test]
    fn html_escapes_every_text() {
        let html = sample().html();
        assert!(!html.contains("<script>"), "{html}");
        assert!(!html.contains("<b>"), "{html}");
        assert!(html.contains("&lt;script&gt;"), "{html}");
        assert!(
            html.contains("Pour &amp; &lt;cure&gt; &quot;slab&quot;"),
            "{html}"
        );
        assert!(html.contains("Tom's &quot;rig&quot; &amp; crane"), "{html}");
        assert!(html.contains("<tr class=\"fail\"><td>A1</td><td class=\"fail\">"));
    }

    #[test]
    fn html_has_no_scripts_or_outside_resources() {
        let html = sample().html();
        for word in ["<script", "<link", "src=", "href=", "url(", "@import"] {
            assert!(!html.contains(word), "{word} in {html}");
        }
    }

    #[test]
    fn markdown_escapes_markup() {
        let markdown = sample().markdown();
        assert!(
            markdown.starts_with("# Pour \\& \\<cure\\> \"slab\"\n"),
            "{markdown}"
        );
        assert!(markdown.contains("| A1 | \\<script\\> |"), "{markdown}");
        assert!(markdown.contains("| --- | --- |"), "{markdown}");
    }
}
