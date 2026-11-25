//! `totalfloat.command/1`: the whole intent of one edit request, as JSON.
//!
//! A version's meaning never changes once released (ARCHITECTURE.md, History and changesets). New behaviour
//! gets a new version, and readers keep every old one.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{Edit, Resolved};

/// The schema this build writes.
pub const COMMAND_SCHEMA: &str = "totalfloat.command/1";

/// Every schema this build reads.
pub const COMMAND_SCHEMAS_READ: [&str; 1] = [COMMAND_SCHEMA];

/// One edit request: its edits and its own settings, so it can be saved,
/// reviewed and replayed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Commands {
    /// Always `totalfloat.command/1`.
    #[schemars(extend("const" = "totalfloat.command/1"))]
    pub schema: String,
    /// The edits, applied in order.
    pub edits: Vec<Edit>,
    /// The request's settings.
    #[serde(default, skip_serializing_if = "CommandOptions::is_default")]
    pub options: CommandOptions,
    /// In history, what each name the edits used meant when they were
    /// recorded, by source ID, so a replay on another branch finds a renamed
    /// activity. `--edits` ignores it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resolved: Vec<Resolved>,
}

/// The settings of an edit request.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CommandOptions {
    /// Only this project (`proj_id`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<u64>,
    /// Move the data date too, as `YYYY-MM-DD HH:MM`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_date: Option<String>,
    /// With `data_date`, record the progress planned before it first.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub as_planned: bool,
}

impl CommandOptions {
    /// True if nothing is set.
    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

impl Commands {
    /// A request in the schema this build writes.
    #[must_use]
    pub fn new(edits: Vec<Edit>, options: CommandOptions) -> Self {
        Self {
            schema: COMMAND_SCHEMA.to_owned(),
            edits,
            options,
            resolved: Vec::new(),
        }
    }

    /// True if this build reads the request's schema.
    #[must_use]
    pub fn readable(&self) -> bool {
        COMMAND_SCHEMAS_READ.contains(&self.schema.as_str())
    }
}
