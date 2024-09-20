use cosmwasm_std::{Addr, Deps, Env, StdResult, Storage};
use cw_storage_plus::{Item, Map};
use rujira_rs::{
    staking::{AccountResponse, StatusResponse},
    AccountPool, AccountPoolAccount, SharePool,
};

use crate::config::Config;

pub static POOL_LIQUID: Item<SharePool> = Item::new("l");
pub static ACCOUNTS: Map<Addr, AccountPoolAccount> = Map::new("a");
pub static POOL_ACCOUNTS: Item<AccountPool> = Item::new("p");

pub fn init(storage: &mut dyn Storage) -> StdResult<()> {
    POOL_LIQUID.save(storage, &Default::default())?;
    POOL_ACCOUNTS.save(storage, &Default::default())?;
    Ok(())
}

pub fn status(env: Env, deps: Deps, config: &Config) -> StdResult<StatusResponse> {
    let liquid = POOL_LIQUID.load(deps.storage)?;
    let account = POOL_ACCOUNTS.load(deps.storage)?;

    let revenue_balance = deps
        .querier
        .query_balance(env.contract.address.clone(), config.revenue_denom.clone())?
        .amount;

    let revenue_surplus = revenue_balance.checked_sub(account.pending)?;

    Ok(StatusResponse {
        account_bond: account.total,
        account_revenue: account.pending,
        liquid_bond_shares: liquid.shares(),
        liquid_bond_size: liquid.size(),
        pending_revenue: revenue_surplus,
    })
}

pub fn account(storage: &dyn Storage, addr: Addr) -> StdResult<AccountResponse> {
    let accounts = POOL_ACCOUNTS.load(storage)?;
    let account = ACCOUNTS.load(storage, addr.clone())?;

    Ok(AccountResponse {
        addr,
        bonded: account.amount,
        pending_revenue: accounts.pending_revenue(&account),
    })
}
