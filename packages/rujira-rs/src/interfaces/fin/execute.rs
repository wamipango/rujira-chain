use crate::{CallbackData, OraclePair};
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Binary, Decimal, SignedDecimal, Uint128};

use super::{price::Price, side::Side, Denoms, Tick};

#[cw_serde]
pub struct InstantiateMsg {
    /// The denoms of the pair. The second denom is the quote denom
    pub denoms: Denoms,

    pub oracles: Option<OraclePair>,

    /// The address of the market maker contract. Must implement [crate::market_maker::MarketMakerQuery],
    /// and return funds with [CallbackMsg::MarketMaker]
    pub market_makers: Vec<String>,

    /// Ticked truncates Decimal to significant figures.
    /// This accommodates prices decreasing and adding zeroes,
    /// but may need adjusting for better UX as prices increase
    /// (e.g. 4sf is plenty for most tokens < $10 - 1 cent, but above $1000 it creates a $1 tick)  
    pub tick: Tick,

    /// The price delta used to calculate the tick size for Range order book population
    pub range_delta: Decimal,

    /// Minimum base/quote reserve for a Range to be included in order book iteration
    pub range_min: Decimal,

    /// The fee charged on swaps, and instantly filled limit orders
    pub fee_taker: Decimal,

    /// The fee charged on withdrawals from filled limit orders
    pub fee_maker: Decimal,

    /// The fee charged on trades executed via Range liquidity providers
    pub fee_range: Decimal,

    /// The destination address for fees collected
    pub fee_address: String,
}

pub type OrderTarget = (Side, Price, Option<Uint128>);

/// Callable interfaces
#[cw_serde]
pub enum ExecuteMsg {
    /// Executes a market trade based on current order book.
    Swap(SwapRequest),

    /// Messages for managing CCL Ranges
    Range(RangeMsg),

    /// Manage all orders
    /// Submit a list of price and target offer amounts
    /// 0. All filled orders will be withdrawn
    /// For each entry:
    /// 1. If no order exists at that price, one will be created
    /// 2. If an order exists, and the `offer_amount` is greater than the target amount, it will be reduced
    /// 3. If the `offer_amount` is less than the target amount, it will be increased
    ///
    /// Funds sent must be equal to the net change of balances. Funds withdrawn in step 0 and retracted in 1's,
    /// can be reused to fund orders in 1 and 3  
    Order((Vec<OrderTarget>, Option<CallbackData>)),

    Arb {
        then: Option<Binary>,
    },

    /// Callback action to support an arb prior to a swap execution
    DoSwap((Addr, SwapRequest)),
    /// Callback action to support an arb prior to a order execution
    DoOrder((Addr, (Vec<OrderTarget>, Option<CallbackData>))),
    /// Callback action to support an arb prior to a range execution
    DoRange((Addr, RangeMsg)),
}

#[cw_serde]
#[serde(untagged)]
pub enum SwapRequest {
    /// Return at least `min_return` or fail
    Min {
        min_return: Uint128,
        to: Option<String>,
        callback: Option<CallbackData>,
    },
    /// Return exactly `exact_return` or fail
    Exact {
        exact_return: Uint128,
        to: Option<String>,
        callback: Option<CallbackData>,
    },
    /// Swap as much of the input as possible at or better than `price`, returning any unused offer
    Limit {
        /// The Limit price.
        /// N.B. this is quoted in the `ask` token. I.e a buy will be a "normal" price, a sell will be 1/"normal" price;
        price: Decimal,
        to: Option<String>,
        callback: Option<CallbackData>,
    },
    /// YOLO swap for whatever you can get
    Yolo {
        to: Option<String>,
        callback: Option<CallbackData>,
    },
}

#[cw_serde]
pub enum RangeMsg {
    Create {
        /// Specific config of the range
        config: RangeConfig,
        /// Optional (price, slippage) parameters to ensure orderly Range creation
        slippage: Option<(Decimal, Decimal)>,
    },
    Claim {
        idx: Uint128,
    },
    Deposit {
        idx: Uint128,
    },
    Withdraw {
        idx: Uint128,
        amount: Decimal,
    },
    Close {
        idx: Uint128,
    },
    Transfer {
        idx: Uint128,
        to: String,
    },
}

#[cw_serde]
pub struct RangeConfig {
    /// Highest price of the Range
    pub high: Decimal,
    /// Lowest price of the Range
    pub low: Decimal,
    /// Slope of order distribution - y = `slope` * x
    pub skew: SignedDecimal,
    /// Percentage deviation of the best bid/ask price offered by the Range,
    /// Away from the XY=K invariant price
    pub spread: Decimal,
    /// Fee collected from the returned tokens. Should be less than spread otherwise the Range will consume itself
    pub fee: Decimal,
}

impl SwapRequest {
    pub fn callback(&self) -> Option<CallbackData> {
        match self {
            SwapRequest::Min { callback, .. } => callback.clone(),
            SwapRequest::Exact { callback, .. } => callback.clone(),
            SwapRequest::Yolo { callback, .. } => callback.clone(),
            SwapRequest::Limit { callback, .. } => callback.clone(),
        }
    }
    pub fn to(&self) -> Option<String> {
        match self {
            SwapRequest::Min { to, .. } => to.clone(),
            SwapRequest::Exact { to, .. } => to.clone(),
            SwapRequest::Yolo { to, .. } => to.clone(),
            SwapRequest::Limit { to, .. } => to.clone(),
        }
    }
}
