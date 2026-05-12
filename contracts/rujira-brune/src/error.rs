use cosmwasm_std::{CheckedFromRatioError, DecimalRangeExceeded, StdError, Uint128};
use cw_utils::PaymentError;
use rujira_rs::query::grpc::QueryError;
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
    DecimalRangeExceeded(#[from] DecimalRangeExceeded),

    #[error("{0}")]
    Query(#[from] QueryError),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("NodeNotRegistered")]
    NodeNotRegistered {},

    #[error("InsufficientFunds")]
    InsufficientFunds {},

    #[error("InsufficientCapacity max {0}")]
    InsufficientCapacity(Uint128),

    #[error("InsufficientReturn  offer {offer} ask {ask}")]
    InsufficientReturn { offer: Uint128, ask: Uint128 },

    #[error("MintCapExceeded max {0}")]
    MintCapExceeded(Uint128),

    #[error("Insolvent {0}")]
    Insolvent(String),

    #[error("Invalid: {0}")]
    Invalid(String),

    #[error("InvalidMinimumBond")]
    InvalidMinimumBond,
    // Add any other custom errors you like here.
    // Look at https://docs.rs/thiserror/1.0.21/thiserror/ for details.
}
