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

#[cw_serde]
pub struct QuoteRequest {
    /// An optional price that should be honoured when present, and set in QuoteResponse::price
    /// Allows consumers to request prices that match specific ticks (eg Orca requesting prices at a given premium)
    /// but also allows a Strategy to set its own best price (eg when market making on FIN)
    pub price: Option<Decimal>,

    /// Denom that the Caller is bidding
    /// ie the token that will be sent by FIN to BOW
    /// Eg. if a market order USDC buy of RUJI on FIN is being
    /// executed, the bid_denom here will be USDC and the ask_denom
    /// RUJI
    pub bid_denom: String,

    /// The denom that the Caller is asking for
    pub ask_denom: String,

    /// Optional binary data that should be pass-through from QuoteResponse::data
    pub data: Option<Binary>,
}

#[cw_serde]
pub struct QuoteResponse {
    /// The price of the offer
    /// Quoted in ask_denom, so eg for RUIJ @ $5,
    /// a bid of RUJI for ask of USDC, price = 5
    /// a bid of USDC for ask of RUJI, price = 0.2
    ///
    pub price: Decimal,

    /// The size of the ask_denom that the market maker is willing to make
    /// at the given price
    pub size: Uint128,

    /// Optionally provide arbitrary binary data that is returned in subsequent QuoteQuery's
    /// Provided in order to allow the market maker to load from storage only once, instead of
    /// for each iteration of the query
    pub data: Option<Binary>,
}
