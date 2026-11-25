//! Scalar decoding, with exact original token retention.

use std::borrow::Cow;

use totalfloat_model::DateTime;

use super::Decimal;
use crate::registry::FieldKind;

/// A nonfatal problem decoding a cell. Messages never include cell contents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, thiserror::Error)]
pub enum ValueError {
    /// Not an integer in the supported range.
    #[error("invalid or out-of-range integer")]
    InvalidInteger,
    /// Not a supported exact decimal.
    #[error("invalid or out-of-range decimal")]
    InvalidDecimal,
    /// Not a valid minute-resolution civil datetime.
    #[error("invalid civil datetime")]
    InvalidDateTime,
    /// A timestamp carries seconds or other unsupported precision.
    #[error("timestamp precision is unsupported; original text retained")]
    UnsupportedPrecision,
    /// Neither Y nor N.
    #[error("expected Y or N")]
    InvalidFlag,
    /// Not a recognized format code for the field.
    #[error("unknown enum code; original text retained")]
    UnknownEnum,
    /// Not a supported compressed or hexadecimal GUID representation.
    #[error("unrecognized GUID representation; original text retained")]
    InvalidGuid,
}

/// The state of a typed cell, distinct from its original spelling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CellState<T> {
    /// Successfully parsed value.
    Parsed(T),
    /// Present empty cell, including omitted trailing cells of a short row.
    Empty,
    /// The column is absent from this table's `%F` declaration.
    Missing,
    /// Unsupported token kept verbatim; never converted to a default value.
    Invalid(ValueError),
}

/// A typed value with its original lexical representation.
///
/// Parsing never throws away a token. `encode()` returns the exact input,
/// including signs, leading/trailing zeros, exponent spelling and invalid text.
/// Construct a replacement with [`Cell::from_value`] to request fresh formatting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cell<'a, T> {
    original: Option<&'a str>,
    state: CellState<T>,
}

impl<'a, T: Value<'a>> Cell<'a, T> {
    /// Decodes a present cell; empty strings are not parsed as zero or false.
    #[must_use]
    pub fn decode(kind: FieldKind, source: &'a str) -> Self {
        let state = if source.is_empty() {
            CellState::Empty
        } else {
            match T::parse(kind, source) {
                Ok(value) => CellState::Parsed(value),
                Err(error) => CellState::Invalid(error),
            }
        };
        Self {
            original: Some(source),
            state,
        }
    }

    /// An absent column, without silently inventing a default.
    #[must_use]
    pub const fn missing() -> Self {
        Self {
            original: None,
            state: CellState::Missing,
        }
    }

    /// A new value, formatted canonically rather than using source spelling.
    #[must_use]
    pub const fn from_value(value: T) -> Self {
        Self {
            original: None,
            state: CellState::Parsed(value),
        }
    }

    /// Parsed, empty, missing or invalid state.
    #[must_use]
    pub const fn state(&self) -> &CellState<T> {
        &self.state
    }

    /// The parsed value, if present and valid.
    #[must_use]
    pub const fn value(&self) -> Option<&T> {
        if let CellState::Parsed(value) = &self.state {
            Some(value)
        } else {
            None
        }
    }

    /// Original text for a decoded cell; `None` for absent or fresh values.
    #[must_use]
    pub const fn original(&self) -> Option<&'a str> {
        self.original
    }

    /// Exact source spelling, or canonical spelling for a fresh value.
    #[must_use]
    pub fn encode(&self) -> Cow<'_, str> {
        if let Some(source) = self.original {
            return Cow::Borrowed(source);
        }
        match &self.state {
            CellState::Parsed(value) => value.encode(),
            _ => Cow::Borrowed(""),
        }
    }
}

/// Scalar types supported by typed rows. The field kind supplies enum/GUID rules.
pub trait Value<'a>: Sized {
    /// Parses a nonempty token without substituting a default.
    ///
    /// # Errors
    /// Returns the reason the original token must remain unparsed.
    fn parse(kind: FieldKind, source: &'a str) -> Result<Self, ValueError>;
    /// Canonical formatting for newly constructed values.
    fn encode(&self) -> Cow<'_, str>;
}

impl Value<'_> for i64 {
    fn parse(_: FieldKind, source: &str) -> Result<Self, ValueError> {
        source.parse().map_err(|_| ValueError::InvalidInteger)
    }
    fn encode(&self) -> Cow<'_, str> {
        Cow::Owned(self.to_string())
    }
}

impl Value<'_> for Decimal {
    fn parse(_: FieldKind, source: &str) -> Result<Self, ValueError> {
        source.parse()
    }
    fn encode(&self) -> Cow<'_, str> {
        Cow::Owned(self.to_string())
    }
}

impl Value<'_> for bool {
    fn parse(_: FieldKind, source: &str) -> Result<Self, ValueError> {
        match source {
            "Y" => Ok(true),
            "N" => Ok(false),
            _ => Err(ValueError::InvalidFlag),
        }
    }
    fn encode(&self) -> Cow<'_, str> {
        Cow::Borrowed(if *self { "Y" } else { "N" })
    }
}

impl Value<'_> for DateTime {
    fn parse(_: FieldKind, source: &str) -> Result<Self, ValueError> {
        if source.len() > 16 && source.as_bytes().get(16) == Some(&b':') {
            return Err(ValueError::UnsupportedPrecision);
        }
        source.parse().map_err(|_| ValueError::InvalidDateTime)
    }
    fn encode(&self) -> Cow<'_, str> {
        Cow::Owned(self.to_string())
    }
}

impl<'a> Value<'a> for &'a str {
    fn parse(kind: FieldKind, source: &'a str) -> Result<Self, ValueError> {
        match kind {
            FieldKind::Enum(codes) if !codes.contains(&source) => Err(ValueError::UnknownEnum),
            FieldKind::Guid if !guid(source) => Err(ValueError::InvalidGuid),
            _ => Ok(source),
        }
    }
    fn encode(&self) -> Cow<'_, str> {
        Cow::Borrowed(self)
    }
}

fn guid(source: &str) -> bool {
    let bytes = source.as_bytes();
    match bytes.len() {
        22 => bytes
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || b"+/_-$".contains(b)),
        32 => bytes.iter().all(u8::is_ascii_hexdigit),
        36 => bytes.iter().enumerate().all(|(i, b)| match i {
            8 | 13 | 18 | 23 => *b == b'-',
            _ => b.is_ascii_hexdigit(),
        }),
        _ => false,
    }
}
