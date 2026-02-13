use std::{collections::BTreeMap, str::FromStr};

use crate::contract::CONTRACT_NAME;

use super::*;
use cosmwasm_std::{
    coin, coins, testing::mock_dependencies, Addr, BankMsg, CosmosMsg, Decimal, Event, Order,
    Response, SignedDecimal, SubMsg, Uint128,
};
use cw_utils::NativeBalance;
use proptest::prelude::*;
use rujira_rs::{
    exchange::{SwapContext, SwapItem, Swapper},
    fin::{Denoms, RangeConfig, Side, Tick},
};

#[test]
fn cl_pricing() {
    let scl = Scl::new(
        Decimal::from_str("4000").unwrap(),
        Decimal::from_str("3000").unwrap(),
        SignedDecimal::zero(),
    );

    let price = scl.price(
        Decimal::from_str("100000").unwrap(),
        Decimal::from_str("401992761").unwrap(),
    );
    assert!(price >= Decimal::from_str("3499").unwrap());
    assert!(price <= Decimal::from_str("3501").unwrap());
}

#[test]
fn range_offer_iter_simple() {
    let mut deps = mock_dependencies();
    let tick = Tick::new(8);
    let owner = Addr::unchecked("owner");

    let range = Range::new(
        deps.as_mut().storage,
        &tick,
        &Decimal::zero(),
        owner.clone(),
        RangeConfig {
            high: Decimal::from_str("4").unwrap(),
            low: Decimal::from_str("3").unwrap(),
            skew: SignedDecimal::zero(),
            spread: Decimal::permille(10),
            fee: Decimal::zero(),
        },
        (
            Decimal::from_str("100000000").unwrap(),
            Decimal::from_str("401992761").unwrap(),
        ),
    )
    .unwrap();
    range.save(deps.as_mut().storage).unwrap();

    let bids: Vec<RangeOffer> =
        RangeOfferIter::new(deps.as_ref().storage, &Side::Quote, &Decimal::permille(50)).collect();
    let asks: Vec<RangeOffer> =
        RangeOfferIter::new(deps.as_ref().storage, &Side::Base, &Decimal::permille(50)).collect();

    assert_eq!(asks[2].clone().end, Decimal::from_str("4").unwrap());
    assert_eq!(
        asks[2].clone().start,
        Decimal::from_str("3.86426261025").unwrap()
    );

    assert_eq!(
        asks[2].clone().total,
        Decimal::from_str("25217749.096797922204283943").unwrap()
    );

    assert_eq!(asks[1].clone().end, asks[2].start);
    assert_eq!(
        asks[1].clone().start,
        Decimal::from_str("3.680250105").unwrap()
    );
    assert_eq!(
        asks[1].clone().total,
        Decimal::from_str("36389411.025499039805181872").unwrap()
    );
    assert_eq!(asks[0].clone().end, asks[1].start);
    assert_eq!(
        asks[0].clone().start,
        Decimal::from_str("3.5050001").unwrap()
    );
    assert_eq!(
        asks[0].clone().total,
        Decimal::from_str("37288050.318055601185689535").unwrap()
    );
    assert_eq!(bids[0].clone().start, Decimal::from_str("3.495").unwrap());
    assert_eq!(
        bids[0].clone().total,
        Decimal::from_str("137118153.600797312162479599").unwrap()
    );
    assert_eq!(bids[1].clone().start, Decimal::from_str("3.32025").unwrap());
    assert_eq!(
        bids[1].clone().total,
        Decimal::from_str("133646244.40868981230894682").unwrap()
    );

    // Ensure we're not over-allocating
    let total = bids.iter().fold(Decimal::zero(), |a, v| a + v.total);
    assert!(total <= range.quote);
    let total = asks.iter().fold(Decimal::zero(), |a, v| a + v.total);
    assert!(total <= range.base);
}

#[test]
fn range_offer_iter_dual() {
    let mut deps = mock_dependencies();
    let tick = Tick::new(8);
    let owner = Addr::unchecked("owner");

    let range_1 = Range::new(
        deps.as_mut().storage,
        &tick,
        &Decimal::zero(),
        owner.clone(),
        RangeConfig {
            high: Decimal::from_str("4").unwrap(),
            low: Decimal::from_str("3").unwrap(),
            skew: SignedDecimal::zero(),
            spread: Decimal::permille(10),
            fee: Decimal::zero(),
        },
        (
            Decimal::from_str("100000000").unwrap(),
            Decimal::from_str("401992761").unwrap(),
        ),
    )
    .unwrap();
    range_1.save(deps.as_mut().storage).unwrap();

    let range_2 = Range::new(
        deps.as_mut().storage,
        &tick,
        &Decimal::zero(),
        owner.clone(),
        RangeConfig {
            high: Decimal::from_str("5").unwrap(),
            low: Decimal::from_str("2").unwrap(),
            skew: SignedDecimal::zero(),
            spread: Decimal::permille(10),
            fee: Decimal::zero(),
        },
        (
            Decimal::from_str("100000000").unwrap(),
            Decimal::from_str("401992761").unwrap(),
        ),
    )
    .unwrap();
    range_2.save(deps.as_mut().storage).unwrap();

    let asks: Vec<RangeOffer> =
        RangeOfferIter::new(deps.as_ref().storage, &Side::Base, &Decimal::permille(50))
            .take(20)
            .collect::<Vec<RangeOffer>>();

    // Range 2 is better priced and has a wider range. we should have complete overlap with 1
    assert_eq!(asks[0].clone().start, range_2.ask);
    assert_eq!(asks[0].clone().end, asks[1].clone().start);
    assert_eq!(asks[0].clone().ranges.len(), 1);
    assert_eq!(
        asks[0].clone().total,
        Decimal::from_str("12992760.525579195390477917").unwrap()
    );
    assert_eq!(
        asks[0].clone().ranges[0].clone().0.idx,
        Uint128::from(2u128)
    );

    assert_eq!(asks[1].clone().ranges.len(), 1);
    assert_eq!(
        asks[1].clone().total,
        Decimal::from_str("27160.065658917166924412").unwrap()
    );
    assert_eq!(
        asks[1].clone().start,
        Decimal::from_str("3.504638235").unwrap()
    );
    // Check we have a definite stop at the bid limit
    assert_eq!(asks[1].clone().end, range_1.ask);

    assert_eq!(asks[2].clone().ranges.len(), 2);
    assert_eq!(
        asks[2].clone().total,
        Decimal::from_str("49967031.707170068300242173").unwrap()
    );
    assert_eq!(asks[2].clone().start, range_1.ask);
    assert_eq!(asks[3].clone().ranges.len(), 2);
    assert_eq!(
        asks[3].clone().total,
        Decimal::from_str("48762829.888048941808333293").unwrap()
    );
    assert_eq!(
        asks[3].clone().start,
        Decimal::from_str("3.680250105").unwrap()
    );
    assert_eq!(asks[5].clone().ranges.len(), 1);
    assert_eq!(
        asks[5].clone().total,
        Decimal::from_str("11868569.739550264659847164").unwrap()
    );
    assert_eq!(asks[5].clone().start, range_1.high);
    assert_eq!(asks[6].clone().ranges.len(), 1);
    assert_eq!(asks[9].clone().end, range_2.high);
}

