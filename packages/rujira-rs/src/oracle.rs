use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Coin, Decimal, OverflowError, QuerierWrapper, Uint128};
use cw_utils::NativeBalance;
use std::ops::Add;
use thiserror::Error;

use crate::{
    query::{
        network::{Network, TryFromNetworkError},
        oracle_price::{OraclePrice, OraclePriceError},
        pool::{Pool, PoolError},
    },
    Layer1Asset, SecuredAsset, SecuredAssetError,
};

#[cw_serde]
pub struct OraclePair([Oracle; 2]);

impl OraclePair {
    pub fn new<T: Into<Oracle>>(a: T, b: T) -> Self {
        Self([a.into(), b.into()])
    }
    pub fn price(&self, q: QuerierWrapper) -> Result<Decimal, OracleError> {
        Ok(self.0[0].price(q)? / self.0[1].price(q)?)
    }
}

#[cw_serde]
#[serde(untagged)]
pub enum Oracle {
    Pool(Layer1Asset),
    Enshrined(String),
}

impl From<Layer1Asset> for Oracle {
    fn from(value: Layer1Asset) -> Self {
        Self::Pool(value)
    }
}

impl From<&str> for Oracle {
    fn from(value: &str) -> Self {
        Self::Enshrined(value.to_string())
    }
}

impl From<String> for Oracle {
    fn from(value: String) -> Self {
        Self::Enshrined(value)
    }
}

pub trait OracleValue {
    fn value_usd(&self, q: QuerierWrapper) -> Result<Decimal, OracleError>;
}

impl Oracle {
    pub fn price(&self, q: QuerierWrapper) -> Result<Decimal, OracleError> {
        match self {
            Oracle::Pool(layer1_asset) => {
                if layer1_asset.is_rune() {
                    Ok(Network::load(q)?.rune_price_in_tor)
                } else {
                    Ok(Pool::load(q, layer1_asset)?.asset_tor_price)
                }
            }
            Oracle::Enshrined(ticker) => {
                let price = OraclePrice::load(q, ticker)?;
                if price.price.is_zero() {
                    return Err(OracleError::InvalidZero(ticker.clone()));
                }
                Ok(price.price)
            }
        }
    }
}

impl OracleValue for Coin {
    fn value_usd(&self, q: QuerierWrapper) -> Result<Decimal, OracleError> {
        Ok(
            Oracle::Enshrined(SecuredAsset::from_denom(&self.denom)?.to_layer_1().ticker())
                .price(q)?
                .checked_mul(Decimal::from_ratio(self.amount, Uint128::one()))?,
        )
    }
}

impl OracleValue for NativeBalance {
    fn value_usd(&self, q: QuerierWrapper) -> Result<Decimal, OracleError> {
        self.clone()
            .into_vec()
            .iter()
            .try_fold(Decimal::zero(), |agg, v| Ok(v.value_usd(q)?.add(agg)))
    }
}

#[derive(Error, Debug)]
pub enum OracleError {
    #[error("{0}")]
    SecuredAsset(#[from] SecuredAssetError),
    #[error("{0}")]
    Pool(#[from] PoolError),
    #[error("{0}")]
    TryFromNetwork(#[from] TryFromNetworkError),
    #[error("{0}")]
    OraclePrice(#[from] OraclePriceError),
    #[error("{0}")]
    Overflow(#[from] OverflowError),
    #[error("InvalidZero {0}")]
    InvalidZero(String),
    #[error("Unavailable")]
    Unavailable {},
}
