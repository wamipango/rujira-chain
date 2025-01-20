use cosmwasm_std::{Addr, StdResult, Storage, Uint128};
use cw_storage_plus::{Item, Map};
use rujira_rs::SharePool;

use crate::config::Config;

// Apply interest to pooled token deposits
pub static DEPOSIT_POOL: Item<SharePool> = Item::new("deposits");
// Apply interest to debt that is issued to trusted contracts when borrows are created
pub static DEBT_POOL: Item<SharePool> = Item::new("debt");

pub static MARKETS: Map<Addr, Uint128> = Map::new("markets");

pub fn update_interest(storage: &mut dyn Storage, config: &Config) -> StdResult<()> {
    // TODO: Calculate interest charged on total debt since last update
    let interest = Uint128::one();

    let mut deposit = DEPOSIT_POOL.load(storage)?;
    let mut debt = DEBT_POOL.load(storage)?;
    // Allocate it to the deposit pool
    deposit.deposit(interest);
    // Charge it to the debt pool, so that outstanding debt tokens are required to
    // pay this interest on return
    debt.deposit(interest);
    DEPOSIT_POOL.save(storage, &deposit)?;
    DEBT_POOL.save(storage, &debt)?;
    Ok(())
}
