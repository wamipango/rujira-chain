use std::ops::Mul;

use cosmwasm_std::{Decimal, Decimal256, DecimalRangeExceeded};

/// An unsigned fixed-point decimal that uses the smallest representation able
/// to hold its value.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DecimalDyn {
    /// A `Decimal` value supporting faster checked 128-bit arithmetic.
    Compact(Decimal),
    /// A value that requires the wider `Decimal256` representation.
    Full(Decimal256),
}

impl DecimalDyn {
    /// Convert to the full-width representation.
    #[inline]
    pub fn full(self) -> Decimal256 {
        match self {
            Self::Compact(value) => Decimal256::from(value),
            Self::Full(value) => value,
        }
    }
}

impl From<Decimal> for DecimalDyn {
    #[inline]
    fn from(value: Decimal) -> Self {
        Self::Compact(value)
    }
}

impl From<Decimal256> for DecimalDyn {
    #[inline]
    fn from(value: Decimal256) -> Self {
        Decimal::try_from(value)
            .map(Self::Compact)
            .unwrap_or(Self::Full(value))
    }
}

impl From<DecimalDyn> for Decimal256 {
    #[inline]
    fn from(value: DecimalDyn) -> Self {
        value.full()
    }
}

impl TryFrom<DecimalDyn> for Decimal {
    type Error = DecimalRangeExceeded;

    #[inline]
    fn try_from(value: DecimalDyn) -> Result<Self, Self::Error> {
        match value {
            DecimalDyn::Compact(value) => Ok(value),
            DecimalDyn::Full(value) => Decimal::try_from(value),
        }
    }
}

impl Mul for DecimalDyn {
    type Output = Self;

    #[inline]
    fn mul(self, rhs: Self) -> Self::Output {
        if let (Self::Compact(lhs), Self::Compact(rhs)) = (self, rhs) {
            if let Ok(value) = lhs.checked_mul(rhs) {
                return Self::Compact(value);
            }
        }

        Self::from(self.full() * rhs.full())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multiplication_stays_compact_when_possible() {
        let lhs = DecimalDyn::from(Decimal::from_ratio(3u128, 2u128));
        let rhs = DecimalDyn::from(Decimal::from_ratio(2u128, 1u128));

        assert_eq!(
            lhs * rhs,
            DecimalDyn::Compact(Decimal::from_ratio(3u128, 1u128))
        );
    }

    #[test]
    fn multiplication_promotes_on_compact_overflow() {
        let lhs = DecimalDyn::from(Decimal::MAX);
        let rhs = DecimalDyn::from(Decimal::from_ratio(2u128, 1u128));
        let product = lhs * rhs;

        assert!(matches!(product, DecimalDyn::Full(_)));
        assert_eq!(
            product.full(),
            Decimal256::from(Decimal::MAX) * Decimal256::from_ratio(2u128, 1u128)
        );
    }

    #[test]
    fn try_into_decimal_checks_full_values() {
        assert_eq!(
            Decimal::try_from(DecimalDyn::Compact(Decimal::one())).unwrap(),
            Decimal::one()
        );
        assert!(Decimal::try_from(DecimalDyn::Full(Decimal256::MAX)).is_err());
    }
}