#[test]
fn range_offer_iter_sparse() {
    // Ensure that we skip gaps between non overlappoing ranges
    let mut deps = mock_dependencies();
    let tick = Tick::new(8);
    let owner = Addr::unchecked("owner");

    let range_1 = Range::new(
        deps.as_mut().storage,
        &tick,
        &Decimal::zero(),
        owner.clone(),
        RangeConfig {
            high: Decimal::from_str("4").unwrap(),
            low: Decimal::from_str("3").unwrap(),
            skew: SignedDecimal::zero(),
            spread: Decimal::permille(10),
            fee: Decimal::zero(),
        },
        (
            Decimal::from_str("100000000").unwrap(),
            Decimal::from_str("401992761").unwrap(),
        ),
    )
    .unwrap();
    range_1.save(deps.as_mut().storage).unwrap();

    let range_2 = Range::new(
        deps.as_mut().storage,
        &tick,
        &Decimal::zero(),
        owner.clone(),
        RangeConfig {
            high: Decimal::from_str("12").unwrap(),
            low: Decimal::from_str("10").unwrap(),
            skew: SignedDecimal::zero(),
            spread: Decimal::permille(10),
            fee: Decimal::zero(),
        },
        (Decimal::from_str("100000000").unwrap(), Decimal::zero()),
    )
    .unwrap();
    range_2.save(deps.as_mut().storage).unwrap();

    let asks: Vec<RangeOffer> =
        RangeOfferIter::new(deps.as_ref().storage, &Side::Base, &Decimal::permille(50))
            .take(20)
            .collect::<Vec<RangeOffer>>();

    assert_eq!(asks[0].clone().start, range_1.ask);
    assert_eq!(
        asks[0].clone().total,
        Decimal::from_str("37288050.318055601185689535").unwrap()
    );
    assert_eq!(asks[0].clone().ranges.len(), 1);
    assert_eq!(asks[0].clone().ranges[0].clone().0.idx, Uint128::one());
    assert_eq!(asks[2].clone().end, range_1.high);
    assert_eq!(asks[2].clone().ranges[0].clone().0.idx, Uint128::one());

    assert_eq!(asks[3].clone().start, range_2.ask);
    assert_eq!(asks[3].clone().ranges[0].clone().0.idx, Uint128::new(2));
    assert_eq!(asks[6].clone().end, range_2.high);
    assert_eq!(asks[6].clone().ranges[0].clone().0.idx, Uint128::new(2));
}

#[test]
fn range_offer_iter_terminates() {
    let mut deps = mock_dependencies();
    let tick = Tick::new(8);
    let owner = Addr::unchecked("owner");

    let range = Range::new(
        deps.as_mut().storage,
        &tick,
        &Decimal::zero(),
        owner.clone(),
        RangeConfig {
            high: Decimal::from_str("4").unwrap(),
            low: Decimal::from_str("3").unwrap(),
            skew: SignedDecimal::zero(),
            spread: Decimal::permille(10),
            fee: Decimal::zero(),
        },
        (
            Decimal::from_str("100000000").unwrap(),
            Decimal::from_str("401992761").unwrap(),
        ),
    )
    .unwrap();
    range.save(deps.as_mut().storage).unwrap();

    let last = RangeOfferIter::new(deps.as_ref().storage, &Side::Quote, &Decimal::permille(50))
        .last()
        .unwrap();
    assert_eq!(last.clone().start, Decimal::from_str("3.1542375").unwrap());
    assert_eq!(last.clone().end, range.low);
    assert_eq!(
        last.clone().total,
        Decimal::from_str("127356150.767469564432284114").unwrap()
    );

    let last = RangeOfferIter::new(deps.as_ref().storage, &Side::Base, &Decimal::permille(50))
        .last()
        .unwrap();

    assert_eq!(
        last.clone().start,
        Decimal::from_str("3.86426261025").unwrap()
    );
    assert_eq!(last.clone().end, range.high);
}

#[test]
fn range_offer_swap_partial_fill() {
    let mut offer = RangeOffer {
        side: Side::Quote,
        start: Decimal::from_str("2").unwrap(),
        end: Decimal::from_str("3").unwrap(),
        total: Decimal::from_str("600").unwrap(),
        ranges: vec![
            (
                Range {
                    idx: Uint128::one(),
                    fees: [Decimal::zero(), Decimal::zero()],
                    owner: Addr::unchecked(""),
                    high: Decimal::from_str("4").unwrap(),
                    low: Decimal::from_str("4").unwrap(),
                    skew: SignedDecimal::zero(),
                    spread: Decimal::zero(),
                    fee: Decimal::zero(),
                    base: Decimal::from_str("200").unwrap(),
                    quote: Decimal::from_str("400").unwrap(),
                    price: Decimal::from_str("2").unwrap(),
                    ask: Decimal::from_str("2").unwrap(),
                    bid: Decimal::from_str("2").unwrap(),
                },
                Decimal::from_str("400").unwrap(),
            ),
            (
                Range {
                    idx: Uint128::new(2),
                    fees: [Decimal::zero(), Decimal::zero()],
                    owner: Addr::unchecked(""),
                    high: Decimal::from_str("4").unwrap(),
                    low: Decimal::from_str("4").unwrap(),
                    skew: SignedDecimal::zero(),
                    spread: Decimal::zero(),
                    fee: Decimal::zero(),
                    base: Decimal::from_str("100").unwrap(),
                    quote: Decimal::from_str("200").unwrap(),
                    price: Decimal::from_str("2").unwrap(),
                    ask: Decimal::from_str("2").unwrap(),
                    bid: Decimal::from_str("2").unwrap(),
                },
                Decimal::from_str("200").unwrap(),
            ),
        ],
    };
    let mut ctx = RangeContext::new(
        Addr::unchecked(""),
        Decimal::zero(),
        Denoms::new("base", "quote"),
        Tick::new(8),
    );
    let (offer_consumed, bids_returned, _) = offer.swap(&mut ctx, Uint128::from(50u128)).unwrap();

    assert_eq!(offer_consumed, Uint128::from(50u128));
    assert_eq!(bids_returned, Uint128::from(100u128));
    let r = ctx.ranges.get(&Uint128::one()).unwrap();
    assert_eq!(r.base, Decimal::from_str("233.333333333333333333").unwrap());
    assert_eq!(
        r.quote,
        Decimal::from_str("333.333333333333333334").unwrap()
    );

    let r = ctx.ranges.get(&Uint128::new(2)).unwrap();
    assert_eq!(r.base, Decimal::from_str("116.666666666666666667").unwrap());
    assert_eq!(
        r.quote,
        Decimal::from_str("166.666666666666666666").unwrap()
    );
}

