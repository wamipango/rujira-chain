use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Decimal, Decimal256, Env, StdResult, Storage, Timestamp, Uint128};
use cw_storage_plus::Item;
use rujira_rs::DecimalSharePool;
use rujira_rs::{ghost::interest::Interest, DecimalScaled, SharePool, SharePoolError};
use std::cmp::min;
use std::ops::{Mul, Sub};

use crate::{config::Config, ContractError};

static STATE: Item<State> = Item::new("state");

#[cw_serde]
pub struct State {
    pub last_updated: Timestamp,

    // Pools representing the ownership of debt, and ownership of deposits + interest
    // Calculated interest is charged on the debt_pool, and the same amount allocated
    // to the deposit_pool. As membership of each grows and shrinks (ie through
    // borrows & repays for debt_pool, and deposits & withdraws for deposit_pool),
    // the effective rate earned by depositors will vary
    pub debt_pool: DecimalSharePool,
    pub deposit_pool: SharePool,
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
                deposit_pool: SharePool::default(),
                pending_interest: DecimalScaled::zero(),
                pending_fees: DecimalScaled::zero(),
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

    pub fn deposit(&mut self, amount: Uint128) -> Result<Uint128, ContractError> {
        Ok(self.deposit_pool.join(amount)?)
    }

    pub fn withdraw(&mut self, shares: Uint128) -> Result<Uint128, ContractError> {
        let amount = self.deposit_pool.leave(shares)?;
        Ok(amount)
    }

    pub fn borrow(&mut self, amount: Uint128) -> Result<Decimal, ContractError> {
        Ok(self.debt_pool.join(Decimal::from_ratio(amount, 1u128))?)
    }

    pub fn repay(&mut self, amount: Uint128, owned: Decimal) -> Result<Decimal, ContractError> {
        if self.debt_pool.size().is_zero() {
            return Err(ContractError::ZeroDebt {});
        }
        let amount_shares = self
            .debt_pool
            .issuance(Decimal::from_ratio(amount, 1u128))
            .min(owned);
        // Calculate the amount of shares that this repay will burn
        // Clamp shares to the size of the pool, to accommodate the final repayment
        let shares = min(amount_shares, self.debt_pool.shares());
        if !shares.is_zero() {
            self.debt_pool.leave(shares)?;
        }
        Ok(shares)
    }

    pub fn capacity(&self) -> Result<Uint128, ContractError> {
        Ok(self
            .deposit_pool
            .size()
            .checked_sub(self.debt_pool.size().to_uint_ceil())?)
    }

    pub fn utilization(&self) -> Result<Decimal, ContractError> {
        // We consider accrued interest and debt in the utilization rate
        if self.deposit_pool.size().is_zero() {
            Ok(Decimal::zero())
        } else {
            Ok(Decimal::one()
                .checked_sub(Decimal::from_ratio(
                    // We use the debt pool size to determine utilization
                    self.capacity()?,
                    self.deposit_pool.size(),
                ))
                .unwrap_or(Decimal::one()))
        }
    }

    pub fn debt_rate(&self, interest: &Interest) -> Result<Decimal, ContractError> {
        Ok(interest.rate(self.utilization()?)?)
    }

    pub fn lend_rate(&self, config: &Config) -> Result<Decimal, ContractError> {
        Ok(config
            .interest
            .rate(self.utilization()?)?
            .mul(self.utilization()?)
            .mul(Decimal::one() - config.fee))
    }

