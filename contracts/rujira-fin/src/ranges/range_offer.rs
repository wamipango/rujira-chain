use std::{
    cell::{Ref, RefCell},
    collections::{btree_map::Entry, BTreeMap},
    ops::{Div, Mul},
    rc::Rc,
};

use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Attribute, Decimal, Decimal256, SignedDecimal, Uint128};
use rujira_rs::{
    exchange::{SwapError, SwapItem},
    fin::{Scl, Side},
    DecimalDyn,
};

use super::{Range, RangeContext};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct CurveKey {
    high: Decimal,
    low: Decimal,
    skew: SignedDecimal,
}

impl From<&Range> for CurveKey {
    fn from(range: &Range) -> Self {
        Self {
            high: range.high,
            low: range.low,
            skew: range.skew,
        }
    }
}

#[derive(Clone, Copy)]
struct OfferData {
    curve: CurveKey,
    liquidity: DecimalDyn,
    available: Decimal,
}

#[derive(Clone, Copy)]
struct CachedSlice {
    amount_per_liquidity: DecimalDyn,
    effective_rate: Decimal256,
}

type SliceCache = BTreeMap<CurveKey, CachedSlice>;

#[derive(Debug, PartialEq)]
struct RangeHandleState {
    range: Range,
    scl: Scl,
    /// Authoritative cached curve liquidity (L₀), refreshed whenever a swap changes reserves.
    liquidity: DecimalDyn,
}

#[derive(Clone, Debug, PartialEq)]
struct RangeHandleCheckpoint {
    range: Range,
    liquidity: DecimalDyn,
}

#[cw_serde]
pub struct RangeHandle {
    pub idx: Uint128,
    #[serde(skip)]
    #[schemars(skip)]
    state: Option<Rc<RefCell<RangeHandleState>>>,
    #[serde(skip)]
    #[schemars(skip)]
    checkpoint: Option<RangeHandleCheckpoint>,
}

impl RangeHandle {
    fn state(&self) -> &Rc<RefCell<RangeHandleState>> {
        self.state.as_ref().expect("range handle missing")
    }

    pub fn new(range: Range) -> Self {
        let idx = range.idx;
        let scl = Scl::new(range.high, range.low, range.skew);
        let liquidity = scl.liquidity(range.base, range.quote, range.price);
        Self {
            idx,
            state: Some(Rc::new(RefCell::new(RangeHandleState {
                range,
                scl,
                liquidity,
            }))),
            checkpoint: None,
        }
    }

    pub(crate) fn borrow(&self) -> Ref<'_, Range> {
        Ref::map(self.state().borrow(), |state| &state.range)
    }

    fn offer_data(&self, side: &Side) -> OfferData {
        let state = self.state().borrow();
        let range = &state.range;
        let available = match side {
            Side::Base => range.base,
            Side::Quote => range.quote,
        };
        OfferData {
            curve: CurveKey::from(range),
            liquidity: state.liquidity,
            available,
        }
    }

    fn curve(&self) -> CurveKey {
        let range = self.borrow();
        CurveKey::from(&*range)
    }

    fn slice(&self, side: &Side, start: Decimal, end: Decimal) -> Option<(DecimalDyn, Decimal256)> {
        self.state().borrow().scl.slice(side, start, end)
    }

    fn swap(
        &self,
        context: &mut RangeContext,
        side: &Side,
        add: Decimal,
        deduct: Decimal,
    ) -> String {
        let mut state = self.state().borrow_mut();
        let used = state.range.swap(context, side, add, deduct);
        let liquidity = state
            .scl
            .liquidity(state.range.base, state.range.quote, state.range.price);
        state.liquidity = liquidity;
        used
    }

    pub fn checkpoint(&mut self) {
        let checkpoint = {
            let state = self.state().borrow();
            RangeHandleCheckpoint {
                range: state.range.clone(),
                liquidity: state.liquidity,
            }
        };
        self.checkpoint = Some(checkpoint);
    }

    pub fn rollback(&mut self) {
        if let Some(checkpoint) = self.checkpoint.take() {
            let mut state = self.state().borrow_mut();
            state.range = checkpoint.range;
            state.liquidity = checkpoint.liquidity;
        }
    }

    pub fn commit(&mut self) {
        self.checkpoint = None;
    }
}

#[cw_serde]
pub struct RangeOffer {
    pub side: Side,
    pub start: Decimal,
    pub end: Decimal,
    pub total: Decimal,
    /// Volume-weighted effective price from the SCL curve (ΔY/ΔX)
    pub effective_rate: Decimal256,
    /// A vec of the (range handle, amount)'s contributing to this offer
    pub ranges: Vec<(RangeHandle, Decimal)>,
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

    pub fn insert_range(&mut self, range: &RangeHandle) {
        if let Some((amount_per_liquidity, effective_price)) =
            range.slice(&self.side, self.start, self.end)
        {
            let data = range.offer_data(&self.side);
            let amount = Self::amount(data.liquidity, amount_per_liquidity, data.available);
            self.insert_contribution(range, amount, effective_price);
        }
    }

