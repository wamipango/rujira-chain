use cosmwasm_std::{CheckedFromRatioError, Decimal, StdError, Uint128};
use cw_utils::PaymentError;
use rujira_rs::SharePoolError;
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
    SharePool(#[from] SharePoolError),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("InsufficientFunds")]
    InsufficientFunds {},

    #[error("Invalid: {0}")]
    Invalid(String),

    #[error("MaxDebtExceeded available {available} requested {requested}")]
    MaxDebtExceeded {
        available: Uint128,
        requested: Uint128,
    },

    #[error("MaxLtvExceeded {result} >= {limit}")]
    MaxLtvExceeded { limit: Decimal, result: Decimal },
    // Add any other custom errors you like here.
    // Look at https://docs.rs/thiserror/1.0.21/thiserror/ for details.
}