#[test]
fn range_offer_swap_overfill() {
    let mut offer = RangeOffer {
        side: Side::Quote,
        start: Decimal::from_str("2").unwrap(),
        end: Decimal::from_str("3").unwrap(),
        total: Decimal::from_str("600").unwrap(),
        ranges: vec![
            (
                Range {
                    idx: Uint128::one(),
                    fees: [Decimal::zero(), Decimal::zero()],
                    owner: Addr::unchecked(""),
                    high: Decimal::from_str("4").unwrap(),
                    low: Decimal::from_str("4").unwrap(),
                    skew: SignedDecimal::zero(),
                    spread: Decimal::zero(),
                    fee: Decimal::zero(),
                    base: Decimal::from_str("200").unwrap(),
                    quote: Decimal::from_str("400").unwrap(),
                    price: Decimal::from_str("2").unwrap(),
                    ask: Decimal::from_str("2").unwrap(),
                    bid: Decimal::from_str("2").unwrap(),
                },
                Decimal::from_str("400").unwrap(),
            ),
            (
                Range {
                    idx: Uint128::new(2),
                    fees: [Decimal::zero(), Decimal::zero()],
                    owner: Addr::unchecked(""),
                    high: Decimal::from_str("4").unwrap(),
                    low: Decimal::from_str("4").unwrap(),
                    skew: SignedDecimal::zero(),
                    spread: Decimal::zero(),
                    fee: Decimal::zero(),
                    base: Decimal::from_str("100").unwrap(),
                    quote: Decimal::from_str("200").unwrap(),
                    price: Decimal::from_str("2").unwrap(),
                    ask: Decimal::from_str("2").unwrap(),
                    bid: Decimal::from_str("2").unwrap(),
                },
                Decimal::from_str("200").unwrap(),
            ),
        ],
    };
    let mut ctx = RangeContext::new(
        Addr::unchecked(""),
        Decimal::zero(),
        Denoms::new("base", "quote"),
        Tick::new(8),
    );
    let (offer_consumed, bids_returned, _) = offer.swap(&mut ctx, Uint128::from(500u128)).unwrap();

    assert_eq!(offer_consumed, Uint128::from(300u128));
    assert_eq!(bids_returned, Uint128::from(600u128));

    let r = ctx.ranges.get(&Uint128::one()).unwrap();
    assert_eq!(r.base, Decimal::from_str("400").unwrap());
    assert_eq!(r.quote, Decimal::zero());

    let r = ctx.ranges.get(&Uint128::new(2)).unwrap();
    assert_eq!(r.base, Decimal::from_str("200").unwrap());
    assert_eq!(r.quote, Decimal::zero());
}

#[test]
fn range_offer_swap_partial_fill_base() {
    let mut offer = RangeOffer {
        side: Side::Base,
        start: Decimal::from_str("2").unwrap(),
        end: Decimal::from_str("3").unwrap(),
        total: Decimal::from_str("300").unwrap(),
        ranges: vec![
            (
                Range {
                    idx: Uint128::one(),
                    fees: [Decimal::zero(), Decimal::zero()],
                    owner: Addr::unchecked(""),
                    high: Decimal::from_str("4").unwrap(),
                    low: Decimal::from_str("4").unwrap(),
                    skew: SignedDecimal::zero(),
                    spread: Decimal::zero(),
                    fee: Decimal::zero(),
                    base: Decimal::from_str("200").unwrap(),
                    quote: Decimal::from_str("400").unwrap(),
                    price: Decimal::from_str("2").unwrap(),
                    ask: Decimal::from_str("2").unwrap(),
                    bid: Decimal::from_str("2").unwrap(),
                },
                Decimal::from_str("200").unwrap(),
            ),
            (
                Range {
                    idx: Uint128::new(2),
                    fees: [Decimal::zero(), Decimal::zero()],
                    owner: Addr::unchecked(""),
                    high: Decimal::from_str("4").unwrap(),
                    low: Decimal::from_str("4").unwrap(),
                    skew: SignedDecimal::zero(),
                    spread: Decimal::zero(),
                    fee: Decimal::zero(),
                    base: Decimal::from_str("100").unwrap(),
                    quote: Decimal::from_str("200").unwrap(),
                    price: Decimal::from_str("2").unwrap(),
                    ask: Decimal::from_str("2").unwrap(),
                    bid: Decimal::from_str("2").unwrap(),
                },
                Decimal::from_str("100").unwrap(),
            ),
        ],
    };

    let mut ctx = RangeContext::new(
        Addr::unchecked(""),
        Decimal::zero(),
        Denoms::new("base", "quote"),
        Tick::new(8),
    );
    let (offer_consumed, bids_returned, _) = offer.swap(&mut ctx, Uint128::from(100u128)).unwrap();

    assert_eq!(offer_consumed, Uint128::from(100u128));
    assert_eq!(bids_returned, Uint128::from(50u128));

    let r = ctx.ranges.get(&Uint128::one()).unwrap();
    assert_eq!(r.base, Decimal::from_str("166.666666666666666667").unwrap());
    assert_eq!(
        r.quote,
        Decimal::from_str("466.666666666666666666").unwrap()
    );

    let r = ctx.ranges.get(&Uint128::new(2)).unwrap();
    assert_eq!(r.base, Decimal::from_str("83.333333333333333333").unwrap());
    assert_eq!(
        r.quote,
        Decimal::from_str("233.333333333333333334").unwrap()
    );
}

#[test]
fn range_offer_swap_overfill_base() {
    let mut offer = RangeOffer {
        side: Side::Base,
        start: Decimal::from_str("2").unwrap(),
        end: Decimal::from_str("3").unwrap(),
        total: Decimal::from_str("300").unwrap(),
        ranges: vec![
            (
                Range {
                    idx: Uint128::one(),
                    fees: [Decimal::zero(), Decimal::zero()],
                    owner: Addr::unchecked(""),
                    high: Decimal::from_str("4").unwrap(),
                    low: Decimal::from_str("4").unwrap(),
                    skew: SignedDecimal::zero(),
                    spread: Decimal::zero(),
                    fee: Decimal::zero(),
                    base: Decimal::from_str("200").unwrap(),
                    quote: Decimal::from_str("400").unwrap(),
                    price: Decimal::from_str("2").unwrap(),
                    ask: Decimal::from_str("2").unwrap(),
                    bid: Decimal::from_str("2").unwrap(),
                },
                Decimal::from_str("200").unwrap(),
            ),
            (
                Range {
                    idx: Uint128::new(2),
                    fees: [Decimal::zero(), Decimal::zero()],
                    owner: Addr::unchecked(""),
                    high: Decimal::from_str("4").unwrap(),
                    low: Decimal::from_str("4").unwrap(),
                    skew: SignedDecimal::zero(),
                    spread: Decimal::zero(),
                    fee: Decimal::zero(),
                    base: Decimal::from_str("100").unwrap(),
                    quote: Decimal::from_str("200").unwrap(),
                    price: Decimal::from_str("2").unwrap(),
                    ask: Decimal::from_str("2").unwrap(),
                    bid: Decimal::from_str("2").unwrap(),
                },
                Decimal::from_str("100").unwrap(),
            ),
        ],
    };

    let mut ctx = RangeContext::new(
        Addr::unchecked(""),
        Decimal::zero(),
        Denoms::new("base", "quote"),
        Tick::new(8),
    );
    let (offer_consumed, bids_returned, _) = offer.swap(&mut ctx, Uint128::from(5000u128)).unwrap();

    assert_eq!(offer_consumed, Uint128::from(600u128));
    assert_eq!(bids_returned, Uint128::from(300u128));

    let r = ctx.ranges.get(&Uint128::one()).unwrap();
    assert_eq!(r.base, Decimal::zero());
    assert_eq!(r.quote, Decimal::from_str("800").unwrap());

    let r = ctx.ranges.get(&Uint128::new(2)).unwrap();
    assert_eq!(r.base, Decimal::zero());
    assert_eq!(r.quote, Decimal::from_str("400").unwrap());
}

