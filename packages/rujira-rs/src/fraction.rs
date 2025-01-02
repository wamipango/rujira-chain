use std::fmt::Write;
use std::ops::{Add, AddAssign, Div, Mul, Sub};
use std::{fmt, str::FromStr};

use cosmwasm_schema::schemars::JsonSchema;
use cosmwasm_schema::serde::{de, ser, Deserialize, Deserializer, Serialize};
use cosmwasm_std::{
    CheckedFromRatioError, CheckedMultiplyRatioError, Decimal256, Fraction, StdError, Uint256,
    Uint512,
};

/// A wraper around Uint256 for working with low value decimal values with high precision
#[derive(Copy, Clone, Default, PartialEq, Eq, PartialOrd, Ord, JsonSchema)]
pub struct Frac(Uint256);

impl Frac {
    const DECIMAL_FRACTIONAL: Uint256 = // 1*10**36
        Uint256::from_u128(1_000_000_000_000_000_000_000_000_000_000_000_000u128);

    pub const DECIMAL_PLACES: u32 = 36;

    /// Create a 1.0 Decimal
    #[inline]
    pub const fn one() -> Self {
        Self(Self::DECIMAL_FRACTIONAL)
    }

    /// Create a 0.0 Decimal
    #[inline]
    pub const fn zero() -> Self {
        Self(Uint256::zero())
    }

    /// Returns the ratio (numerator / denominator) as a Decimal256
    pub fn from_ratio(numerator: impl Into<Uint256>, denominator: impl Into<Uint256>) -> Self {
        match Self::checked_from_ratio(numerator, denominator) {
            Ok(value) => value,
            Err(CheckedFromRatioError::DivideByZero) => {
                panic!("Denominator must not be zero")
            }
            Err(CheckedFromRatioError::Overflow) => panic!("Multiplication overflow"),
        }
    }

    /// Returns the ratio (numerator / denominator) as a Decimal256
    pub fn checked_from_ratio(
        numerator: impl Into<Uint256>,
        denominator: impl Into<Uint256>,
    ) -> Result<Self, CheckedFromRatioError> {
        let numerator: Uint256 = numerator.into();
        let denominator: Uint256 = denominator.into();
        match numerator.checked_multiply_ratio(Self::DECIMAL_FRACTIONAL, denominator) {
            Ok(ratio) => {
                // numerator * DECIMAL_FRACTIONAL / denominator
                Ok(Self(ratio))
            }
            Err(CheckedMultiplyRatioError::Overflow) => Err(CheckedFromRatioError::Overflow),
            Err(CheckedMultiplyRatioError::DivideByZero) => {
                Err(CheckedFromRatioError::DivideByZero)
            }
        }
    }

    pub const fn is_zero(&self) -> bool {
        self.0.is_zero()
    }
}

impl fmt::Display for Frac {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let whole = (self.0) / Self::DECIMAL_FRACTIONAL;
        let fractional = (self.0).checked_rem(Self::DECIMAL_FRACTIONAL).unwrap();
        if fractional.is_zero() {
            write!(f, "{whole}")
        } else {
            let fractional_string = format!(
                "{:0>padding$}",
                fractional,
                padding = Self::DECIMAL_PLACES as usize
            );
            f.write_str(&whole.to_string())?;
            f.write_char('.')?;
            f.write_str(fractional_string.trim_end_matches('0'))?;
            Ok(())
        }
    }
}

impl FromStr for Frac {
    type Err = StdError;

    /// Converts the decimal string to a Decimal
    /// Possible inputs: "1.23", "1", "000012", "1.123000000"
    /// Disallowed: "", ".23"
    ///
    /// This never performs any kind of rounding.
    /// More than DECIMAL_PLACES fractional digits, even zeros, result in an error.
    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let mut parts_iter = input.split('.');

        let whole_part = parts_iter.next().unwrap(); // split always returns at least one element
        let whole = whole_part
            .parse::<Uint256>()
            .map_err(|_| StdError::generic_err("Error parsing whole"))?;
        let mut atomics = whole
            .checked_mul(Self::DECIMAL_FRACTIONAL)
            .map_err(|_| StdError::generic_err("Value too big"))?;

        if let Some(fractional_part) = parts_iter.next() {
            let fractional = fractional_part
                .parse::<Uint256>()
                .map_err(|_| StdError::generic_err("Error parsing fractional"))?;
            let exp = (Self::DECIMAL_PLACES.checked_sub(fractional_part.len() as u32)).ok_or_else(
                || {
                    StdError::generic_err(format!(
                        "Cannot parse more than {} fractional digits",
                        Self::DECIMAL_PLACES
                    ))
                },
            )?;
            debug_assert!(exp <= Self::DECIMAL_PLACES);
            let fractional_factor = Uint256::from(10u128).pow(exp);
            atomics = atomics
                .checked_add(
                    // The inner multiplication can't overflow because
                    // fractional < 10^DECIMAL_PLACES && fractional_factor <= 10^DECIMAL_PLACES
                    fractional.checked_mul(fractional_factor).unwrap(),
                )
                .map_err(|_| StdError::generic_err("Value too big"))?;
        }

