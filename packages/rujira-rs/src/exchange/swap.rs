use cosmwasm_std::{Attribute, Decimal, Response, Storage, Uint128};
use itertools::EitherOrBoth;
use std::ops::Add;

use super::error::SwapError;

/// Swappable trait allows any struct or container to be used in an Iterator and consumed by Swapper
/// E.g. EitherOrBoth<T, V> is implemented in order to support merged, ordered iterators for solving
/// swaps from multiple liquidity providers
pub trait SwapItem {
    type Context: SwapContext;

    /// The rate that the swap should be executed at
    fn rate(&self) -> Decimal;

    /// Total amount of bids available for swapping
    fn total(&self) -> Uint128;

    /// Returns the (offer_consumed, bids_returned) amounts
    fn swap(
        &mut self,
        context: &mut Self::Context,
        offer: Uint128,
    ) -> Result<(Uint128, Uint128, Vec<Attribute>), SwapError>;

    fn checkpoint(&mut self) {}
    fn rollback(&mut self) {}

    /// Whether a zero fill proves that the remaining, price-ordered items
    /// cannot settle another atomic unit.
    ///
    /// Most item types can return zero because the current tier itself is too
    /// small, so the conservative default is to keep looking. Implementations
    /// should only return true when their zero result is caused by the offered
    /// amount being below the current rate's integer settlement threshold.
    fn zero_fill_is_terminal(&self) -> bool {
        false
    }
}

pub trait SwapContext: Clone {
    /// Checkpoint & rollback provide explcit rollback semantics
    fn checkpoint(&mut self) {}
    fn rollback(&mut self) {}
    /// Commits the result of the Swap.
    /// Storage is provided to commit local state
    /// Response can be updated for inter-contract commitment
    fn commit(
        &mut self,
        storage: &mut dyn Storage,
        response: Response,
    ) -> Result<Response, SwapError>;
}

impl SwapContext for () {
    fn commit(&mut self, _: &mut dyn Storage, response: Response) -> Result<Response, SwapError> {
        Ok(response)
    }
}

impl<T, V> SwapItem for EitherOrBoth<T, V>
where
    T: SwapItem + Clone,
    V: SwapItem + Clone,
{
    type Context = (T::Context, V::Context);
    fn rate(&self) -> Decimal {
        match self {
            EitherOrBoth::Both(a, _) => a.rate(),
            EitherOrBoth::Left(x) => x.rate(),
            EitherOrBoth::Right(x) => x.rate(),
        }
    }

    fn total(&self) -> Uint128 {
        match self {
            EitherOrBoth::Both(a, b) => a.total().add(b.total()),
            EitherOrBoth::Left(a) => a.total(),
            EitherOrBoth::Right(a) => a.total(),
        }
    }

    fn swap(
        &mut self,
        (t, v): &mut Self::Context,
        amount: Uint128,
    ) -> Result<(Uint128, Uint128, Vec<Attribute>), SwapError> {
        match self {
            EitherOrBoth::Both(a, b) => {
                let offer_a = amount.checked_multiply_ratio(a.total(), a.total().add(b.total()))?;
                let offer_b = amount.saturating_sub(offer_a);
                let (consumed_offer_a, consumed_bids_a, mut atts_a) = a.swap(t, offer_a)?;
                let (consumed_offer_b, consumed_bids_b, mut atts_b) = b.swap(v, offer_b)?;
                atts_a.append(&mut atts_b);
                Ok((
                    consumed_offer_a + consumed_offer_b,
                    consumed_bids_a + consumed_bids_b,
                    atts_a,
                ))
            }
            EitherOrBoth::Left(x) => x.swap(t, amount),
            EitherOrBoth::Right(x) => x.swap(v, amount),
        }
    }

    fn checkpoint(&mut self) {
        match self {
            EitherOrBoth::Both(a, b) => {
                a.checkpoint();
                b.checkpoint();
            }
            EitherOrBoth::Left(x) => x.checkpoint(),
            EitherOrBoth::Right(x) => x.checkpoint(),
        }
    }

    fn rollback(&mut self) {
        match self {
            EitherOrBoth::Both(a, b) => {
                a.rollback();
                b.rollback();
            }
            EitherOrBoth::Left(x) => x.rollback(),
            EitherOrBoth::Right(x) => x.rollback(),
        }
    }

    fn zero_fill_is_terminal(&self) -> bool {
        match self {
            EitherOrBoth::Both(a, b) => a.zero_fill_is_terminal() && b.zero_fill_is_terminal(),
            EitherOrBoth::Left(x) => x.zero_fill_is_terminal(),
            EitherOrBoth::Right(x) => x.zero_fill_is_terminal(),
        }
    }
}

impl<T: SwapContext, V: SwapContext> SwapContext for (T, V) {
    fn checkpoint(&mut self) {
        self.0.checkpoint();
        self.1.checkpoint();
    }

    fn rollback(&mut self) {
        self.0.rollback();
        self.1.rollback();
    }

    fn commit(
        &mut self,
        storage: &mut dyn Storage,
        response: Response,
    ) -> Result<Response, SwapError> {
        let response = self.0.commit(storage, response)?;
        self.1.commit(storage, response)
    }
}

