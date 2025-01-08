use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Binary, Coin, Decimal, Uint128};

use super::Tick;

/// FIN is configured with a privileged, passive market maker.
///
#[cw_serde]
pub struct MarketMakerQuery {
    /// Denom that the market maker should be bidding
    /// ie the token that will be requested by FIN from BOW
    /// Eg. if a market order USDC buy of RUJI on FIN is being
    /// executed, the bid_denom here will be RUJI and the ask_denom
    /// USDC
    pub bid_denom: String,
    /// The denom that the market maker is asking for
    pub ask_denom: String,
    /// The previous price quoted for
    pub last_price: Option<Decimal>,

    /// Total amount of bid_denom that has been committed by the
    /// market maker, prior to execution
    /// Eg in the case of an XY=K market maker, for a given quote
    /// K would = (X - bid_total) * (Y + ask_total)
    pub bid_total: Uint128,

    /// Total amount of ask_denom that FIN will return in exchange for bid_total
    /// of bid_denom at the end of the transaction
    pub ask_total: Uint128,

    /// Tick size of the pair
    pub tick: Tick,

    /// Optional binary data that is simply read from MarketMakerResponse::data and returned by FIN
    pub data: Option<Binary>,
}

#[cw_serde]
pub struct MarketMakerResponse {
    pub price: Decimal,

    /// The size of the bid that the market maker is willing to make
    /// at the requested price
    pub size: Uint128,

    /// Optionally provide arbitrary binary data that is returned in subsequent MarketMakerQuery's
    /// Provided in order to allow the market maker to load from storage only once, instead of
    /// for each iteration of the query
    pub data: Option<Binary>,
}

/// This interface should be callable on the market maker contract
/// min_return should be a straight pass-through to MarketMakerCallback, which
/// returns funds to fin
#[cw_serde]
pub struct MarketMakerExecute {
    pub min_return: Coin,
}

#[cw_serde]
pub struct MarketMakerCallback {
    pub min_return: Coin,
}
impl From<MarketMakerExecute> for MarketMakerCallback {
    fn from(value: MarketMakerExecute) -> Self {
        Self {
            min_return: value.min_return,
        }
    }
}
