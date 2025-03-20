use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Decimal, StdResult, Storage, Uint128};
use cw_storage_plus::Map;
use cw_utils::NativeBalance;
use rujira_rs::ghost_vault;
use std::ops::Mul;

use crate::{config::Status, ContractError};

static POSITIONS: Map<Addr, (Uint128, Uint128)> = Map::new("positions");

#[cw_serde]
pub struct Position {
    pub owner: Addr,
    // Amount of collateral owned by this position
    pub collateral: Uint128,

    // Allocation of the vault's debt denom owed by this position
    pub debt_shares: Uint128,

    // Value of debt shares that must be repaid
    pub debt_amount: Uint128,
}

impl Position {
    pub fn load(
        storage: &dyn Storage,
        addr: &Addr,
        vault_status: &ghost_vault::StatusResponse,
    ) -> Self {
        let (collateral, debt) = POSITIONS.load(storage, addr.clone()).unwrap_or_default();

        Self {
            owner: addr.clone(),
            collateral,
            debt_shares: debt,
            debt_amount: Decimal::from_ratio(debt, 1u128)
                .mul(vault_status.debt_pool.ratio)
                .to_uint_ceil(),
        }
    }

    pub fn save(&self, storage: &mut dyn Storage) -> StdResult<()> {
        if self.is_zero() {
            POSITIONS.remove(storage, self.owner.clone());
            Ok(())
        } else {
            POSITIONS.save(
                storage,
                self.owner.clone(),
                &(self.collateral, self.debt_shares),
            )
        }
    }

    pub fn is_zero(&self) -> bool {
        self.collateral.is_zero() && self.debt_shares.is_zero()
    }

    pub fn adjust(
        &mut self,
        _status: Status,
        _deposited: Uint128,
        _borrowed: Uint128,
        _funds: NativeBalance,
    ) -> Result<NativeBalance, ContractError> {
        todo!()
    }
}
