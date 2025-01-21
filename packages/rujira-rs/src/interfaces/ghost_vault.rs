use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::Uint128;

use crate::TokenMetadata;

#[cw_serde]
pub struct InstantiateMsg {
    /// The denom string that can be deposited and lent
    pub denom: String,
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
    // /// Whitelist a new market, allowing it to borrow from the money market.
    // WhitelistMarket(MarketConfigMsg),
    // /// Update a whitelisted market's parameters
    // UpdateMarket(MarketConfigMsg),
    // /// Update contract config
    // UpdateConfig(ConfigUpdate),
    // /// Update contract interest parameters
    // UpdateInterest(InterestUpdate),
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {}
