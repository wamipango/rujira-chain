use cosmwasm_std::{Attribute, Decimal, Storage, Uint128};
use itertools::EitherOrBoth;
use std::ops::Add;

use super::{commit::SwapCommit, error::SwapError};

/// Swappable trait allows any struct or container to be used in an Iterator and consumed by Swapper
/// E.g. EitherOrBoth<T, V> is implemented in order to support merged, ordered iterators for solving
/// swaps from multiple liquidity providers
pub trait Swappable {
    /// The rate that the swap should be executed at
    fn rate(&self) -> Decimal;

    /// The prefix used when emitting Event's
    fn prefix(&self) -> &str;

    /// Extra attributes to append to trade events
    fn attributes(&self) -> Vec<Attribute>;

    /// Total amount of bids available for swapping
    fn total(&self) -> Uint128;

    /// Returns the (offer_consumed, bids_returned) amounts
    fn swap(&mut self, offer: Uint128) -> Result<(Uint128, Uint128), SwapError>;

    /// Commits the result of the Swap.
    /// Storage is provided to commit local state
    /// SwapCommit is returned for commitments that require inter-contract communication
    fn commit(&self, storage: &mut dyn Storage) -> Result<SwapCommit, SwapError>;
}

impl<T, V> Swappable for EitherOrBoth<T, V>
where
    T: Swappable + Clone,
    V: Swappable + Clone,
{
    fn prefix(&self) -> &str {
        match self {
            EitherOrBoth::Both(a, _) => a.prefix(),
            EitherOrBoth::Left(x) => x.prefix(),
            EitherOrBoth::Right(x) => x.prefix(),
        }
    }

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

    fn swap(&mut self, amount: Uint128) -> Result<(Uint128, Uint128), SwapError> {
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

    fn commit(&self, storage: &mut dyn Storage) -> Result<SwapCommit, SwapError> {
        match self {
            EitherOrBoth::Both(a, b) => Ok(a.commit(storage)? + b.commit(storage)?),
            EitherOrBoth::Left(a) => a.commit(storage),
            EitherOrBoth::Right(a) => a.commit(storage),
        }
    }
}
