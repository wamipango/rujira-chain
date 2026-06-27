use std::{hint::black_box, mem::size_of, str::FromStr, time::Instant};

use cosmwasm_std::{
    testing::mock_dependencies, Addr, Decimal, Decimal256, SignedDecimal, Storage, Uint128,
};
use cw_storage_plus::Map;
use rujira_fin::ranges::{Range, RangeHandle, RangeOffer};
use rujira_rs::{
    fin::{RangeConfig, Scl, Side, Tick},
    DecimalDyn,
};

const RANGE_COUNT: usize = 50;
const OFFER_COUNT: usize = 200;
const OUTER_ITERS: usize = 1000;
const STORED_SLICES: Map<&[u8], (Decimal256, Decimal256)> = Map::new("bench_slices");

#[derive(Clone)]
struct OwnedRangeOffer {
    side: Side,
    start: Decimal,
    end: Decimal,
    total: Decimal,
    effective_rate: Decimal256,
    ranges: Vec<(Range, Decimal)>,
}

impl OwnedRangeOffer {
    fn new(side: &Side, start: Decimal, end: Decimal) -> Self {
        Self {
            side: side.clone(),
            start,
            end,
            total: Decimal::zero(),
            effective_rate: Decimal256::from(start),
            ranges: Vec::new(),
        }
    }

    fn insert_range(&mut self, range: &Range) {
        let amount = range.amount_in_slice(&self.side, self.start, self.end);
        let scl = Scl::new(range.high, range.low, range.skew);
        if let Some(eff_price) = scl.effective_price(self.start, self.end) {
            let old_total = Decimal256::from(self.total);
            self.total += amount;
            if !self.total.is_zero() {
                self.effective_rate = (old_total * self.effective_rate
                    + Decimal256::from(amount) * eff_price)
                    / Decimal256::from(self.total);
                self.ranges.push((range.clone(), amount));
            }
        }
    }
}

struct StorageCachedRange {
    range: Range,
    liquidity: DecimalDyn,
}

impl StorageCachedRange {
    fn new(range: Range) -> Self {
        let scl = Scl::new(range.high, range.low, range.skew);
        let liquidity = scl.liquidity(range.base, range.quote, range.price);
        Self { range, liquidity }
    }
}

struct StorageCachedOffer {
    total: Decimal,
    effective_rate: Option<Decimal256>,
    mixed_rates: bool,
    ranges: Vec<(Uint128, Decimal, Decimal256)>,
}

impl StorageCachedOffer {
    fn new() -> Self {
        Self {
            total: Decimal::zero(),
            effective_rate: None,
            mixed_rates: false,
            ranges: Vec::with_capacity(RANGE_COUNT),
        }
    }

    fn insert_range(
        &mut self,
        storage: &mut dyn Storage,
        range: &StorageCachedRange,
        start: Decimal,
        end: Decimal,
    ) {
        let key = slice_key(&range.range, start, end);
        let (amount_per_liquidity, effective_rate) = STORED_SLICES
            .may_load(storage, &key)
            .unwrap()
            .unwrap_or_else(|| {
                let slice = Scl::new(range.range.high, range.range.low, range.range.skew)
                    .slice(&Side::Base, start, end)
                    .map(|(amount, rate)| (amount.full(), rate))
                    .unwrap();
                STORED_SLICES.save(storage, &key, &slice).unwrap();
                slice
            });

        let amount = Decimal::try_from(range.liquidity * DecimalDyn::from(amount_per_liquidity))
            .unwrap_or_default()
            .min(range.range.base);
        self.total += amount;
        if !self.total.is_zero() {
            if let Some(first) = self.effective_rate {
                self.mixed_rates |= first != effective_rate;
            } else {
                self.effective_rate = Some(effective_rate);
            }
            self.ranges.push((range.range.idx, amount, effective_rate));
        }
    }

    fn finalize(&mut self, start: Decimal) {
        if self.total.is_zero()
            || (!self.mixed_rates
                && self
                    .ranges
                    .iter()
                    .all(|(_, amount, _)| *amount == amount.floor()))
        {
            return;
        }

        let mut total = Decimal::zero();
        let mut effective_rate = Decimal256::from(start);
        for (_, amount, rate) in &self.ranges {
            let old_total = Decimal256::from(total);
            total += *amount;
            effective_rate = (old_total * effective_rate + Decimal256::from(*amount) * *rate)
                / Decimal256::from(total);
        }
        self.effective_rate = Some(effective_rate);
    }
}

fn slice_key(range: &Range, start: Decimal, end: Decimal) -> [u8; 81] {
    let mut key = [0u8; 81];
    key[0..16].copy_from_slice(&range.high.atomics().to_be_bytes());
    key[16..32].copy_from_slice(&range.low.atomics().to_be_bytes());
    key[32..48].copy_from_slice(&range.skew.atomics().to_be_bytes());
    key[48..64].copy_from_slice(&start.atomics().to_be_bytes());
    key[64..80].copy_from_slice(&end.atomics().to_be_bytes());
    key[80] = 0; // Side::Base
    key
}

