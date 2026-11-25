//! Hand-typed assignment spreads: `TASKRSRC.target_crv`, `remain_crv` and
//! `actual_crv` (TF-039).
//!
//! A cell is a list of stretches separated by `;`, each `units:hours`: the
//! units in the next `hours` of the assignment's work time, in order from its
//! start. Units and hours are unsigned decimals. Hours are whole minutes
//! written at three decimal places, rounded, with trailing zeros left off, so
//! 2 minutes is `0.033` and 40 hours is `40`. An empty cell is no spread.
//!
//! The corpus marks an assignment whose spread was typed by naming curve
//! [`MARKER_CURVE`] in `curv_id`, a curve no file holds.

use totalfloat_model::{Minutes, SpreadSegment};

/// The `curv_id` the corpus gives every assignment with a hand-typed spread,
/// a curve no file holds (TF-039).
pub const MARKER_CURVE: u64 = 9;

/// The longest integer part read, which keeps the arithmetic in range.
const MAX_DIGITS: usize = 15;

/// Why a spread cell can't be read. Positions count stretches from 0, and
/// never carry the cell's values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum SpreadDataError {
    /// A stretch isn't `units:hours` with unsigned decimals.
    #[error("stretch {0} isn't `units:hours` with unsigned decimals")]
    Syntax(usize),
    /// A stretch's hours are zero, too large, or not a whole number of minutes
    /// at three decimal places.
    #[error("stretch {0}'s hours aren't a positive whole number of minutes")]
    Hours(usize),
}

/// Reads a spread cell. An empty cell is an empty list.
///
/// # Errors
///
/// [`SpreadDataError`] for the first stretch that can't be read.
pub fn decode(cell: &str) -> Result<Vec<SpreadSegment>, SpreadDataError> {
    if cell.is_empty() {
        return Ok(Vec::new());
    }
    cell.split(';')
        .enumerate()
        .map(|(n, stretch)| {
            let (units, hours) = stretch.split_once(':').ok_or(SpreadDataError::Syntax(n))?;
            let units = decimal(units).ok_or(SpreadDataError::Syntax(n))?;
            let (whole, fraction) = decimal(hours).ok_or(SpreadDataError::Syntax(n))?;
            let units = units_value(units).ok_or(SpreadDataError::Syntax(n))?;
            let work = minutes(whole, fraction).ok_or(SpreadDataError::Hours(n))?;
            Ok(SpreadSegment { units, work })
        })
        .collect()
}

/// Writes a spread cell, or `None` if a stretch has units that are negative
/// or not finite, or work that isn't positive.
#[must_use]
pub fn encode(segments: &[SpreadSegment]) -> Option<String> {
    let mut out = String::new();
    for (n, segment) in segments.iter().enumerate() {
        if !segment.units.is_finite() || segment.units < 0.0 || segment.work.get() <= 0 {
            return None;
        }
        if n > 0 {
            out.push(';');
        }
        // `Display` gives the shortest spelling that reads back exactly, with
        // no exponent and no `.0` on a whole number; `-0` is written as `0`.
        let units = if segment.units == 0.0 {
            0.0
        } else {
            segment.units
        };
        out.push_str(&units.to_string());
        out.push(':');
        out.push_str(&hours(segment.work.get())?);
    }
    Some(out)
}

/// An unsigned decimal's integer digits and fraction digits.
fn decimal(text: &str) -> Option<(&str, &str)> {
    let (whole, fraction) = text.split_once('.').unwrap_or((text, ""));
    let digits = |s: &str| s.bytes().all(|b| b.is_ascii_digit());
    let ok = !whole.is_empty()
        && digits(whole)
        && digits(fraction)
        && (fraction.is_empty() != text.contains('.'));
    ok.then_some((whole, fraction))
}

fn units_value((whole, fraction): (&str, &str)) -> Option<f64> {
    let text = if fraction.is_empty() {
        whole.to_owned()
    } else {
        format!("{whole}.{fraction}")
    };
    text.parse::<f64>().ok().filter(|u| u.is_finite())
}

/// The whole minutes whose hours, rounded to three decimal places, are this
/// decimal, or `None` if there are none or they aren't positive.
fn minutes(whole: &str, fraction: &str) -> Option<Minutes> {
    let whole = whole.trim_start_matches('0');
    if whole.len() > MAX_DIGITS {
        return None;
    }
    let (kept, rest) = fraction.split_at(fraction.len().min(3));
    if rest.bytes().any(|b| b != b'0') {
        return None;
    }
    let whole: i64 = if whole.is_empty() {
        0
    } else {
        whole.parse().ok()?
    };
    let kept: i64 = format!("{kept:0<3}").parse().ok()?;
    let thousandths = whole.checked_mul(1000)?.checked_add(kept)?;
    // Thousandths of an hour are 0.06 minutes, so the nearest minute is the
    // only candidate.
    let minutes = (i128::from(thousandths) * 60 + 500) / 1000;
    let minutes = i64::try_from(minutes).ok()?;
    (minutes > 0 && thousandths_of(minutes)? == thousandths).then_some(Minutes::new(minutes))
}