    /// Insert a group of ranges while sharing slice calculations between
    /// ranges with identical bounds and skew.
    pub fn insert_ranges<'a>(&mut self, ranges: impl IntoIterator<Item = &'a RangeHandle>) {
        let mut ranges = ranges.into_iter();
        if !self.ranges.is_empty() {
            for range in ranges {
                self.insert_range(range);
            }
            return;
        }

        self.ranges
            .reserve(ranges.size_hint().1.unwrap_or_default());
        let mut slices = SliceCache::new();

        for range in &mut ranges {
            let data = range.offer_data(&self.side);
            let slice = match slices.entry(data.curve) {
                Entry::Occupied(entry) => *entry.get(),
                Entry::Vacant(entry) => {
                    let Some((amount_per_liquidity, effective_rate)) =
                        range.slice(&self.side, self.start, self.end)
                    else {
                        continue;
                    };
                    *entry.insert(CachedSlice {
                        amount_per_liquidity,
                        effective_rate,
                    })
                }
            };

            let amount = Self::amount(data.liquidity, slice.amount_per_liquidity, data.available);
            self.append_range(range, amount);
        }

        self.finalize_effective_rate(&slices);
    }

    fn finalize_effective_rate(&mut self, slices: &SliceCache) {
        if self.total.is_zero() {
            return;
        }

        // Whole-token contributions preserve a common rate exactly under the
        // legacy sequential average, so they can skip all weighted divisions.
        if let Some(rate) = self.exact_common_rate(slices) {
            self.effective_rate = rate;
            return;
        }

        self.effective_rate = self.sequential_effective_rate(slices);
    }

    fn exact_common_rate(&self, slices: &SliceCache) -> Option<Decimal256> {
        if self
            .ranges
            .iter()
            .any(|(_, amount)| *amount != amount.floor())
        {
            return None;
        }

        let mut rates = slices.values().map(|slice| slice.effective_rate);
        let first = rates.next()?;
        rates.all(|rate| rate == first).then_some(first)
    }

    /// Reproduce the historical atom-level rounding for fractional or
    /// mixed-rate contributions.
    fn sequential_effective_rate(&self, slices: &SliceCache) -> Decimal256 {
        let mut total = Decimal::zero();
        let mut effective_rate = Decimal256::from(self.start);

        for (range, amount) in &self.ranges {
            let rate = slices
                .get(&range.curve())
                .expect("range offer curve slice missing")
                .effective_rate;
            let old_total = Decimal256::from(total);
            total += *amount;
            effective_rate = (old_total * effective_rate + Decimal256::from(*amount) * rate)
                / Decimal256::from(total);
        }

        effective_rate
    }

    fn amount(
        liquidity: DecimalDyn,
        amount_per_liquidity: DecimalDyn,
        available: Decimal,
    ) -> Decimal {
        Decimal::try_from(liquidity * amount_per_liquidity)
            .unwrap_or_default()
            .min(available)
    }

    fn insert_contribution(
        &mut self,
        range: &RangeHandle,
        amount: Decimal,
        effective_price: Decimal256,
    ) {
        // Volume-weighted effective price: (old_total * old_rate + amount * rate) / new_total
        let old_total = Decimal256::from(self.total);
        self.append_range(range, amount);
        if !self.total.is_zero() {
            self.effective_rate = (old_total * self.effective_rate
                + Decimal256::from(amount) * effective_price)
                / Decimal256::from(self.total);
        }
    }

    #[inline]
    fn append_range(&mut self, range: &RangeHandle, amount: Decimal) {
        self.total += amount;
        if !self.total.is_zero() {
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

        // The return leg is floored so the range never pays fractional tokens.
        // Once that integer return is chosen, round the input leg up so the
        // range is paid for the whole amount it returns. Rounding both legs
        // down lets same-range arb loops create value from truncation.
        let bids_used = Decimal::try_from(match self.side {
            Side::Base => offer256 * rate_inv256,
            Side::Quote => offer256 * rate256,
        })?
        .min(self.total)
        .floor();

        let offer_used = Decimal::try_from(match self.side {
            Side::Base => Decimal256::from(bids_used) * rate256,
            Side::Quote => Decimal256::from(bids_used) * rate_inv256,
        })?;

        // Integer settlement cannot execute a trade once either leg rounds to zero.
        // Returning a partial zero-leg fill here creates phantom profit in arb loops.
        if offer_used.to_uint_ceil().is_zero() || bids_used.to_uint_floor().is_zero() {
            return Ok((Uint128::zero(), Uint128::zero(), vec![]));
        }
        let offer_used = Decimal::from_atomics(offer_used.to_uint_ceil(), 0)?;

        let mut remaining_bids_used = bids_used;
        let mut remaining_offer_used = offer_used;
        let len = self.ranges.len();
        for i in 0..len {
            let (range_handle, v) = &mut self.ranges[i];

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
            *v -= deduct.min(*v);
            let idx = range_handle.idx;
            // Fold onto the existing cell so a range swapped on both arb sides keeps both legs.
            let handle = context
                .ranges
                .get(&idx)
                .cloned()
                .unwrap_or_else(|| range_handle.clone());
            let used_id = handle.swap(context, &self.side, add, deduct);
            context.ranges.insert(idx, handle);
            used.push(used_id);
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

    fn checkpoint(&mut self) {
        for (range, _) in &mut self.ranges {
            range.checkpoint();
        }
    }

    fn rollback(&mut self) {
        for (range, _) in &mut self.ranges {
            range.rollback();
        }
    }
}
