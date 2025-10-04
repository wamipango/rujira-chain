use cosmwasm_std::{Decimal, QuerierWrapper};
use thiserror::Error;

use crate::{
    query::{
        network::{Network, TryFromNetworkError},
        oracle_price::{OraclePrice, OraclePriceError},
        pool::{Pool, PoolError},
    },
    Layer1Asset,
};

pub trait Oracle {
    fn tor_price(&self, q: QuerierWrapper) -> Result<Decimal, OracleError>;
    fn oracle_price(&self, q: QuerierWrapper) -> Result<Decimal, OracleError>;
}

impl Oracle for Layer1Asset {
    fn tor_price(&self, q: QuerierWrapper) -> Result<Decimal, OracleError> {
        if self.is_rune() {
            Ok(Network::load(q)?.rune_price_in_tor)
        } else {
            Ok(Pool::load(q, self)?.asset_tor_price)
        }
    }
    fn oracle_price(&self, q: QuerierWrapper) -> Result<Decimal, OracleError> {
        self.ticker().oracle_price(q)
    }
}

impl Oracle for String {
    fn tor_price(&self, _q: QuerierWrapper) -> Result<Decimal, OracleError> {
        Err(OracleError::Unavailable {})
    }
    fn oracle_price(&self, q: QuerierWrapper) -> Result<Decimal, OracleError> {
        Ok(OraclePrice::load(q, self)?.price)
    }
}

impl<T: Oracle> Oracle for [T; 2] {
    fn tor_price(&self, q: QuerierWrapper) -> Result<Decimal, OracleError> {
        Ok(self[0].tor_price(q)? / self[1].tor_price(q)?)
    }
    fn oracle_price(&self, q: QuerierWrapper) -> Result<Decimal, OracleError> {
        Ok(self[0].oracle_price(q)? / self[1].oracle_price(q)?)
    }
}

#[derive(Error, Debug)]
pub enum OracleError {
    #[error("{0}")]
    Pool(#[from] PoolError),
    #[error("{0}")]
    TryFromNetwork(#[from] TryFromNetworkError),
    #[error("{0}")]
    OraclePrice(#[from] OraclePriceError),
    #[error("Unavailable")]
    Unavailable {},
}
