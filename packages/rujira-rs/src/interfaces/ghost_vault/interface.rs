use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::Uint128;

use crate::TokenMetadata;

use super::interest::Interest;

#[cw_serde]
pub struct InstantiateMsg {
    /// The denom string that can be deposited and lent
    pub denom: String,
    pub interest: Interest,
    pub receipt: TokenMetadata,
    pub debt: TokenMetadata,
}

#[cw_serde]
pub enum ExecuteMsg {
    /// Deposit the borrowable asset into the money market.
    Deposit {},
    /// Withdraw the borrowable asset from the money market.
    Withdraw {},
    /// Borrow the borrowable asset from the money market. Only callable by whitelisted market contracts.
    Borrow { amount: Uint128 },
    /// Repay a borrow. Only callable by whitelisted market contracts.
    Repay {},
}

#[cw_serde]
pub enum SudoMsg {
    /// Whitelist a new borrower
    AddBorrower { addr: String, borrow_limit: Uint128 },
    /// Update a whitelisted Borrower's parameters
    UpdateBorrower { addr: String, borrow_limit: Uint128 },
    /// Update contract interest parameters
    UpdateInterest { interest: Interest },
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {}
