use cosmwasm_std::{
    coins, Addr, CosmosMsg, Decimal, Deps, Env, QuerierWrapper, StdError, StdResult, Storage,
    Uint128, WasmMsg,
};
use cw_storage_plus::{Item, Map};
use rujira_rs::{
    staking::{AccountResponse, StatusResponse},
    AccountPool, AccountPoolAccount, SharePool,
};
use std::ops::Add;

use crate::{config::Config, ContractError};

static POOL_LIQUID: Item<SharePool> = Item::new("l");
static ACCOUNTS: Map<Addr, AccountPoolAccount> = Map::new("a");
static POOL_ACCOUNTS: Item<AccountPool> = Item::new("p");

pub fn init(storage: &mut dyn Storage) -> StdResult<()> {
    POOL_LIQUID.save(storage, &Default::default())?;
    POOL_ACCOUNTS.save(storage, &Default::default())?;
    Ok(())
}

pub fn execute_account_bond(
    storage: &mut dyn Storage,
    owner: &Addr,
    amount: Uint128,
) -> Result<Uint128, ContractError> {
    let mut pool = POOL_ACCOUNTS.load(storage)?;

    match ACCOUNTS.load(storage, owner.clone()) {
        Ok(mut account) => {
            let rewards = pool.claim(&mut account);
            let account = pool.increase_account(&account, amount);
            ACCOUNTS.save(storage, owner.clone(), &account)?;
            POOL_ACCOUNTS.save(storage, &pool)?;

            Ok(rewards)
        }
        Err(StdError::NotFound { .. }) => {
            let account = pool.join(amount);
            ACCOUNTS.save(storage, owner.clone(), &account)?;
            POOL_ACCOUNTS.save(storage, &pool)?;

            Ok(Uint128::default())
        }
        Err(err) => Err(ContractError::Std(err)),
    }
}

pub fn execute_account_claim(storage: &mut dyn Storage, owner: &Addr) -> StdResult<Uint128> {
    let mut pool = POOL_ACCOUNTS.load(storage)?;
    let mut account = ACCOUNTS.load(storage, owner.clone())?;
    let rewards = pool.claim(&mut account);
    ACCOUNTS.save(storage, owner.clone(), &account)?;
    POOL_ACCOUNTS.save(storage, &pool)?;
    Ok(rewards)
}

pub fn execute_account_withdraw(
    storage: &mut dyn Storage,
    owner: &Addr,
    amount: Option<Uint128>,
) -> Result<(Uint128, Uint128), ContractError> {
    let mut pool = POOL_ACCOUNTS.load(storage)?;
    let mut account = ACCOUNTS.load(storage, owner.clone())?;
    let rewards = pool.claim(&mut account);
    let amount = amount.unwrap_or_else(|| account.amount);
    let account = pool.decrease_account(&account, amount)?;
    ACCOUNTS.save(storage, owner.clone(), &account)?;
    POOL_ACCOUNTS.save(storage, &pool)?;
    Ok((rewards, amount))
}

pub fn execute_liquid_bond(
    storage: &mut dyn Storage,
    amount: Uint128,
) -> Result<Uint128, ContractError> {
    // Add Bond token to the Compounding pool, mint and return the Compound Share Token
    let mut pool = POOL_LIQUID.load(storage)?;
    let shares = pool.join(amount)?;
    POOL_LIQUID.save(storage, &pool)?;
    Ok(shares)
}

pub fn execute_liquid_unbond(
    storage: &mut dyn Storage,
    shares: Uint128,
) -> Result<Uint128, ContractError> {
    let mut pool = POOL_LIQUID.load(storage)?;
    let returned = pool.leave(shares)?;
    POOL_LIQUID.save(storage, &pool)?;
    Ok(returned)
}

/// Calculates the amount to be distributed between (account, liquid) pools;
/// Revenue balance is queried and surplus (ie not allocated to Account stakers) is split pro-rata between ACCOUNT stakers, and LIQUID pool size.
/// Revenue allocated to LIQUID is transformed to a Wasm Execute msg to swap to the bond token
/// The surplus of bond_balance - LIQUID.size() is the return value of the previous swap and can be allocated to the total liquid pool
pub fn distribute(
    env: &Env,
    querier: QuerierWrapper,
    storage: &mut dyn Storage,
    config: &Config,
) -> StdResult<CosmosMsg> {
    let mut account = POOL_ACCOUNTS.load(storage)?;
    let mut liquid = POOL_LIQUID.load(storage)?;

    let bond_balance = querier
        .query_balance(env.contract.address.clone(), config.bond_denom.clone())?
        .amount;

    let revenue_balance = querier
        .query_balance(env.contract.address.clone(), config.revenue_denom.clone())?
        .amount;

    let revenue_surplus = revenue_balance.checked_sub(account.pending)?;

    let account_allocation = Decimal::from_ratio(
        account.total * revenue_surplus,
        account.total.add(liquid.size()),
    )
    .to_uint_floor();

    let liquid_allocation = Decimal::from_ratio(
        liquid.size() * revenue_surplus,
        account.total.add(liquid.size()),
    )
    .to_uint_floor();

    account.distribute(account_allocation);
    POOL_ACCOUNTS.save(storage, &account)?;

    let bond_surplus = bond_balance.checked_sub(liquid.size())?;
    liquid.deposit(bond_surplus);
    POOL_LIQUID.save(storage, &liquid)?;

    Ok(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: config.revenue_converter.0.to_string(),
        msg: config.revenue_converter.1.clone(),
        funds: coins(liquid_allocation.u128(), config.revenue_denom.clone()),
    }))
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
