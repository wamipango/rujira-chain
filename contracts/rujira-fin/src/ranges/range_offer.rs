use std::ops::{Div, Mul};

use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Attribute, Decimal, Decimal256, Fraction, Uint128};
use rujira_rs::{
    exchange::{SwapError, SwapItem},
    fin::Side,
};

use super::{Range, RangeContext};

#[cw_serde]
pub struct RangeOffer {
    pub side: Side,
    pub start: Decimal,
    pub end: Decimal,
    pub total: Decimal,
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
            ranges: Default::default(),
        }
    }

    pub fn insert_range(&mut self, range: &Range) {
        // Use skew-aware amount calculation
        let amount = range.amount_in_slice(&self.side, self.start, self.end);

        self.total += amount;
        self.ranges.push((range.clone(), amount));
    }
}

impl SwapItem for RangeOffer {
    type Context = RangeContext;
    fn rate(&self) -> Decimal {
        self.start
    }

    fn total(&self) -> Uint128 {
        self.total.to_uint_floor()
    }

    fn swap(
        &mut self,
        context: &mut Self::Context,
        offer: Uint128,
    ) -> Result<(Uint128, Uint128, Vec<Attribute>), SwapError> {
        let mut iter = self.ranges.iter().peekable();
        let mut used: Vec<String> = vec![];
        let offer_dec = Decimal::from_atomics(offer, 0).unwrap();

        let rate_inv = self.start.inv().unwrap();
        // The amount of the bids in the RangeOffer consimed by this swap
        let bids_used = match self.side {
            Side::Base => offer_dec * rate_inv,
            Side::Quote => offer_dec * self.start,
        }
        .min(self.total);

        // Convert the consumed bids back to the offer consumed
        let offer_used = match self.side {
            Side::Base => bids_used * self.start,
            Side::Quote => bids_used * rate_inv,
        };
        let mut remaining_bids_used = bids_used;
        let mut remaining_offer_used = offer_used;
        while let Some((r, v)) = iter.next() {
            // Pro-rate the provision of return amount to the Range's share of total
            let (deduct, add) = match iter.peek() {
                Some(_) => {
                    let (a, b) = (Decimal256::from(*v), Decimal256::from(self.total));
                    (
                        Decimal::try_from(a.mul(Decimal256::from(bids_used)).div(b))?,
                        Decimal::try_from(a.mul(Decimal256::from(offer_used)).div(b))?,
                    )
                }
                // Collect dust
                None => (remaining_bids_used, remaining_offer_used),
            };
            remaining_bids_used -= deduct;
            remaining_offer_used -= add;

            let mut ctx_r = context.ranges.remove(&r.idx).unwrap_or_else(|| r.clone());
            used.push(ctx_r.swap(context, &self.side, add, deduct));
            context.ranges.insert(ctx_r.idx, ctx_r);
        }
        self.total -= bids_used;
        Ok((
            offer_used.to_uint_ceil(),
            bids_used.to_uint_floor(),
            vec![
                Attribute::new("price", format!("ccl:{}", self.start)),
                Attribute::new("side", self.side.to_string()),
                Attribute::new("ranges", used.join(",")),
            ],
        ))
    }
}
