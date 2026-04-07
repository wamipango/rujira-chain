use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Coin, Decimal, SignedDecimal, Timestamp, Uint128};

use crate::OraclePair;

use super::{side::Side, Denoms, Price, Tick};

/// Standard interface to query contract state
#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(ConfigResponse)]
    Config {},

    #[returns(SimulationResponse)]
    Simulate(Coin),

    /// Find a specific order for a user at a price
    #[returns(OrderResponse)]
    Order((String, Side, Price)),

    /// Paginate orders using cursor-based pagination.
    /// When owner is None, all orders are returned
    #[returns(OrdersResponse)]
    Orders {
        owner: Option<String>,
        /// When Side is provided, orders are sorted by price
        /// N.B: This sorts on the underlying Price key, not the effective execution price,
        /// so Oracle and Fixed prices will be grouped
        side: Option<Side>,
        /// Cursor from the last item of the previous page: (owner, side, price)
        start_after: Option<(String, Side, Price)>,
        limit: Option<u8>,
    },

    #[returns(RangeResponse)]
    Range(Uint128),

    #[returns(RangesResponse)]
    Ranges {
        owner: Option<String>,
        cursor: Option<Uint128>,
        limit: Option<u8>,
    },

    #[returns(BookResponse)]
    Book {
        limit: Option<u8>,
        offset: Option<u32>,
    },
}

#[cw_serde]
pub struct ConfigResponse {
    /// See [InstantiateMsg::denoms]
    pub denoms: Denoms,

    /// See [InstantiateMsg::oracles]
    pub oracles: Option<OraclePair>,

    /// See [InstantiateMsg::market_maker]
    pub market_makers: Vec<String>,

    /// See [InstantiateMsg::tick]
    pub tick: Tick,

    /// See [InstantiateMsg::range_delta]
    pub range_delta: Decimal,

    /// See [InstantiateMsg::range_min]
    pub range_min: Decimal,

    /// See [InstantiateMsg::fee_taker]    
    pub fee_taker: Decimal,

    /// See [InstantiateMsg::fee_maker]
    pub fee_maker: Decimal,

    /// See [InstantiateMsg::fee_range]
    pub fee_range: Decimal,

    /// See [InstantiateMsg::fee_address]
    pub fee_address: String,
}

#[cw_serde]
pub struct OrderResponse {
    /// The account which placed the order
    pub owner: String,

    /// The side of the order
    pub side: Side,

    /// The quote price of this order
    pub price: Price,

    /// The rate at which this order would execute at the current moment in time
    pub rate: Decimal,

    /// The last time this order was touched (created, incremented or reduced) in an Order execution
    pub updated_at: Timestamp,

    /// Offer amount at updated_at time
    pub offer: Uint128,

    /// The remaining offer amount
    pub remaining: Uint128,

    /// Amount of filled order awaiting withdrawal
    pub filled: Uint128,
}

#[cw_serde]
pub struct OrdersResponse {
    pub orders: Vec<OrderResponse>,
}

#[cw_serde]
pub struct RangeResponse {
    pub idx: Uint128,
    pub owner: String,
    pub high: Decimal,
    pub low: Decimal,
    pub skew: SignedDecimal,
    pub spread: Decimal,
    pub fee: Decimal,
    pub base: Decimal,
    pub quote: Decimal,
    pub price: Decimal,
    pub ask: Decimal,
    pub bid: Decimal,
    pub fees: [Decimal; 2],
}

#[cw_serde]
pub struct RangesResponse {
    pub ranges: Vec<RangeResponse>,
}

#[cw_serde]
pub struct BookResponse {
    pub base: Vec<BookItemResponse>,
    pub quote: Vec<BookItemResponse>,
}

#[cw_serde]
pub struct BookItemResponse {
    pub price: Decimal,
    pub total: Uint128,
}

#[cw_serde]
pub struct SimulationResponse {
    pub returned: Uint128,
    pub fee: Uint128,
}