        if parts_iter.next().is_some() {
            return Err(StdError::generic_err("Unexpected number of dots"));
        }

        Ok(Self(atomics))
    }
}

impl Serialize for Frac {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

/// Deserializes as a base64 string
impl<'de> Deserialize<'de> for Frac {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_str(DecimalVisitor)
    }
}

impl Fraction<Uint256> for Frac {
    fn numerator(&self) -> Uint256 {
        self.0
    }

    fn denominator(&self) -> Uint256 {
        Self::DECIMAL_FRACTIONAL
    }

    fn inv(&self) -> Option<Self> {
        if self.is_zero() {
            None
        } else {
            Some(Self(Self::DECIMAL_FRACTIONAL / self.0))
        }
    }
}

struct DecimalVisitor;

impl<'de> de::Visitor<'de> for DecimalVisitor {
    type Value = Frac;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("string-encoded decimal")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        match Self::Value::from_str(v) {
            Ok(d) => Ok(d),
            Err(e) => Err(E::custom(format!("Error parsing decimal '{v}': {e}"))),
        }
    }
}

impl fmt::Debug for Frac {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Fraction({self})")
    }
}

impl Add for Frac {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self(self.0 + other.0)
    }
}

impl AddAssign for Frac {
    fn add_assign(&mut self, rhs: Frac) {
        *self = *self + rhs;
    }
}

impl Mul for Frac {
    type Output = Self;

    #[allow(clippy::suspicious_arithmetic_impl)]
    fn mul(self, other: Self) -> Self {
        // Decimals are fractions. We can multiply two decimals a and b
        // via
        //       (a.numerator() * b.numerator()) / (a.denominator() * b.denominator())
        //     = (a.numerator() * b.numerator()) / a.denominator() / b.denominator()

        let result_as_uint256 = self.numerator().full_mul(other.numerator())
            / Uint512::from_uint256(Self::DECIMAL_FRACTIONAL); // from_uint128 is a const method and should be "free"
        match result_as_uint256.try_into() {
            Ok(result) => Self(result),
            Err(_) => panic!("attempt to multiply with overflow"),
        }
    }
}

impl Div for Frac {
    type Output = Self;

    fn div(self, other: Self) -> Self {
        match Frac::checked_from_ratio(self.numerator(), other.numerator()) {
            Ok(ratio) => ratio,
            Err(CheckedFromRatioError::DivideByZero) => {
                panic!("Division failed - denominator must not be zero")
            }
            Err(CheckedFromRatioError::Overflow) => {
                panic!("Division failed - multiplication overflow")
            }
        }
    }
}

impl Sub for Frac {
    type Output = Self;

    fn sub(self, other: Self) -> Self {
        Self(self.0 - other.0)
    }
}

impl Mul<Frac> for Uint256 {
    // Max integer value of Frac is relatively low, so we return Uint256 to prevent overflow
    type Output = Uint256;

    fn mul(self, rhs: Frac) -> Self::Output {
        rhs * self
    }
}

impl Mul<Uint256> for Frac {
    type Output = Uint256;

    fn mul(self, rhs: Uint256) -> Self::Output {
        self.numerator() * rhs / Self::DECIMAL_FRACTIONAL
    }
}

impl Mul<Decimal256> for Frac {
    type Output = Frac;

    fn mul(self, rhs: Decimal256) -> Self::Output {
        Self::from_ratio(
            rhs.numerator() * self.numerator(),
            rhs.denominator() * self.denominator(),
        )
    }
}

#[cfg(test)]
mod tests {
    use std::{ops::Mul, str::FromStr};

    use super::*;
    use cosmwasm_std::Uint256;

    #[test]
    fn multiply() {
        assert_eq!(
            Frac::from_ratio(1u128, 2u128).mul(Frac::from_ratio(1u128, 4u128)),
            Frac::from_ratio(1u128, 8u128)
        )
    }

    #[test]
    fn from_ratio() {
        assert_eq!(
            Frac::from_ratio(10010000000000000u128, 1000000000000000000u128),
            Frac::from_str("0.01001").unwrap()
        )
    }

    #[test]
    fn mult() {
        assert_eq!(
            Frac::from_str("0.000067").unwrap() * Uint256::from(15001000000u128),
            Uint256::from(1005067u128)
        );
        assert_eq!(
            Frac::from_str("0.000067").unwrap()
                * Frac::from_ratio(Uint256::from(15001u128), Uint256::from(1000067u128)),
            Frac::from_str("0.000001004999665022443496285748854826").unwrap()
        )
    }
}
