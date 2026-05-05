use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Decimal, Decimal256, Env, StdResult, Storage, Timestamp, Uint128};
use cw_storage_plus::Item;
use rujira_rs::DecimalSharePool;
use rujira_rs::{ghost::interest::Interest, DecimalScaled};
use std::cmp::min;
use std::ops::{Mul, Sub};

use crate::{config::Config, ContractError};

static STATE: Item<State> = Item::new("state");

#[cw_serde]
pub struct State {
    pub last_updated: Timestamp,
    pub minted: Uint128,
    pub debt_pool: DecimalSharePool,
    #[serde(default)]
    pub pending_interest: DecimalScaled,
    #[serde(default)]
    pub pending_fees: DecimalScaled,
}

impl State {
    pub fn init(storage: &mut dyn Storage, env: &Env) -> StdResult<()> {
        STATE.save(
            storage,
            &Self {
                last_updated: env.block.time,
                debt_pool: DecimalSharePool::default(),
                pending_interest: DecimalScaled::zero(),
                pending_fees: DecimalScaled::zero(),
                minted: Uint128::zero(),
            },
        )?;

        Ok(())
    }

    pub fn load(storage: &dyn Storage) -> StdResult<Self> {
        STATE.load(storage)
    }

    pub fn save(&self, storage: &mut dyn Storage) -> StdResult<()> {
        STATE.save(storage, self)
    }

    pub fn borrow(&mut self, amount: Uint128) -> Result<Decimal, ContractError> {
        self.minted += amount;
        Ok(self.debt_pool.join(Decimal::from_ratio(amount, 1u128))?)
    }

    pub fn repay(&mut self, amount: Uint128, owned: Decimal) -> Result<Decimal, ContractError> {
        if self.debt_pool.size().is_zero() {
            return Err(ContractError::ZeroDebt {});
        }
        self.minted = self.minted.saturating_sub(amount);
        let amount_shares =
            Decimal::from_ratio(amount, 1u128) * self.debt_pool.shares() / self.debt_pool.size();
        // Calculate the amount of shares that this repay will burn
        // Clamp shares to the size of the pool, to accommodate the final repayment
        let shares = min(amount_shares, self.debt_pool.shares()).min(owned);
        if !shares.is_zero() {
            self.debt_pool.leave(shares)?;
        }
        Ok(shares)
    }

    pub fn capacity(&self, mint_cap: Uint128) -> Result<Uint128, ContractError> {
        Ok(mint_cap.saturating_sub(self.debt_pool.size().to_uint_ceil()))
    }

    pub fn utilization(&self, mint_cap: Uint128) -> Result<Decimal, ContractError> {
        // We consider accrued interest and debt in the utilization rate
        if self.debt_pool.size().is_zero() || mint_cap.is_zero() {
            Ok(Decimal::zero())
        } else {
            // We use the debt pool size to determine utilization.
            Ok(self.debt_pool.size() / Decimal::from_ratio(mint_cap, 1u128))
        }
    }

    pub fn debt_rate(
        &self,
        interest: &Interest,
        mint_cap: Uint128,
    ) -> Result<Decimal, ContractError> {
        Ok(interest.rate(self.utilization(mint_cap)?)?)
    }

