use crate::{
    config::{Config, RUNE},
    contract::rune_balance,
    events::{event_burn, event_mint},
    state::{State, StateCtx},
    ContractError,
};
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{
    coins, from_json, to_json_binary, BankMsg, Binary, Coin, Decimal, MessageInfo, Response,
    Uint128,
};
use cw_utils::must_pay;
use rujira_rs::{
    brune::RangeConfig,
    fin::{market_maker::QuoteResponse, Scl, Side},
    TokenFactory,
};
use std::cmp::Ordering;

#[cw_serde]
pub struct QuoteData {
    next: Decimal,
}

impl State {
    pub fn quote(
        &self,
        ctx: StateCtx<'_>,
        balance: &Decimal,
        bonded: &Decimal,
        data: &Option<Binary>,
        (ask_denom, offer_denom): (String, String),
    ) -> Result<QuoteResponse, ContractError> {
        let token_denom = TokenFactory::new(ctx.env, &ctx.config.token_id).denom();
        if ask_denom == RUNE && offer_denom == token_denom {
            self.quote_bid(ctx.config, balance, bonded, data)
        } else if ask_denom == token_denom && offer_denom == RUNE {
            self.quote_ask(ctx, data)
        } else {
            Err(ContractError::InsufficientFunds {})
        }
    }

    fn quote_ask(
        &self,
        ctx: StateCtx<'_>,
        data: &Option<Binary>,
    ) -> Result<QuoteResponse, ContractError> {
        match data {
            Some(_) => Err(ContractError::InsufficientFunds {}),
            None => {
                let size = self
                    .nodes(ctx.deps, ctx.env, ctx.config, ctx.min_bond)?
                    .capacity()
                    .checked_div(ctx.config.target_utilization)?
                    .to_uint_floor()
                    .min(
                        ctx.config
                            .mint_cap
                            .saturating_sub(self.minted.to_uint_floor()),
                    );

                if size.is_zero() {
                    return Err(ContractError::InsufficientFunds {});
                }

                Ok(QuoteResponse {
                    price: Decimal::one(),
                    size,
                    data: Some(Binary::default()),
                })
            }
        }
    }

    fn quote_bid(
        &self,
        config: &Config,
        balance: &Decimal,
        bonded: &Decimal,
        data: &Option<Binary>,
    ) -> Result<QuoteResponse, ContractError> {
        let data = data.clone().map(from_json::<QuoteData>).transpose()?;
        let start = data
            .as_ref()
            .map(|data| data.next)
            .unwrap_or(config.range.high);

        let quote = self.quote_liquidity(balance, bonded)?;

        let Some((price, size, next)) = quote_slice(&config.range, quote, start) else {
            return Err(ContractError::InsufficientFunds {});
        };

        if size < Uint128::new(50) {
            return Err(ContractError::InsufficientFunds {});
        }

        Ok(QuoteResponse {
            price,
            size,
            data: Some(to_json_binary(&QuoteData { next })?),
        })
    }

    pub fn swap(
        &mut self,
        ctx: StateCtx<'_>,
        info: &MessageInfo,
        min_return: &Coin,
        response: Response,
        bonded: &Decimal,
        rune: &Decimal,
    ) -> Result<Response, ContractError> {
        let token_denom = TokenFactory::new(ctx.env, &ctx.config.token_id).denom();

        if min_return.denom == token_denom {
            self.swap_join(ctx, info, min_return, response)
        } else if min_return.denom == RUNE {
            self.swap_leave(ctx, info, min_return, response, bonded, rune)
        } else {
            Err(ContractError::InsufficientFunds {})
        }
    }

    fn swap_join(
        &mut self,
        ctx: StateCtx<'_>,
        info: &MessageInfo,
        min_return: &Coin,
        response: Response,
    ) -> Result<Response, ContractError> {
        let token = TokenFactory::new(ctx.env, &ctx.config.token_id);
        let rune = must_pay(info, RUNE)?;
        let capacity = self
            .nodes(ctx.deps, ctx.env, ctx.config, ctx.min_bond)?
            .capacity()
            .checked_div(ctx.config.target_utilization)?
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
        if self.minted.to_uint_ceil().gt(&ctx.config.mint_cap) {
            return Err(ContractError::MintCapExceeded(ctx.config.mint_cap));
        }
        Ok(response
            .add_event(event_mint(&info.sender, minted, &token.denom()))
            .add_message(token.mint_msg(minted, info.sender.clone())))
    }

    pub fn swap_leave(
        &mut self,
        ctx: StateCtx<'_>,
        info: &MessageInfo,
        min_return: &Coin,
        response: Response,
        bonded: &Decimal,
        received: &Decimal,
    ) -> Result<Response, ContractError> {
        let token = TokenFactory::new(ctx.env, &ctx.config.token_id);
        let token_denom = token.denom();
        let amount = must_pay(info, &token_denom)?;
        let balance = Self::effective_balance(rune_balance(&ctx.deps, ctx.env)?, Some(*received));
        let quote = self.quote_liquidity(&balance, bonded)?;
        let returned = quote_return(&ctx.config.range, quote, amount);

        if returned.cmp(&min_return.amount) == Ordering::Less {
            return Err(ContractError::InsufficientFunds {});
        }

        self.withdraw(amount)?;

        Ok(response
            .add_event(event_burn(&info.sender, amount, &token_denom))
            .add_message(BankMsg::Send {
                to_address: info.sender.to_string(),
                amount: coins(min_return.amount.u128(), RUNE),
            })
            .add_message(token.burn_msg(amount)))
    }
}

