use cosmwasm_schema::cw_serde;
use cosmwasm_std::{coin, Decimal, QuerierWrapper, Uint128};

use crate::{CallbackData, OracleError, OracleValue};

#[cw_serde]
pub enum MarketMsg {
    /// Borrow the borrowable asset from the money market. Only callable by whitelisted market contracts.
    Borrow {
        amount: Uint128,
        callback: Option<CallbackData>,
        /// optional delegate address for the debt obligation to be allocated to
        delegate: Option<String>,
    },
    /// Repay a borrow. Only callable by whitelisted market contracts.
    Repay {
        /// Optionally repay a delegate's debt obligation instead of the caller's
        delegate: Option<String>,
    },
}

#[cw_serde]
pub struct DebtPoolResponse {
    /// The total deposits into the pool
    pub size: Uint128,
    /// The total ownership of the pool
    pub shares: Decimal,
    /// Ratio of shares / size
    pub ratio: Decimal,
}

#[cw_serde]
pub struct DepositPoolResponse {
    /// The total deposits into the pool
    pub size: Uint128,
    /// The total ownership of the pool
    pub shares: Uint128,
    /// Ratio of shares / size
    pub ratio: Decimal,
}

#[cw_serde]
pub struct BorrowerResponse {
    pub addr: String,
    /// The denom being borrowed
    pub denom: String,
    /// The borrower's borrow limit
    pub limit: Uint128,
    /// The borrower's current utilization
    pub current: Uint128,
    /// The shares allocated to the current debt
    pub shares: Decimal,
    /// The remaining amount of borrowable funds for this borrower
    pub available: Uint128,
}

#[cw_serde]
pub struct BorrowersResponse {
    pub borrowers: Vec<BorrowerResponse>,
}

#[cw_serde]
pub struct DelegateResponse {
    pub borrower: BorrowerResponse,
    pub addr: String,
    /// The borrower's current utilization
    pub current: Uint128,
    /// The shares allocated to the current debt
    pub shares: Decimal,
}

impl OracleValue for DelegateResponse {
    fn value_usd(&self, q: QuerierWrapper) -> Result<Decimal, OracleError> {
        coin(self.current.u128(), &self.borrower.denom).value_usd(q)
    }
}
