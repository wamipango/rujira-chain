use crate::{
    config::{Config, RUNE},
    events::{event_burn, event_mint},
    state::State,
    ContractError,
};
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{
    coins, from_json, to_json_binary, BankMsg, Binary, Coin, Decimal, Deps, Env, MessageInfo,
    Response, Uint128,
};
use cw_utils::must_pay;
use rujira_rs::{
    bow::{self, QuoteRequest, QuoteResponse, XykState},
    TokenFactory,
};
use std::ops::{Div, Mul, Sub};

#[cw_serde]
struct QuoteData {
    bid: Decimal,
    last: Decimal,
    y: Decimal,
    x: Decimal,
    fac: Decimal,
}

impl State {
    pub fn quote(
        &self,
        deps: Deps,
        env: &Env,
        config: &Config,
        req: &QuoteRequest,
    ) -> Result<QuoteResponse, ContractError> {
        let token = TokenFactory::new(env, &config.token_id);
        if req.ask_denom == RUNE && req.offer_denom == token.denom() {
            let delta = Decimal::permille(995);
            self.quote_bid(deps, env, config, delta, req)
        } else if req.ask_denom == token.denom() && req.offer_denom == RUNE {
            self.quote_ask(deps, env, config, req)
        } else {
            Err(ContractError::InsufficientFunds {})
        }
    }

    fn quote_ask(
        &self,
        deps: Deps,
        env: &Env,
        config: &Config,
        req: &QuoteRequest,
    ) -> Result<QuoteResponse, ContractError> {
        match req.data {
            // We've already quoted once
            Some(_) => Err(ContractError::InsufficientFunds {}),
            None => {
                let size = self
                    .nodes(deps, env, config)?
                    .capacity()
                    .div(config.target_utilization)
                    .to_uint_floor()
                    .min(config.mint_cap - self.minted.to_uint_floor());

                if size.is_zero() {
                    return Err(ContractError::InsufficientFunds {});
                }

                Ok(bow::QuoteResponse {
                    price: Decimal::one(),
                    size,
                    data: Some(Binary::default()),
                })
            }
        }
    }

    fn quote_bid(
        &self,
        deps: Deps,
        env: &Env,
        config: &Config,
        delta: Decimal,
        req: &QuoteRequest,
    ) -> Result<QuoteResponse, ContractError> {
        let (x, y) = match req.data.clone().map(from_json::<QuoteData>).transpose()? {
            None => self.xy(deps, env, config, None)?,
            Some(QuoteData { y, x, .. }) => (x, y),
        };
        let fac = Decimal::one().sub(config.spread);
        let bid = y.checked_div(x)?.min(Decimal::one()).mul(fac);
        let next = bid * delta;
        let price = bid.mul(next).sqrt();
        if bid < Decimal::one() {
            return Err(ContractError::InsufficientFunds {});
        }
        let size = bid.sub(next).mul(y).div(bid).to_uint_floor();
        if size < Uint128::new(50) {
            return Err(ContractError::InsufficientFunds {});
        }
        Ok(QuoteResponse {
            price,
            size,
            data: Some(to_json_binary(&QuoteData {
                bid,
                last: next,
                y,
                x,
                fac,
            })?),
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn swap(
        &mut self,
        deps: Deps,
        env: &Env,
        info: &MessageInfo,
        config: &Config,
        min_return: &Coin,
        response: Response,
        rune: Decimal,
    ) -> Result<Response, ContractError> {
        let token = TokenFactory::new(env, &config.token_id);

        if min_return.denom == token.denom() {
            self.swap_join(deps, env, info, config, &token, min_return, response)
        } else if min_return.denom == RUNE {
            self.swap_leave(deps, env, info, config, &token, min_return, response, rune)
        } else {
            Err(ContractError::InsufficientFunds {})
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn swap_join(
        &mut self,
        deps: Deps,
        env: &Env,
        info: &MessageInfo,
        config: &Config,
        token: &TokenFactory,
        min_return: &Coin,
        response: Response,
    ) -> Result<Response, ContractError> {
        let rune = must_pay(info, RUNE)?;
        let capacity = self
            .nodes(deps, env, config)?
            .capacity()
            .div(config.target_utilization)
            .to_uint_floor();

        if min_return.amount > rune {
            return Err(ContractError::InsufficientReturn {
                offer: rune,
                ask: min_return.amount,
            });
        }

        if rune > capacity {
            return Err(ContractError::InsufficientCapacity(capacity));
        }
        let minted = self.deposit(rune)?;
        if self.minted.to_uint_ceil().gt(&config.mint_cap) {
            return Err(ContractError::MintCapExceeded(config.mint_cap));
        }
        Ok(response
            .add_event(event_mint(&info.sender, minted, &token.denom()))
            .add_message(token.mint_msg(minted, info.sender.clone())))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn swap_leave(
        &mut self,
        deps: Deps,
        env: &Env,
        info: &MessageInfo,
        config: &Config,
        token: &TokenFactory,
        min_return: &Coin,
        response: Response,
        received: Decimal,
    ) -> Result<Response, ContractError> {
        let (x, y) = self.xy(deps, env, config, Some(received))?;
        let mut xyk = XykState::new();
        let amount = must_pay(info, &token.denom())?;
        xyk.set(x.to_uint_floor(), y.to_uint_floor());
        if xyk.swap(&amount)? >= min_return.amount {
            self.withdraw(amount)?;

            Ok(response
                .add_event(event_burn(&info.sender, amount, &token.denom()))
                .add_message(BankMsg::Send {
                    to_address: info.sender.to_string(),
                    amount: coins(min_return.amount.u128(), RUNE),
                })
                .add_message(token.burn_msg(amount)))
        } else {
            Err(ContractError::InsufficientFunds {})
        }
    }
}
