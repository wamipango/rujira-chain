use crate::{CallbackData, Layer1Asset};
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Decimal, Uint128};

use super::Denoms;

#[cw_serde]
pub struct InstantiateMsg {
    /// The denoms of the pair. The second denom is the bid denom
    pub denoms: Denoms,

    pub oracles: [Layer1Asset; 2],

    /// The maximum premium that can be bid (in %)
    pub max_premium: u8,

    /// The address of the market maker contract. Must implement [crate::market_maker::MarketMakerQuery],
    /// and return funds with [CallbackMsg::MarketMaker]
    pub market_maker: Option<String>,

    /// The fee charged on swaps, and instantly filled limit orders
    pub fee_taker: Decimal,

    /// The fee charged on withdrawals from filled limit orders
    pub fee_maker: Decimal,

    /// The destination address for fees collected
    pub fee_address: String,
}

/// Callable interfaces
#[cw_serde]
pub enum ExecuteMsg {
    /// Executes a market trade based on current order book.
    Swap {
        min_return: Option<Uint128>,
        to: Option<String>,

        /// An optional callback that FIN will execute with the funds from the swap.
        /// The callback is executed on the sender's address.
        #[serde(skip_serializing_if = "Option::is_none")]
        callback: Option<CallbackData>,
    },

    /// Manage all orders
    /// Submit a list of premium and offer amounts
    /// 0. All filled orders will be withdrawn
    /// For each entry:
    /// 1. If no order exists at that premium, one will be created
    /// 2. If an order exists, and the `offer_amount` is greater than the target amount, it will be reduced
    /// 3. If the `offer_amount` is less than the target amount, it will be increased
    ///
    /// Funds sent must be equal to the net change of balances. Funds withdrawn in step 0 and retracted in 1's,
    /// can be reused to fund orders in 1 and 3  
    Order(Vec<(u8, Uint128)>),
}
