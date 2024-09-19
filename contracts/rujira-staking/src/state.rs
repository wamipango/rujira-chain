use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Decimal, StdResult, Storage, Uint128};
use cw_storage_plus::{Item, Map};
use rujira_rs::SharePool;
use std::ops::Mul;

use crate::ContractError;

pub static POOL_LIQUID: Item<SharePool> = Item::new("l");
pub static ACCOUNTS: Map<Addr, PoolAccount> = Map::new("a");
pub static POOL_ACCOUNTS: Item<PoolAccounts> = Item::new("p");

#[cw_serde]
pub struct PoolAccounts {
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
            sum_snapshot: self.sum,
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
            sum_snapshot: self.sum,
        })
    }

    pub fn distribute(&mut self, amount: Uint128) {
        self.sum += Decimal::from_ratio(amount, self.total);
    }

    pub fn claim(&self, account: &PoolAccount) -> Uint128 {
        (self.sum - account.sum_snapshot)
            .mul(Decimal::from_ratio(account.amount, 1u128))
            .to_uint_floor()
    }
}

impl Default for PoolAccounts {
    fn default() -> Self {
        Self {
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
