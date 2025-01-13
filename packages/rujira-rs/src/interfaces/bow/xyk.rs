use crate::{
    bow::error::StrategyError,
    fin::{MarketMakerQuery, MarketMakerResponse},
    query::Pool,
    Chain, Layer1Asset,
};
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{ensure, Coin, Deps, Env, MessageInfo, StdResult, Uint128};
use cw_utils::{may_pay, NativeBalance};
use std::ops::{Add, Mul};

use super::strategy::Strategy;

#[cw_serde]
pub struct Xyk {
    base: String,
    quote: String,
}

impl Xyk {
    fn load_balances(&self, deps: Deps, env: Env) -> StdResult<(Uint128, Uint128)> {
        let base = deps
            .querier
            .query_balance(env.contract.address.clone(), self.base.clone())?;
        let quote = deps
            .querier
            .query_balance(env.contract.address, self.quote.clone())?;
        Ok((base.amount, quote.amount))
    }
}

impl Strategy for Xyk {
    fn validate(&self, deps: Deps, env: Env, offer: Coin, ask: Coin) -> Result<(), StrategyError> {
        let balances = self.load_balances(deps, env)?;
        let k = balances.0 * balances.1;

        if offer.denom == self.base && ask.denom == self.quote {
            let base = balances.0.checked_sub(offer.amount)?;
            let quote = balances.1.add(ask.amount);
            ensure!(base.mul(quote) >= k, StrategyError::InsufficientReturn {})
        };

        if offer.denom == self.quote && ask.denom == self.base {
            let base = balances.1.checked_sub(offer.amount)?;
            let quote = balances.0.add(ask.amount);
            ensure!(base.mul(quote) >= k, StrategyError::InsufficientReturn {})
        };

        Err(StrategyError::InvalidPair {})
    }

    fn quote(&self, req: MarketMakerQuery) -> Result<MarketMakerResponse, StrategyError> {
        todo!()
    }

    fn calculate_share(
        &self,
        deps: Deps,
        env: Env,
        info: MessageInfo,
    ) -> Result<Uint128, StrategyError> {
        let base = may_pay(&info, self.base.as_str())?;
        let balances = self.load_balances(deps, env)?;
        let k = balances.0 * balances.1;
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
