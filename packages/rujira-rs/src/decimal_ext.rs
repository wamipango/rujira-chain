use cosmwasm_std::{Decimal, Decimal256};

pub trait DecimalExt: Into<Decimal256> + Sized {
    fn safe_share<N, D>(self, numerator: N, denominator: D) -> Decimal
    where
        N: Into<Decimal256>,
        D: Into<Decimal256>,
    {
        let value: Decimal256 = self.into();
        let numerator: Decimal256 = numerator.into();
        let denominator: Decimal256 = denominator.into();
        if denominator.is_zero() {
            return Decimal::zero();
        }

        (value * numerator / denominator).try_into().unwrap()
    }
}

impl<T> DecimalExt for T where T: Into<Decimal256> {}
