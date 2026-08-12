use std::{collections::BTreeMap, ops::Add};

use cosmwasm_std::{Decimal, Fraction, Order, StdResult, Storage, Uint128};
use rujira_rs::fin::Side;

use super::{range_offer::RangeHandle, Range, RangeOffer};

#[cfg(test)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct RangeIterStats {
    pub(crate) rows_visited: usize,
    pub(crate) eligible_ranges: usize,
    pub(crate) offers_built: usize,
    pub(crate) range_contributions: usize,
    pub(crate) executable_offers: usize,
}

#[cfg(test)]
thread_local! {
    static RANGE_ITER_STATS: std::cell::Cell<RangeIterStats> = const {
        std::cell::Cell::new(RangeIterStats {
            rows_visited: 0,
            eligible_ranges: 0,
            offers_built: 0,
            range_contributions: 0,
            executable_offers: 0,
        })
    };
}

#[cfg(test)]
pub(crate) fn reset_range_iter_stats() {
    RANGE_ITER_STATS.set(RangeIterStats::default());
}

#[cfg(test)]
pub(crate) fn range_iter_stats() -> RangeIterStats {
    RANGE_ITER_STATS.get()
}

pub struct RangeOfferIter<'a> {
    /// The default step between prices
    delta: Decimal,
    /// The previous end price
    previous: Option<Decimal>,
    /// The most recent Item popped from the iterator, that wasn't part of the current group
    next: Option<RangeHandle>,
    side: Side,
    /// a Map of active ranges, grouped and keyed by the price at which they go out of range for easy rejection
    ranges: BTreeMap<u128, Vec<RangeHandle>>,
    /// Base Range iter
    iter: Box<dyn Iterator<Item = RangeHandle> + 'a>,
}

impl<'a> RangeOfferIter<'a> {
    fn filter(
        side: Side,
        min: Decimal,
    ) -> impl Fn(StdResult<(u128, Range)>) -> Option<RangeHandle> {
        move |res| match res {
            Ok((_, range)) => {
                let accepted = match side {
                    Side::Base => range.base >= min && range.ask < range.high,
                    Side::Quote => range.quote >= min && range.bid > range.low,
                };
                #[cfg(test)]
                RANGE_ITER_STATS.with(|stats| {
                    let mut current = stats.get();
                    current.rows_visited += 1;
                    current.eligible_ranges += usize::from(accepted);
                    stats.set(current);
                });
                accepted.then_some(RangeHandle::new(range))
            }
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
            delta: *delta,
            iter: Box::new(base_iter),
            next: None,
            previous: None,
        }
    }

    fn fill_ranges(&mut self, curr: Decimal) {
        while let Some(next) = self.next.clone() {
            if match self.side {
                Side::Base => next.borrow().ask,
                Side::Quote => next.borrow().bid,
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
    fn insert_range(&mut self, range: &RangeHandle) {
        let key = match self.side {
            Side::Base => range.borrow().high,
            Side::Quote => range.borrow().low,
        }
        .numerator()
        .u128();
        self.ranges.entry(key).or_default().push(range.clone());
    }

    fn open_price(&self, range: &RangeHandle) -> Decimal {
        match self.side {
            Side::Base => range.borrow().ask,
            Side::Quote => range.borrow().bid,
        }
    }

    fn start(&mut self) -> Option<Decimal> {
        let first = self.iter.next()?;
        self.next = self.iter.next();
        self.insert_range(&first);
        Some(self.open_price(&first))
    }

    fn build_offer(&mut self, start: &Decimal) -> Option<RangeOffer> {
        let mut offer = RangeOffer::new(&self.side, start, &self.previous?);
        if self.ranges.is_empty() && self.next.is_none() {
            return None;
        }
        #[cfg(test)]
        RANGE_ITER_STATS.with(|stats| {
            let mut current = stats.get();
            current.offers_built += 1;
            current.range_contributions += self.ranges.values().map(Vec::len).sum::<usize>();
            stats.set(current);
        });
        offer.insert_ranges(self.ranges.values().flatten());
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
                self.next.clone().map(|x| x.borrow().ask),
                Some(start.add(step)),
            ],
            Side::Quote => [
                self.ranges
                    .keys()
                    .last()
                    .map(|v| Decimal::new(Uint128::new(*v))),
                self.next.clone().map(|x| x.borrow().bid),
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
                #[cfg(test)]
                RANGE_ITER_STATS.with(|stats| {
                    let mut current = stats.get();
                    current.executable_offers +=
                        usize::from(!offer.total.to_uint_floor().is_zero());
                    stats.set(current);
                });
                return Some(offer);
            }
        }
    }
}
