use cosmwasm_schema::cw_serde;
use cosmwasm_std::{ensure, Addr, Decimal, Env, StdResult, Storage, Timestamp, Uint128};
use cw_storage_plus::{Item, Map};
use rujira_rs::SharePool;

use crate::{config::Config, ContractError};

// Apply interest to pooled token deposits
pub static DEPOSIT_POOL: Item<SharePool> = Item::new("deposits");

pub static MARKETS: Map<Addr, Uint128> = Map::new("markets");

pub static STATE: Item<State> = Item::new("state");

#[cw_serde]
pub struct State {
    last_updated: Timestamp,
    // Total amount of debt that interest is chargeable on
    total_borrows: Uint128,

    debt_pool: SharePool,
}

impl State {
    pub fn init(storage: &mut dyn Storage, env: &Env) -> StdResult<()> {
        DEPOSIT_POOL.save(storage, &SharePool::default())?;
        STATE.save(
            storage,
            &Self {
                last_updated: env.block.time,
                total_borrows: Uint128::zero(),
                debt_pool: SharePool::default(),
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

    pub fn borrow(&mut self, amount: Uint128) -> Result<Uint128, ContractError> {
        let mint = self.debt_pool.join(amount)?;
        self.total_borrows += amount;

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
        self.total_borrows -= amount;
        Ok(())
    }

    pub fn calculate_interest(&mut self, to: Timestamp) -> Uint128 {
        let debt_amount = self.total_borrows;
        let rate = Decimal::one();

        self.last_updated = to;
        Uint128::one()
    }

    pub fn distribute_interest(
        &mut self,
        storage: &mut dyn Storage,
        env: &Env,
        config: &Config,
    ) -> StdResult<()> {
        let mut deposit = DEPOSIT_POOL.load(storage)?;

        // TODO: Calculate interest charged on total debt since last update
        let interest = self.calculate_interest(env.block.time);

        // Allocate it to the deposit pool
        deposit.deposit(interest);
        // Charge it to the debt pool, so that outstanding debt tokens are required to
        // pay this interest on return
        self.debt_pool.deposit(interest);
        DEPOSIT_POOL.save(storage, &deposit)?;
        Ok(())
    }
}
