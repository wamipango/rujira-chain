use cosmwasm_schema::cw_serde;
use cosmwasm_std::{
    from_json, to_json_binary, Binary, Decimal, Fraction, QuerierWrapper, StdResult, Uint128,
};
use rujira_rs::{fin::market_maker::QuoteResponse, query::Pool, Asset};

use crate::{config::Config, route::Route, ContractError};

pub fn build_route(
    q: QuerierWrapper,
    offer_denom: &String,
    ask_denom: &String,
) -> Result<Route, ContractError> {
    match (Step::load(q, offer_denom)?, Step::load(q, ask_denom)?) {
        (Step::Pool { asset: a, rune: r }, Step::Rune {}) => Ok(Route::AR { a, r }),
        (Step::Rune {}, Step::Pool { asset: b, rune: r }) => Ok(Route::RB { r, b }),
        (Step::Pool { asset: a, rune: r1 }, Step::Pool { asset: b, rune: r2 }) => {
            Ok(Route::ARB { a, r1, r2, b })
        }
        (Step::Rune {}, Step::Rune {}) => Err(ContractError::InvalidRoute {}),
    }
}
#[cw_serde]
pub struct QuoteState {
    route: Route,
    /// Cumulative input (offer) already executed
    input: Uint128,
    /// Cumulative output (ask) already received
    output: Uint128,

    // Cached values to reduce iteration gas cost
    size: Uint128,
    step_ratio: Decimal,
    borrow_limit: Uint128,
    fee: Decimal,
}

#[cw_serde]
pub enum Step {
    Rune {},
    Pool { asset: Uint128, rune: Uint128 },
}

impl Step {
    pub fn load(q: QuerierWrapper, denom: &String) -> Result<Self, ContractError> {
        match denom.as_str() {
            "rune" => Ok(Self::Rune {}),
            _ => {
                let pool = Pool::load(q, &Asset::from_denom(denom)?.to_layer_1())?;
                if pool.trading_halted {
                    return Err(ContractError::InvalidRoute {});
                }
                Ok(Self::Pool {
                    asset: pool.balance_asset,
                    rune: pool.balance_rune,
                })
            }
        }
    }
}

impl QuoteState {
    pub fn quote(&mut self) -> Result<Option<QuoteResponse>, ContractError> {
        let mut input = self.size;
        self.size = self.size.mul_floor(self.step_ratio);

        if input.is_zero() {
            return Ok(None);
        }

        let total_output = self.route.swap(self.input + input);
        let step_output = total_output.checked_sub(self.output).unwrap_or_default();
        if step_output.is_zero() {
            return Ok(None);
        }
        let price = Decimal::from_ratio(step_output, input);
        // Reduce the quote size if we're out of borrowable funds
        let remaining_borrow = self
            .borrow_limit
            .checked_sub(self.output)
            .unwrap_or_default();
        let size = step_output.min(remaining_borrow);
        if size.lt(&step_output) {
            input = size.mul_floor(price.inv().unwrap());
        }

        let fee = size.mul_ceil(self.fee);
        let net_size = size.checked_sub(fee).unwrap_or_default();
        if net_size.is_zero() {
            return Ok(None);
        }
        // Commit new cumulative state
        self.input += input;
        // Use size including fee, otherwise subsequent quotes will hav progressively smaller total outputs
        self.output += size;

        Ok(Some(QuoteResponse {
            // Re-calculate price to accommodate fee
            price: Decimal::from_ratio(net_size, input),
            size: net_size,
            data: Some(self.encode()?),
        }))
    }

    pub fn decode(data: &Binary) -> StdResult<Self> {
        from_json(data)
    }

    pub fn encode(&self) -> StdResult<Binary> {
        to_json_binary(&self)
    }

    pub fn load(
        q: QuerierWrapper,
        offer_denom: &String,
        ask_denom: &String,
        config: &Config,
        borrow_limit: Uint128,
    ) -> Result<Self, ContractError> {
        let route = build_route(q, offer_denom, ask_denom)?;

        let available = route.return_balance().mul_floor(config.max_borrow_ratio);
        Ok(Self {
            route: route.clone(),
            input: Uint128::zero(),
            output: Uint128::zero(),
            size: route.size(config.spread_bps),
            borrow_limit: borrow_limit.min(available),
            fee: config.reserve_fee + config.fee.0,
            step_ratio: config.stream_step_ratio,
        })
    }
}
