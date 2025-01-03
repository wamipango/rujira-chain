use cosmwasm_std::{CheckedFromRatioError, ConversionOverflowError, StdError};
use cw_utils::PaymentError;
use rujira_rs::{bid_pool::BidPoolError, query::PoolError};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("{0}")]
    Payment(#[from] PaymentError),

    #[error("{0}")]
    CheckedFromRatio(#[from] CheckedFromRatioError),

    #[error("{0}")]
    ConversionOverflow(#[from] ConversionOverflowError),

    #[error("{0}")]
    BidPool(#[from] BidPoolError),

    #[error("{0}")]
    Pool(#[from] PoolError),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("NotFound")]
    NotFound {},

    #[error("Invalid: {0}")]
    Invalid(String),
    // Add any other custom errors you like here.
    // Look at https://docs.rs/thiserror/1.0.21/thiserror/ for details.
}