    pub fn distribute_interest(
        &mut self,
        env: &Env,
        config: &Config,
    ) -> Result<Uint128, ContractError> {
        // 1. Calculate the interest accrued at the current debt rate since last allocation
        let debt_rate = Decimal256::from(self.debt_rate(&config.interest)?);
        let duration = env.block.time.seconds().sub(self.last_updated.seconds());
        self.last_updated = env.block.time;
        let part = Decimal256::from_ratio(duration, 31_536_000u128);
        let interest_total = DecimalScaled::from(
            Decimal256::from(self.debt_pool.size())
                .mul(debt_rate)
                .mul(part),
        );

        // 2. Accrue the interest between depositors and protocol fee
        let protocol_fee = interest_total.mul(DecimalScaled::from(Decimal256::from(config.fee)));
        self.pending_fees = self.pending_fees + protocol_fee;
        self.pending_interest = self.pending_interest + interest_total.sub(protocol_fee);

        // 3. Transfer the interest from debt pool to deposit pool.
        // We can only transfer whole amounts to the deposit pool, so decompose and retain remainder
        let (amount, remaining) = self.pending_interest.decompose();
        let amount = Uint128::try_from(amount)?;
        self.pending_interest = remaining;
        self.deposit_pool.deposit(amount)?;
        self.debt_pool.deposit(Decimal::from_ratio(amount, 1u128))?;

        // 4. deposit the protocol fee to the deposit pool to issue shares
        // We can only allocate whole amounts to the deposit pool,so decompose and retain remainder
        let (amount, remaining) = self.pending_fees.decompose();
        let amount = Uint128::try_from(amount)?;
        match self.deposit_pool.join(amount) {
            Ok(shares) => {
                self.debt_pool.deposit(Decimal::from_ratio(amount, 1u128))?;
                self.pending_fees = remaining;
                Ok(shares)
            }
            // if no fee shares were issued, don't update the pending_fees
            Err(SharePoolError::Zero(_)) => Ok(Uint128::zero()),
            Err(err) => Err(err.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::{testing::mock_env, Addr, Decimal};
    use rujira_rs::DecimalScaled;

    #[test]
    fn repay_does_not_burn_more_than_actor_shares() {
        let env = mock_env();
        let mut storage = cosmwasm_std::testing::MockStorage::new();
        State::init(&mut storage, &env).unwrap();
        let mut state = State::load(&storage).unwrap();

        let actor_shares = state.borrow(Uint128::new(50)).unwrap();
        state.borrow(Uint128::new(50)).unwrap();
        state.debt_pool.deposit(Decimal::one()).unwrap();

        let repay_amount = state.debt_pool.ownership(actor_shares).to_uint_ceil();
        let burned = state.repay(repay_amount, actor_shares).unwrap();

        assert_eq!(burned, actor_shares);
    }

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
            fee_address: Addr::unchecked("fee_addr"),
        };

        // Deposit 1000, borrow 800
        state.deposit(Uint128::new(1000)).unwrap();
        state.borrow(Uint128::new(800)).unwrap();

        // Wait 1 second
        let mut env = mock_env();
        env.block.time = state.last_updated.plus_seconds(1);

        // Distribute interest
        let shares = state.distribute_interest(&env, &config).unwrap();

        // No shares minted (fee_int = 0)
        assert_eq!(shares, Uint128::zero());

        // Pool sizes unchanged (net_int = 0)
        assert_eq!(state.deposit_pool.size(), Uint128::new(1000));
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
            fee_address: Addr::unchecked("fee_addr"),
        };

        // Deposit 1000, borrow 800
        state.deposit(Uint128::new(1000)).unwrap();
        state.borrow(Uint128::new(800)).unwrap();

        // Wait 1 year
        let mut env = mock_env();
        env.block.time = state.last_updated.plus_seconds(31_536_000);

        // Distribute interest
        let shares = state.distribute_interest(&env, &config).unwrap();

        // 19 shares minted for protocol fee
        assert_eq!(shares, Uint128::new(19));

        // Pool sizes: net_interest(216) + fee(24) = 240 total
        assert_eq!(state.deposit_pool.size(), Uint128::new(1240));
        assert_eq!(state.debt_pool.size(), Decimal::percent(104000));

        // No pending amounts (exact integer calculations)
        assert_eq!(state.pending_interest, DecimalScaled::zero());
        assert_eq!(state.pending_fees, DecimalScaled::zero());

        // Both pools grow by 240
        assert_eq!(state.deposit_pool.size().u128() - 1000, 240);
        assert_eq!(state.debt_pool.size().to_uint_ceil().u128() - 800, 240);
    }
}