fn make_range(index: usize, varied: bool) -> Range {
    let mut deps = mock_dependencies();
    let (high, low, skew) = if varied {
        (
            Decimal::from_ratio(4000u128 + index as u128, 1000u128),
            Decimal::from_ratio(3000u128 - index as u128, 1000u128),
            SignedDecimal::from_atomics(index as i128 - 25, 2).unwrap(),
        )
    } else {
        (
            Decimal::from_str("4").unwrap(),
            Decimal::from_str("3").unwrap(),
            SignedDecimal::zero(),
        )
    };
    Range::new(
        deps.as_mut().storage,
        &Tick::new(8),
        &Decimal::zero(),
        Addr::unchecked("bench"),
        RangeConfig {
            high,
            low,
            skew,
            spread: Decimal::permille(10),
            fee: Decimal::zero(),
        },
        (
            Decimal::from_str("100000000").unwrap(),
            Decimal::from_str("401992761").unwrap(),
        ),
    )
    .unwrap()
}

fn build_ranges(varied: bool) -> Vec<Range> {
    (0..RANGE_COUNT)
        .map(|index| make_range(index, varied))
        .collect()
}

fn run_handle_path(ranges: &[RangeHandle]) -> Decimal {
    let mut checksum = Decimal::zero();
    for _ in 0..OFFER_COUNT {
        let mut offer = RangeOffer::new(
            &Side::Base,
            &Decimal::from_str("3.25").unwrap(),
            &Decimal::from_str("3.75").unwrap(),
        );
        offer.insert_ranges(ranges);
        checksum += offer.total;
        black_box(&offer);
    }
    checksum
}

fn run_owned_path(ranges: &[Range]) -> Decimal {
    let mut checksum = Decimal::zero();
    for _ in 0..OFFER_COUNT {
        let mut offer = OwnedRangeOffer::new(
            &Side::Base,
            Decimal::from_str("3.25").unwrap(),
            Decimal::from_str("3.75").unwrap(),
        );
        for range in ranges {
            offer.insert_range(range);
        }
        checksum += offer.total;
        black_box(&offer);
    }
    checksum
}

fn run_storage_cache_path(
    ranges: &[StorageCachedRange],
    storage: &mut dyn Storage,
    clear_each_offer: bool,
) -> Decimal {
    let mut checksum = Decimal::zero();
    for _ in 0..OFFER_COUNT {
        if clear_each_offer {
            STORED_SLICES.clear(storage);
        }
        let start = Decimal::from_str("3.25").unwrap();
        let end = Decimal::from_str("3.75").unwrap();
        let mut offer = StorageCachedOffer::new();
        for range in ranges {
            offer.insert_range(storage, range, start, end);
        }
        offer.finalize(start);
        checksum += offer.total;
        black_box(&offer);
    }
    checksum
}

fn time_path<F>(label: &str, outer_iters: usize, mut f: F)
where
    F: FnMut() -> Decimal,
{
    let started = Instant::now();
    let mut checksum = Decimal::zero();
    for _ in 0..outer_iters {
        checksum += f();
    }
    let elapsed = started.elapsed();
    let total_offers = outer_iters * OFFER_COUNT;
    let nanos_per_offer = elapsed.as_nanos() as f64 / total_offers as f64;
    println!(
        "{label}: {elapsed:?} total, {nanos_per_offer:.1} ns/offer, checksum={}",
        checksum
    );
}

fn main() {
    let outer_iters = std::env::var("RANGE_BENCH_ITERS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(OUTER_ITERS);
    let path = std::env::var("RANGE_BENCH_PATH").unwrap_or_else(|_| "both".to_owned());
    let varied = std::env::var("RANGE_BENCH_CURVES").is_ok_and(|value| value == "varied");

    let ranges = build_ranges(varied);
    let handles: Vec<_> = ranges.iter().cloned().map(RangeHandle::new).collect();
    let stored_ranges: Vec<_> = ranges
        .iter()
        .cloned()
        .map(StorageCachedRange::new)
        .collect();
    let mut storage = mock_dependencies().storage;

    println!("range count: {RANGE_COUNT}");
    println!("offer count per run: {OFFER_COUNT}");
    println!("outer iters: {outer_iters}");
    println!(
        "curve shapes: {}",
        if varied { "varied" } else { "identical" }
    );
    println!("size_of::<Range>(): {} bytes", size_of::<Range>());
    println!(
        "size_of::<RangeHandle>(): {} bytes",
        size_of::<RangeHandle>()
    );
    println!("size_of::<RangeOffer>(): {} bytes", size_of::<RangeOffer>());
    println!(
        "size_of::<OwnedRangeOffer>(): {} bytes",
        size_of::<OwnedRangeOffer>()
    );
    println!(
        "size_of::<(Range, Decimal)>(): {} bytes",
        size_of::<(Range, Decimal)>()
    );
    println!(
        "size_of::<(RangeHandle, Decimal)>(): {} bytes",
        size_of::<(RangeHandle, Decimal)>()
    );
    println!(
        "50-entry payload: owned={} bytes, handle={} bytes",
        RANGE_COUNT * size_of::<(Range, Decimal)>(),
        RANGE_COUNT * size_of::<(RangeHandle, Decimal)>()
    );

    if path == "both" || path == "owned" {
        time_path("owned-range baseline", outer_iters, || {
            run_owned_path(&ranges)
        });
    }
    if path == "both" || path == "handle" || path == "compare" {
        time_path("handle-based current", outer_iters, || {
            run_handle_path(&handles)
        });
    }
    if path == "storage" || path == "compare" {
        time_path("cw-storage-plus Map", outer_iters, || {
            run_storage_cache_path(&stored_ranges, &mut storage, false)
        });
    }
    if path == "storage-cold" {
        time_path("cw-storage-plus Map (cold)", outer_iters, || {
            run_storage_cache_path(&stored_ranges, &mut storage, true)
        });
    }
}
