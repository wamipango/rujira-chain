use cosmwasm_std::{ConversionOverflowError, OverflowError, StdError};
use cw_utils::PaymentError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum StrategyError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("{0}")]
    Overflow(#[from] OverflowError),

    #[error("{0}")]
    Payment(#[from] PaymentError),

    #[error("{0}")]
    ConversionOverflow(#[from] ConversionOverflowError),

    #[error("InvalidPair")]
    InvalidPair {},

    #[error("InvalidDeposit")]
    InvalidDeposit {},

    #[error("InsufficientReturn")]
    InsufficientReturn {},
}
