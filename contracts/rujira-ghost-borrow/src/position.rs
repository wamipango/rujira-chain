use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, StdResult, Storage, Uint128};
use cw_storage_plus::Map;
use cw_utils::NativeBalance;

static POSITIONS: Map<Addr, (Uint128, Uint128)> = Map::new("positions");

#[cw_serde]
pub struct Position {
    pub owner: Addr,
    // Amount of collateral owned by this position
    pub collateral: Uint128,
    // Allocation of the vault's debt denom owed by this position
    pub debt: Uint128,
}

impl Position {
    pub fn load(storage: &dyn Storage, addr: &Addr) -> Self {
        let (collateral, debt) = POSITIONS.load(storage, addr.clone()).unwrap_or_default();

        Self {
            owner: addr.clone(),
            collateral,
            debt,
        }
    }

    pub fn save(&self, storage: &mut dyn Storage) -> StdResult<()> {
        if self.is_zero() {
            Ok(POSITIONS.remove(storage, self.owner.clone()))
        } else {
            POSITIONS.save(storage, self.owner.clone(), &(self.collateral, self.debt))
        }
    }

    pub fn is_zero(&self) -> bool {
        self.collateral.is_zero() && self.debt.is_zero()
    }

    pub fn adjust(
        &mut self,
        _deposited: Uint128,
        _borrowed: Uint128,
        _funds: NativeBalance,
    ) -> StdResult<NativeBalance> {
        todo!()
    }
}
