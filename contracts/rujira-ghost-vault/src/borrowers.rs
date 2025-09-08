use crate::ContractError;
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Order, StdError, StdResult, Storage, Uint128};
use cw_storage_plus::{Bound, Map};
use std::{
    cmp::min,
    ops::{Add, Sub},
};

static BORROWERS: Map<Addr, Borrower> = Map::new("borrowers");

#[cw_serde]
pub struct Borrower {
    pub addr: Addr,
    pub limit: Uint128,
    pub shares: Uint128,
}

impl Borrower {
    pub fn load(storage: &dyn Storage, addr: Addr) -> Result<Self, ContractError> {
        match BORROWERS.load(storage, addr.clone()) {
            Ok(x) => Ok(x),
            Err(StdError::NotFound { .. }) => Err(ContractError::UnauthorizedBorrower {}),
            Err(err) => Err(ContractError::Std(err)),
        }
    }

    pub fn save(&self, storage: &mut dyn Storage) -> StdResult<()> {
        BORROWERS.save(storage, self.addr.clone(), self)
    }

    pub fn borrow(&mut self, amount: Uint128) -> Result<(), ContractError> {
        if self.shares.add(amount).gt(&self.limit) {
            return Err(ContractError::BorrowLimitReached { limit: self.limit });
        }
        self.shares += amount;

        Ok(())
    }

    pub fn repay(&mut self, amount: Uint128) -> Uint128 {
        let repaid = min(amount, self.shares);
        // Ignore excessive repay.
        self.shares -= repaid;
        // return any overpayment
        amount.sub(repaid)
    }

    pub fn set(storage: &mut dyn Storage, addr: Addr, limit: Uint128) -> StdResult<()> {
        let mut borrower = BORROWERS.load(storage, addr.clone()).unwrap_or(Borrower {
            addr: addr.clone(),
            limit: Default::default(),
            shares: Default::default(),
        });
        borrower.limit = limit;
        BORROWERS.save(storage, addr, &borrower)
    }

    pub fn list(
        storage: &dyn Storage,
        limit: Option<u8>,
        start_after: Option<Addr>,
    ) -> impl Iterator<Item = StdResult<Self>> + '_ {
        let limit = limit.unwrap_or(100) as usize;
        let min = start_after.map(Bound::exclusive);
        BORROWERS
            .range(storage, min, None, Order::Ascending)
            .take(limit)
            .map(|x| x.map(|(_, v)| v))
    }
}
