use cosmwasm_std::{Decimal, QuerierWrapper};
use rujira_rs::{
    fin::OraclePrice,
    query::{Pool, PoolError},
    Layer1Asset,
};
use std::ops::{Div, Mul};

#[derive(Clone)]
pub struct Oracle {
    pools: [Pool; 2],
}

impl Oracle {
    pub fn load(q: QuerierWrapper, assets: &[Layer1Asset; 2]) -> Result<Self, PoolError> {
        Ok(Self {
            pools: [Pool::load(q, &assets[0])?, Pool::load(q, &assets[1])?],
        })
    }

    fn human_price(&self) -> Decimal {
        let num = self.pools[0].asset_tor_price;
        let den = self.pools[1].asset_tor_price;
        num / den
    }

    fn normalized_price(&self) -> Decimal {
        let ten = Decimal::from_ratio(10u128, 1u128);
        self.human_price()
            .mul(ten.pow(self.pools[1].clone().decimals as u32))
            .div(ten.pow(self.pools[0].clone().decimals as u32))
    }
}

impl OraclePrice for Oracle {
    fn adjust(&self, bps: &i16) -> Decimal {
        match bps {
            i16::MIN..=-1 => {
                self.normalized_price()
                    * Decimal::from_ratio(10000u16 - bps.unsigned_abs(), 10000u16)
            }
            0 => self.normalized_price(),
            1..=i16::MAX => {
                self.normalized_price()
                    * Decimal::from_ratio(bps.unsigned_abs() + 10000u16, 10000u16)
            }
        }
    }
}
