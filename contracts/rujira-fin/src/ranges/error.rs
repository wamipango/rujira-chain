use rujira_rs::fin::TickError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum RangeError {
    #[error("Range high must be greater than low")]
    InvalidBounds {},

    #[error("Range spread must be < 1")]
    InvalidSpread {},

    #[error("Range fee must be < spread")]
    InvalidFee {},

    #[error("Skew must be in (-2, 2) exclusive")]
    InvalidSkew {},

    #[error("Range fee must be 0 when spread is 0")]
    InvalidFeeWithZeroSpread {},

    #[error("{0}")]
    Tick(#[from] TickError),
}
