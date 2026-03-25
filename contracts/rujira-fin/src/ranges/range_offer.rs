use std::ops::{Div, Mul};

use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Attribute, Decimal, Decimal256, Uint128};
use rujira_rs::{
    exchange::{SwapError, SwapItem},
    fin::Side,
};

use super::{Range, RangeContext, Scl};

#[cw_serde]
pub struct RangeOffer {
    pub side: Side,
    pub start: Decimal,
    pub end: Decimal,
    pub total: Decimal,
    /// Volume-weighted effective price from the SCL curve (ΔY/ΔX)
    pub effective_rate: Decimal256,
    /// A vec of the (range, amount)'s contributing to this offer
    pub ranges: Vec<(Range, Decimal)>,
}

impl RangeOffer {
    pub fn new(side: &Side, start: &Decimal, end: &Decimal) -> Self {
        Self {
            side: side.clone(),
            start: *start,
            end: *end,
            total: Default::default(),
            effective_rate: Decimal256::from(*start),
            ranges: Default::default(),
        }
    }

    pub fn insert_range(&mut self, range: &mut Range) {
        // Use skew-aware amount calculation
        let amount = range.amount_in_slice(&self.side, self.start, self.end);

        let scl = Scl::new(range.high, range.low, range.skew);
        if let Some(eff_price) = scl.effective_price(self.start, self.end) {
            // Volume-weighted effective price: (old_total * old_rate + amount * eff_price) / new_total
            let old_total = Decimal256::from(self.total);
            self.total += amount;
            if !self.total.is_zero() {
                self.effective_rate = (old_total * self.effective_rate
                    + Decimal256::from(amount) * eff_price)
                    / Decimal256::from(self.total);
            }
            self.ranges.push((range.clone(), amount));
        }
    }
}

impl SwapItem for RangeOffer {
    type Context = RangeContext;
    fn rate(&self) -> Decimal {
        Decimal::try_from(self.effective_rate).unwrap()
    }

    fn total(&self) -> Uint128 {
        self.total.to_uint_floor()
    }

    fn swap(
        &mut self,
        context: &mut Self::Context,
        offer: Uint128,
    ) -> Result<(Uint128, Uint128, Vec<Attribute>), SwapError> {
        let mut used: Vec<String> = vec![];
        let offer_dec = Decimal::from_atomics(offer, 0).unwrap();

        let rate256 = self.effective_rate;
        let rate_inv256 = Decimal256::one() / rate256;
        let offer256 = Decimal256::from(offer_dec);

        // The amount of the bids in the RangeOffer consumed by this swap
        let bids_used = Decimal::try_from(match self.side {
            Side::Base => offer256 * rate_inv256,
            Side::Quote => offer256 * rate256,
        })?
        .min(self.total);

        // Convert the consumed bids back to the offer consumed
        let offer_used = Decimal::try_from(match self.side {
            Side::Base => Decimal256::from(bids_used) * rate256,
            Side::Quote => Decimal256::from(bids_used) * rate_inv256,
        })?;
        let mut remaining_bids_used = bids_used;
        let mut remaining_offer_used = offer_used;
        let len = self.ranges.len();
        for i in 0..len {
            let (r, v) = &self.ranges[i].clone();

            // Pro-rate the provision of return amount to the Range's share of total
            let (deduct, add) = if i + 1 == len {
                // Collect dust
                (remaining_bids_used, remaining_offer_used)
            } else {
                let (a, b) = (Decimal256::from(*v), Decimal256::from(self.total));
                (
                    Decimal::try_from(a.mul(Decimal256::from(bids_used)).div(b))?,
                    Decimal::try_from(a.mul(Decimal256::from(offer_used)).div(b))?,
                )
            };
            remaining_bids_used -= deduct;
            remaining_offer_used -= add;
            self.ranges[i].1 -= deduct.min(*v);
            let mut ctx_r = context.ranges.remove(&r.idx).unwrap_or_else(|| r.clone());
            used.push(ctx_r.swap(context, &self.side, add, deduct));
            context.ranges.insert(ctx_r.idx, ctx_r);
        }
        self.total -= bids_used;
        Ok((
            offer_used.to_uint_ceil(),
            bids_used.to_uint_floor(),
            vec![
                Attribute::new("price", format!("ccl:{}", self.rate())),
                Attribute::new("side", self.side.to_string()),
                Attribute::new("ranges", used.join(",")),
            ],
        ))
    }
}
