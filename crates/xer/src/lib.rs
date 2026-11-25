//! Lossless Primavera P6 XER codec for TotalFloat (ARCHITECTURE.md, Lossless round trips).
//!
//! Layers: a raw lossless reader and writer (byte-identical round-trip), a
//! field registry, typed table decoding with diagnostics, mapping to and from
//! the [`totalfloat_model`], a patch writer for minimal edits, and a fresh
//! writer for schedules with no source file.
//!
//! Codecs never schedule: they read and write what the file says.
//!
//! Built so far: the [`raw`] reader/writer, compiled [`registry`], [`typed`] rows,
//! [`calendar_data`] decoding, model [`mapping`] and the [`patch`] writer.

#![forbid(unsafe_code)]

pub mod anonymize;
pub mod calendar_data;
pub mod fresh;
pub mod mapping;
pub mod monitoring;
pub mod network;
pub mod patch;
pub mod raw;
pub mod rawdiff;
pub mod registry;
pub mod spread_data;
pub mod typed;

pub use raw::{RawXer, ReadError, read};

pub use mapping::to_model;
pub use network::{LostRows, lost_network_rows};
pub use patch::{PatchError, patch, patch_all};
