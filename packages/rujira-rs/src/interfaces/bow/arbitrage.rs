use crate::{bow::error::StrategyError, Layer1Asset};
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Binary, Coin, Coins, Decimal, Deps, Env, MessageInfo, Uint128};
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
    fn denom(&self) -> String {
        "bow-arb".to_string()
    }

    fn validate(
        &self,
        _deps: Deps,
        _env: Env,
        _info: MessageInfo,
        _offer: Coin,
        _ask: Coin,
    ) -> Result<(), StrategyError> {
        todo!()
    }

    fn quote(
        &self,
        _deps: Deps,
        _env: Env,
        req: QuoteRequest,
    ) -> Result<Option<QuoteResponse>, StrategyError> {
        Route::try_from(req.clone())?.quote(req.min_price, req.data)
    }

    fn calculate_share(
        &self,
        _deps: Deps,
        _env: Env,
        _info: MessageInfo,
        // Current supply of the share token
        _supply: Uint128,
    ) -> Result<(Coins, Uint128), StrategyError> {
        todo!()
    }

    fn calculate_ownership(
        &self,
        _deps: Deps,
        _env: Env,
        _info: MessageInfo,
        _supply: Uint128,
        _balance: Uint128,
    ) -> Result<NativeBalance, StrategyError> {
        todo!()
    }
}

enum Route {
    Single(Layer1Asset),
    Dual(Layer1Asset, Layer1Asset),
}

impl Route {
    pub fn quote(
        &self,
        _min_price: Option<Decimal>,
        _data: Option<Binary>,
    ) -> Result<Option<QuoteResponse>, StrategyError> {
        match self {
            Route::Single(_layer1_asset) => todo!(),
            Route::Dual(_layer1_asset, _layer1_asset1) => todo!(),
        }
    }
}

impl TryFrom<QuoteRequest> for Route {
    type Error = StrategyError;

    fn try_from(value: QuoteRequest) -> Result<Self, Self::Error> {
        match (value.ask_denom.as_str(), value.offer_denom.as_str()) {
            ("rune", asset) => Ok(Self::Single(Layer1Asset::from_native(asset.to_string())?)),
            (asset, "rune") => Ok(Self::Single(Layer1Asset::from_native(asset.to_string())?)),
            (a, b) => Ok(Self::Dual(
                Layer1Asset::from_native(a.to_string())?,
                Layer1Asset::from_native(b.to_string())?,
            )),
        }
    }
}
