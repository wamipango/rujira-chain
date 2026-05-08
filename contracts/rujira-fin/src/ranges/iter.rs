use std::{collections::BTreeMap, ops::Add};

use cosmwasm_std::{Decimal, Fraction, Order, StdResult, Storage, Uint128};
use rujira_rs::fin::Side;

use super::{range_offer::RangeUpdates, Range, RangeOffer};

pub struct RangeOfferIter<'a> {
    /// The default step between prices
    delta: Decimal,
    /// The previous end price
    previous: Option<Decimal>,
    /// The most recent Item popped from the iterator, that wasn't part of the current group
    next: Option<Range>,
    side: Side,
    /// a Map of active ranges, grouped and keyed by the price at which they go out of range for easy rejection
    ranges: BTreeMap<u128, Vec<Range>>,
    updates: RangeUpdates,
    /// Base Range iter
    iter: Box<dyn Iterator<Item = Range> + 'a>,
}

impl<'a> RangeOfferIter<'a> {
    fn filter(side: Side, min: Decimal) -> impl Fn(StdResult<(u128, Range)>) -> Option<Range> {
        move |res| match res {
            Ok((_, range)) => match side {
                Side::Base => range.base >= min && range.ask < range.high,
                Side::Quote => range.quote >= min && range.bid > range.low,
            }
            .then_some(range),
            Err(_) => None,
        }
    }

    pub fn new(
        storage: &'a dyn Storage,
        side: &Side,
        delta: &Decimal,
        range_min: &Decimal,
    ) -> Self {
        let min = *range_min;
        let base_iter = match side {
            Side::Base => Range::map()
                .idx
                .asks
                .range(storage, None, None, Order::Ascending),
            Side::Quote => Range::map()
                .idx
                .bids
                .range(storage, None, None, Order::Descending),
        }
        .filter_map(Self::filter(side.clone(), min));

        Self {
            side: side.clone(),
            ranges: BTreeMap::new(),
            updates: Default::default(),
            delta: *delta,
            iter: Box::new(base_iter),
            next: None,
            previous: None,
        }
    }

    fn fill_ranges(&mut self, curr: Decimal) {
        while let Some(next) = self.next.clone() {
            if match self.side {
                Side::Base => next.ask,
                Side::Quote => next.bid,
            } == curr
            {
                self.insert_range(&next);
                self.next = self.iter.next();
            } else {
                self.next = Some(next);
                break;
            };
        }
    }

    /// Inserts a Range into `ranges`, keyed by the boundary at which it goes out of range
    fn insert_range(&mut self, range: &Range) {
        let key = match self.side {
            Side::Base => range.high,
            Side::Quote => range.low,
        }
        .numerator()
        .u128();
        let mut value = self.ranges.get(&key).unwrap_or(&vec![]).clone();
        value.push(range.clone());
        self.ranges.insert(key, value);
    }

    fn open_price(&self, range: &Range) -> Decimal {
        match self.side {
            Side::Base => range.ask,
            Side::Quote => range.bid,
        }
    }

    fn start(&mut self) -> Option<Decimal> {
        let first = self.iter.next();
        self.next = self.iter.next();
        self.insert_range(&first.clone()?);
        match self.side {
            Side::Base => Some(first?.ask),
            Side::Quote => Some(first?.bid),
        }
    }

    fn build_offer(&mut self, start: &Decimal) -> Option<RangeOffer> {
        self.updates.apply(self.ranges.values_mut().flatten());
        let mut offer = RangeOffer::new(&self.side, start, &self.previous?);
        if self.ranges.is_empty() && self.next.is_none() {
            return None;
        }
        for r in self.ranges.values().flatten() {
            offer.insert_range(r);
        }
        offer.updates = self.updates.clone();
        Some(offer)
    }

    /// returns (close, open, step) candidates for the next tick
    fn candidates(&self, start: &Decimal) -> [Option<Decimal>; 3] {
        let step = start * self.delta;
        match self.side {
            Side::Base => [
                self.ranges
                    .keys()
                    .next()
                    .map(|v| Decimal::new(Uint128::new(*v))),
                self.next.clone().map(|x| x.ask),
                Some(start.add(step)),
            ],
            Side::Quote => [
                self.ranges
                    .keys()
                    .last()
                    .map(|v| Decimal::new(Uint128::new(*v))),
                self.next.clone().map(|x| x.bid),
                start.checked_sub(step).ok(),
            ],
        }
    }

    /// Add and remove Ranges as determined by their entry and exit ticks
    fn prune(&mut self, close: &Option<Decimal>, open: &Option<Decimal>) -> Option<()> {
        if let Some(p) = close {
            // We're at a range close limit, we need to remove these ranges once they've been used here
            if p == self.previous? {
                self.ranges.remove(&p.numerator().u128())?;
            }
        }

        if let Some(p) = open {
            // We're at a range open limit, we need to add the ranges for the next use here
            if p == self.previous? {
                self.fill_ranges(*p)
            }
        }
        Some(())
    }
}

impl<'a> Iterator for RangeOfferIter<'a> {
    type Item = RangeOffer;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            // if no previous end price, we're starting
            let mut start = self.previous.or_else(|| self.start())?;

            if self.ranges.is_empty() {
                start = self.open_price(&self.next.clone()?);
                self.previous = Some(start);
                self.fill_ranges(start);
            }

            // Next price is best(price + step, min(ranges.close), next.open) to ensure we have the smallest tick
            let opts = self.candidates(&start);
            // Get smallest step
            self.previous = match self.side {
                Side::Base => opts.into_iter().flatten().min(),
                Side::Quote => opts.into_iter().flatten().max(),
            };
            let offer = self.build_offer(&start)?;
            self.prune(&opts[0], &opts[1])?;
            if !offer.total.is_zero() {
                return Some(offer);
            }
        }
    }
}