#[test]
fn range_offer_commit_changes() {
    let mut deps = mock_dependencies();
    let tick = Tick::new(8);
    let owner = Addr::unchecked("owner");

    let mut range = Range::new(
        deps.as_mut().storage,
        &tick,
        &Decimal::zero(),
        owner.clone(),
        RangeConfig {
            high: Decimal::from_str("4").unwrap(),
            low: Decimal::from_str("3").unwrap(),
            skew: SignedDecimal::zero(),
            spread: Decimal::permille(10),
            fee: Decimal::zero(),
        },
        (
            Decimal::from_str("100000000").unwrap(),
            Decimal::from_str("401992761").unwrap(),
        ),
    )
    .unwrap();
    range.save(deps.as_mut().storage).unwrap();

    let bids: Vec<RangeOffer> =
        RangeOfferIter::new(deps.as_ref().storage, &Side::Quote, &Decimal::permille(50))
            .take(2)
            .collect::<Vec<RangeOffer>>();
    assert_eq!(bids[0].clone().start, Decimal::from_str("3.495").unwrap());
    assert_eq!(
        bids[0].clone().total,
        Decimal::from_str("137118153.600797312162479599").unwrap()
    );

    // Swap less then the first bid
    assert_eq!(
        range.price,
        Decimal::from_str("3.500000000130949811").unwrap()
    );
    assert_eq!(range.ask, Decimal::from_str("3.5050001").unwrap());
    assert_eq!(range.bid, Decimal::from_str("3.495").unwrap());

    let mut ctx = RangeContext::new(
        Addr::unchecked(""),
        Decimal::zero(),
        Denoms::new("base", "quote"),
        tick.clone(),
    );
    range.swap(
        &mut ctx,
        &Side::Quote,
        Decimal::from_str("1000000").unwrap(),
        Decimal::from_str("3495000").unwrap(),
    );
    assert_eq!(range.quote, Decimal::from_str("398497761").unwrap());
    assert_eq!(range.base, Decimal::from_str("101000000").unwrap());
    assert_eq!(
        range.price,
        Decimal::from_str("3.495485042631049667").unwrap()
    );
    assert_eq!(range.ask, Decimal::from_str("3.5005302").unwrap());
    assert_eq!(range.bid, Decimal::from_str("3.4905301").unwrap());
}

#[test]
fn range_offer_iter_commit_changes() {
    let mut deps = mock_dependencies();
    let tick = Tick::new(8);
    let owner = Addr::unchecked("owner");

    let range = Range::new(
        deps.as_mut().storage,
        &tick,
        &Decimal::zero(),
        owner.clone(),
        RangeConfig {
            high: Decimal::from_str("4").unwrap(),
            low: Decimal::from_str("3").unwrap(),
            skew: SignedDecimal::zero(),
            spread: Decimal::permille(10),
            fee: Decimal::zero(),
        },
        (
            Decimal::from_str("100000000").unwrap(),
            Decimal::from_str("401992761").unwrap(),
        ),
    )
    .unwrap();
    let deps_mut = deps.as_mut();
    range.save(deps_mut.storage).unwrap();
    let mut swapper = Swapper::new(
        CONTRACT_NAME,
        RangeContext::new(
            Addr::unchecked(""),
            Decimal::zero(),
            Denoms::new("base", "quote"),
            Tick::new(8),
        ),
        Uint128::new(1000000),
        rujira_rs::fin::SwapRequest::Yolo {
            to: None,
            callback: None,
        },
        Decimal::zero(),
    );

    let res = {
        let mut iter: RangeOfferIter<'_> =
            RangeOfferIter::new(deps_mut.storage, &Side::Quote, &Decimal::permille(50));
        swapper.swap(&mut iter).unwrap()
    };
    assert_eq!(res.consumed_offer, Uint128::new(1_000_000));
    assert_eq!(res.return_amount, Uint128::new(3_495_000));
    assert_eq!(
        res.events[0],
        Event::new("rujira-fin/trade").add_attributes(vec![
            ("rate", "3.495"),
            ("offer", "1000000"),
            ("bid", "3495000"),
            ("price", "ccl:3.495"),
            ("side", "quote"),
            (
                "ranges",
                "1:3-4:100999999.999999999996585:398497761:3495000:999999.999999999996585:0:"
            )
        ])
    );

    swapper
        .context
        .commit(deps_mut.storage, Response::default())
        .unwrap();

    let range = Range::load(deps.as_ref().storage, Uint128::one()).unwrap();
    // Check range has been saved
    assert_eq!(
        range.price,
        Decimal::from_str("3.495485042631049667").unwrap()
    );
}

#[test]
fn range_swap_allocates_fees() {
    let mut range = Range {
        idx: Uint128::one(),
        fees: [Decimal::zero(), Decimal::zero()],
        owner: Addr::unchecked(""),
        high: Decimal::from_str("4").unwrap(),
        low: Decimal::from_str("4").unwrap(),
        skew: SignedDecimal::zero(),
        spread: Decimal::zero(),
        fee: Decimal::percent(5),
        base: Decimal::from_str("200").unwrap(),
        quote: Decimal::from_str("400").unwrap(),
        price: Decimal::from_str("2").unwrap(),
        ask: Decimal::from_str("2").unwrap(),
        bid: Decimal::from_str("2").unwrap(),
    };

    let mut ctx = RangeContext::new(
        Addr::unchecked(""),
        Decimal::zero(),
        Denoms::new("base", "quote"),
        Tick::new(8),
    );
    range.swap(
        &mut ctx,
        &Side::Base,
        Decimal::from_str("200").unwrap(),
        Decimal::from_str("100").unwrap(),
    );

    assert_eq!(range.quote, Decimal::from_str("590").unwrap());
    assert_eq!(range.base, Decimal::from_str("100").unwrap());
    assert_eq!(
        range.fees,
        [Decimal::zero(), Decimal::from_str("10").unwrap()]
    );

    range.swap(
        &mut ctx,
        &Side::Quote,
        Decimal::from_str("50").unwrap(),
        Decimal::from_str("100").unwrap(),
    );

    assert_eq!(range.quote, Decimal::from_str("490").unwrap());
    assert_eq!(range.base, Decimal::from_str("147.5").unwrap());
    assert_eq!(
        range.fees,
        [
            Decimal::from_str("2.5").unwrap(),
            Decimal::from_str("10").unwrap()
        ]
    );
}

#[test]
fn test_range_commit_fee() {
    let mut deps = mock_dependencies();
    let denoms = Denoms::new("btc", "usdc");
    let mut ctx = RangeContext::new(
        Addr::unchecked("fee-address"),
        Decimal::percent(5),
        denoms.clone(),
        Tick::new(6),
    );

    RangeOffer {
        side: Side::Quote,
        start: Decimal::from_str("2").unwrap(),
        total: Decimal::from_str("600").unwrap(),
        ranges: vec![
            (
                Range {
                    idx: Uint128::one(),
                    fees: [Decimal::zero(), Decimal::zero()],
                    owner: Addr::unchecked(""),
                    high: Decimal::from_str("4").unwrap(),
                    low: Decimal::from_str("4").unwrap(),
                    skew: SignedDecimal::zero(),
                    spread: Decimal::zero(),
                    fee: Decimal::zero(),
                    base: Decimal::from_str("200").unwrap(),
                    quote: Decimal::from_str("400").unwrap(),
                    price: Decimal::from_str("2").unwrap(),
                    ask: Decimal::from_str("2").unwrap(),
                    bid: Decimal::from_str("2").unwrap(),
                },
                Decimal::from_str("400").unwrap(),
            ),
            (
                Range {
                    idx: Uint128::new(2),
                    fees: [Decimal::zero(), Decimal::zero()],
                    owner: Addr::unchecked(""),
                    high: Decimal::from_str("4").unwrap(),
                    low: Decimal::from_str("4").unwrap(),
                    skew: SignedDecimal::zero(),
                    spread: Decimal::zero(),
                    fee: Decimal::zero(),
                    base: Decimal::from_str("100").unwrap(),
                    quote: Decimal::from_str("200").unwrap(),
                    price: Decimal::from_str("2").unwrap(),
                    ask: Decimal::from_str("2").unwrap(),
                    bid: Decimal::from_str("2").unwrap(),
                },
                Decimal::from_str("200").unwrap(),
            ),
        ],
        end: Decimal::from_str("3").unwrap(),
    }
    .swap(&mut ctx, Uint128::new(1000))
    .unwrap();

    let res = ctx
        .commit(deps.as_mut().storage, Response::default())
        .unwrap();

    // We're selling BTC into the quote side
    // 1000 BTC = 2000 USDC
    // Fee should be 5% of the 300 BTC bought by the 600 USDC available

    assert_eq!(
        res.messages,
        vec![SubMsg::reply_never(CosmosMsg::Bank(BankMsg::Send {
            to_address: "fee-address".to_string(),
            amount: coins(15, "btc"),
        }))]
    );

    assert_eq!(
        res.events,
        vec![
            Event::new("rujira-fin/range.fee").add_attributes(vec![("base", "15"), ("quote", "0")])
        ]
    );
}