impl<T> SwapItem for Vec<T>
where
    T: SwapItem + Clone,
{
    type Context = T::Context;
    fn rate(&self) -> Decimal {
        self.first().map(|x| x.rate()).unwrap_or(Decimal::zero())
    }

    fn total(&self) -> Uint128 {
        self.iter().fold(Uint128::zero(), |acc, x| acc + x.total())
    }

    fn swap(
        &mut self,
        context: &mut Self::Context,
        amount: Uint128,
    ) -> Result<(Uint128, Uint128, Vec<Attribute>), SwapError> {
        let total = self.total();
        let mut remaining = amount;
        let mut consumed_offer = Uint128::zero();
        let mut consumed_bids = Uint128::zero();
        let mut atts = vec![];
        for x in self.iter_mut() {
            if remaining.is_zero() {
                break;
            }
            let offer = amount.multiply_ratio(x.total(), total).min(remaining);
            let (c_offer, c_bids, mut c_atts) = x.swap(context, offer)?;
            consumed_offer += c_offer;
            consumed_bids += c_bids;
            remaining -= c_offer.min(remaining);
            atts.append(&mut c_atts);
        }
        Ok((consumed_offer, consumed_bids, atts))
    }

    fn checkpoint(&mut self) {
        for x in self.iter_mut() {
            x.checkpoint();
        }
    }

    fn rollback(&mut self) {
        for x in self.iter_mut() {
            x.rollback();
        }
    }

    fn zero_fill_is_terminal(&self) -> bool {
        !self.is_empty() && self.iter().all(SwapItem::zero_fill_is_terminal)
    }
}

#[cfg(test)]
mod tests {
    use crate::exchange::testing::{TestItem, TestIterContext};

    use super::*;
    use cosmwasm_std::Uint128;

    // price and is_sell must be the same for all items
    // Swappable for vec assumes that all the quotes are identical but different amounts
    fn item(amount: u128) -> TestItem {
        TestItem::new("1", amount, false)
    }

    #[test]
    fn vec_swappable_proportional_even_split_same_price() {
        // Totals: 50 + 50; Offer: 100 -> expect 50/50
        let mut v = vec![item(50), item(50)];
        let mut ctx = TestIterContext::default();
        let (consumed_offer, consumed_bids, _) = v.swap(&mut ctx, Uint128::new(100)).unwrap();

        assert_eq!(consumed_offer, Uint128::new(100));
        assert_eq!(consumed_bids, Uint128::new(100));
        assert_eq!(v[0].amount, Uint128::new(0));
        assert_eq!(v[1].amount, Uint128::new(0));
    }

    #[test]
    fn vec_swappable_proportional_weighted_same_price() {
        // Totals: 30 + 70; Offer: 100 -> expect 30/70
        let mut v = vec![item(30), item(70)];
        let mut ctx = TestIterContext::default();
        let (consumed_offer, consumed_bids, _) = v.swap(&mut ctx, Uint128::new(100)).unwrap();

        assert_eq!(consumed_offer, Uint128::new(100));
        assert_eq!(consumed_bids, Uint128::new(100));
        assert_eq!(v[0].amount, Uint128::new(0)); // 30 consumed
        assert_eq!(v[1].amount, Uint128::new(0)); // 70 consumed
    }

    #[test]
    fn vec_swappable_zero_liquidity_is_ignored_same_price() {
        // Totals: 0 + 100; Offer: 25 -> only second contributes
        let mut v = vec![item(0), item(100)];
        let mut ctx = TestIterContext::default();
        let (consumed_offer, consumed_bids, _) = v.swap(&mut ctx, Uint128::new(25)).unwrap();

        assert_eq!(consumed_offer, Uint128::new(25));
        assert_eq!(consumed_bids, Uint128::new(25));
        assert_eq!(v[0].amount, Uint128::new(0));
        assert_eq!(v[1].amount, Uint128::new(75)); // 25 consumed
    }

    #[test]
    fn vec_swappable_capacity_cap_same_price() {
        // Totals: 10 + 90; Offer: 200 -> can only consume 10 + 90 = 100 total
        let mut v = vec![item(10), item(90)];
        let mut ctx = TestIterContext::default();
        let (consumed_offer, consumed_bids, _) = v.swap(&mut ctx, Uint128::new(200)).unwrap();

        assert_eq!(consumed_offer, Uint128::new(100));
        assert_eq!(consumed_bids, Uint128::new(100));
        assert_eq!(v[0].amount, Uint128::new(0));
        assert_eq!(v[1].amount, Uint128::new(0));
    }

    #[test]
    fn vec_swappable_three_pools_clean_division_same_price() {
        // Totals: 20 + 30 + 50; Offer: 100 -> expect 20/30/50
        let mut v = vec![item(20), item(30), item(50)];
        let mut ctx = TestIterContext::default();
        let (consumed_offer, consumed_bids, _) = v.swap(&mut ctx, Uint128::new(100)).unwrap();

        assert_eq!(consumed_offer, Uint128::new(100));
        assert_eq!(consumed_bids, Uint128::new(100));
        assert_eq!(v[0].amount, Uint128::new(0));
        assert_eq!(v[1].amount, Uint128::new(0));
        assert_eq!(v[2].amount, Uint128::new(0));
    }

    #[test]
    fn vec_swappable_rounding_behaviour_same_price() {
        // Totals: 1 + 1 + 1; Offer: 2 -> each target share is 2/3;
        // total consumed = 0 because multiply_ratio always floors
        let mut v = vec![item(1), item(1), item(1)];
        let mut ctx = TestIterContext::default();
        let (consumed_offer, consumed_bids, _) = v.swap(&mut ctx, Uint128::new(2)).unwrap();

        assert_eq!(consumed_offer, Uint128::new(0));
        assert_eq!(consumed_bids, Uint128::new(0));

        let remaining: u128 = v.iter().map(|x| x.amount.u128()).sum();
        assert_eq!(remaining, 3);
    }
}
