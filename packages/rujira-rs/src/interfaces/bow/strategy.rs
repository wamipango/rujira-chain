use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Coin, Coins, Deps, Env, MessageInfo, Uint128};
use cw_utils::NativeBalance;

use super::{arbitrage::Arbitrage, error::StrategyError, xyk::Xyk, QuoteRequest, QuoteResponse};

pub trait Strategy {
    /// The receipt token denom string for the strategy
    fn denom(&self) -> String;
    /// Validates a swap size against the strategy
    /// Offer is the amount offered _to_ the strategy (ie increase in local balance)
    /// and Ask is amount requested _from_ the strategy (decrease)
    fn validate(
        &self,
        deps: Deps,
        env: Env,
        info: MessageInfo,
        offer: Coin,
        ask: Coin,
    ) -> Result<(), StrategyError>;

    /// Quotes for a FIN market maker request
    fn quote(
        &self,
        deps: Deps,
        env: Env,
        req: QuoteRequest,
    ) -> Result<Option<QuoteResponse>, StrategyError>;

    /// Calculates the number of share tokens to be minted for a new deposit
    fn calculate_share(
        &self,
        deps: Deps,
        env: Env,
        info: MessageInfo,
        // Current supply of the share token
        supply: Uint128,
    ) -> Result<(Coins, Uint128), StrategyError>;

    /// Calculates the underlying assets owned by a given share token amount
    fn calculate_ownership(
        &self,
        deps: Deps,
        env: Env,
        info: MessageInfo,
        supply: Uint128,
        balance: Uint128,
    ) -> Result<NativeBalance, StrategyError>;
}

#[cw_serde]
pub enum Strategies {
    Arbitrage(Arbitrage),
    Xyk(Xyk),
}

macro_rules! delegate_strategy {
    ($self:ident, $method:ident $(, $args:expr)*) => {
        match $self {
            Strategies::Arbitrage(inner) => inner.$method($($args),*),
            Strategies::Xyk(inner) => inner.$method($($args),*),
        }
    };
}

impl Strategy for Strategies {
    fn denom(&self) -> String {
        delegate_strategy!(self, denom)
    }

    fn validate(
        &self,
        deps: Deps,
        env: Env,
        info: MessageInfo,
        offer: Coin,
        ask: Coin,
    ) -> Result<(), StrategyError> {
        delegate_strategy!(self, validate, deps, env, info, offer, ask)
    }

    fn quote(
        &self,
        deps: Deps,
        env: Env,
        req: QuoteRequest,
    ) -> Result<Option<QuoteResponse>, StrategyError> {
        delegate_strategy!(self, quote, deps, env, req)
    }

    fn calculate_share(
        &self,
        deps: Deps,
        env: Env,
        info: MessageInfo,
        supply: Uint128,
    ) -> Result<(Coins, Uint128), StrategyError> {
        delegate_strategy!(self, calculate_share, deps, env, info, supply)
    }

    fn calculate_ownership(
        &self,
        deps: Deps,
        env: Env,
        info: MessageInfo,
        supply: Uint128,
        balance: Uint128,
    ) -> Result<NativeBalance, StrategyError> {
        delegate_strategy!(self, calculate_ownership, deps, env, info, supply, balance)
    }
}