#[test]
fn range_withdraw() {
    let mut deps = mock_dependencies();
    let tick = Tick::new(8);
    let owner = Addr::unchecked("owner");

    let mut range = Range::new(
        deps.as_mut().storage,
        &tick,
        &Decimal::zero(),
        owner.clone(),
        RangeConfig {
            high: Decimal::from_str("4").unwrap(),
            low: Decimal::from_str("3").unwrap(),
            skew: SignedDecimal::zero(),
            spread: Decimal::permille(10),
            fee: Decimal::zero(),
        },
        (
            Decimal::from_str("100000000").unwrap(),
            Decimal::from_str("200000000").unwrap(),
        ),
    )
    .unwrap();
    range.save(deps.as_mut().storage).unwrap();

    let res = range.withdraw(
        &Denoms::new("base", "quote"),
        Decimal::from_ratio(1u128, 3u128),
    );

    assert_eq!(
        range.base,
        Decimal::from_str("66666667.3333333333").unwrap()
    );
    assert_eq!(
        range.quote,
        Decimal::from_str("133333334.6666666666").unwrap()
    );
    assert_eq!(
        res,
        Some(NativeBalance(vec![
            coin(33333333, "base"),
            coin(66666666, "quote"),
        ]))
    )
}

fn range_inputs() -> impl Strategy<Value = (u64, u64, u128, u128, u64)> {
    (100000u64..900000u64).prop_flat_map(|low| {
        (
            Just(low),
            (low + 1)..1_000_000u64,
            1u128..1_000_000u128,
            1u128..1_000_000u128,
            1u64..900u64,
        )
    })
}

fn assert_range_offer_invariants(
    storage: &dyn cosmwasm_std::Storage,
    side: Side,
    delta: Decimal,
    ranges: &BTreeMap<Uint128, Range>,
) -> Result<(), TestCaseError> {
    let mut totals: BTreeMap<Uint128, Decimal> = BTreeMap::new();

    for offer in RangeOfferIter::new(storage, &side, &delta) {
        for (range, amount) in &offer.ranges {
            let entry = totals.entry(range.idx).or_insert(Decimal::zero());
            *entry += *amount;

            match side {
                Side::Base => {
                    prop_assert!(offer.start >= range.ask);
                    prop_assert!(offer.start <= range.high);
                }
                Side::Quote => {
                    prop_assert!(offer.start <= range.bid);
                    prop_assert!(offer.start >= range.low);
                }
            }
        }
    }

    for (idx, total) in totals {
        let range = ranges.get(&idx).unwrap();
        let cap = match side {
            Side::Base => range.base,
            Side::Quote => range.quote,
        };
        prop_assert!(total <= cap);
    }

    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 64, ..ProptestConfig::default() })]
    #[test]
    fn range_offer_iter_invariants(range_inputs in prop::collection::vec(range_inputs(), 1..20)) {
        let mut deps = mock_dependencies();
        let tick = Tick::new(6);
        let owner = Addr::unchecked("owner");
        let mut ranges: BTreeMap<Uint128, Range> = BTreeMap::new();

        for (low, high, base, quote, spread_permille) in range_inputs {
            let range = Range::new(
                deps.as_mut().storage,
                &tick,
                &Decimal::zero(),
                owner.clone(),
                RangeConfig {
                    high: Decimal::from_atomics(high as u128, 0).unwrap(),
                    low: Decimal::from_atomics(low as u128, 0).unwrap(),
                    skew: SignedDecimal::zero(),
                    spread: Decimal::permille(spread_permille),
                    fee: Decimal::zero(),
                },
                (Decimal::from_atomics(base, 0).unwrap(), Decimal::from_atomics(quote, 0).unwrap()),
            )
            .unwrap();
            range.save(deps.as_mut().storage).unwrap();
            ranges.insert(range.idx, range);
        }

        let delta = Decimal::permille(50);
        assert_range_offer_invariants(deps.as_ref().storage, Side::Base, delta, &ranges)?;
        assert_range_offer_invariants(deps.as_ref().storage, Side::Quote, delta, &ranges)?;
    }
}

#[test]
fn scl_zero_skew_matches_cl() {
    let mut deps = mock_dependencies();
    let tick = Tick::new(8);
    let owner = Addr::unchecked("owner");

    let range = Range::new(
        deps.as_mut().storage,
        &tick,
        &Decimal::zero(),
        owner.clone(),
        RangeConfig {
            high: Decimal::from_str("4").unwrap(),
            low: Decimal::from_str("3").unwrap(),
            skew: SignedDecimal::zero(),
            spread: Decimal::permille(10),
            fee: Decimal::zero(),
        },
        (
            Decimal::from_str("100000000").unwrap(),
            Decimal::from_str("401992761").unwrap(),
        ),
    )
    .unwrap();
    range.save(deps.as_mut().storage).unwrap();

    let asks: Vec<RangeOffer> =
        RangeOfferIter::new(deps.as_ref().storage, &Side::Base, &Decimal::permille(50)).collect();
    let bids: Vec<RangeOffer> =
        RangeOfferIter::new(deps.as_ref().storage, &Side::Quote, &Decimal::permille(50)).collect();

    assert_eq!(
        range.price,
        Decimal::from_str("3.500000000130949811").unwrap()
    );
    assert_eq!(range.ask, Decimal::from_str("3.5050001").unwrap());
    assert_eq!(range.bid, Decimal::from_str("3.495").unwrap());

    assert_eq!(asks.len(), 3);
    assert_eq!(bids.len(), 3);

    assert_eq!(asks[0].start, Decimal::from_str("3.5050001").unwrap());
    assert_eq!(
        asks[0].total,
        Decimal::from_str("37288050.318055601185689535").unwrap()
    );

    assert_eq!(bids[0].start, Decimal::from_str("3.495").unwrap());
    assert_eq!(
        bids[0].total,
        Decimal::from_str("137118153.600797312162479599").unwrap()
    );

    let total_asks = asks.iter().fold(Decimal::zero(), |a, v| a + v.total);
    let total_bids = bids.iter().fold(Decimal::zero(), |a, v| a + v.total);
    assert!(total_asks <= range.base);
    assert!(total_bids <= range.quote);
}

