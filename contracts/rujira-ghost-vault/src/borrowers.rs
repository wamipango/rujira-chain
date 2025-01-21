use crate::ContractError;
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Order, StdError, StdResult, Storage, Uint128};
use cw_storage_plus::{Bound, Map};
use rujira_rs::ghost_vault::BorrowerResponse;
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
            .map(|x| {
                x.map(|(addr, (limit, current))| Self {
                    addr,
                    limit,
                    current,
                })
            })
    }
}

impl From<Borrower> for BorrowerResponse {
    fn from(value: Borrower) -> Self {
        Self {
            addr: value.addr.to_string(),
            limit: value.limit,
            current: value.current,
        }
    }
}
