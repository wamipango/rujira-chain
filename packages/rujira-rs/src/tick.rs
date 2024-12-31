use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Decimal, Fraction, Uint128};
use thiserror::Error;

pub trait Ticked {
    /// Truncate the value to the size of the tick
    fn truncate(&self, tick: &Tick) -> Self;
}

#[cw_serde]
pub struct Tick(u8);

impl Tick {
    pub fn new(size: u8) -> Self {
        Self(size)
    }

    pub fn validate<T: Ticked + PartialEq>(&self, v: T) -> Result<(), TickError> {
        if v == v.truncate(self) {
            return Ok(());
        }
        return Err(TickError::Invalid {});
    }
}

/// We truncate to significant figures,
impl Ticked for Decimal {
    fn truncate(&self, tick: &Tick) -> Self {
        let int = self.numerator();
        let len = int.to_string().as_str().bytes().len() as u32;
        let decimals: u32 = len - tick.0 as u32;
        let pow = Uint128::from(10u128).pow(decimals);
        let truncated = Uint128::one().mul_floor(Self::from_ratio(int, pow));
        Self::from_ratio(truncated * pow, self.denominator())
    }
}

#[derive(Error, Debug)]
pub enum TickError {
    #[error("Invalid")]
    Invalid {},
}

#[cfg(test)]
mod tests {

    use std::str::FromStr;

    use super::*;

    #[test]
    fn decimal() {
        let tick = Tick::new(2u8);

        tick.validate(Decimal::from_str("123").unwrap())
            .unwrap_err();

        tick.validate(Decimal::from_str("12").unwrap()).unwrap();
        tick.validate(Decimal::from_str("12.3").unwrap())
            .unwrap_err();
        tick.validate(Decimal::from_str("1.2").unwrap()).unwrap();

        tick.validate(Decimal::from_str("0.00000123").unwrap())
            .unwrap_err();

        assert_eq!(
            Decimal::from_str("0.00000123").unwrap().truncate(&tick),
            Decimal::from_str("0.0000012").unwrap()
        );

        assert_eq!(
            Decimal::from_str("0.00000129").unwrap().truncate(&tick),
            Decimal::from_str("0.0000012").unwrap()
        );

        tick.validate(Decimal::from_str("0.00012").unwrap())
            .unwrap();
    }
}
