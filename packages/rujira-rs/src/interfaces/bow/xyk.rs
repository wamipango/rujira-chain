use crate::bow::error::StrategyError;
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{
    coin, ensure, from_json, to_json_binary, Coin, Decimal, Decimal256, Deps, Env, Fraction, Isqrt,
    MessageInfo, StdResult, Uint128, Uint256,
};
use cw_utils::NativeBalance;
use std::{
    cmp::min,
    ops::{Add, Div, Mul, Sub},
};

use super::{strategy::Strategy, QuoteRequest, QuoteResponse};

#[cw_serde]
pub struct Xyk {
    x: String,
    y: String,
}

#[cw_serde]
struct PoolState {
    x: Uint128,
    y: Uint128,
    k: Uint256,
}

impl Xyk {
    pub fn new(x: &str, y: &str) -> Self {
        Self {
            x: x.to_string(),
            y: y.to_string(),
        }
    }
    fn state(
        &self,
        deps: Deps,
        env: Env,
        offer: &str,
        ask: &str,
    ) -> Result<PoolState, StrategyError> {
        let x = deps
            .querier
            .query_balance(env.contract.address.clone(), self.x.clone())?;
        let y = deps
            .querier
            .query_balance(env.contract.address, self.y.clone())?;

        if offer == self.x && ask == self.y {
            return Ok(PoolState::new(x.amount, y.amount));
        };

        // Invert the balances at load time, so all subsequent operations
        // are agnostic to the sale direction
        if offer == self.y && ask == self.x {
            return Ok(PoolState::new(y.amount, x.amount));
        };
        Err(StrategyError::InvalidPair {})
    }
}

impl PoolState {
    /// Swaps an offer_size of X and returns (Amount, Price) of Y
    /// State is loaded via Strategy::state, which orientates X and Y according to the direction of the swap
    pub fn swap(&mut self, offer_amount: Uint128) -> Result<(Uint128, Decimal), StrategyError> {
        let k = self.k;
        self.x = self.x.add(offer_amount);
        let ratio = Decimal256::from_ratio(self.k, self.x);
        let y = self.y;
        // Ceil the ask amount in order to ensure rounding maintains new_k > self.k
        self.y = Decimal256::one().mul(ratio).to_uint_ceil().try_into()?;
        // Y has decreased, get the delta
        let return_amount = y.sub(self.y);
        let price: Decimal = Decimal::from_ratio(return_amount, offer_amount);
        self.k = Uint256::from(self.x) * Uint256::from(self.y);

        // Ensure K has not decreased
        ensure!(self.k >= k, StrategyError::InsufficientReturn {});
        Ok((return_amount, price))
    }

    pub fn new(x: Uint128, y: Uint128) -> Self {
        Self {
            x,
            y,
            k: Uint256::from(x) * Uint256::from(y),
        }
    }
}

impl Strategy for Xyk {
    fn validate(&self, deps: Deps, env: Env, offer: Coin, ask: Coin) -> Result<(), StrategyError> {
        let mut state = self.state(deps, env, offer.denom.as_str(), ask.denom.as_str())?;
        let (return_amount, _) = state.swap(offer.amount)?;
        ensure!(
            return_amount >= ask.amount,
            StrategyError::InsufficientReturn {}
        );

        Ok(())
    }

    fn quote(
        &self,
        deps: Deps,
        env: Env,
        req: QuoteRequest,
    ) -> Result<Option<QuoteResponse>, StrategyError> {
        let mut state: PoolState = match req.data {
            Some(binary) => from_json(binary)?,
            None => self.state(deps, env, req.bid_denom.as_str(), req.ask_denom.as_str())?,
        };
        if state.k.is_zero() {
            return Ok(None);
        }

        match req.price {
            Some(price) => {
                let offer_dec = Decimal::from_ratio(state.x, 1u128);
                let ask_dec = Decimal::from_ratio(state.y, 1u128);
                let offer_size = offer_dec.mul(price).sub(ask_dec).div(price).to_uint_floor();
                let (ask_size, price) = state.swap(offer_size)?;

                Ok(Some(QuoteResponse {
                    price,
                    size: ask_size,
                    data: Some(to_json_binary(&state)?),
                }))
            }
            None => {
                // No price requested. Offer with 0.1% of total funds
                let offer_size = state.x.multiply_ratio(1u128, 1000u128);
                let (ask_size, price) = state.swap(offer_size)?;
                let size_min = Uint128::from(10_000u128);
                if ask_size.lt(&size_min) {
                    return Ok(None);
                }

                Ok(Some(QuoteResponse {
                    price,
                    size: ask_size,
                    data: Some(to_json_binary(&state)?),
                }))
            }
        }
    }

