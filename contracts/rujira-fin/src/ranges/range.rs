use std::ops::Add;

use cosmwasm_schema::cw_serde;
use cosmwasm_std::{
    coin, ensure, Addr, Decimal, Fraction, Order, SignedDecimal, StdResult, Storage, Uint128,
};
use cw_storage_plus::{Bound, Index, IndexList, IndexedMap, Item, MultiIndex};
use cw_utils::NativeBalance;
use rujira_rs::{
    fin::{Denoms, RangeConfig, Scl, Side, Tick},
    utils::split_floor,
};

use crate::ranges::error::RangeError;

use super::RangeContext;

const KEY: &str = "r";
const KEY_OWNER: &str = "r_o";
const KEY_ASKS: &str = "r_a";
const KEY_BIDS: &str = "r_b";
const RANGE_COUNTER: Item<u128> = Item::new("range_counter");

/// A Range defines the Custom Curve to provide liquidity over
/// As per Uniswap v3, the Concentraded Liquidity algorithm takes a window of an XYK invariant, between `high` and `low` bounds,
/// and allocates all liquidity to that range, along the same curve
/// When mapped to an orderbook, XY=K is simple an ven distribution of uniform orders - ie a flat line
/// As such, a Range will distribute all available `quote` token for its position between the Position price and the `low` limit,
/// and all `base` tokens between price and `high` limit. Orders are placed with even separation, along the line defined by `skew`,
/// offset from the price by `spread`.
#[cw_serde]
pub struct Range {
    /// ID of the Range
    pub idx: Uint128,
    /// Range owner
    pub owner: Addr,
    /// Highest price of the Range
    pub high: Decimal,
    /// Lowest price of the Range
    pub low: Decimal,
    /// Skew in (-2, 2) for the current linear, non-symmetric distribution.
    /// Avoid using `skew` for now until the symmetric quadratic formula has been added.
    pub skew: SignedDecimal,
    pub spread: Decimal,
    /// Fee collected from the returned tokens. Should be less than spread otherwise the Range will consume itself
    pub fee: Decimal,
    /// Total base tokens owned by this Range
    pub base: Decimal,
    /// Total quote tokens owned by this Range
    pub quote: Decimal,
    /// Center price of the Range
    pub price: Decimal,
    /// Best ask price currently offered by this Range, truncated to the pair's tick
    pub ask: Decimal,
    /// Best bid price currently offered by this Range, truncated to the pair's tick
    pub bid: Decimal,
    /// (base, quote) Claimable fees collected by the range
    pub fees: [Decimal; 2],
}

pub struct Indexes<'a> {
    pub owner: MultiIndex<'a, Addr, Range, u128>,
    pub asks: MultiIndex<'a, u128, Range, u128>,
    pub bids: MultiIndex<'a, u128, Range, u128>,
}

impl<'a> IndexList<Range> for Indexes<'a> {
    fn get_indexes(&'_ self) -> Box<dyn Iterator<Item = &'_ dyn Index<Range>> + '_> {
        let v: Vec<&dyn Index<Range>> = vec![&self.owner, &self.asks, &self.bids];
        Box::new(v.into_iter())
    }
}

impl Range {
    pub fn id(&self) -> String {
        format!("{}:{}-{}", self.idx, self.low, self.high)
    }

    pub(crate) fn map<'a>() -> IndexedMap<u128, Self, Indexes<'a>> {
        let indexes = Indexes {
            owner: MultiIndex::new(|_pk, d| d.owner.clone(), KEY, KEY_OWNER),
            asks: MultiIndex::new(
                |_pk, d| {
                    if d.base.is_zero() {
                        // Move this out of the way of the iterator if it's empty
                        u128::MAX
                    } else {
                        d.ask.numerator().u128()
                    }
                },
                KEY,
                KEY_ASKS,
            ),
            bids: MultiIndex::new(
                |_pk, d| {
                    if d.quote.is_zero() {
                        // Move this out of the way of the iterator if it's empty
                        0
                    } else {
                        d.bid.numerator().u128()
                    }
                },
                KEY,
                KEY_BIDS,
            ),
        };
        IndexedMap::new(KEY, indexes)
    }

