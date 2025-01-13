use crate::{
    bow::error::StrategyError,
    fin::{MarketMakerQuery, MarketMakerResponse},
};
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{ensure, Coin, Deps, Env, MessageInfo, StdResult, Uint128};
use cw_utils::{may_pay, NativeBalance};
use std::ops::{Add, Mul};

use super::strategy::Strategy;

/// The Arbitrage strategy quotes for FIN based on the THORChain Pools size & price,
/// and provides instant liquidity for atomic swaps by borrowing from GHOST until
/// the end of the block, when the swap is executed against base layer liquidity,
/// and the loan is repaid.
/// This allows swaps to execute against Fixed Limit Orders, Oracle Limit Orders,
/// and Base Layer Liquidity atomically, with the best overall execution price.
/// Liquidity Providers for this strategy will earn arbitrage profits on this volume
#[cw_serde]
pub struct Arbitrage {}

impl Strategy for Arbitrage {
    fn quote(&self, req: MarketMakerQuery) -> Result<MarketMakerResponse, StrategyError> {
        dbg!(req);
        todo!()
    }
    fn validate(&self, deps: Deps, env: Env, offer: Coin, ask: Coin) -> Result<(), StrategyError> {
        todo!()
    }

    fn calculate_share(
        &self,
        deps: Deps,
        env: Env,
        info: MessageInfo,
    ) -> Result<Uint128, StrategyError> {
        todo!()
    }

    fn calculate_ownership(
        &self,
        deps: Deps,
        env: Env,
        balance: Uint128,
    ) -> Result<NativeBalance, StrategyError> {
        todo!()
    }
}
