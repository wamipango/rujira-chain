use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Decimal, Uint128};
use std::ops::{AddAssign, SubAssign};
use thiserror::Error;

#[cw_serde]
pub struct SharePool {
    size: Uint128,
    shares: Uint128,
}

impl SharePool {
    /// Adds a deposit to the pool, returning the new shares issued
    pub fn join(&mut self, amount: Uint128) -> Result<Uint128, SharePoolError> {
        if amount.is_zero() {
            return Err(SharePoolError::ZeroAmount {});
        }

        if self.size.is_zero() {
            self.size.add_assign(amount);
            self.shares.add_assign(amount);
            return Ok(amount);
        }

        let issuance = Decimal::from_ratio(self.shares * amount, self.size).to_uint_floor();
        self.shares.add_assign(issuance);
        self.size.add_assign(amount);
        Ok(issuance)
    }

    /// Removes a share from the pool, returning the amount removed from the pool and deducted
    pub fn leave(&mut self, amount: Uint128) -> Result<Uint128, SharePoolError> {
        if amount.is_zero() {
            return Err(SharePoolError::ZeroAmount {});
        }

        if amount.gt(&self.shares) {
            return Err(SharePoolError::ShareOverflow {});
        }

        if amount.eq(&self.shares) {
            let claim = self.size;
            self.size = Uint128::zero();
            self.shares = Uint128::zero();
            return Ok(claim);
        }

        let claim: Uint128 = Decimal::from_ratio(self.size * amount, self.shares).to_uint_floor();
        self.size.sub_assign(claim);
        self.shares.sub_assign(amount);
        Ok(claim)
    }

    pub fn ratio(&self) -> Decimal {
        if self.shares.is_zero() {
            return Decimal::zero();
        }
        Decimal::from_ratio(self.size, self.shares)
    }

    pub fn size(&self) -> Uint128 {
        self.size
    }

    pub fn ownership(&self, shares: Uint128) -> Uint128 {
        if shares.is_zero() {
            return Uint128::zero();
        }
        Decimal::from_ratio(shares * self.size, self.shares).to_uint_floor()
    }

    pub fn deposit(&mut self, amount: Uint128) {
        self.size.add_assign(amount);
    }
}

impl Default for SharePool {
    fn default() -> Self {
        Self {
            size: Default::default(),
            shares: Default::default(),
        }
    }
}

#[derive(Error, Debug)]
pub enum SharePoolError {
    #[error("ShareOverflow")]
    ShareOverflow {},
    #[error("ZeroAmount")]
    ZeroAmount {},
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn lifecycle() {
        let mut pool = SharePool::default();
        pool.leave(Uint128::one()).unwrap_err();
        pool.join(Uint128::zero()).unwrap_err();
        let shares = pool.join(Uint128::from(1000u128)).unwrap();
        assert_eq!(shares, Uint128::from(1000u128));
        assert_eq!(pool.shares, Uint128::from(1000u128));
        assert_eq!(pool.size, Uint128::from(1000u128));

        assert_eq!(
            pool.ownership(Uint128::from(1000u128)),
            Uint128::from(1000u128)
        );

        let shares = pool.join(Uint128::from(5000u128)).unwrap();
        assert_eq!(shares, Uint128::from(5000u128));
        assert_eq!(pool.shares, Uint128::from(6000u128));
        assert_eq!(pool.size, Uint128::from(6000u128));

        pool.deposit(Uint128::from(2000u128));

        assert_eq!(pool.shares, Uint128::from(6000u128));
        assert_eq!(pool.size, Uint128::from(8000u128));

        assert_eq!(
            pool.ownership(Uint128::from(1000u128)),
            Uint128::from(1333u128)
        );

        let shares = pool.join(Uint128::from(1000u128)).unwrap();
        assert_eq!(shares, Uint128::from(750u128));
        assert_eq!(pool.shares, Uint128::from(6750u128));
        assert_eq!(pool.size, Uint128::from(9000u128));
        assert_eq!(pool.ownership(shares), Uint128::from(1000u128));

        let redeem = pool.leave(Uint128::from(500u128)).unwrap();
        assert_eq!(redeem, Uint128::from(666u128));
        assert_eq!(pool.shares, Uint128::from(6250u128));
        assert_eq!(pool.size, Uint128::from(8334u128));
        assert_eq!(pool.ownership(shares), Uint128::from(1000u128));

        pool.leave(Uint128::from(6251u128)).unwrap_err();
        let redeem = pool.leave(Uint128::from(6250u128)).unwrap();
        assert_eq!(redeem, Uint128::from(8334u128));
        assert_eq!(pool.shares, Uint128::zero());
        assert_eq!(pool.size, Uint128::zero());
    }
}