    fn calculate_share(
        &self,
        deps: Deps,
        env: Env,
        info: MessageInfo,
        supply: Uint128,
    ) -> Result<Uint128, StrategyError> {
        let deposit_x = info
            .funds
            .iter()
            .find(|x| x.denom == self.x)
            .map(|x| x.amount)
            .unwrap_or_default();
        let deposit_y = info
            .funds
            .iter()
            .find(|x| x.denom == self.y)
            .map(|x| x.amount)
            .unwrap_or_default();

        let data = self.state(deps, env, self.x.as_str(), self.y.as_str())?;
        Ok(if supply.is_zero() {
            Uint256::from(deposit_x)
                .mul(Uint256::from(deposit_y))
                .isqrt()
        } else {
            min(
                Uint256::from(deposit_x).multiply_ratio(supply, data.x),
                Uint256::from(deposit_y).multiply_ratio(supply, data.y),
            )
        }
        .try_into()?)
    }

    fn calculate_ownership(
        &self,
        deps: Deps,
        env: Env,
        supply: Uint128,
        balance: Uint128,
    ) -> Result<NativeBalance, StrategyError> {
        if supply.is_zero() || balance.is_zero() {
            return Ok(NativeBalance::default());
        }
        let state = self.state(deps, env, self.x.as_str(), self.y.as_str())?;
        if supply == balance {
            let mut balances = NativeBalance(vec![
                coin(state.x.u128(), self.x.as_str()),
                coin(state.y.u128(), self.y.as_str()),
            ]);
            balances.normalize();
            return Ok(balances);
        }

        let mut balances = NativeBalance(vec![
            coin(
                state.x.multiply_ratio(balance, supply).u128(),
                self.x.as_str(),
            ),
            coin(
                state.y.multiply_ratio(balance, supply).u128(),
                self.y.as_str(),
            ),
        ]);
        balances.normalize();

        Ok(balances)
    }
}

#[cfg(test)]
mod test {
    use std::str::FromStr;

    use cosmwasm_std::{
        testing::{
            message_info, mock_dependencies, mock_dependencies_with_balances, mock_env,
            MOCK_CONTRACT_ADDR,
        },
        Addr,
    };

    use super::*;

    #[test]
    fn test_validate() {
        let xyk = Xyk {
            x: "denom_x".to_string(),
            y: "denom_y".to_string(),
        };
        let deps = mock_dependencies_with_balances(&[(
            MOCK_CONTRACT_ADDR,
            &vec![coin(1000, "denom_x"), coin(2000, "denom_y")],
        )]);

        xyk.validate(
            deps.as_ref(),
            mock_env(),
            coin(50, "denom_x"),
            coin(95, "denom_y"),
        )
        .unwrap();

        xyk.validate(
            deps.as_ref(),
            mock_env(),
            coin(50, "denom_x"),
            coin(96, "denom_y"),
        )
        .unwrap_err();

        xyk.validate(
            deps.as_ref(),
            mock_env(),
            coin(100, "denom_y"),
            coin(47, "denom_x"),
        )
        .unwrap();

        xyk.validate(
            deps.as_ref(),
            mock_env(),
            coin(100, "denom_y"),
            coin(48, "denom_x"),
        )
        .unwrap_err();
    }

    #[test]
    fn test_share() {
        let xyk = Xyk {
            x: "denom_x".to_string(),
            y: "denom_y".to_string(),
        };

        // Initial deposit. Share = sqrt(k)
        let deps = mock_dependencies();

        let amount = xyk
            .calculate_share(
                deps.as_ref(),
                mock_env(),
                message_info(
                    &Addr::unchecked("sender"),
                    &vec![coin(1000, "denom_x"), coin(2000, "denom_y")],
                ),
                Uint128::zero(),
            )
            .unwrap();

        assert_eq!(amount, Uint128::from(1414u128));

        let deps = mock_dependencies_with_balances(&[(
            MOCK_CONTRACT_ADDR,
            &vec![coin(1000, "denom_x"), coin(2000, "denom_y")],
        )]);

        // Adding 50% in correct ratio
        let amount = xyk
            .calculate_share(
                deps.as_ref(),
                mock_env(),
                message_info(
                    &Addr::unchecked("sender"),
                    &vec![coin(500, "denom_x"), coin(1000, "denom_y")],
                ),
                Uint128::from(1414u128),
            )
            .unwrap();
        assert_eq!(amount, Uint128::from(707u128));

        // Adding more than 50% on one side
        let amount = xyk
            .calculate_share(
                deps.as_ref(),
                mock_env(),
                message_info(
                    &Addr::unchecked("sender"),
                    &vec![coin(500, "denom_x"), coin(2000, "denom_y")],
                ),
                Uint128::from(1414u128),
            )
            .unwrap();
        assert_eq!(amount, Uint128::from(707u128));

        let amount = xyk
            .calculate_share(
                deps.as_ref(),
                mock_env(),
                message_info(
                    &Addr::unchecked("sender"),
                    &vec![coin(2000, "denom_x"), coin(1000, "denom_y")],
                ),
                Uint128::from(1414u128),
            )
            .unwrap();
        assert_eq!(amount, Uint128::from(707u128));
    }

