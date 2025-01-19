use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Binary, Decimal, Uint128};

use crate::CallbackData;

use super::strategy::Strategies;

#[cw_serde]
pub struct InstantiateMsg {
    pub strategy: Strategies,
}

#[cw_serde]
pub enum ExecuteMsg {
    /// Executes a market trade based on current order book.
    Swap {
        min_return: Uint128,
        to: Option<String>,

        /// An optional callback that BOW will execute with the funds from the swap.
        /// The callback is executed on the sender's address.
        #[serde(skip_serializing_if = "Option::is_none")]
        callback: Option<CallbackData>,
    },
    Deposit {},
    Withdraw {},
}

#[cw_serde]
pub enum SudoMsg {}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    ///
    #[returns(QuoteResponse)]
    Quote(QuoteRequest),
}

/// FIN is configured with a privileged, passive market maker.
///
#[cw_serde]
pub struct QuoteRequest {
    /// Denom that the market maker should be bidding
    /// ie the token that will be requested by FIN from BOW
    /// Eg. if a market order USDC buy of RUJI on FIN is being
    /// executed, the bid_denom here will be RUJI and the ask_denom
    /// USDC
    pub bid_denom: String,

    /// The denom that the market maker is asking for
    pub ask_denom: String,

    /// Optional binary data that should be pass-through from QuoteResponse::data
    pub data: Option<Binary>,
}

#[cw_serde]
pub struct QuoteResponse {
    pub price: Decimal,

    /// The size of the bid that the market maker is willing to make
    /// at the requested price
    pub size: Uint128,

    /// Optionally provide arbitrary binary data that is returned in subsequent QuoteQuery's
    /// Provided in order to allow the market maker to load from storage only once, instead of
    /// for each iteration of the query
    pub data: Option<Binary>,
}