    /// Balances base/quote amounts for a target price, returning ((base, quote), refund).
    pub fn balance(
        config: &RangeConfig,
        price: Decimal,
        base: Decimal,
        quote: Decimal,
    ) -> ((Decimal, Decimal), (Decimal, Decimal)) {
        if price <= config.low {
            return ((base, Decimal::zero()), (Decimal::zero(), quote));
        }
        if price >= config.high {
            return ((Decimal::zero(), quote), (base, Decimal::zero()));
        }

        let scl = Scl::new(config.high, config.low, config.skew);
        let ratio = scl.balance_ratio(price);

        let ratio = match ratio {
            Some(r) if !r.is_zero() => r,
            Some(_) => return ((base, Decimal::zero()), (Decimal::zero(), quote)),
            None => return ((Decimal::zero(), quote), (base, Decimal::zero())),
        };

        let quote_needed = base * ratio;
        if quote_needed <= quote {
            (
                (base, quote_needed),
                (Decimal::zero(), quote - quote_needed),
            )
        } else {
            let base_needed = quote / ratio;
            ((base_needed, quote), (base - base_needed, Decimal::zero()))
        }
    }

    pub fn new(
        storage: &mut dyn Storage,
        tick: &Tick,
        fee_range: &Decimal,
        owner: Addr,
        rc: RangeConfig,
        (base, quote): (Decimal, Decimal),
    ) -> StdResult<Self> {
        let RangeConfig {
            high,
            low,
            skew,
            spread,
            fee,
        } = rc;
        let idx =
            Uint128::from(RANGE_COUNTER.may_load(storage)?.unwrap_or_default()).add(Uint128::one());
        RANGE_COUNTER.save(storage, &idx.u128())?;

        let scl = Scl::new(high, low, skew);
        let price = scl.price(base, quote);
        let two = Decimal::one() + Decimal::one();
        let offset = spread / two + *fee_range;

        Ok(Self {
            idx,
            owner,
            high,
            low,
            skew,
            spread,
            fee,
            base,
            quote,
            fees: [Decimal::zero(), Decimal::zero()],
            price,
            bid: scl.bid(price, offset, tick),
            ask: scl.ask(price, offset, tick),
        })
    }

    pub fn validate(&self, tick: &Tick) -> Result<(), RangeError> {
        ensure!(self.high > self.low, RangeError::InvalidBounds {});
        let two = SignedDecimal::from_atomics(2i128, 0).unwrap();
        ensure!(
            self.skew > -two && self.skew < two,
            RangeError::InvalidSkew {}
        );
        ensure!(self.spread < Decimal::one(), RangeError::InvalidSpread {});
        if self.spread.is_zero() {
            ensure!(self.fee.is_zero(), RangeError::InvalidFeeWithZeroSpread {});
        } else {
            let limit = self.spread * Decimal::from_ratio(98u128, 100u128);
            ensure!(self.fee <= limit, RangeError::InvalidFee {});
        }
        tick.validate_price(&self.high)?;
        tick.validate_price(&self.low)?;
        Ok(())
    }

    pub fn list(
        storage: &dyn Storage,
        cursor: Option<Uint128>,
        limit: usize,
        owner: Option<Addr>,
    ) -> StdResult<Vec<Self>> {
        match owner {
            Some(v) => Self::map()
                .idx
                .owner
                .prefix(v)
                .range(
                    storage,
                    cursor.map(|x| Bound::exclusive(x.u128())),
                    None,
                    Order::Ascending,
                )
                .map(|x| Ok(x?.1))
                .take(limit)
                .collect(),
            None => Self::map()
                .range(
                    storage,
                    cursor.map(|x| Bound::exclusive(x.u128())),
                    None,
                    Order::Ascending,
                )
                .map(|x| Ok(x?.1))
                .take(limit)
                .collect(),
        }
    }

    pub fn load(storage: &dyn Storage, idx: Uint128) -> StdResult<Self> {
        Self::map().load(storage, idx.u128())
    }

    pub fn save(&self, storage: &mut dyn Storage) -> StdResult<()> {
        Self::map().save(storage, self.idx.u128(), self)
    }

    pub fn by_owner(storage: &dyn Storage, owner: &Addr, limit: usize) -> StdResult<Vec<Self>> {
        Self::map()
            .idx
            .owner
            .prefix(owner.clone())
            .range(storage, None, None, Order::Ascending)
            .take(limit)
            .map(|x| Ok(x?.1))
            .collect()
    }