#[test]
fn scl_skew_changes_distribution() {
    let mut deps = mock_dependencies();
    let tick = Tick::new(8);
    let owner = Addr::unchecked("owner");
    let delta = Decimal::permille(100);

    let high = Decimal::from_str("4").unwrap();
    let low = Decimal::from_str("3").unwrap();
    let spread = Decimal::permille(10);
    let base = Decimal::from_str("100000000").unwrap();
    let quote = Decimal::from_str("401992761").unwrap();

    // Uniform (skew=0)
    let range_uniform = Range::new(
        deps.as_mut().storage,
        &tick,
        &Decimal::zero(),
        owner.clone(),
        RangeConfig {
            high,
            low,
            skew: SignedDecimal::zero(),
            spread,
            fee: Decimal::zero(),
        },
        (base, quote),
    )
    .unwrap();
    range_uniform.save(deps.as_mut().storage).unwrap();

    let uniform_asks: Vec<RangeOffer> =
        RangeOfferIter::new(deps.as_ref().storage, &Side::Base, &delta).collect();

    Range::map()
        .remove(deps.as_mut().storage, range_uniform.idx.u128())
        .unwrap();

    // Positive skew
    let range_positive = Range::new(
        deps.as_mut().storage,
        &tick,
        &Decimal::zero(),
        owner.clone(),
        RangeConfig {
            high,
            low,
            skew: SignedDecimal::from_str("0.8").unwrap(),
            spread,
            fee: Decimal::zero(),
        },
        (base, quote),
    )
    .unwrap();
    range_positive.save(deps.as_mut().storage).unwrap();

    let positive_asks: Vec<RangeOffer> =
        RangeOfferIter::new(deps.as_ref().storage, &Side::Base, &delta).collect();

    Range::map()
        .remove(deps.as_mut().storage, range_positive.idx.u128())
        .unwrap();

    // Negative skew
    let range_negative = Range::new(
        deps.as_mut().storage,
        &tick,
        &Decimal::zero(),
        owner.clone(),
        RangeConfig {
            high,
            low,
            skew: SignedDecimal::from_str("-0.8").unwrap(),
            spread,
            fee: Decimal::zero(),
        },
        (base, quote),
    )
    .unwrap();
    range_negative.save(deps.as_mut().storage).unwrap();

    let negative_asks: Vec<RangeOffer> =
        RangeOfferIter::new(deps.as_ref().storage, &Side::Base, &delta).collect();

    assert!(!uniform_asks.is_empty());
    assert!(!positive_asks.is_empty());
    assert!(!negative_asks.is_empty());

    let uniform_first = uniform_asks.first().unwrap().total;
    let uniform_last = uniform_asks.last().unwrap().total;
    let positive_first = positive_asks.first().unwrap().total;
    let positive_last = positive_asks.last().unwrap().total;
    let negative_first = negative_asks.first().unwrap().total;
    let negative_last = negative_asks.last().unwrap().total;

    let uniform_ratio = uniform_first / uniform_last;
    let positive_ratio = positive_first / positive_last;
    let negative_ratio = negative_first / negative_last;

    // Ratios should differ between skew configurations
    assert!(positive_ratio != uniform_ratio);
    assert!(negative_ratio != uniform_ratio);
    assert!(positive_ratio != negative_ratio);

    // Totals must not exceed base
    let uniform_total: Decimal = uniform_asks.iter().map(|o| o.total).sum();
    let positive_total: Decimal = positive_asks.iter().map(|o| o.total).sum();
    let negative_total: Decimal = negative_asks.iter().map(|o| o.total).sum();

    assert!(uniform_total <= base);
    assert!(positive_total <= base);
    assert!(negative_total <= base);
}

#[test]
fn scl_deposit_tracks_base_quote() {
    let mut deps = mock_dependencies();
    let tick = Tick::new(8);
    let owner = Addr::unchecked("owner");

    let initial_base = Decimal::from_str("100000000").unwrap();
    let initial_quote = Decimal::from_str("350000000").unwrap();

    let mut range = Range::new(
        deps.as_mut().storage,
        &tick,
        &Decimal::zero(),
        owner.clone(),
        RangeConfig {
            high: Decimal::from_str("4").unwrap(),
            low: Decimal::from_str("3").unwrap(),
            skew: SignedDecimal::from_str("0.5").unwrap(),
            spread: Decimal::permille(10),
            fee: Decimal::zero(),
        },
        (initial_base, initial_quote),
    )
    .unwrap();

    assert_eq!(range.base, Decimal::from_str("100000000").unwrap());
    assert_eq!(range.quote, Decimal::from_str("350000000").unwrap());

    let initial_price = range.price;

    range.deposit(
        Decimal::from_str("50000000").unwrap(),
        Decimal::from_str("175000000").unwrap(),
    );

    assert_eq!(range.base, Decimal::from_str("150000000").unwrap());
    assert_eq!(range.quote, Decimal::from_str("525000000").unwrap());

    range.reprice(&tick, &Decimal::zero());

    // Proportional deposit should not significantly change price
    let price_change = if range.price > initial_price {
        range.price - initial_price
    } else {
        initial_price - range.price
    };
    assert!(price_change < Decimal::from_str("0.05").unwrap());

    // Adding only quote increases price
    let price_before = range.price;
    range.deposit(Decimal::zero(), Decimal::from_str("200000000").unwrap());
    range.reprice(&tick, &Decimal::zero());
    assert!(range.price > price_before);

    // Adding only base decreases price
    let price_before = range.price;
    range.deposit(Decimal::from_str("200000000").unwrap(), Decimal::zero());
    range.reprice(&tick, &Decimal::zero());
    assert!(range.price < price_before);
}

#[test]
fn scl_withdraw_returns_correct_amounts() {
    let mut deps = mock_dependencies();
    let tick = Tick::new(8);
    let owner = Addr::unchecked("owner");
    let denoms = Denoms::new("base", "quote");

    let mut range = Range::new(
        deps.as_mut().storage,
        &tick,
        &Decimal::zero(),
        owner.clone(),
        RangeConfig {
            high: Decimal::from_str("4").unwrap(),
            low: Decimal::from_str("3").unwrap(),
            skew: SignedDecimal::from_str("0.5").unwrap(),
            spread: Decimal::permille(10),
            fee: Decimal::zero(),
        },
        (
            Decimal::from_str("100000000").unwrap(),
            Decimal::from_str("200000000").unwrap(),
        ),
    )
    .unwrap();

    let initial_price = range.price;

    // Withdraw 25%
    let payout = range.withdraw(&denoms, Decimal::from_str("0.25").unwrap());
    assert!(payout.is_some());

    let payout = payout.unwrap();
    assert_eq!(
        payout,
        NativeBalance(vec![coin(25000000, "base"), coin(50000000, "quote"),])
    );

    assert_eq!(range.base, Decimal::from_str("75000000").unwrap());
    assert_eq!(range.quote, Decimal::from_str("150000000").unwrap());

    range.reprice(&tick, &Decimal::zero());

    // Proportional withdrawal should not change price
    let price_change = if range.price > initial_price {
        range.price - initial_price
    } else {
        initial_price - range.price
    };
    assert!(price_change < Decimal::from_str("0.001").unwrap());

    // Full withdrawal
    let withdraw_all = range.withdraw(&denoms, Decimal::one());
    assert!(withdraw_all.is_some());

    let withdraw_all = withdraw_all.unwrap();
    assert_eq!(
        withdraw_all,
        NativeBalance(vec![coin(75000000, "base"), coin(150000000, "quote"),])
    );
}

