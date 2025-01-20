use crate::bow::error::StrategyError;
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{
    coin, ensure, from_json, to_json_binary, Coin, Coins, Decimal, Decimal256, Deps, Env, Isqrt,
    MessageInfo, Uint128, Uint256,
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
    // The % deployed in the first request
    step: Decimal,
    // The minimum value of K required for the pool to be active
    min_k: Uint256,
}

#[cw_serde]
struct PoolState {
    x: Uint128,
    y: Uint128,
    k: Uint256,
}

impl Xyk {
    pub fn new(x: String, y: String, step: Decimal, min_k: Uint256) -> Self {
        Self { x, y, step, min_k }
    }
    fn state(
        &self,
        deps: Deps,
        env: Env,
        funds: Vec<Coin>,
        offer: &str,
        ask: &str,
    ) -> Result<PoolState, StrategyError> {
        // Funds deposited in the tx need to be removed from the current balances to get the correct state
        let mut balances = NativeBalance(deps.querier.query_all_balances(env.contract.address)?);
        for c in funds {
            balances = (balances - c)?;
        }

        let vec = balances.into_vec();
        let x = vec
            .iter()
            .find(|x| x.denom == self.x)
            .map(|x| x.amount)
            .unwrap_or_default();
        let y = vec
            .iter()
            .find(|y| y.denom == self.y)
            .map(|y| y.amount)
            .unwrap_or_default();

        if offer == self.x && ask == self.y {
            return Ok(PoolState::new(x, y));
        }

        // Invert the balances at load time, so all subsequent operations
        // are agnostic to the sale direction
        if offer == self.y && ask == self.x {
            return Ok(PoolState::new(y, x));
        }
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
        let return_amount = y.sub(self.y);
        let price: Decimal = Decimal::from_ratio(return_amount, offer_amount);
        self.k = Uint256::from(self.x) * Uint256::from(self.y);
        ensure!(self.k >= k, StrategyError::Underflow {});
        Ok((return_amount, price))
    }

    pub fn new(x: Uint128, y: Uint128) -> Self {
        let k = Uint256::from(x) * Uint256::from(y);
        Self { x, y, k }
    }

    pub fn price(&self) -> Decimal {
        Decimal::from_ratio(self.y, self.x)
    }
}

impl Strategy for Xyk {
    fn denom(&self) -> String {
        format!("bow-xyk-{}-{}", self.x, self.y)
    }

    fn validate(
        &self,
        deps: Deps,
        env: Env,
        info: MessageInfo,
        offer: Coin,
        ask: Coin,
    ) -> Result<(), StrategyError> {
        let mut state = self.state(
            deps,
            env,
            info.funds,
            offer.denom.as_str(),
            ask.denom.as_str(),
        )?;
        let (return_amount, _) = state.swap(offer.amount)?;
        ensure!(
            return_amount >= ask.amount,
            StrategyError::InsufficientReturn {
                expected: ask.amount,
                returned: return_amount,
            }
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
            None => self.state(
                deps,
                env,
                // Quote is always a query, no need for info.funds
                vec![],
                req.offer_denom.as_str(),
                req.ask_denom.as_str(),
            )?,
        };
        if state.k.lt(&self.min_k) {
            return Ok(None);
        }

        let offer_size = match req.min_price {
            Some(price) => {
                let offer_dec = Decimal::from_ratio(state.x, 1u128);
                let ask_dec = Decimal::from_ratio(state.y, 1u128);
                offer_dec.mul(price).sub(ask_dec).div(price).to_uint_floor()
            }
            None => Decimal::from_ratio(state.x, 1u128)
                .mul(self.step)
                .to_uint_floor(),
        };

        let current_price = state.price();
        let (ask_size, price) = state.swap(offer_size)?;

        if price.ge(&current_price) {
            return Ok(None);
        }

        Ok(Some(QuoteResponse {
            price,
            size: ask_size,
            data: Some(to_json_binary(&state)?),
        }))
    }

