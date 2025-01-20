use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Decimal, Fraction, Uint128};
use thiserror::Error;

#[cw_serde]
pub struct Tick(u8);

impl Tick {
    pub fn new(size: u8) -> Self {
        Self(size)
    }

    pub fn validate(&self, v: &Decimal) -> Result<(), TickError> {
        if v == self.truncate_floor(v) {
            return Ok(());
        }
        Err(TickError::Invalid { v: *v })
    }

    pub fn truncate_floor(&self, v: &Decimal) -> Decimal {
        self.do_truncate(v, |x, y| x.mul_floor(y))
    }

    pub fn truncate_ceil(&self, v: &Decimal) -> Decimal {
        self.do_truncate(v, |x, y| x.mul_ceil(y))
    }

    fn do_truncate<F>(&self, v: &Decimal, fn_trunc: F) -> Decimal
    where
        F: Fn(Uint128, Decimal) -> Uint128,
    {
        let int = v.numerator();
        let len = int.to_string().as_str().bytes().len() as u32;
        let decimals: u32 = len - self.0 as u32;
        let pow = Uint128::from(10u128).pow(decimals);
        let truncated = fn_trunc(Uint128::one(), Decimal::from_ratio(int, pow));
        Decimal::from_ratio(truncated * pow, v.denominator())
    }
}

#[derive(Error, Debug)]
pub enum TickError {
    #[error("Invalid Tick Size {v}")]
    Invalid { v: Decimal },
}

#[cfg(test)]
mod tests {

    use std::str::FromStr;

    use super::*;

    #[test]
    fn decimal() {
        let tick = Tick::new(2u8);

        tick.validate(&Decimal::from_str("123").unwrap())
            .unwrap_err();

        tick.validate(&Decimal::from_str("12").unwrap()).unwrap();
        tick.validate(&Decimal::from_str("12.3").unwrap())
            .unwrap_err();
        tick.validate(&Decimal::from_str("1.2").unwrap()).unwrap();

        tick.validate(&Decimal::from_str("0.00000123").unwrap())
            .unwrap_err();

        assert_eq!(
            tick.truncate_floor(&Decimal::from_str("0.00000123").unwrap()),
            Decimal::from_str("0.0000012").unwrap()
        );

        assert_eq!(
            tick.truncate_floor(&Decimal::from_str("0.00000129").unwrap()),
            Decimal::from_str("0.0000012").unwrap()
        );

        tick.validate(&Decimal::from_str("0.00012").unwrap())
            .unwrap();
    }
}
