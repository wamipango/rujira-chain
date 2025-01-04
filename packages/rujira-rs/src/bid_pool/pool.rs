use std::ops::Mul;

use crate::Frac;

use super::{bid::Bid, sum_snapshot::SumSnapshot};
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Decimal256, Fraction as _, StdResult, Uint256};

/// Pooled bidding to distribute tokens between bidders at a specific exchange rate,
/// consuming their bids, in O(1) time
///
/// CRITICAL: When loading a Bid, you **must** call sync_bid to update the Bid with the latest pool state
#[cw_serde]
#[derive(Copy)]
pub struct Pool {
    pub(crate) total: Uint256,
    pub(crate) sum: Frac,
    pub(crate) product: Frac,
    pub(crate) epoch: u32,
    pub(crate) scale: u16,
}

impl Default for Pool {
    fn default() -> Self {
        Self {
            total: Uint256::zero(),
            product: Frac::one(),
            sum: Frac::zero(),
            epoch: 0,
            scale: 0,
        }
    }
}

impl Pool {
    pub fn new_bid(&mut self, amount: Uint256) -> Bid {
        self.total += amount;

        Bid {
            amount,
            filled: Uint256::zero(),
            product_snapshot: self.product,
            sum_snapshot: self.sum,
            epoch_snapshot: self.epoch,
            scale_snapshot: self.scale,
        }
    }

    pub fn is_zero(&self) -> bool {
        self.total.is_zero()
    }

    /// Distribute the given amount at the given rate, consuming the total amount of the pool,
    pub fn distribute(
        &mut self,
        offer: Uint256,
        rate: &Decimal256,
    ) -> StdResult<DistributionResult> {
        if offer.is_zero() || self.total.is_zero() {
            return Ok(DistributionResult::default());
        }
        let bids_value = offer.mul_floor(*rate);

        // Switch between entirely consuming either the pool or the offer.
        // This effectively carries the rounding truncation from the distribute_full,
        // until we get to distribute_partial, where it's fully consumed and we have
        // symmetry via the fair distro algo
        if bids_value >= self.total {
            // Offer is worth more than pool. Empty the pool an increase the epoch
            return self.distribute_full(rate);
        }
        // Pool is worth more than the offer. Consume the rest of the offer
        self.distribute_partial(bids_value, offer)
    }

    pub fn total(&self) -> Uint256 {
        self.total
    }

    fn distribute_full(&mut self, rate: &Decimal256) -> StdResult<DistributionResult> {
        let inv = rate.inv().unwrap();
        let consumed_offer = Decimal256::from_atomics(self.total, 0).unwrap() * inv;
        let consumed_bids = self.total;

        self.sum += self.product.mul(inv);

        let mut snapshots = vec![SumSnapshot::from(*self)];
        self.increment_epoch();
        snapshots.push(SumSnapshot::from(*self));

        Ok(DistributionResult {
            consumed_offer: consumed_offer.to_uint_ceil(),
            consumed_bids,
            snapshots,
        })
    }

    fn distribute_partial(
        &mut self,
        bids_value: Uint256,
        offer: Uint256,
    ) -> StdResult<DistributionResult> {
        // S + E / D * P
        self.sum += self.product * Frac::from_ratio(offer, self.total);

        // 1 - Q / D
        self.product = self.product * (Frac::one() - Frac::from_ratio(bids_value, self.total));

        self.total -= bids_value;
        let snapshots = vec![SumSnapshot::from(*self)];

        Ok(DistributionResult {
            consumed_offer: offer,
            consumed_bids: bids_value,
            snapshots,
        })
    }

    pub fn sync_bid(&self, bid: &mut Bid, sum_snapshot: Option<Frac>) -> StdResult<()> {
        bid.filled += self.bid_filled_amount(bid, sum_snapshot)?;
        bid.amount = self.bid_remaining_amount(bid)?;
        bid.product_snapshot = self.product;
        bid.sum_snapshot = self.sum;
        bid.scale_snapshot = self.scale;

        Ok(())
    }

    fn bid_filled_amount(&self, bid: &Bid, sum_snapshot: Option<Frac>) -> StdResult<Uint256> {
        if bid.product_snapshot.is_zero() {
            return Ok(Uint256::zero());
        }
        let inv = bid.product_snapshot.inv().unwrap();
        let reference_ss = sum_snapshot.unwrap_or(bid.sum_snapshot);
        let res = (reference_ss - bid.sum_snapshot).mul(inv).mul(bid.amount);
        Ok(res)
    }

    fn bid_remaining_amount(&self, bid: &mut Bid) -> StdResult<Uint256> {
        if bid.product_snapshot.is_zero() {
            return Ok(bid.amount);
        }

        let epoch_diff: Uint256 =
            Uint256::from(self.epoch).checked_sub(Uint256::from(bid.epoch_snapshot))?;

        if !epoch_diff.is_zero() {
            return Ok(Uint256::zero());
        }
        let ratio = self.product / bid.product_snapshot;
        let amount = ratio * bid.amount;
        Ok(amount)
    }

    fn increment_epoch(&mut self) {
        self.total = Uint256::zero();
        self.sum = Frac::zero();
        self.product = Frac::one();
        self.scale = 0;
        self.epoch += 1;
    }
}

#[derive(Default)]
pub struct DistributionResult {
    pub consumed_offer: Uint256,
    pub consumed_bids: Uint256,
    pub snapshots: Vec<SumSnapshot>,
}