    fn calculate_share(
        &self,
        deps: Deps,
        env: Env,
        info: MessageInfo,
        supply: Uint128,
    ) -> Result<(Coins, Uint128), StrategyError> {
        let deposit_x = info.funds.iter().find(|x| x.denom == self.x);
        let deposit_y = info.funds.iter().find(|x| x.denom == self.y);

        match (deposit_x, deposit_y) {
            (Some(x), Some(y)) => {
                let mut deposited = Coins::default();
                deposited.add(x.clone())?;
                deposited.add(y.clone())?;

                let data = self.state(
                    deps,
                    env,
                    info.funds.clone(),
                    self.x.as_str(),
                    self.y.as_str(),
                )?;
                let minted = if supply.is_zero() {
                    Uint256::from(x.amount).mul(Uint256::from(y.amount)).isqrt()
                } else {
                    min(
                        Uint256::from(x.amount).multiply_ratio(supply, data.x),
                        Uint256::from(y.amount).multiply_ratio(supply, data.y),
                    )
                }
                .try_into()?;

                Ok((deposited, minted))
            }
            _ => Err(StrategyError::InvalidDeposit {}),
        }
    }

    fn calculate_ownership(
        &self,
        deps: Deps,
        env: Env,
        info: MessageInfo,
        supply: Uint128,
        balance: Uint128,
    ) -> Result<NativeBalance, StrategyError> {
        if supply.is_zero() || balance.is_zero() {
            return Ok(NativeBalance::default());
        }
        let state = self.state(deps, env, info.funds, self.x.as_str(), self.y.as_str())?;
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
            min_k: Uint256::zero(),
            step: Decimal::zero(),
        };
        let deps = mock_dependencies_with_balances(&[(
            MOCK_CONTRACT_ADDR,
            &vec![coin(1000, "denom_x"), coin(2000, "denom_y")],
        )]);
        let sender = Addr::unchecked("sender");

        xyk.validate(
            deps.as_ref(),
            mock_env(),
            message_info(&sender, &vec![]),
            coin(50, "denom_x"),
            coin(95, "denom_y"),
        )
        .unwrap();

        xyk.validate(
            deps.as_ref(),
            mock_env(),
            message_info(&sender, &vec![]),
            coin(50, "denom_x"),
            coin(96, "denom_y"),
        )
        .unwrap_err();

        xyk.validate(
            deps.as_ref(),
            mock_env(),
            message_info(&sender, &vec![]),
            coin(100, "denom_y"),
            coin(47, "denom_x"),
        )
        .unwrap();

        xyk.validate(
            deps.as_ref(),
            mock_env(),
            message_info(&sender, &vec![]),
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
            min_k: Uint256::zero(),
            step: Decimal::zero(),
        };

        // Initial deposit. Share = sqrt(k)
        let deps = mock_dependencies();

        let (_, amount) = xyk
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
        let (_, amount) = xyk
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
        let (_, amount) = xyk
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