/// `minutes` in thousandths of an hour, rounded. A minute is 16⅔
/// thousandths, so there's never a tie.
fn thousandths_of(minutes: i64) -> Option<i64> {
    i64::try_from((i128::from(minutes) * 1000 + 30) / 60).ok()
}

/// `minutes` as hours at three decimal places, without trailing zeros.
fn hours(minutes: i64) -> Option<String> {
    let t = thousandths_of(minutes)?;
    let (whole, fraction) = (t / 1000, t % 1000);
    if fraction == 0 {
        return Some(whole.to_string());
    }
    let fraction = format!("{fraction:03}");
    Some(format!("{whole}.{}", fraction.trim_end_matches('0')))
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn seg(units: f64, minutes: i64) -> SpreadSegment {
        SpreadSegment {
            units,
            work: Minutes::new(minutes),
        }
    }

    #[test]
    fn reads_stretches_of_units_and_work_hours() {
        assert_eq!(decode(""), Ok(vec![]));
        assert_eq!(
            decode("29:40;9:128"),
            Ok(vec![seg(29.0, 40 * 60), seg(9.0, 128 * 60)])
        );
        assert_eq!(
            decode("1.999:95.967;0.001:0.033"),
            Ok(vec![seg(1.999, 95 * 60 + 58), seg(0.001, 2)])
        );
        assert_eq!(decode("0.000:39.95"), Ok(vec![seg(0.0, 2397)]));
        assert_eq!(decode("5:0.017"), Ok(vec![seg(5.0, 1)]));
        assert_eq!(decode("5:40.000"), Ok(vec![seg(5.0, 2400)]));
    }

    #[test]
    fn malformed_cells_are_reported_by_position() {
        use SpreadDataError::{Hours, Syntax};
        for (cell, error) in [
            (";", Syntax(0)),
            ("1:8;", Syntax(1)),
            ("1:8;;2:8", Syntax(1)),
            ("1", Syntax(0)),
            ("1:8:2", Syntax(0)),
            ("-1:8", Syntax(0)),
            ("1:-8", Syntax(0)),
            ("+1:8", Syntax(0)),
            ("1e3:8", Syntax(0)),
            ("NaN:8", Syntax(0)),
            ("inf:8", Syntax(0)),
            (".5:8", Syntax(0)),
            ("5.:8", Syntax(0)),
            ("1:8;x:8", Syntax(1)),
            (" 1:8", Syntax(0)),
            ("1:0", Hours(0)),
            ("1:0.000", Hours(0)),
            ("1:8;1:0.02", Hours(1)),
            ("1:0.0333", Hours(0)),
            ("1:99999999999999999999", Hours(0)),
        ] {
            assert_eq!(decode(cell), Err(error), "{cell}");
        }
    }

    #[test]
    fn writes_what_p6_spells() {
        let cell = [
            seg(29.0, 2400),
            seg(2.088, 5758),
            seg(0.001, 2),
            seg(0.0, 40),
            seg(-0.0, 1),
        ];
        assert_eq!(
            encode(&cell).as_deref(),
            Some("29:40;2.088:95.967;0.001:0.033;0:0.667;0:0.017")
        );
        assert_eq!(encode(&[]).as_deref(), Some(""));
        for bad in [
            seg(-1.0, 60),
            seg(f64::NAN, 60),
            seg(f64::INFINITY, 60),
            seg(1.0, 0),
            seg(1.0, -60),
        ] {
            assert_eq!(encode(&[seg(1.0, 60), bad]), None, "{bad:?}");
        }
    }

    proptest! {
        #[test]
        fn written_cells_read_back(
            stretches in proptest::collection::vec((0_u32..2_000_000, 1_i64..1_000_000), 0..12),
        ) {
            let segments: Vec<_> = stretches
                .iter()
                .map(|&(thousandths, minutes)| seg(f64::from(thousandths) / 1000.0, minutes))
                .collect();
            let cell = encode(&segments).unwrap();
            prop_assert_eq!(decode(&cell), Ok(segments));
        }

        #[test]
        fn arbitrary_text_never_panics(cell in ".{0,40}") {
            let _ = decode(&cell);
        }
    }
}
