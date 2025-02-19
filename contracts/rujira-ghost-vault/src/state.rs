use cosmwasm_schema::cw_serde;
use cosmwasm_std::{ensure, Decimal, Env, StdResult, Storage, Timestamp, Uint128};
use cw_storage_plus::Item;
use rujira_rs::{ghost_vault::Interest, SharePool};
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
    pub debt_pool: SharePool,
    pub deposit_pool: SharePool,
}

impl State {
    pub fn init(storage: &mut dyn Storage, env: &Env) -> StdResult<()> {
        STATE.save(
            storage,
            &Self {
                last_updated: env.block.time,
                debt_pool: SharePool::default(),
                deposit_pool: SharePool::default(),
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

    pub fn withdraw(&mut self, amount: Uint128) -> Result<Uint128, ContractError> {
        let withdrawn = self.deposit_pool.leave(amount)?;
        Ok(withdrawn)
    }

    pub fn borrow(&mut self, amount: Uint128) -> Result<Uint128, ContractError> {
        let mint = self.debt_pool.join(amount)?;

        Ok(mint)
    }

    pub fn repay(&mut self, amount: Uint128, debt_tokens: Uint128) -> Result<(), ContractError> {
        let withdrawn = self.debt_pool.leave(debt_tokens)?;
        // Ensure that the amount being repaid is at least the value of the debt being burned
        ensure!(
            amount.ge(&withdrawn),
            ContractError::InsufficientRepay {
                debt: debt_tokens,
                value: withdrawn,
                repaid: amount
            }
        );
        Ok(())
    }

    pub fn utilization(&self) -> Decimal {
        // We consider accrued interest and debt in the utilization rate
        if self.deposit_pool.size().is_zero() {
            Decimal::zero()
        } else {
            Decimal::one()
                - Decimal::from_ratio(
                    // We use the debt pool size to determine utilization
                    self.deposit_pool.size().sub(self.debt_pool.size()),
                    self.deposit_pool.size(),
                )
        }
    }

    pub fn debt_rate(&self, interest: &Interest) -> StdResult<Decimal> {
        interest.rate(self.utilization())
    }

    pub fn lend_rate(&self, interest: &Interest) -> StdResult<Decimal> {
        Ok(interest.rate(self.utilization())? * self.utilization())
    }

    pub fn calculate_interest(
        &mut self,
        interest: &Interest,
        to: Timestamp,
    ) -> Result<Uint128, ContractError> {
        let rate = self.debt_rate(interest)?;
        let seconds = to.seconds().sub(self.last_updated.seconds());
        let part = Decimal::from_ratio(seconds, 31_536_000u128);
        Ok(Decimal::from_ratio(self.debt_pool.size(), 1u128)
            .mul(rate)
            .mul(part)
            .to_uint_floor())
    }

    pub fn distribute_interest(&mut self, env: &Env, config: &Config) -> Result<(), ContractError> {
        // Calculate interest charged on total debt since last update
        let interest = self.calculate_interest(&config.interest, env.block.time)?;

        // Allocate it to the deposit pool
        self.deposit_pool.deposit(interest)?;
        // Charge it to the debt pool, so that outstanding debt tokens are required to
        // pay this interest on return
        self.debt_pool.deposit(interest)?;
        self.last_updated = env.block.time;

        Ok(())
    }
}
