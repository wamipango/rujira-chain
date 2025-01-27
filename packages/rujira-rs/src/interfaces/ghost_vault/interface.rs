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
    pub debt: TokenMetadata,
    /// Lending market registry
    pub registry: String,
}

#[cw_serde]
pub enum ExecuteMsg {
    /// Deposit the borrowable asset into the money market.
    Deposit { callback: Option<CallbackData> },
    /// Withdraw the borrowable asset from the money market.
    Withdraw { callback: Option<CallbackData> },
    /// Borrow the borrowable asset from the money market. Only callable by whitelisted market contracts.
    Borrow {
        amount: Uint128,
        callback: Option<CallbackData>,
    },
    /// Repay a borrow. Only callable by whitelisted market contracts.
    Repay {},

    /// Priviledged to allow registry to call
    Sudo(SudoMsg),
}

#[cw_serde]
pub enum SudoMsg {
    /// Whitelist a new borrower
    AddBorrower { addr: String, debt_limit: Uint128 },
    /// Update a whitelisted Borrower's parameters
    UpdateBorrower { addr: String, debt_limit: Uint128 },
    /// Update contract interest parameters
    UpdateInterest { interest: Interest },
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(Interest)]
    Interest {},

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
    pub limit: Uint128,
    pub current: Uint128,
}

#[cw_serde]
pub struct BorrowersResponse {
    pub borrowers: Vec<BorrowerResponse>,
}