    pub fn distribute_interest(
        &mut self,
        env: &Env,
        config: &Config,
    ) -> Result<Uint128, ContractError> {
        // 1. Calculate the interest accrued at the current debt rate since last allocation
        let debt_rate = Decimal256::from(self.debt_rate(&config.interest, config.mint_cap)?);
        let duration = env.block.time.seconds().sub(self.last_updated.seconds());
        self.last_updated = env.block.time;
        let part = Decimal256::from_ratio(duration, 31_536_000u128);
        let interest_total = DecimalScaled::from(
            Decimal256::from(self.debt_pool.size())
                .mul(debt_rate)
                .mul(part),
        );

        // 2. Accrue the interest between debt holders and protocol fee
        let protocol_fee = interest_total.mul(DecimalScaled::from(Decimal256::from(config.fee)));
        self.pending_fees = self.pending_fees + protocol_fee;
        self.pending_interest = self.pending_interest + interest_total.sub(protocol_fee);

        // 3. Charge the integerized interest and fees to the debt pool
        let (interest, interest_remaining) = self.pending_interest.decompose();
        let interest = Uint128::try_from(interest)?;
        self.pending_interest = interest_remaining;

        let (fee, fee_remaining) = self.pending_fees.decompose();
        let fee = Uint128::try_from(fee)?;
        self.pending_fees = fee_remaining;

        self.debt_pool
            .deposit(Decimal::from_ratio(interest.checked_add(fee)?, 1u128))?;

        Ok(fee)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::{testing::mock_env, Decimal};
    use rujira_rs::DecimalScaled;

    #[test]
    fn test_distribute_interest_no_mint_path() {
        let env = mock_env();
        let mut storage = cosmwasm_std::testing::MockStorage::new();
        State::init(&mut storage, &env).unwrap();
        let mut state = State::load(&storage).unwrap();

        let config = Config {
            denom: "test".to_string(),
            interest: Interest {
                target_utilization: Decimal::from_ratio(8u128, 10u128),
                base_rate: Decimal::from_ratio(1u128, 1000000u128), // 0.0001% per year
                step1: Decimal::from_ratio(20u128, 100u128),
                step2: Decimal::from_ratio(100u128, 100u128),
            },
            fee: Decimal::from_ratio(1u128, 10u128), // 10% fee
            fee_address: cosmwasm_std::Addr::unchecked("fee_addr"),
            mint_cap: Uint128::new(1000),
        };

        state.borrow(Uint128::new(800)).unwrap();

        // Wait 1 second
        let mut env = mock_env();
        env.block.time = state.last_updated.plus_seconds(1);

        // Distribute interest
        let shares = state.distribute_interest(&env, &config).unwrap();

        // No shares minted (fee_int = 0)
        assert_eq!(shares, Uint128::zero());

        // Pool sizes unchanged (net_int = 0)
        assert_eq!(state.debt_pool.size(), Decimal::percent(80000));

        // All interest in pending amounts
        assert_eq!(
            state.pending_interest,
            DecimalScaled::from_ratio(45662328766017u128, 10u128.pow(19))
        );
        assert_eq!(
            state.pending_fees,
            DecimalScaled::from_ratio(5073592085113u128, 10u128.pow(19))
        );
    }

    #[test]
    fn test_distribute_interest_mint_path() {
        let env = mock_env();
        let mut storage = cosmwasm_std::testing::MockStorage::new();
        State::init(&mut storage, &env).unwrap();
        let mut state = State::load(&storage).unwrap();

        let config = Config {
            denom: "test".to_string(),
            interest: Interest {
                target_utilization: Decimal::from_ratio(8u128, 10u128),
                base_rate: Decimal::from_ratio(10u128, 100u128), // 10% base rate
                step1: Decimal::from_ratio(20u128, 100u128),
                step2: Decimal::from_ratio(100u128, 100u128),
            },
            fee: Decimal::from_ratio(1u128, 10u128), // 10% fee
            fee_address: cosmwasm_std::Addr::unchecked("fee_addr"),
            mint_cap: Uint128::new(1000),
        };

        state.borrow(Uint128::new(800)).unwrap();

        // Wait 1 year
        let mut env = mock_env();
        env.block.time = state.last_updated.plus_seconds(31_536_000);

        // Distribute interest
        let shares = state.distribute_interest(&env, &config).unwrap();

        assert_eq!(shares, Uint128::new(24));
        assert_eq!(state.debt_pool.size(), Decimal::percent(104000));

        // No pending amounts (exact integer calculations)
        assert_eq!(state.pending_interest, DecimalScaled::zero());
        assert_eq!(state.pending_fees, DecimalScaled::zero());
        assert_eq!(state.debt_pool.size().to_uint_ceil().u128() - 800, 240);
    }
}