#[test]
fn scl_swap_updates_reserves_and_price() {
    let mut deps = mock_dependencies();
    let tick = Tick::new(8);
    let owner = Addr::unchecked("owner");

    let mut range = Range::new(
        deps.as_mut().storage,
        &tick,
        &Decimal::zero(),
        owner.clone(),
        RangeConfig {
            high: Decimal::from_str("4").unwrap(),
            low: Decimal::from_str("3").unwrap(),
            skew: SignedDecimal::from_str("0.5").unwrap(),
            spread: Decimal::permille(10),
            fee: Decimal::zero(),
        },
        (
            Decimal::from_str("100000000").unwrap(),
            Decimal::from_str("350000000").unwrap(),
        ),
    )
    .unwrap();

    let initial_price = range.price;

    let mut ctx = RangeContext::new(
        Addr::unchecked("fee"),
        Decimal::zero(),
        Denoms::new("base", "quote"),
        tick.clone(),
    );

    // Quote swap: add base, deduct quote
    range.swap(
        &mut ctx,
        &Side::Quote,
        Decimal::from_str("10000000").unwrap(),
        Decimal::from_str("2857143").unwrap(),
    );

    assert_eq!(range.base, Decimal::from_str("110000000").unwrap());
    assert_eq!(range.quote, Decimal::from_str("347142857").unwrap());
    assert!(range.price < initial_price);

    let price_after_first = range.price;

    // Base swap: add quote, deduct base
    range.swap(
        &mut ctx,
        &Side::Base,
        Decimal::from_str("17500000").unwrap(),
        Decimal::from_str("5000000").unwrap(),
    );

    assert_eq!(range.base, Decimal::from_str("105000000").unwrap());
    assert_eq!(range.quote, Decimal::from_str("364642857").unwrap());
    assert!(range.price > price_after_first);
}

#[test]
fn scl_multiple_ranges_aggregate() {
    let mut deps = mock_dependencies();
    let tick = Tick::new(8);
    let owner = Addr::unchecked("owner");
    let delta = Decimal::permille(50);

    let range1 = Range::new(
        deps.as_mut().storage,
        &tick,
        &Decimal::zero(),
        owner.clone(),
        RangeConfig {
            high: Decimal::from_str("4").unwrap(),
            low: Decimal::from_str("3").unwrap(),
            skew: SignedDecimal::from_str("0.5").unwrap(),
            spread: Decimal::permille(10),
            fee: Decimal::zero(),
        },
        (
            Decimal::from_str("50000000").unwrap(),
            Decimal::from_str("175000000").unwrap(),
        ),
    )
    .unwrap();
    range1.save(deps.as_mut().storage).unwrap();

    let range2 = Range::new(
        deps.as_mut().storage,
        &tick,
        &Decimal::zero(),
        owner.clone(),
        RangeConfig {
            high: Decimal::from_str("4").unwrap(),
            low: Decimal::from_str("3").unwrap(),
            skew: SignedDecimal::from_str("-0.5").unwrap(),
            spread: Decimal::permille(10),
            fee: Decimal::zero(),
        },
        (
            Decimal::from_str("50000000").unwrap(),
            Decimal::from_str("175000000").unwrap(),
        ),
    )
    .unwrap();
    range2.save(deps.as_mut().storage).unwrap();

    let range3 = Range::new(
        deps.as_mut().storage,
        &tick,
        &Decimal::zero(),
        owner.clone(),
        RangeConfig {
            high: Decimal::from_str("4").unwrap(),
            low: Decimal::from_str("3").unwrap(),
            skew: SignedDecimal::zero(),
            spread: Decimal::permille(10),
            fee: Decimal::zero(),
        },
        (
            Decimal::from_str("50000000").unwrap(),
            Decimal::from_str("175000000").unwrap(),
        ),
    )
    .unwrap();
    range3.save(deps.as_mut().storage).unwrap();

    let asks: Vec<RangeOffer> =
        RangeOfferIter::new(deps.as_ref().storage, &Side::Base, &delta).collect();

    // Overlapping ranges produce slices with multiple ranges
    let multi_range_slices: Vec<_> = asks.iter().filter(|o| o.ranges.len() > 1).collect();
    assert!(!multi_range_slices.is_empty());

    for slice in &multi_range_slices {
        assert!(slice.ranges.len() <= 3);
    }

    // Total offered must not exceed combined base
    let total_offered: Decimal = asks.iter().map(|o| o.total).sum();
    let combined_base = range1.base + range2.base + range3.base;
    assert!(total_offered <= combined_base);

    // Per-range totals must not exceed individual base
    let mut per_range_totals: BTreeMap<Uint128, Decimal> = BTreeMap::new();
    for offer in &asks {
        for (r, amt) in &offer.ranges {
            *per_range_totals.entry(r.idx).or_default() += *amt;
        }
    }

    assert!(
        per_range_totals
            .get(&range1.idx)
            .unwrap_or(&Decimal::zero())
            <= &range1.base
    );
    assert!(
        per_range_totals
            .get(&range2.idx)
            .unwrap_or(&Decimal::zero())
            <= &range2.base
    );
    assert!(
        per_range_totals
            .get(&range3.idx)
            .unwrap_or(&Decimal::zero())
            <= &range3.base
    );
}

#[test]
fn scl_swap_via_iterator() {
    let mut deps = mock_dependencies();
    let tick = Tick::new(8);
    let owner = Addr::unchecked("owner");

    let range = Range::new(
        deps.as_mut().storage,
        &tick,
        &Decimal::zero(),
        owner.clone(),
        RangeConfig {
            high: Decimal::from_str("4").unwrap(),
            low: Decimal::from_str("3").unwrap(),
            skew: SignedDecimal::from_str("0.3").unwrap(),
            spread: Decimal::permille(10),
            fee: Decimal::zero(),
        },
        (
            Decimal::from_str("100000000").unwrap(),
            Decimal::from_str("401992761").unwrap(),
        ),
    )
    .unwrap();

    let initial_base = range.base;
    let initial_quote = range.quote;
    let initial_price = range.price;

    let deps_mut = deps.as_mut();
    range.save(deps_mut.storage).unwrap();

    let mut swapper = Swapper::new(
        CONTRACT_NAME,
        RangeContext::new(
            Addr::unchecked("fee"),
            Decimal::zero(),
            Denoms::new("base", "quote"),
            tick.clone(),
        ),
        Uint128::new(1000000),
        rujira_rs::fin::SwapRequest::Yolo {
            to: None,
            callback: None,
        },
        Decimal::zero(),
    );

    let res = {
        let mut iter = RangeOfferIter::new(deps_mut.storage, &Side::Base, &Decimal::permille(50));
        swapper.swap(&mut iter).unwrap()
    };

    assert!(res.consumed_offer > Uint128::zero());
    assert!(res.return_amount > Uint128::zero());

    swapper
        .context
        .commit(deps_mut.storage, Response::default())
        .unwrap();

    let updated_range = Range::load(deps.as_ref().storage, Uint128::one()).unwrap();

    // After buying base: base decreases, quote increases, price increases
    assert!(updated_range.base < initial_base);
    assert!(updated_range.quote > initial_quote);
    assert!(updated_range.price > initial_price);
}

