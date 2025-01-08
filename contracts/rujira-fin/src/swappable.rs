use cosmwasm_std::{Attribute, Decimal, StdResult, Storage, Uint128};
use itertools::EitherOrBoth;
use std::ops::{Add, AddAssign};

use crate::ContractError;

pub trait Swappable {
    // Effective price
    fn rate(&self) -> Decimal;

    // Extra attributes to append to trade events
    fn attributes(&self) -> Vec<Attribute>;
    // Total amount of bids
    fn total(&self) -> Uint128;

    fn swap(&mut self, offer: Uint128) -> Result<(Uint128, Uint128), ContractError>;

    fn commit(&self, storage: &mut dyn Storage) -> StdResult<SwapCommit>;
}

#[derive(Debug, Default)]
pub struct SwapCommit {
    /// The (offer, ask) totals committed to by the MarketMaker
    market_maker: (Uint128, Uint128),
}

impl Add for SwapCommit {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self {
            market_maker: (
                self.market_maker.0.add(rhs.market_maker.0),
                self.market_maker.1.add(rhs.market_maker.1),
            ),
        }
    }
}

impl AddAssign for SwapCommit {
    fn add_assign(&mut self, rhs: Self) {
        self.market_maker.0 += rhs.market_maker.0;
        self.market_maker.1 += rhs.market_maker.1;
    }
}

impl<T, V> Swappable for EitherOrBoth<T, V>
where
    T: Swappable + Clone,
    V: Swappable + Clone,
{
    fn rate(&self) -> Decimal {
        match self {
            EitherOrBoth::Both(a, _) => a.rate(),
            EitherOrBoth::Left(x) => x.rate(),
            EitherOrBoth::Right(x) => x.rate(),
        }
    }

    fn attributes(&self) -> Vec<Attribute> {
        match self {
            EitherOrBoth::Both(a, b) => {
                let mut res = a.attributes();
                res.append(&mut b.attributes());
                res
            }
            EitherOrBoth::Left(x) => x.attributes(),
            EitherOrBoth::Right(x) => x.attributes(),
        }
    }

    fn total(&self) -> Uint128 {
        match self {
            EitherOrBoth::Both(a, b) => a.total().add(b.total()),
            EitherOrBoth::Left(a) => a.total(),
            EitherOrBoth::Right(a) => a.total(),
        }
    }

    fn swap(&mut self, amount: Uint128) -> Result<(Uint128, Uint128), ContractError> {
        match self {
            EitherOrBoth::Both(a, b) => {
                let offer_a = Decimal::from_ratio(a.total() * amount, a.total().add(b.total()))
                    .to_uint_floor();
                let offer_b = amount - offer_a;
                let (consumed_offer_a, consumed_bids_a) = a.swap(offer_a)?;
                let (consumed_offer_b, consumed_bids_b) = b.swap(offer_b)?;

                Ok((
                    consumed_offer_a + consumed_offer_b,
                    consumed_bids_a + consumed_bids_b,
                ))
            }
            EitherOrBoth::Left(x) => {
                let (consumed_offer, consumed_bids) = x.swap(amount)?;
                Ok((consumed_offer, consumed_bids))
            }
            EitherOrBoth::Right(x) => {
                let (consumed_offer, consumed_bids) = x.swap(amount)?;
                Ok((consumed_offer, consumed_bids))
            }
        }
    }

    fn commit(&self, storage: &mut dyn Storage) -> StdResult<SwapCommit> {
        match self {
            EitherOrBoth::Both(a, b) => Ok(a.commit(storage)? + b.commit(storage)?),
            EitherOrBoth::Left(a) => a.commit(storage),
            EitherOrBoth::Right(a) => a.commit(storage),
        }
    }
}
