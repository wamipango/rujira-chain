use cosmwasm_std::{Decimal, QuerierWrapper};
use thiserror::Error;

use crate::{
    query::{
        network::{Network, TryFromNetworkError},
        Pool, PoolError,
    },
    Layer1Asset,
};

pub trait Oracle {
    fn price(&self, q: QuerierWrapper) -> Result<Decimal, OracleError>;
}

impl Oracle for Layer1Asset {
    fn price(&self, q: QuerierWrapper) -> Result<Decimal, OracleError> {
        if self.is_rune() {
            Ok(Network::load(q)?.rune_price_in_tor)
        } else {
            Ok(Pool::load(q, self)?.asset_tor_price)
        }
    }
}

impl<T: Oracle> Oracle for [T; 2] {
    fn price(&self, q: QuerierWrapper) -> Result<Decimal, OracleError> {
        Ok(self[0].price(q)? / self[1].price(q)?)
    }
}

#[derive(Error, Debug)]
pub enum OracleError {
    #[error("{0}")]
    Pool(#[from] PoolError),
    #[error("{0}")]
    TryFromNetwork(#[from] TryFromNetworkError),
}