fn quote_slice(
    range: &RangeConfig,
    quote: Decimal,
    start: Decimal,
) -> Option<(Decimal, Uint128, Decimal)> {
    if quote.is_zero() || start <= range.low || start > range.high {
        return None;
    }

    let scl = Scl::new(range.high, range.low, range.skew);
    let step = start * range.step;
    let end = start.checked_sub(step).ok()?.max(range.low);
    if end >= start {
        return None;
    }

    let size = scl.amount_in_slice(&Side::Quote, Decimal::zero(), quote, range.high, start, end);
    let price = Decimal::try_from(scl.effective_price(start, end)?).ok()?;
    let size = size.to_uint_floor();

    if size.is_zero() {
        return None;
    }

    Some((price, size, end))
}

fn quote_return(range: &RangeConfig, quote: Decimal, offer: Uint128) -> Uint128 {
    let mut total = Uint128::zero();
    let mut remaining = offer;
    let mut start = range.high;

    while !remaining.is_zero() {
        let Some((price, size, next)) = quote_slice(range, quote, start) else {
            break;
        };
        let capacity = Decimal::from_ratio(size, 1u128) / price;
        let capacity = capacity.to_uint_floor();

        if capacity.is_zero() {
            break;
        }

        if remaining < capacity {
            total += (Decimal::from_ratio(remaining, 1u128) * price).to_uint_floor();
            break;
        }

        total += size;
        remaining -= capacity;
        start = next;
    }

    total
}

#[cfg(test)]
mod tests {
    use super::*;

    use cosmwasm_std::{Addr, SignedDecimal};
    use cw_utils::Duration;

    fn config() -> Config {
        Config {
            target_utilization: Decimal::percent(75),
            min_node_fee: Decimal::percent(5),
            range: rujira_rs::brune::RangeConfig {
                high: Decimal::percent(99),
                low: Decimal::percent(80),
                skew: SignedDecimal::zero(),
                step: Decimal::percent(10),
            },
            permissionless: false,
            fin_contract: Addr::unchecked("fin"),
            stake_contract: Addr::unchecked("stake"),
            revenue_smear: 1_000,
            quarantine: Duration::Height(10),
            token_id: "brune".to_string(),
            max_bond: Uint128::new(10_000),
            max_effective_bond: Uint128::new(1_000_000),
            mint_cap: Uint128::new(1_000_000),
        }
    }

    fn slice_capacity(price: Decimal, size: Uint128) -> Uint128 {
        (Decimal::from_ratio(size, 1u128) / price).to_uint_floor()
    }

    #[test]
    fn quote_steps_walk_down_the_range() {
        let config = config();
        let quote = Decimal::from_ratio(10_000u128, 1u128);
        let mut start = config.range.high;
        let mut prices = vec![];
        let mut nexts = vec![];

        while let Some((price, size, next)) = quote_slice(&config.range, quote, start) {
            assert!(size > Uint128::zero());
            assert!(price < start);
            assert!(next < start);
            assert!(next >= config.range.low);
            prices.push(price);
            nexts.push(next);
            start = next;
        }

        assert!(prices.len() > 1);
        assert!(prices.windows(2).all(|w| w[1] < w[0]));
        assert_eq!(nexts.last().copied(), Some(config.range.low));
        assert!(quote_slice(&config.range, quote, config.range.low).is_none());
    }

    #[test]
    fn quote_return_matches_full_and_partial_step_fills() {
        let config = config();
        let quote = Decimal::from_ratio(10_000u128, 1u128);

        let (price_1, size_1, next) = quote_slice(&config.range, quote, config.range.high).unwrap();
        let (price_2, _size_2, _) = quote_slice(&config.range, quote, next).unwrap();

        let cap_1 = slice_capacity(price_1, size_1);
        let partial_2 = Uint128::new(37);
        let offer = cap_1 + partial_2;

        let expected = size_1 + (Decimal::from_ratio(partial_2, 1u128) * price_2).to_uint_floor();

        assert_eq!(quote_return(&config.range, quote, offer), expected);
    }

    #[test]
    fn quote_return_stops_at_total_range_liquidity() {
        let config = config();
        let quote = Decimal::from_ratio(10_000u128, 1u128);
        let mut start = config.range.high;
        let mut total_size = Uint128::zero();
        let mut total_capacity = Uint128::zero();

        while let Some((price, size, next)) = quote_slice(&config.range, quote, start) {
            total_size += size;
            total_capacity += slice_capacity(price, size);
            start = next;
        }

        assert_eq!(
            quote_return(&config.range, quote, total_capacity + Uint128::new(1_000)),
            total_size
        );
    }
}
