use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Decimal, Timestamp, Uint128};

use crate::{CallbackData, TokenMetadata};

use super::interest::Interest;

#[cw_serde]
pub struct InstantiateMsg {
    /// The denom string that can be deposited and lent
    pub denom: String,
    pub interest: Interest,
    pub receipt: TokenMetadata,
}

#[cw_serde]
pub enum ExecuteMsg {
    /// Deposit the borrowable asset into the money market.
    Deposit { callback: Option<CallbackData> },
    /// Withdraw the borrowable asset from the money market.
    Withdraw { callback: Option<CallbackData> },
    /// Privileged Msgs for whitelisted contracts
    Market(MarketMsg),
}

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
pub enum SudoMsg {
    SetBorrower { contract: String, limit: Uint128 },
    SetInterest(Interest),
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(StatusResponse)]
    Status {},

    #[returns(BorrowerResponse)]
    Borrower { addr: String },

    #[returns(BorrowersResponse)]
    Borrowers {
        limit: Option<u8>,
        start_after: Option<String>,
    },
}

#[cw_serde]
pub struct StatusResponse {
    pub last_updated: Timestamp,

    pub utilization_ratio: Decimal,

    pub debt_rate: Decimal,

    pub lend_rate: Decimal,
    // Share pool that accounts for accrued debt interest
    pub debt_pool: PoolResponse,
    // Share pool that allocated collected debt interest to lenders
    pub deposit_pool: PoolResponse,

    pub interest: Interest,
}

#[cw_serde]
pub struct PoolResponse {
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
    /// The borrower's borrow limit
    pub limit: Uint128,
    /// The borrower's current utilization
    pub current: Uint128,
    /// The shares allocated to the current debt
    pub shares: Uint128,
    /// The remaining amount of borrowable funds for this borrower
    pub available: Uint128,
}

#[cw_serde]
pub struct BorrowersResponse {
    pub borrowers: Vec<BorrowerResponse>,
}
