use crate::{CallbackData, Layer1Asset};
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Binary, Decimal, Uint128};

use super::{price::Price, side::Side, Denoms, Tick};

#[cw_serde]
pub struct InstantiateMsg {
    /// The denoms of the pair. The second denom is the quote denom
    pub denoms: Denoms,

    pub oracles: Option<[Layer1Asset; 2]>,

    /// The address of the market maker contract. Must implement [crate::market_maker::MarketMakerQuery],
    /// and return funds with [CallbackMsg::MarketMaker]
    pub market_maker: Option<String>,

    /// Ticked truncates Decimal to significant figures.
    /// This accommodates prices decreasing and adding zeroes,
    /// but may need adjusting for better UX as prices increase
    /// (e.g. 4sf is plenty for most tokens < $10 - 1 cent, but above $1000 it creates a $1 tick)  
    pub tick: Tick,

    /// The fee charged on swaps, and instantly filled limit orders
    pub fee_taker: Decimal,

    /// The fee charged on withdrawals from filled limit orders
    pub fee_maker: Decimal,

    /// The destination address for fees collected
    pub fee_address: String,
}

pub type OrderTarget = (Side, Price, Option<Uint128>);

/// Callable interfaces
#[cw_serde]
pub enum ExecuteMsg {
    /// Executes a market trade based on current order book.
    Swap(SwapRequest),

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
}

#[cw_serde]
pub struct SwapRequest {
    pub min_return: Option<Uint128>,
    pub to: Option<String>,
    pub callback: Option<CallbackData>,
}