#[test]
fn scl_price_stays_in_bounds() {
    let high = Decimal::from_str("4").unwrap();
    let low = Decimal::from_str("3").unwrap();

    // High positive skew, mostly quote
    let scl = Scl::new(high, low, SignedDecimal::from_str("0.9").unwrap());
    let price = scl.price(
        Decimal::from_str("100").unwrap(),
        Decimal::from_str("1000000").unwrap(),
    );
    assert!(price >= low);
    assert!(price <= high);

    // High negative skew, mostly quote
    let scl = Scl::new(high, low, SignedDecimal::from_str("-0.9").unwrap());
    let price = scl.price(
        Decimal::from_str("100").unwrap(),
        Decimal::from_str("1000000").unwrap(),
    );
    assert!(price >= low);
    assert!(price <= high);

    // Mostly base
    let scl = Scl::new(high, low, SignedDecimal::from_str("0.5").unwrap());
    let price = scl.price(
        Decimal::from_str("1000000").unwrap(),
        Decimal::from_str("1").unwrap(),
    );
    assert!(price >= low);
    assert!(price <= high);

    // Extreme positive skew
    let scl = Scl::new(high, low, SignedDecimal::from_str("0.99").unwrap());
    let price = scl.price(
        Decimal::from_str("500000").unwrap(),
        Decimal::from_str("500000").unwrap(),
    );
    assert!(price >= low);
    assert!(price <= high);

    // Extreme negative skew
    let scl = Scl::new(high, low, SignedDecimal::from_str("-0.99").unwrap());
    let price = scl.price(
        Decimal::from_str("500000").unwrap(),
        Decimal::from_str("500000").unwrap(),
    );
    assert!(price >= low);
    assert!(price <= high);
}

#[test]
fn scl_withdraw_preserves_price_ratio() {
    let mut deps = mock_dependencies();
    let tick = Tick::new(8);
    let owner = Addr::unchecked("owner");
    let denoms = Denoms::new("base", "quote");

    let mut range = Range::new(
        deps.as_mut().storage,
        &tick,
        &Decimal::zero(),
        owner.clone(),
        RangeConfig {
            high: Decimal::from_str("4").unwrap(),
            low: Decimal::from_str("3").unwrap(),
            skew: SignedDecimal::from_str("0.5").unwrap(),
            spread: Decimal::permille(10),
            fee: Decimal::zero(),
        },
        (
            Decimal::from_str("100000000").unwrap(),
            Decimal::from_str("200000000").unwrap(),
        ),
    )
    .unwrap();

    let initial_price = range.price;
    let initial_ratio = range.quote / range.base;

    range.withdraw(&denoms, Decimal::from_str("0.5").unwrap());
    range.reprice(&tick, &Decimal::zero());

    // Price should not change significantly
    let price_diff = if range.price > initial_price {
        range.price - initial_price
    } else {
        initial_price - range.price
    };
    assert!(price_diff < Decimal::from_str("0.001").unwrap());

    // Ratio should not change significantly
    let final_ratio = range.quote / range.base;
    let ratio_diff = if final_ratio > initial_ratio {
        final_ratio - initial_ratio
    } else {
        initial_ratio - final_ratio
    };
    assert!(ratio_diff < Decimal::from_str("0.001").unwrap());
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 64, ..ProptestConfig::default() })]
    #[test]
    fn scl_range_offer_iter_invariants(range_inputs in prop::collection::vec(
        (100000u64..900000u64).prop_flat_map(|low| {
            (
                Just(low),
                (low + 1)..1_000_000u64,
                1u128..1_000_000u128,
                1u128..1_000_000u128,
                1u64..900u64,
                -100i64..100i64, // skew * 100
            )
        }),
        1..10
    )) {
        let mut deps = mock_dependencies();
        let tick = Tick::new(6);
        let owner = Addr::unchecked("owner");
        let mut ranges: BTreeMap<Uint128, Range> = BTreeMap::new();

        for (low, high, base, quote, spread_permille, skew_100) in range_inputs {
            let skew = SignedDecimal::from_str(&format!("0.{:02}", skew_100.abs())).unwrap();
            let skew = if skew_100 < 0 { -skew } else { skew };

            let range = Range::new(
                deps.as_mut().storage,
                &tick,
                &Decimal::zero(),
                owner.clone(),
                RangeConfig {
                    high: Decimal::from_atomics(high as u128, 0).unwrap(),
                    low: Decimal::from_atomics(low as u128, 0).unwrap(),
                    skew,
                    spread: Decimal::permille(spread_permille),
                    fee: Decimal::zero(),
                },
                (Decimal::from_atomics(base, 0).unwrap(), Decimal::from_atomics(quote, 0).unwrap()),
            )
            .unwrap();

            // Price should always be in range
            prop_assert!(range.price >= Decimal::from_atomics(low as u128, 0).unwrap());
            prop_assert!(range.price <= Decimal::from_atomics(high as u128, 0).unwrap());

            range.save(deps.as_mut().storage).unwrap();
            ranges.insert(range.idx, range);
        }

        let delta = Decimal::permille(50);
        assert_range_offer_invariants(deps.as_ref().storage, Side::Base, delta, &ranges)?;
        assert_range_offer_invariants(deps.as_ref().storage, Side::Quote, delta, &ranges)?;
    }
}

#[test]
fn multiindex_moves_zero_liquidity_ranges() {
    let mut deps = mock_dependencies();
    let tick = Tick::new(8);
    let owner = Addr::unchecked("owner");

    // Range 1: base=0 (no ask liquidity) - moved to end of asks
    let range_1 = Range::new(
        deps.as_mut().storage,
        &tick,
        &Decimal::zero(),
        owner.clone(),
        RangeConfig {
            high: Decimal::one(),
            low: Decimal::from_str("0.9").unwrap(),
            skew: SignedDecimal::zero(),
            spread: Decimal::permille(1),
            fee: Decimal::zero(),
        },
        (Decimal::zero(), Decimal::from_str("1000").unwrap()),
    )
    .unwrap();
    range_1.save(deps.as_mut().storage).unwrap();

    // Range 2: quote=0 (no bid liquidity) - moved to end of bids
    let range_2 = Range::new(
        deps.as_mut().storage,
        &tick,
        &Decimal::zero(),
        owner.clone(),
        RangeConfig {
            high: Decimal::from_str("1.1").unwrap(),
            low: Decimal::one(),
            skew: SignedDecimal::zero(),
            spread: Decimal::permille(1),
            fee: Decimal::zero(),
        },
        (Decimal::from_str("1000").unwrap(), Decimal::zero()),
    )
    .unwrap();
    range_2.save(deps.as_mut().storage).unwrap();

    // Range 3: has both base and quote - appears normally in both indexes
    let range_3 = Range::new(
        deps.as_mut().storage,
        &tick,
        &Decimal::zero(),
        owner.clone(),
        RangeConfig {
            high: Decimal::from_str("1.05").unwrap(),
            low: Decimal::from_str("0.95").unwrap(),
            skew: SignedDecimal::zero(),
            spread: Decimal::permille(1),
            fee: Decimal::zero(),
        },
        (
            Decimal::from_str("1000").unwrap(),
            Decimal::from_str("1000").unwrap(),
        ),
    )
    .unwrap();
    range_3.save(deps.as_mut().storage).unwrap();

    // ASKS (ascending): range_1 (base=0) should be last
    let asks: Vec<Range> = Range::map()
        .idx
        .asks
        .range(deps.as_ref().storage, None, None, Order::Ascending)
        .filter_map(|r| r.ok().map(|(_, range)| range))
        .collect();

    assert_eq!(asks.len(), 3);
    assert_eq!(asks.last().unwrap().idx, range_1.idx);
    assert!(asks.last().unwrap().base.is_zero());

    // BIDS (descending): range_2 (quote=0) should be last
    let bids: Vec<Range> = Range::map()
        .idx
        .bids
        .range(deps.as_ref().storage, None, None, Order::Descending)
        .filter_map(|r| r.ok().map(|(_, range)| range))
        .collect();

    assert_eq!(bids.len(), 3);
    assert_eq!(bids.last().unwrap().idx, range_2.idx);
    assert!(bids.last().unwrap().quote.is_zero());
}