    #[test]
    fn test_ownership() {
        let xyk = Xyk {
            x: "denom_x".to_string(),
            y: "denom_y".to_string(),
        };

        // Initial deposit. Share = sqrt(k)
        let deps = mock_dependencies_with_balances(&[(
            MOCK_CONTRACT_ADDR,
            &vec![coin(1000, "denom_x"), coin(2000, "denom_y")],
        )]);

        let amount = xyk
            .calculate_ownership(deps.as_ref(), mock_env(), Uint128::zero(), Uint128::zero())
            .unwrap();

        assert_eq!(amount, NativeBalance::default());

        let amount = xyk
            .calculate_ownership(
                deps.as_ref(),
                mock_env(),
                Uint128::from(1000u128),
                Uint128::zero(),
            )
            .unwrap();

        assert_eq!(amount, NativeBalance::default());

        let amount = xyk
            .calculate_ownership(
                deps.as_ref(),
                mock_env(),
                Uint128::from(1000u128),
                Uint128::from(125u128),
            )
            .unwrap();

        assert_eq!(
            amount,
            NativeBalance(vec![coin(125, "denom_x"), coin(250, "denom_y")])
        );
    }

    #[test]
    fn pool_state_swaps() {
        let mut state = PoolState::new(Uint128::from(10_000u128), Uint128::from(20_000u128));
        let (returned, price) = state.swap(Uint128::from(100u128)).unwrap();
        assert_eq!(returned, Uint128::from(198u128));
        assert_eq!(price, Decimal::from_str("1.98").unwrap());
        assert_eq!(state.x, Uint128::from(10_100u128));
        assert_eq!(state.y, Uint128::from(19_802u128));
        assert_eq!(state.k, Uint256::from(200_000_200u128));

        let mut state = PoolState::new(Uint128::from(20_000u128), Uint128::from(10_000u128));
        let (returned, price) = state.swap(Uint128::from(100u128)).unwrap();
        assert_eq!(returned, Uint128::from(49u128));
        assert_eq!(price, Decimal::from_str("0.49").unwrap());
        assert_eq!(state.x, Uint128::from(20_100u128));
        assert_eq!(state.y, Uint128::from(9_951u128));
        assert_eq!(state.k, Uint256::from(200_015_100u128));
    }

    #[test]
    fn test_quote() {
        let xyk = Xyk {
            x: "denom_x".to_string(),
            y: "denom_y".to_string(),
        };

        let deps = mock_dependencies();

        // Check no quote returned when pool is empty
        let quote = xyk
            .quote(
                deps.as_ref(),
                mock_env(),
                QuoteRequest {
                    price: None,
                    bid_denom: "denom_x".to_string(),
                    ask_denom: "denom_y".to_string(),
                    data: None,
                },
            )
            .unwrap();
        assert_eq!(quote, None);

        let deps = mock_dependencies_with_balances(&[(
            MOCK_CONTRACT_ADDR,
            &vec![coin(1000, "denom_x"), coin(2000, "denom_y")],
        )]);
        // Test with low balances
        let quote = xyk
            .quote(
                deps.as_ref(),
                mock_env(),
                QuoteRequest {
                    price: None,
                    bid_denom: "denom_x".to_string(),
                    ask_denom: "denom_y".to_string(),
                    data: None,
                },
            )
            .unwrap();
        assert_eq!(quote, None);

        let deps = mock_dependencies_with_balances(&[(
            MOCK_CONTRACT_ADDR,
            &vec![
                coin(1_000_000_000, "denom_x"),
                coin(2_000_000_000, "denom_y"),
            ],
        )]);
        // Test initial price
        let quote = xyk
            .quote(
                deps.as_ref(),
                mock_env(),
                QuoteRequest {
                    price: None,
                    bid_denom: "denom_x".to_string(),
                    ask_denom: "denom_y".to_string(),
                    data: None,
                },
            )
            .unwrap()
            .unwrap();
        // Request is to sell X for Y. Price should be lower than base price (2/1)
        assert_eq!(quote.price, Decimal::from_str("1.998001").unwrap());
        assert_eq!(quote.size, Uint128::from(1_998_001u128));
        let data: PoolState = from_json(quote.data.clone().unwrap()).unwrap();
        assert_eq!(data.x, Uint128::from(1_001_000_000u128));
        assert_eq!(data.y, Uint128::from(1_998_001_999u128));
        assert_eq!(data.k, Uint256::from(2_000_000_000_999_000_000u128));

        // Test subsequent quote
        let quote = xyk
            .quote(
                deps.as_ref(),
                mock_env(),
                QuoteRequest {
                    // Request next tick
                    price: Some(Decimal::from_str("1.9979").unwrap()),
                    bid_denom: "denom_x".to_string(),
                    ask_denom: "denom_y".to_string(),
                    data: quote.data,
                },
            )
            .unwrap()
            .unwrap();
        assert_eq!(
            quote.price,
            Decimal::from_str("1.994114522849561513").unwrap()
        );
        assert_eq!(quote.size, Uint128::from(1_892_307u128));
        let data: PoolState = from_json(quote.data.unwrap()).unwrap();
        assert_eq!(data.x, Uint128::from(1_001_948_946u128));
        assert_eq!(data.y, Uint128::from(1_996_109_692u128));
        assert_eq!(data.k, Uint256::from(2_000_000_001_999_784_632u128));
    }
}
