use crate::bow::error::StrategyError;
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Coin, Deps, Env, MessageInfo, Uint128};
use cw_utils::NativeBalance;

use super::{strategy::Strategy, QuoteRequest, QuoteResponse};

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
    fn validate(
        &self,
        _deps: Deps,
        _env: Env,
        _offer: Coin,
        _ask: Coin,
    ) -> Result<(), StrategyError> {
        todo!()
    }

    fn quote(
        &self,
        _deps: Deps,
        _env: Env,
        _req: QuoteRequest,
    ) -> Result<Option<QuoteResponse>, StrategyError> {
        todo!()
    }

    fn calculate_share(
        &self,
        _deps: Deps,
        _env: Env,
        _info: MessageInfo,
        // Current supply of the share token
        _supply: Uint128,
    ) -> Result<Uint128, StrategyError> {
        todo!()
    }

    fn calculate_ownership(
        &self,
        _deps: Deps,
        _env: Env,
        _supply: Uint128,
        _balance: Uint128,
    ) -> Result<NativeBalance, StrategyError> {
        todo!()
    }
    //
}
