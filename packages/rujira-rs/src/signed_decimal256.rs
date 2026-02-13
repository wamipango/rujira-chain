//! SignedDecimal256 - High precision signed decimal for intermediate calculations
//!
//! Provides 256-bit precision with sign tracking for mathematical operations
//! that may involve negative intermediate values.

use std::fmt;
use std::ops::{Add, Div, Mul, Neg, Sub};

use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Decimal, Decimal256, SignedDecimal, Uint256};

/// A signed 256-bit decimal number.
///
/// Internally represented as an absolute value plus sign flag.
/// This provides maximum precision for intermediate calculations
/// while supporting negative values.
#[cw_serde]
#[derive(Copy, Default)]
pub struct SignedDecimal256 {
    abs: Decimal256,
    neg: bool,
}

impl SignedDecimal256 {
    /// Create zero value
    pub const fn zero() -> Self {
        Self {
            abs: Decimal256::zero(),
            neg: false,
        }
    }

    /// Create value of one
    pub fn one() -> Self {
        Self {
            abs: Decimal256::one(),
            neg: false,
        }
    }

    /// Create from unsigned Decimal
    pub fn from_decimal(d: Decimal) -> Self {
        Self {
            abs: Decimal256::from(d),
            neg: false,
        }
    }

    /// Create from unsigned Decimal256
    pub fn from_decimal256(d: Decimal256) -> Self {
        Self { abs: d, neg: false }
    }

    /// Create from signed Decimal (128-bit)
    pub fn from_signed_decimal(s: SignedDecimal) -> Self {
        Self {
            abs: Decimal256::from(s.abs_diff(SignedDecimal::zero())),
            neg: s < SignedDecimal::zero(),
        }
    }

    /// Create from ratio of two unsigned values
    pub fn from_ratio(numerator: impl Into<Uint256>, denominator: impl Into<Uint256>) -> Self {
        Self {
            abs: Decimal256::from_ratio(numerator, denominator),
            neg: false,
        }
    }

    /// Returns true if zero
    pub fn is_zero(&self) -> bool {
        self.abs.is_zero()
    }

    /// Returns true if negative (and non-zero)
    pub fn is_negative(&self) -> bool {
        self.neg && !self.abs.is_zero()
    }

    /// Returns true if positive (and non-zero)
    pub fn is_positive(&self) -> bool {
        !self.neg && !self.abs.is_zero()
    }

    /// Returns the absolute value
    pub fn abs(&self) -> Decimal256 {
        self.abs
    }

    /// Negate the value
    pub fn negate(self) -> Self {
        if self.abs.is_zero() {
            self
        } else {
            Self {
                abs: self.abs,
                neg: !self.neg,
            }
        }
    }

    /// Convert to unsigned Decimal, returns None if negative
    pub fn to_decimal(self) -> Option<Decimal> {
        if self.is_negative() {
            None
        } else {
            Decimal::try_from(self.abs).ok()
        }
    }

    /// Convert to unsigned Decimal256, returns None if negative
    pub fn to_decimal256(self) -> Option<Decimal256> {
        if self.is_negative() {
            None
        } else {
            Some(self.abs)
        }
    }

    /// Convert to unsigned Decimal, returning zero if negative
    pub fn to_decimal_or_zero(self) -> Decimal {
        self.to_decimal().unwrap_or(Decimal::zero())
    }

    /// Convert to unsigned Decimal256, returning zero if negative
    pub fn to_decimal256_or_zero(self) -> Decimal256 {
        self.to_decimal256().unwrap_or(Decimal256::zero())
    }

    /// Checked addition
    pub fn checked_add(self, other: Self) -> Option<Self> {
        Some(if self.neg == other.neg {
            Self {
                abs: self.abs.checked_add(other.abs).ok()?,
                neg: self.neg,
            }
        } else if self.abs >= other.abs {
            Self {
                abs: self.abs.checked_sub(other.abs).ok()?,
                neg: self.neg,
            }
        } else {
            Self {
                abs: other.abs.checked_sub(self.abs).ok()?,
                neg: other.neg,
            }
        })
    }

    /// Checked subtraction
    pub fn checked_sub(self, other: Self) -> Option<Self> {
        self.checked_add(other.negate())
    }

    /// Checked multiplication
    pub fn checked_mul(self, other: Self) -> Option<Self> {
        Some(Self {
            abs: self.abs.checked_mul(other.abs).ok()?,
            neg: self.neg != other.neg && !self.abs.is_zero() && !other.abs.is_zero(),
        })
    }

