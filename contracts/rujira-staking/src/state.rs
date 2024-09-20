use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Decimal, Deps, Env, StdResult, Storage, Uint128};
use cw_storage_plus::{Item, Map};
use rujira_rs::{
    staking::{AccountResponse, StatusResponse},
    SharePool,
};
use std::ops::Mul;

use crate::{config::Config, ContractError};

pub static POOL_LIQUID: Item<SharePool> = Item::new("l");
pub static ACCOUNTS: Map<Addr, PoolAccount> = Map::new("a");
pub static POOL_ACCOUNTS: Item<PoolAccounts> = Item::new("p");

#[cw_serde]
pub struct PoolAccounts {
    pub pending: Uint128,
    pub total: Uint128,
    sum: Decimal,
}

impl PoolAccounts {
    pub fn join(&mut self, amount: Uint128) -> PoolAccount {
        self.total += amount;
        PoolAccount {
            amount,
            sum_snapshot: self.sum,
        }
    }

    pub fn increase_account(&mut self, account: &PoolAccount, amount: Uint128) -> PoolAccount {
        self.total += amount;
        PoolAccount {
            amount: account.amount + amount,
            ..account.clone()
        }
    }

    pub fn decrease_account(
        &mut self,
        account: &PoolAccount,
        amount: Uint128,
    ) -> Result<PoolAccount, ContractError> {
        self.total = self.total.checked_sub(amount)?;
        Ok(PoolAccount {
            amount: account.amount.checked_sub(amount)?,
            ..account.clone()
        })
    }

    pub fn distribute(&mut self, amount: Uint128) {
        self.pending += amount;
        self.sum += Decimal::from_ratio(amount, self.total);
    }

    pub fn pending_revenue(&self, account: &PoolAccount) -> Uint128 {
        (self.sum - account.sum_snapshot)
            .mul(Decimal::from_ratio(account.amount, 1u128))
            .to_uint_floor()
    }

    pub fn claim(&mut self, account: &mut PoolAccount) -> Uint128 {
        let amount = self.pending_revenue(account);
        self.pending -= amount;
        account.sum_snapshot = self.sum;
        amount
    }
}

impl Default for PoolAccounts {
    fn default() -> Self {
        Self {
            pending: Default::default(),
            total: Default::default(),
            sum: Default::default(),
        }
    }
}

#[cw_serde]
pub struct PoolAccount {
    pub amount: Uint128,
    pub sum_snapshot: Decimal,
}

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
#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn test_lifecycle() {
        let mut pool = PoolAccounts::default();
        let account = pool.join(Uint128::zero());

        assert_eq!(pool.pending, Uint128::zero());
        assert_eq!(pool.total, Uint128::zero());
        assert_eq!(pool.pending_revenue(&account), Uint128::zero());
        assert_eq!(account.amount, Uint128::zero());

        // Single join and claim
        let mut account = pool.join(Uint128::from(1000u128));
        assert_eq!(pool.pending, Uint128::zero());
        assert_eq!(pool.total, Uint128::from(1000u128));
        assert_eq!(pool.pending_revenue(&account), Uint128::zero());
        assert_eq!(account.amount, Uint128::from(1000u128));

        pool.distribute(Uint128::from(500u128));
        assert_eq!(pool.pending, Uint128::from(500u128));
        assert_eq!(pool.total, Uint128::from(1000u128));
        assert_eq!(pool.pending_revenue(&account), Uint128::from(500u128));
        let claimed = pool.claim(&mut account);
        assert_eq!(claimed, Uint128::from(500u128));
        assert_eq!(pool.pending_revenue(&account), Uint128::zero());

        let mut account1 = pool.join(Uint128::from(1000u128));
        let mut account2 = pool.join(Uint128::from(2000u128));
        assert_eq!(pool.pending, Uint128::zero());
        assert_eq!(pool.total, Uint128::from(4000u128));

        pool.distribute(Uint128::from(10_000u128));
        assert_eq!(pool.pending, Uint128::from(10_000u128));
        assert_eq!(pool.total, Uint128::from(4000u128));

        assert_eq!(pool.pending_revenue(&account), Uint128::from(2500u128));
        assert_eq!(pool.pending_revenue(&account1), Uint128::from(2500u128));
        assert_eq!(pool.pending_revenue(&account2), Uint128::from(5000u128));

        let claimed = pool.claim(&mut account1);
        let account1 = pool
            .decrease_account(&account1, Uint128::from(500u128))
            .unwrap();
        assert_eq!(claimed, Uint128::from(2500u128));
        assert_eq!(pool.pending_revenue(&account), Uint128::from(2500u128));
        assert_eq!(pool.pending_revenue(&account1), Uint128::zero());
        assert_eq!(pool.pending_revenue(&account2), Uint128::from(5000u128));
    }
}
