//! A small tree over an XML event reader, for formats read whole.
//!
//! Entities are limited to XML's five and character references, so a file can't
//! expand itself, and the tree's depth is bounded.

use quick_xml::Reader;
use quick_xml::events::Event;

use crate::ImportError;

/// How deep elements can nest. Schedule formats need a handful of levels.
const MAX_DEPTH: usize = 64;

/// One element, with its text joined and namespaces ignored.
#[derive(Debug, Default)]
pub(crate) struct Element {
    pub(crate) name: String,
    pub(crate) text: String,
    pub(crate) children: Vec<Self>,
}

impl Element {
    /// The first child with this name.
    pub(crate) fn child(&self, name: &str) -> Option<&Self> {
        self.children.iter().find(|c| c.name == name)
    }

    /// Children with this name, in order.
    pub(crate) fn all<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Self> {
        self.children.iter().filter(move |c| c.name == name)
    }

    /// A child's text, if it has any.
    pub(crate) fn text_of(&self, name: &str) -> Option<&str> {
        self.child(name)
            .map(|c| c.text.trim())
            .filter(|t| !t.is_empty())
    }

    /// A child's text as a whole number.
    pub(crate) fn number(&self, name: &str) -> Option<i64> {
        self.text_of(name)?.parse().ok()
    }

    /// True if a child's text says `1`.
    pub(crate) fn flag(&self, name: &str) -> bool {
        self.text_of(name) == Some("1")
    }
}

fn local(name: &str) -> String {
    name.rsplit(':').next().unwrap_or_default().to_owned()
}

fn bad(error: impl std::fmt::Display) -> ImportError {
    ImportError::Xml(error.to_string())
}

fn entity(name: &str) -> Result<char, ImportError> {
    match name {
        "amp" => Ok('&'),
        "lt" => Ok('<'),
        "gt" => Ok('>'),
        "quot" => Ok('"'),
        "apos" => Ok('\''),
        other => Err(ImportError::Xml(format!("unknown entity {other}"))),
    }
}

/// Reads a document into its root element.
pub(crate) fn parse(text: &str) -> Result<Element, ImportError> {
    let mut reader = Reader::from_str(text.strip_prefix('\u{feff}').unwrap_or(text));
    reader.config_mut().expand_empty_elements = true;
    let mut stack: Vec<Element> = Vec::new();
    loop {
        match reader.read_event().map_err(bad)? {
            Event::Start(start) => {
                if stack.len() == MAX_DEPTH {
                    return Err(ImportError::Xml("elements nest too deeply".to_owned()));
                }
                stack.push(Element {
                    name: local(start.name().as_ref()),
                    ..Element::default()
                });
            }
            Event::End(_) => {
                let done = stack
                    .pop()
                    .ok_or_else(|| ImportError::Xml("an element closes twice".to_owned()))?;
                match stack.last_mut() {
                    Some(parent) => parent.children.push(done),
                    None => return Ok(done),
                }
            }
            Event::Text(text) => {
                if let Some(open) = stack.last_mut() {
                    open.text.push_str(&text);
                }
            }
            Event::CData(text) => {
                if let Some(open) = stack.last_mut() {
                    open.text.push_str(&text);
                }
            }
            Event::GeneralRef(reference) => {
                let found = if reference.is_char_ref() {
                    reference
                        .resolve_char_ref()
                        .map_err(bad)?
                        .ok_or_else(|| ImportError::Xml("an empty reference".to_owned()))?
                } else {
                    entity(&reference)?
                };
                if let Some(open) = stack.last_mut() {
                    open.text.push(found);
                }
            }
            Event::Eof => return Err(ImportError::Xml("the document ends early".to_owned())),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_entities_and_empty_elements_read_as_written() {
        let root = parse(
            "\u{feff}<?xml version=\"1.0\"?><Project xmlns=\"x\"><Name>A &amp; B&#33;</Name>\
             <Empty/><p:Tag><![CDATA[<raw>]]></p:Tag></Project>",
        )
        .unwrap();
        assert_eq!(root.name, "Project");
        assert_eq!(root.text_of("Name"), Some("A & B!"));
        assert_eq!(root.text_of("Empty"), None);
        assert!(root.child("Empty").is_some());
        assert_eq!(root.text_of("Tag"), Some("<raw>"));
    }

    #[test]
    fn malformed_documents_are_errors_not_panics() {
        for text in ["", "<a>", "<a></b>", "</a>", "<a>&nope;</a>", "not xml"] {
            assert!(parse(text).is_err(), "{text}");
        }
        let deep = "<a>".repeat(100);
        assert!(parse(&deep).is_err());
    }
}