    /// Checked division
    pub fn checked_div(self, other: Self) -> Option<Self> {
        if other.abs.is_zero() {
            return None;
        }
        Some(Self {
            abs: self.abs.checked_div(other.abs).ok()?,
            neg: self.neg != other.neg && !self.abs.is_zero(),
        })
    }
}

impl Add for SignedDecimal256 {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        self.checked_add(other).expect("SignedDecimal256 overflow")
    }
}

impl Sub for SignedDecimal256 {
    type Output = Self;

    fn sub(self, other: Self) -> Self {
        self.checked_sub(other).expect("SignedDecimal256 overflow")
    }
}

impl Mul for SignedDecimal256 {
    type Output = Self;

    fn mul(self, other: Self) -> Self {
        self.checked_mul(other).expect("SignedDecimal256 overflow")
    }
}

impl Div for SignedDecimal256 {
    type Output = Self;

    fn div(self, other: Self) -> Self {
        self.checked_div(other)
            .expect("SignedDecimal256 division by zero")
    }
}

impl Neg for SignedDecimal256 {
    type Output = Self;

    fn neg(self) -> Self {
        self.negate()
    }
}

impl From<Decimal> for SignedDecimal256 {
    fn from(d: Decimal) -> Self {
        Self::from_decimal(d)
    }
}

impl From<Decimal256> for SignedDecimal256 {
    fn from(d: Decimal256) -> Self {
        Self::from_decimal256(d)
    }
}

impl From<SignedDecimal> for SignedDecimal256 {
    fn from(s: SignedDecimal) -> Self {
        Self::from_signed_decimal(s)
    }
}

impl From<u64> for SignedDecimal256 {
    fn from(val: u64) -> Self {
        Self::from_decimal256(Decimal256::from_ratio(val, 1u64))
    }
}

impl From<u128> for SignedDecimal256 {
    fn from(val: u128) -> Self {
        Self::from_decimal256(Decimal256::from_ratio(val, 1u128))
    }
}

impl fmt::Display for SignedDecimal256 {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if self.neg && !self.abs.is_zero() {
            write!(f, "-{}", self.abs)
        } else {
            write!(f, "{}", self.abs)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_basic_arithmetic() {
        let a = SignedDecimal256::from(5u64);
        let b = SignedDecimal256::from(3u64);

        assert_eq!((a + b).abs(), Decimal256::from_ratio(8u64, 1u64));
        assert_eq!((a - b).abs(), Decimal256::from_ratio(2u64, 1u64));
        assert!((a - b).is_positive());

        let c = b - a; // 3 - 5 = -2
        assert!(c.is_negative());
        assert_eq!(c.abs(), Decimal256::from_ratio(2u64, 1u64));
    }

    #[test]
    fn test_multiplication_signs() {
        let pos = SignedDecimal256::from(2u64);
        let neg = -SignedDecimal256::from(3u64);

        assert!((pos * pos).is_positive());
        assert!((neg * neg).is_positive());
        assert!((pos * neg).is_negative());
        assert!((neg * pos).is_negative());
    }

    #[test]
    fn test_division() {
        let a = SignedDecimal256::from(10u64);
        let b = SignedDecimal256::from(4u64);

        let result = a / b;
        assert_eq!(result.abs(), Decimal256::from_ratio(10u64, 4u64));
    }

    #[test]
    fn test_to_decimal() {
        let pos = SignedDecimal256::from(5u64);
        let neg = -SignedDecimal256::from(5u64);

        assert!(pos.to_decimal().is_some());
        assert!(neg.to_decimal().is_none());
        assert_eq!(neg.to_decimal_or_zero(), Decimal::zero());
    }

    #[test]
    fn test_from_signed_decimal() {
        let neg = SignedDecimal::from_str("-2.5").unwrap();
        let sd256 = SignedDecimal256::from(neg);

        assert!(sd256.is_negative());
        assert_eq!(sd256.abs(), Decimal256::from_ratio(25u64, 10u64));
    }

    #[test]
    fn test_zero_operations() {
        let zero = SignedDecimal256::zero();
        let one = SignedDecimal256::one();

        assert!(zero.is_zero());
        assert!(!zero.is_negative());
        assert!(!zero.is_positive());

        assert_eq!((zero + one).abs(), Decimal256::one());
        assert_eq!((zero - one).abs(), Decimal256::one());
        assert!((zero - one).is_negative());
        assert!((zero * one).is_zero());
    }

    #[test]
    fn test_negation() {
        let pos = SignedDecimal256::from(5u64);
        let neg = pos.negate();

        assert!(neg.is_negative());
        assert_eq!(neg.abs(), pos.abs());

        let double_neg = neg.negate();
        assert!(double_neg.is_positive());
        assert_eq!(double_neg.abs(), pos.abs());
    }
}
