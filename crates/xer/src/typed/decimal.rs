//! Exact decimal tokens. No binary floating-point arithmetic (ARCHITECTURE.md, Data model).

use std::fmt;
use std::str::FromStr;

use super::ValueError;

/// Exact base-ten value: `coefficient * 10^(-scale)`.
///
/// The coefficient fits in `i128` and the stored scale is between 0 and 64.
/// Positive exponents are expanded only when the coefficient still fits.
/// Larger tokens stay raw with a diagnostic instead of overflowing or rounding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Decimal {
    coefficient: i128,
    scale: i32,
}

impl Decimal {
    /// Constructs an exact decimal, rejecting an unsupported scale.
    ///
    /// # Errors
    /// Returns [`ValueError::InvalidDecimal`] for a scale outside -64..=64
    /// or if expanding a negative scale overflows the coefficient.
    pub fn new(coefficient: i128, scale: i32) -> Result<Self, ValueError> {
        if !(-64..=64).contains(&scale) {
            return Err(ValueError::InvalidDecimal);
        }
        if scale < 0 {
            let coefficient = if coefficient == 0 {
                0
            } else {
                coefficient
                    .checked_mul(
                        10i128
                            .checked_pow(scale.unsigned_abs())
                            .ok_or(ValueError::InvalidDecimal)?,
                    )
                    .ok_or(ValueError::InvalidDecimal)?
            };
            Ok(Self {
                coefficient,
                scale: 0,
            })
        } else {
            Ok(Self { coefficient, scale })
        }
    }

    /// Exact integer conversion, rejecting fractional or out-of-range values.
    #[must_use]
    pub fn checked_integer(self) -> Option<i128> {
        if self.coefficient == 0 {
            return Some(0);
        }
        let divisor = 10i128.checked_pow(self.scale.unsigned_abs())?;
        (self.coefficient % divisor == 0).then(|| self.coefficient / divisor)
    }

    /// Converts decimal hours to integer minutes without rounding or binary floats.
    #[must_use]
    pub fn checked_minutes(self) -> Option<totalfloat_model::Minutes> {
        if self.coefficient == 0 {
            return Some(totalfloat_model::Minutes::ZERO);
        }
        let (divisor, factor) = self.minute_ratio()?;
        if self.coefficient % divisor != 0 {
            return None;
        }
        let minutes = (self.coefficient / divisor).checked_mul(factor)?;
        Some(totalfloat_model::Minutes::new(i64::try_from(minutes).ok()?))
    }

    /// Whether decimal hours represent an integral number of minutes, regardless of range.
    #[must_use]
    pub fn is_whole_minutes(self) -> bool {
        self.coefficient == 0
            || self
                .minute_ratio()
                .is_some_and(|(divisor, _)| self.coefficient % divisor == 0)
    }

    /// Reads whole minutes, including six-or-more-place export spellings.
    ///
    /// A nonintegral token is accepted only if it exactly equals a whole
    /// minute expressed in hours, rounded or truncated at this decimal's
    /// scale (TF-012). No floating point or tolerance is used.
    #[must_use]
    pub fn export_minutes(self) -> Option<totalfloat_model::Minutes> {
        if let Some(minutes) = self.checked_minutes() {
            return Some(minutes);
        }
        if self.scale < 6 {
            return None;
        }
        // At scales above 40, even one minute needs more coefficient digits
        // than i128 can hold. At scale 40 only +/-1 can be candidates.
        let truncated = match self.scale.cmp(&40) {
            std::cmp::Ordering::Greater => return None,
            std::cmp::Ordering::Equal => 0,
            std::cmp::Ordering::Less => {
                let (divisor, factor) = self.minute_ratio()?;
                (self.coefficient / divisor)
                    .checked_mul(factor)?
                    .checked_add((self.coefficient % divisor).checked_mul(factor)? / divisor)?
            }
        };
        for candidate in [truncated, truncated.checked_add(self.coefficient.signum())?] {
            let minutes = i64::try_from(candidate).ok()?;
            let magnitude = candidate.unsigned_abs();
            let mut coefficient = magnitude / 60;
            let mut remainder = magnitude % 60;
            for _ in 0..self.scale {
                remainder *= 10;
                coefficient = coefficient.checked_mul(10)?.checked_add(remainder / 60)?;
                remainder %= 60;
            }
            let rounded = coefficient.checked_add(u128::from(remainder >= 30))?;
            if [coefficient, rounded].contains(&self.coefficient.unsigned_abs()) {
                return Some(totalfloat_model::Minutes::new(minutes));
            }
        }
        None
    }

    fn minute_ratio(self) -> Option<(i128, i128)> {
        // Cancel factors shared by 60 and powers of ten before multiplying.
        Some(match self.scale {
            0 => (1, 60),
            1 => (1, 6),
            _ => (
                5i128.checked_mul(10i128.checked_pow(self.scale.unsigned_abs() - 2)?)?,
                3,
            ),
        })
    }

    /// Signed coefficient, without a decimal point.
    #[must_use]
    pub const fn coefficient(self) -> i128 {
        self.coefficient
    }

    /// Number of decimal places (0..=64). Negative input scales are expanded.
    #[must_use]
    pub const fn scale(self) -> i32 {
        self.scale
    }
}

impl FromStr for Decimal {
    type Err = ValueError;

    fn from_str(source: &str) -> Result<Self, Self::Err> {
        let invalid = ValueError::InvalidDecimal;
        let (mantissa, exponent) = if let Some((mantissa, exponent)) = source.split_once(['e', 'E'])
        {
            (mantissa, exponent.parse::<i32>().map_err(|_| invalid)?)
        } else {
            (source, 0)
        };
        let negative = mantissa.starts_with('-');
        let mantissa = mantissa.strip_prefix(['+', '-']).unwrap_or(mantissa);
        let mut coefficient = 0i128;
        let mut scale = 0i32;
        let mut point = false;
        let mut digits = false;
        for byte in mantissa.bytes() {
            if byte == b'.' && !point {
                point = true;
            } else if byte.is_ascii_digit() {
                digits = true;
                // Accumulate negative numbers negatively, so i128::MIN is valid.
                coefficient = coefficient.checked_mul(10).ok_or(invalid)?;
                let digit = i128::from(byte - b'0');
                coefficient = if negative {
                    coefficient.checked_sub(digit)
                } else {
                    coefficient.checked_add(digit)
                }
                .ok_or(invalid)?;
                if point {
                    scale = scale.checked_add(1).ok_or(invalid)?;
                }
            } else {
                return Err(invalid);
            }
        }
        if !digits {
            return Err(invalid);
        }
        Self::new(coefficient, scale.checked_sub(exponent).ok_or(invalid)?)
    }
}

impl fmt::Display for Decimal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let digits = self.coefficient.unsigned_abs().to_string();
        if self.coefficient < 0 {
            f.write_str("-")?;
        }
        if self.scale <= 0 {
            f.write_str(&digits)?;
            for _ in 0..self.scale.unsigned_abs() {
                f.write_str("0")?;
            }
        } else {
            // Bounded by Decimal::new; conversion cannot truncate on supported targets.
            let scale = usize::try_from(self.scale).map_err(|_| fmt::Error)?;
            if scale >= digits.len() {
                f.write_str("0.")?;
                for _ in 0..scale - digits.len() {
                    f.write_str("0")?;
                }
                f.write_str(&digits)?;
            } else {
                let split = digits.len() - scale;
                write!(f, "{}.{}", &digits[..split], &digits[split..])?;
            }
        }
        Ok(())
    }
}
