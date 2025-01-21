use crate::ContractError;
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, StdError, StdResult, Storage, Uint128};
use cw_storage_plus::Map;
use std::{
    cmp::min,
    ops::{Add, Sub},
};

static BORROWERS: Map<Addr, (Uint128, Uint128)> = Map::new("borrowers");

#[cw_serde]
pub struct Borrower {
    pub addr: Addr,
    pub limit: Uint128,
    pub current: Uint128,
}

impl Borrower {
    pub fn load(storage: &dyn Storage, addr: Addr) -> Result<Self, ContractError> {
        match BORROWERS.load(storage, addr.clone()) {
            Ok((limit, current)) => Ok(Self {
                addr,
                limit,
                current,
            }),
            Err(StdError::NotFound { .. }) => Err(ContractError::UnauthorizedBorrower {}),
            Err(err) => Err(ContractError::Std(err)),
        }
    }

    pub fn save(&self, storage: &mut dyn Storage) -> StdResult<()> {
        BORROWERS.save(storage, self.addr.clone(), &(self.limit, self.current))
    }

    pub fn borrow(&mut self, amount: Uint128) -> Result<(), ContractError> {
        if self.current.add(amount).gt(&self.limit) {
            return Err(ContractError::BorrowLimitReached { limit: self.limit });
        }
        self.current += amount;
        Ok(())
    }

    pub fn repay(&mut self, amount: Uint128) -> Uint128 {
        let repaid = min(amount, self.current);
        // Ignore excessive repay.
        self.current -= repaid;
        // return any overpayment
        amount.sub(repaid)
    }

    pub fn set(storage: &mut dyn Storage, addr: Addr, limit: Uint128) -> StdResult<()> {
        let (_, current) = BORROWERS.load(storage, addr.clone()).unwrap_or_default();
        BORROWERS.save(storage, addr, &(limit, current))
    }
}