        let (_, amount) = xyk
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
            min_k: Uint256::zero(),
            step: Decimal::zero(),
        };

        // Initial deposit. Share = sqrt(k)
        let deps = mock_dependencies_with_balances(&[(
            MOCK_CONTRACT_ADDR,
            &vec![coin(1000, "denom_x"), coin(2000, "denom_y")],
        )]);

        let amount = xyk
            .calculate_ownership(
                deps.as_ref(),
                mock_env(),
                message_info(&Addr::unchecked("sender"), &vec![]),
                Uint128::zero(),
                Uint128::zero(),
            )
            .unwrap();

        assert_eq!(amount, NativeBalance::default());

        let amount = xyk
            .calculate_ownership(
                deps.as_ref(),
                mock_env(),
                message_info(&Addr::unchecked("sender"), &vec![]),
                Uint128::from(1000u128),
                Uint128::zero(),
            )
            .unwrap();

        assert_eq!(amount, NativeBalance::default());

        let amount = xyk
            .calculate_ownership(
                deps.as_ref(),
                mock_env(),
                message_info(&Addr::unchecked("sender"), &vec![]),
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
            min_k: Uint256::from(2_000_001u128),
            step: Decimal::from_ratio(1u128, 1000u128),
        };

        let deps = mock_dependencies();

        // Check no quote returned when pool is empty
        let quote = xyk
            .quote(
                deps.as_ref(),
                mock_env(),
                QuoteRequest {
                    min_price: None,
                    offer_denom: "denom_x".to_string(),
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
                    min_price: None,
                    offer_denom: "denom_x".to_string(),
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
                    min_price: None,
                    offer_denom: "denom_x".to_string(),
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
                    min_price: Some(Decimal::from_str("1.9979").unwrap()),
                    offer_denom: "denom_x".to_string(),
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

        // Test inverse
        let quote = xyk
            .quote(
                deps.as_ref(),
                mock_env(),
                QuoteRequest {
                    min_price: None,
                    offer_denom: "denom_y".to_string(),
                    ask_denom: "denom_x".to_string(),
                    data: None,
                },
            )
            .unwrap()
            .unwrap();
        // Check direction, poolstate tested above
        assert_eq!(quote.price, Decimal::from_str("0.4995").unwrap());
        assert_eq!(quote.size, Uint128::from(999_000u128));

        // Test subsequent quote
        let quote = xyk
            .quote(
                deps.as_ref(),
                mock_env(),
                QuoteRequest {
                    // Request next tick
                    min_price: Some(Decimal::from_str("0.4994").unwrap()),
                    offer_denom: "denom_y".to_string(),
                    ask_denom: "denom_x".to_string(),
                    data: quote.data,
                },
            )
            .unwrap()
            .unwrap();
        assert_eq!(
            quote.price,
            Decimal::from_str("0.49860314475526708").unwrap()
        );
        assert_eq!(quote.size, Uint128::from(796_527u128));
    }

    #[test]
    fn test_quote_2() {
        let xyk = Xyk {
            x: "btc".to_string(),
            y: "usdc".to_string(),
            min_k: Uint256::from(2_000_001u128),
            step: Decimal::from_ratio(1u128, 1000u128),
        };
        let sender = Addr::unchecked("sender");

        let deps = mock_dependencies_with_balances(&[(
            MOCK_CONTRACT_ADDR,
            &vec![coin(199900100, "btc"), coin(200100000000, "usdc")],
        )]);

        let quote = xyk
            .quote(
                deps.as_ref(),
                mock_env(),
                QuoteRequest {
                    min_price: None,
                    offer_denom: "btc".to_string(),
                    ask_denom: "usdc".to_string(),
                    data: None,
                },
            )
            .unwrap()
            .unwrap();

        assert_eq!(quote.price, Decimal::from_str("1000").unwrap());
        assert_eq!(quote.size, Uint128::from(199900000u128));

        let quote = xyk
            .quote(
                deps.as_ref(),
                mock_env(),
                QuoteRequest {
                    min_price: Some(quote.price),
                    offer_denom: "btc".to_string(),
                    ask_denom: "usdc".to_string(),
                    data: quote.data,
                },
            )
            .unwrap()
            .unwrap();

        assert_eq!(
            quote.price,
            Decimal::from_str("998.003991995997998999").unwrap()
        );
        assert_eq!(quote.size, Uint128::from(199500998u128));

        let quote = xyk
            .quote(
                deps.as_ref(),
                mock_env(),
                QuoteRequest {
                    min_price: Some(quote.price),
                    offer_denom: "btc".to_string(),
                    ask_denom: "usdc".to_string(),
                    data: quote.data,
                },
            )
            .unwrap()
            .unwrap();

        assert_eq!(
            quote.price,
            Decimal::from_str("996.013957048309396245").unwrap()
        );
        assert_eq!(quote.size, Uint128::from(199102194u128));

        // Simulate a swap of 500_000 of btc to usdc across these bids
        // 199900000 for 199900 @ 1000
        // 199500998 for 199900 @ 998.003991995997998999
        // 99800599 for 100200 @ 996.01396207585

        xyk.validate(
            deps.as_ref(),
            mock_env(),
            message_info(&sender, &vec![]),
            coin(500_000, "btc"),
            coin(499_201_597, "usdc"),
        )
        .unwrap();
    }
}
// ---- interfaces::bow::xyk::test::test_quote_2 stdout ----
// [packages/rujira-rs/src/interfaces/bow/xyk.rs:97:9] &state = PoolState {
//     x: Uint128(
//         199900100,
//     ),
//     y: Uint128(
//         200100000000,
//     ),
//     k: Uint256(
//         40000010010000000000,
//     ),
// }
// offer 500000btc
// ask 499201597usdc
// return_amount 499251247
// [packages/rujira-rs/src/interfaces/bow/xyk.rs:102:9] &state = PoolState {
//     x: Uint128(
//         200400100,
//     ),
//     y: Uint128(
//         199600748753,
//     ),
//     k: Uint256(
//         40000010010176075300,
//     ),
// }
