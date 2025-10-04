use super::grpc::{QueryError, Queryable};
use crate::{
    asset::Layer1AssetError,
    proto::types::{QueryOraclePriceRequest, QueryOraclePriceResponse},
};
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Decimal, QuerierWrapper, StdError};
use std::{
    num::{ParseIntError, TryFromIntError},
    str::FromStr,
};
use thiserror::Error;

#[cw_serde]
pub struct OraclePrice {
    pub symbol: String,
    pub price: Decimal,
}

impl TryFrom<QueryOraclePriceResponse> for OraclePrice {
    type Error = TryFromOraclePriceError;
    fn try_from(v: QueryOraclePriceResponse) -> Result<Self, Self::Error> {
        match v.price {
            Some(price_data) => Ok(Self {
                symbol: price_data.symbol,
                price: Decimal::from_str(&price_data.price)?,
            }),
            None => Err(StdError::not_found("Oracle price not found").into()),
        }
    }
}

#[derive(Error, Debug)]
pub enum TryFromOraclePriceError {
    #[error("{0}")]
    Std(#[from] StdError),
    #[error("{0}")]
    TryFromInt(#[from] TryFromIntError),
    #[error("{0}")]
    ParseInt(#[from] ParseIntError),
    #[error("{0}")]
    Layer1Asset(#[from] Layer1AssetError),
}

impl OraclePrice {
    pub fn load(q: QuerierWrapper, symbol: &str) -> Result<Self, OraclePriceError> {
        let req = QueryOraclePriceRequest {
            height: "0".to_string(),
            symbol: symbol.to_owned(),
        };
        let res = QueryOraclePriceResponse::get(q, req)?;
        Ok(OraclePrice::try_from(res)?)
    }
}

#[derive(Error, Debug)]
pub enum OraclePriceError {
    #[error("{0}")]
    TryFrom(#[from] TryFromOraclePriceError),
    #[error("{0}")]
    Query(#[from] QueryError),
}
