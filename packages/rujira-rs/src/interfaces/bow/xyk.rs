use crate::bow::error::StrategyError;
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{
    coin, ensure, from_json, to_json_binary, Coin, Decimal, Decimal256, Deps, Env, Isqrt,
    MessageInfo, StdResult, Uint128, Uint256,
};
use cw_utils::NativeBalance;
use std::{
    cmp::min,
    ops::{Add, Mul, Sub},
};

use super::{strategy::Strategy, QuoteRequest, QuoteResponse};

#[cw_serde]
pub struct Xyk {
    x: String,
    y: String,
}

#[cw_serde]
struct Data {
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
    fn load_balances(&self, deps: Deps, env: Env) -> StdResult<Data> {
        let x = deps
            .querier
            .query_balance(env.contract.address.clone(), self.x.clone())?;
        let y = deps
            .querier
            .query_balance(env.contract.address, self.y.clone())?;
        let k = Uint256::from(x.amount) * Uint256::from(y.amount);
        Ok(Data {
            x: x.amount,
            y: y.amount,
            k,
        })
    }
    /// Orientates X & Y into (Offer, Ask) for a given offer_denom
    fn direction_for(
        &self,
        data: &Data,
        offer: &str,
        ask: &str,
    ) -> Result<(Uint128, Uint128), StrategyError> {
        if offer == self.x && ask == self.y {
            return Ok((data.x, data.y));
        };

        if offer == self.y && ask == self.x {
            return Ok((data.y, data.x));
        };
        Err(StrategyError::InvalidPair {})
    }
}

impl Strategy for Xyk {
    fn validate(&self, deps: Deps, env: Env, offer: Coin, ask: Coin) -> Result<(), StrategyError> {
        let balances = self.load_balances(deps, env)?;
        let (offer_balance, ask_balance) =
            self.direction_for(&balances, offer.denom.as_str(), ask.denom.as_str())?;
        let new_offer = Uint256::from(offer_balance.add(offer.amount));
        let new_ask = Uint256::from(ask_balance.checked_sub(ask.amount)?);
        ensure!(
            new_offer.mul(new_ask) >= balances.k,
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
        let mut data: Data = match req.data {
            Some(binary) => from_json(binary)?,
            None => self.load_balances(deps, env)?,
        };
        if data.k.is_zero() {
            return Ok(None);
        }

        let (offer_balance, ask_balance) =
            self.direction_for(&data, req.bid_denom.as_str(), req.ask_denom.as_str())?;

        match req.price {
            Some(_) => Ok(None),
            None => {
                // No price requested. Offer with 0.1% of total funds
                // Following terminology from the perspective of the external query.
                // i.e. offer == denom offered _to_ us, and increasing local balance
                let offer_size = offer_balance.multiply_ratio(1u128, 1000u128);
                let new_offer = offer_balance.sub(offer_size);
                let ratio = Decimal256::from_ratio(data.k, new_offer);
                // Ceil the ask amount in order to ensure rounding keeps new K > K
                let new_ask: Uint128 = Decimal256::one().mul(ratio).to_uint_ceil().try_into()?;
                let ask_size = new_ask.sub(ask_balance);
                let price = Decimal::from_ratio(offer_size, ask_size);
                let size_min = Uint128::from(10000u128);
                if ask_size.lt(&size_min) {
                    return Ok(None);
                }

                // TODO: Apply direction
                data.x += offer_size;
                data.y -= ask_size;
                data.k = Uint256::from(new_ask) * Uint256::from(new_offer);

                Ok(Some(QuoteResponse {
                    price,
                    size: offer_size,
                    data: Some(to_json_binary(&data)?),
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

        let data = self.load_balances(deps, env)?;
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
        let data = self.load_balances(deps, env)?;
        let mut balances = NativeBalance(vec![
            coin(
                data.x.multiply_ratio(balance, supply).u128(),
                self.x.as_str(),
            ),
            coin(
                data.y.multiply_ratio(balance, supply).u128(),
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
        // Test iniital price
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
        // Quote to sell when X/Y price = 0.5
        assert_eq!(
            quote.price,
            Decimal::from_str("0.499499750999374126").unwrap()
        );
        assert_eq!(quote.size, Uint128::from(1000000u128));
        let data: Data = from_json(quote.data.unwrap()).unwrap();
        dbg!(&data);
        assert_eq!(data.x, Uint128::from(1_001_000_000u128));
        assert_eq!(data.y, Uint128::from(1_997_997_997u128));
        assert_eq!(data.k, Uint256::from(2_000_000_000_997_000_000u128));
    }
}
