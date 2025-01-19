use crate::bow::error::StrategyError;
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{
    coin, ensure, from_json, to_json_binary, Coin, Decimal, Deps, Env, Isqrt, MessageInfo,
    StdResult, Uint128, Uint256,
};
use cw_utils::{may_pay, NativeBalance};
use std::{
    cmp::min,
    ops::{Add, Mul},
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
}

impl Strategy for Xyk {
    fn validate(&self, deps: Deps, env: Env, offer: Coin, ask: Coin) -> Result<(), StrategyError> {
        let balances = self.load_balances(deps, env)?;

        if offer.denom == self.x && ask.denom == self.y {
            let x = Uint256::from(balances.x.checked_sub(offer.amount)?);
            let y = Uint256::from(balances.y.add(ask.amount));
            ensure!(x.mul(y) >= balances.k, StrategyError::InsufficientReturn {});
            return Ok(());
        };

        if offer.denom == self.y && ask.denom == self.x {
            let x = Uint256::from(balances.x.checked_sub(ask.amount)?);
            let y = Uint256::from(balances.y.add(offer.amount));
            ensure!(x.mul(y) >= balances.k, StrategyError::InsufficientReturn {});
            return Ok(());
        };

        Err(StrategyError::InvalidPair {})
    }

    fn quote(
        &self,
        deps: Deps,
        env: Env,
        req: QuoteRequest,
    ) -> Result<QuoteResponse, StrategyError> {
        let data: Data = match req.data {
            Some(binary) => from_json(binary)?,
            None => self.load_balances(deps, env)?,
        };
        let price = Decimal::one();
        let size = Uint128::one();
        Ok(QuoteResponse {
            price,
            size,
            data: Some(to_json_binary(&data)?),
        })
    }

    fn calculate_share(
        &self,
        deps: Deps,
        env: Env,
        info: MessageInfo,
        supply: Uint128,
    ) -> Result<Uint128, StrategyError> {
        let deposit_x = may_pay(&info, &self.x.as_str())?;
        let deposit_y = may_pay(&info, self.y.as_str())?;
        let data = self.load_balances(deps, env)?;
        Ok(if supply.is_zero() {
            data.k.isqrt()
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
    use cosmwasm_std::testing::{mock_dependencies_with_balances, mock_env, MOCK_CONTRACT_ADDR};

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
            coin(106, "denom_y"),
        )
        .unwrap();

        xyk.validate(
            deps.as_ref(),
            mock_env(),
            coin(50, "denom_x"),
            coin(105, "denom_y"),
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
}