    pub fn swap(
        &mut self,
        context: &mut RangeContext,
        side: &Side,
        add: Decimal,
        deduct: Decimal,
    ) -> String {
        let amm_fee = context.apply_fee(side, add);
        let add = add - amm_fee;
        let two = Decimal::one() + Decimal::one();
        let fee = add * self.fee / two;
        let add = add.saturating_sub(fee);
        match side {
            Side::Base => {
                self.fees[1] += fee;
                self.quote += add;
                self.base -= deduct.min(self.base);
                self.reprice(&context.tick, &context.fee_range());
                format!(
                    "{}:{}:{}:{deduct}:{add}::{fee}",
                    self.id(),
                    self.base,
                    self.quote
                )
            }
            Side::Quote => {
                self.fees[0] += fee;
                self.quote -= deduct.min(self.quote);
                self.base += add;
                self.reprice(&context.tick, &context.fee_range());
                format!(
                    "{}:{}:{}:{deduct}:{add}:{fee}:",
                    self.id(),
                    self.base,
                    self.quote
                )
            }
        }
    }

    pub fn reprice(&mut self, tick: &Tick, fee_range: &Decimal) {
        let scl = Scl::new(self.high, self.low, self.skew);
        self.price = scl.price(self.base, self.quote);
        let two = Decimal::one() + Decimal::one();
        let offset = self.spread / two + *fee_range;
        self.bid = scl.bid(self.price, offset, tick);
        self.ask = scl.ask(self.price, offset, tick);
    }

    pub fn collect(&mut self, denoms: &Denoms) -> Option<NativeBalance> {
        let (a_int, a_rem) = split_floor(self.fees[0]);
        let (b_int, b_rem) = split_floor(self.fees[1]);
        let mut payout = NativeBalance(vec![
            coin(a_int.u128(), denoms.base()),
            coin(b_int.u128(), denoms.quote()),
        ]);
        payout.normalize();
        self.fees = [a_rem, b_rem];
        if payout.is_empty() {
            return None;
        }
        Some(payout)
    }

    pub fn withdraw(&mut self, denoms: &Denoms, amount: Decimal) -> Option<NativeBalance> {
        let base = (self.base * amount).floor();
        let quote = (self.quote * amount).floor();

        self.base -= base;
        self.quote -= quote;

        if self.base < Decimal::one() {
            self.base = Decimal::zero();
        }
        if self.quote < Decimal::one() {
            self.quote = Decimal::zero();
        }

        let mut payout = NativeBalance(vec![
            coin(base.to_uint_floor().u128(), denoms.base()),
            coin(quote.to_uint_floor().u128(), denoms.quote()),
        ]);
        payout.normalize();
        if payout.is_empty() {
            return None;
        };

        Some(payout)
    }

    pub fn deposit(&mut self, base: Decimal, quote: Decimal) {
        self.base += base;
        self.quote += quote;
    }

    pub fn close(&mut self, denoms: &Denoms) -> Option<NativeBalance> {
        let mut payout = NativeBalance(vec![
            coin(self.base.to_uint_floor().u128(), denoms.base()),
            coin(self.quote.to_uint_floor().u128(), denoms.quote()),
            coin(self.fees[0].to_uint_floor().u128(), denoms.base()),
            coin(self.fees[1].to_uint_floor().u128(), denoms.quote()),
        ]);
        // This will be deleted, but zero here for completeness
        self.base = Decimal::zero();
        self.quote = Decimal::zero();
        self.fees = [Decimal::zero(), Decimal::zero()];

        payout.normalize();
        if payout.is_empty() {
            return None;
        }

        Some(payout)
    }

    /// Compute amount of tokens available in a price slice [p_start, p_end]
    pub fn amount_in_slice(&self, side: &Side, p_start: Decimal, p_end: Decimal) -> Decimal {
        let scl = Scl::new(self.high, self.low, self.skew);
        let amount = scl.amount_in_slice(side, self.base, self.quote, self.price, p_start, p_end);
        match side {
            Side::Base => amount.min(self.base),
            Side::Quote => amount.min(self.quote),
        }
    }
}
