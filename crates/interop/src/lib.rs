//! Schedule formats other than XER. Pure: text in, model out, no I/O.
//!
//! Each reader gives a [`Schedule`](totalfloat_model::Schedule), each writer takes
//! one, and both give a list of [`Note`]s saying what didn't translate. TotalFloat calculates by its own
//! rules (docs/rules), so dates from another tool's file can differ from that tool's (ARCHITECTURE.md, File interop).

#![forbid(unsafe_code)]

pub mod msproject;
pub mod msproject_write;
mod xml;

use serde::Serialize;

/// Why a file couldn't be read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ImportError {
    /// The file isn't well-formed XML.
    #[error("the file isn't valid XML: {0}")]
    Xml(String),
    /// The XML isn't the format expected.
    #[error("this isn't {0}")]
    WrongFormat(&'static str),
    /// A required value is missing or unreadable.
    #[error("{what} is missing or unreadable: {value}")]
    BadValue {
        /// What was being read.
        what: &'static str,
        /// The text found.
        value: String,
    },
}

/// Something that didn't translate exactly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Note {
    /// Stable code, such as `summary-logic`.
    pub kind: &'static str,
    /// What happened, in words.
    pub message: &'static str,
    /// How many times, counting all of them.
    pub count: usize,
    /// The first few task or link names.
    pub examples: Vec<String>,
}

const MAX_EXAMPLES: usize = 10;

/// Collects notes in the order they're first seen.
#[derive(Debug, Default)]
pub(crate) struct Notes(Vec<Note>);

impl Notes {
    pub(crate) fn add(&mut self, kind: &'static str, message: &'static str, example: &str) {
        let index = self
            .0
            .iter()
            .position(|n| n.kind == kind)
            .unwrap_or_else(|| {
                self.0.push(Note {
                    kind,
                    message,
                    count: 0,
                    examples: Vec::new(),
                });
                self.0.len() - 1
            });
        let note = &mut self.0[index];
        note.count += 1;
        if note.examples.len() < MAX_EXAMPLES {
            note.examples.push(example.to_owned());
        }
    }

    pub(crate) fn finish(self) -> Vec<Note> {
        self.0
    }
}
