use std::{
    collections::{BTreeMap, BTreeSet},
    str::FromStr,
};

use crate::config::Config;
use crate::contract::CONTRACT_NAME;
use crate::market_makers::MarketMakers;
use crate::order_pool::Pool;
use crate::swap_iter::{create_context, SwapIter};

use super::*;
use cosmwasm_std::{
    coin, coins,
    testing::{mock_dependencies, MockQuerier},
    Addr, BankMsg, CosmosMsg, Decimal, Decimal256, Event, Order, QuerierWrapper, Response,
    SignedDecimal, SubMsg, Timestamp, Uint128,
};
use cw_utils::NativeBalance;
use proptest::prelude::*;
use rujira_rs::{
    exchange::{Arber, Arbitrage, SwapContext, SwapItem, Swapper},
    fin::{Denoms, Price, RangeConfig, Scl, Side, SwapRequest, Tick},
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

    let bids: Vec<RangeOffer> = RangeOfferIter::new(
        deps.as_ref().storage,
        &Side::Quote,
        &Decimal::permille(50),
        &Decimal::one(),
    )
    .collect();
    let asks: Vec<RangeOffer> = RangeOfferIter::new(
        deps.as_ref().storage,
        &Side::Base,
        &Decimal::permille(50),
        &Decimal::one(),
    )
    .collect();

    assert_eq!(asks[2].clone().end, Decimal::from_str("4").unwrap());
    assert_eq!(
        asks[2].clone().start,
        Decimal::from_str("3.87804386025").unwrap()
    );

    assert_eq!(
        asks[2].clone().total,
        Decimal::from_str("22597172.267534579296755009").unwrap()
    );

    assert_eq!(asks[1].clone().end, asks[2].start);
    assert_eq!(
        asks[1].clone().start,
        Decimal::from_str("3.693375105").unwrap()
    );
    assert_eq!(
        asks[1].clone().total,
        Decimal::from_str("36324695.679974781312538978").unwrap()
    );
    assert_eq!(asks[0].clone().end, asks[1].start);
    assert_eq!(
        asks[0].clone().start,
        Decimal::from_str("3.5175001").unwrap()
    );
    assert_eq!(
        asks[0].clone().total,
        Decimal::from_str("37221736.822116691229457652").unwrap()
    );
    assert_eq!(bids[0].clone().start, Decimal::from_str("3.4825").unwrap());
    assert_eq!(
        bids[0].clone().total,
        Decimal::from_str("136872729.825035047756939994").unwrap()
    );
    assert_eq!(
        bids[1].clone().start,
        Decimal::from_str("3.308375").unwrap()
    );
    assert_eq!(
        bids[1].clone().total,
        Decimal::from_str("133407034.901721681478251664").unwrap()
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

    let asks: Vec<RangeOffer> = RangeOfferIter::new(
        deps.as_ref().storage,
        &Side::Base,
        &Decimal::permille(50),
        &Decimal::one(),
    )
    .take(20)
    .collect::<Vec<RangeOffer>>();

    // Range 2 is better priced and has a wider range. we should have complete overlap with 1
    assert_eq!(asks[0].clone().start, range_2.ask);
    assert_eq!(asks[0].clone().end, asks[1].clone().start);
    assert_eq!(asks[0].clone().ranges.len(), 1);
    assert_eq!(
        asks[0].clone().total,
        Decimal::from_str("12993121.780379823707211751").unwrap()
    );
    assert_eq!(
        asks[0].clone().ranges[0].clone().0.idx,
        Uint128::from(2u128)
    );

    assert_eq!(asks[1].clone().ranges.len(), 1);
    assert_eq!(
        asks[1].clone().total,
        Decimal::from_str("977411.670896641190920156").unwrap()
    );
    assert_eq!(
        asks[1].clone().start,
        Decimal::from_str("3.504443355").unwrap()
    );
    // Check we have a definite stop at the bid limit
    assert_eq!(asks[1].clone().end, range_1.ask);

    assert_eq!(asks[2].clone().ranges.len(), 2);
    assert_eq!(
        asks[2].clone().total,
        Decimal::from_str("49878169.765450675473132008").unwrap()
    );
    assert_eq!(asks[2].clone().start, range_1.ask);
    assert_eq!(asks[3].clone().ranges.len(), 2);
    assert_eq!(
        asks[3].clone().total,
        Decimal::from_str("48676109.512642635605240065").unwrap()
    );
    assert_eq!(
        asks[3].clone().start,
        Decimal::from_str("3.693375105").unwrap()
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

    let asks: Vec<RangeOffer> = RangeOfferIter::new(
        deps.as_ref().storage,
        &Side::Base,
        &Decimal::permille(50),
        &Decimal::one(),
    )
    .take(20)
    .collect::<Vec<RangeOffer>>();

    assert_eq!(asks[0].clone().start, range_1.ask);
    assert_eq!(
        asks[0].clone().total,
        Decimal::from_str("37221736.822116691229457652").unwrap()
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

    let last = RangeOfferIter::new(
        deps.as_ref().storage,
        &Side::Quote,
        &Decimal::permille(50),
        &Decimal::one(),
    )
    .last()
    .unwrap();
    assert_eq!(last.clone().start, Decimal::from_str("3.14295625").unwrap());
    assert_eq!(last.clone().end, range.low);
    assert_eq!(
        last.clone().total,
        Decimal::from_str("118148118.395592052047160687").unwrap()
    );

    let last = RangeOfferIter::new(
        deps.as_ref().storage,
        &Side::Base,
        &Decimal::permille(50),
        &Decimal::one(),
    )
    .last()
    .unwrap();

    assert_eq!(
        last.clone().start,
        Decimal::from_str("3.87804386025").unwrap()
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
        effective_rate: Decimal256::from_str("2").unwrap(),
        ranges: vec![
            (
                RangeHandle::new(Range {
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
                }),
                Decimal::from_str("400").unwrap(),
            ),
            (
                RangeHandle::new(Range {
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
                }),
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
    let r = ctx.ranges.get(&Uint128::one()).unwrap().borrow();
    assert_eq!(r.base, Decimal::from_str("233.333333333333333333").unwrap());
    assert_eq!(
        r.quote,
        Decimal::from_str("333.333333333333333334").unwrap()
    );

    let r = ctx.ranges.get(&Uint128::new(2)).unwrap().borrow();
    assert_eq!(r.base, Decimal::from_str("116.666666666666666667").unwrap());
    assert_eq!(
        r.quote,
        Decimal::from_str("166.666666666666666666").unwrap()
    );
}

#[test]
fn range_offer_swap_rounds_input_up_for_small_fill() {
    let mut offer = RangeOffer {
        side: Side::Quote,
        start: Decimal::from_str("2281.6").unwrap(),
        end: Decimal::from_str("2281.59").unwrap(),
        total: Decimal::from_str("3040823.826021148969545223").unwrap(),
        effective_rate: Decimal256::from_str("2281.595000076186318857").unwrap(),
        ranges: vec![(
            RangeHandle::new(Range {
                idx: Uint128::one(),
                fees: [Decimal::zero(), Decimal::zero()],
                owner: Addr::unchecked(""),
                high: Decimal::from_str("3000").unwrap(),
                low: Decimal::from_str("1500").unwrap(),
                skew: SignedDecimal::zero(),
                spread: Decimal::zero(),
                fee: Decimal::zero(),
                base: Decimal::one(),
                quote: Decimal::from_str("5000").unwrap(),
                price: Decimal::from_str("2281.595000076186318857").unwrap(),
                ask: Decimal::from_str("2281.6").unwrap(),
                bid: Decimal::from_str("2281.59").unwrap(),
            }),
            Decimal::from_str("3040823.826021148969545223").unwrap(),
        )],
    };
    let mut ctx = RangeContext::new(
        Addr::unchecked(""),
        Decimal::zero(),
        Denoms::new("base", "quote"),
        Tick::new(8),
    );

    let (offer_consumed, bids_returned, attrs) = offer.swap(&mut ctx, Uint128::one()).unwrap();

    assert_eq!(offer_consumed, Uint128::one());
    assert_eq!(bids_returned, Uint128::new(2281));
    assert!(!attrs.is_empty());
    let r = ctx.ranges.get(&Uint128::one()).unwrap().borrow();
    assert_eq!(r.base, Decimal::from_str("2").unwrap());
    assert_eq!(r.quote, Decimal::from_str("2719").unwrap());
}

#[test]
fn range_offer_swap_rounds_input_up_to_prevent_self_arb() {
    let range = Range {
        idx: Uint128::new(33),
        owner: Addr::unchecked("owner"),
        high: Decimal::from_str("150000").unwrap(),
        low: Decimal::from_str("50000").unwrap(),
        skew: SignedDecimal::zero(),
        spread: Decimal::from_str("0.0002").unwrap(),
        fee: Decimal::zero(),
        base: Decimal::from_str("1402414.4836493394313063").unwrap(),
        quote: Decimal::from_str("87313050961.282760127224579746").unwrap(),
        price: Decimal::from_str("80041.661274186676520142").unwrap(),
        ask: Decimal::from_str("80069.7").unwrap(),
        bid: Decimal::from_str("80013.6").unwrap(),
        fees: [Decimal::zero(), Decimal::zero()],
    };

    let mut quote_offer = RangeOffer {
        side: Side::Quote,
        start: Decimal::from_str("79993.594099619232279608").unwrap(),
        end: Decimal::from_str("80013.6").unwrap(),
        total: Decimal::from_str("104119177").unwrap(),
        effective_rate: Decimal256::from_str("79993.594099619232279608").unwrap(),
        ranges: vec![(
            RangeHandle::new(range.clone()),
            Decimal::from_str("104119177").unwrap(),
        )],
    };
    let mut ctx = RangeContext::new(
        Addr::unchecked("fee"),
        Decimal::from_str("0.00025").unwrap(),
        Denoms::new("btc-btc", "eth-usdc"),
        Tick::new(6),
    );

    let (offer_consumed, bids_returned, _) = quote_offer.swap(&mut ctx, Uint128::new(230)).unwrap();

    assert_eq!(offer_consumed, Uint128::new(230));
    assert_eq!(bids_returned, Uint128::new(18398526));

    let base_offer = RangeOffer {
        side: Side::Base,
        start: Decimal::from_str("80073.249892463149593116").unwrap(),
        end: Decimal::from_str("80069.7").unwrap(),
        total: Decimal::from_str("230").unwrap(),
        effective_rate: Decimal256::from_str("80073.249892463149593116").unwrap(),
        ranges: vec![(
            RangeHandle::new(range.clone()),
            Decimal::from_str("230").unwrap(),
        )],
    };
    let quote_offer = RangeOffer {
        side: Side::Quote,
        start: Decimal::from_str("79993.594099619232279608").unwrap(),
        end: Decimal::from_str("80013.6").unwrap(),
        total: Decimal::from_str("104119177").unwrap(),
        effective_rate: Decimal256::from_str("79993.594099619232279608").unwrap(),
        ranges: vec![(
            RangeHandle::new(range),
            Decimal::from_str("104119177").unwrap(),
        )],
    };
    let mut arb = Arber::new(
        RangeContext::new(
            Addr::unchecked("fee"),
            Decimal::from_str("0.00025").unwrap(),
            Denoms::new("btc-btc", "eth-usdc"),
            Tick::new(6),
        ),
        CONTRACT_NAME.to_string(),
    );
    let res = arb
        .run(vec![base_offer].into_iter(), vec![quote_offer].into_iter())
        .unwrap();

    assert_eq!(res.profit_base, Uint128::zero());
    assert_eq!(res.profit_quote, Uint128::zero());
    assert!(res.events.is_empty());
}

#[test]
fn range_offer_swap_overfill() {
    let mut offer = RangeOffer {
        side: Side::Quote,
        start: Decimal::from_str("2").unwrap(),
        end: Decimal::from_str("3").unwrap(),
        total: Decimal::from_str("600").unwrap(),
        effective_rate: Decimal256::from_str("2").unwrap(),
        ranges: vec![
            (
                RangeHandle::new(Range {
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
                }),
                Decimal::from_str("400").unwrap(),
            ),
            (
                RangeHandle::new(Range {
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
                }),
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

    let r = ctx.ranges.get(&Uint128::one()).unwrap().borrow();
    assert_eq!(r.base, Decimal::from_str("400").unwrap());
    assert_eq!(r.quote, Decimal::zero());

    let r = ctx.ranges.get(&Uint128::new(2)).unwrap().borrow();
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
        effective_rate: Decimal256::from_str("2").unwrap(),
        ranges: vec![
            (
                RangeHandle::new(Range {
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
                }),
                Decimal::from_str("200").unwrap(),
            ),
            (
                RangeHandle::new(Range {
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
                }),
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

    let r = ctx.ranges.get(&Uint128::one()).unwrap().borrow();
    assert_eq!(r.base, Decimal::from_str("166.666666666666666667").unwrap());
    assert_eq!(
        r.quote,
        Decimal::from_str("466.666666666666666666").unwrap()
    );

    let r = ctx.ranges.get(&Uint128::new(2)).unwrap().borrow();
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
        effective_rate: Decimal256::from_str("2").unwrap(),
        ranges: vec![
            (
                RangeHandle::new(Range {
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
                }),
                Decimal::from_str("200").unwrap(),
            ),
            (
                RangeHandle::new(Range {
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
                }),
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

    let r = ctx.ranges.get(&Uint128::one()).unwrap().borrow();
    assert_eq!(r.base, Decimal::zero());
    assert_eq!(r.quote, Decimal::from_str("800").unwrap());

    let r = ctx.ranges.get(&Uint128::new(2)).unwrap().borrow();
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

    let bids: Vec<RangeOffer> = RangeOfferIter::new(
        deps.as_ref().storage,
        &Side::Quote,
        &Decimal::permille(50),
        &Decimal::one(),
    )
    .take(2)
    .collect::<Vec<RangeOffer>>();
    assert_eq!(bids[0].clone().start, Decimal::from_str("3.4825").unwrap());
    assert_eq!(
        bids[0].clone().total,
        Decimal::from_str("136872729.825035047756939994").unwrap()
    );

    // Swap less then the first bid
    assert_eq!(
        range.price,
        Decimal::from_str("3.500000000130949811").unwrap()
    );
    assert_eq!(range.ask, Decimal::from_str("3.5175001").unwrap());
    assert_eq!(range.bid, Decimal::from_str("3.4825").unwrap());

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
    assert_eq!(range.ask, Decimal::from_str("3.5129625").unwrap());
    assert_eq!(range.bid, Decimal::from_str("3.4780076").unwrap());
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
        let mut iter: RangeOfferIter<'_> = RangeOfferIter::new(
            deps_mut.storage,
            &Side::Quote,
            &Decimal::permille(50),
            &Decimal::one(),
        );
        swapper.swap(&mut iter).unwrap()
    };
    assert_eq!(res.consumed_offer, Uint128::new(1_000_000));
    // return_amount reflects curve-effective pricing (ΔY/ΔX) instead of slice boundary
    assert_eq!(res.return_amount, Uint128::new(3_394_321));

    swapper
        .context
        .commit(deps_mut.storage, Response::default())
        .unwrap();

    let _range = Range::load(deps.as_ref().storage, Uint128::one()).unwrap();
}

#[test]
fn rejected_limit_tier_does_not_mutate_accepted_range_state() {
    let mut deps = mock_dependencies();
    let range = Range::new(
        deps.as_mut().storage,
        &Tick::new(8),
        &Decimal::zero(),
        Addr::unchecked("owner"),
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

    let delta = Decimal::permille(5);
    let mut preview =
        RangeOfferIter::new(deps.as_ref().storage, &Side::Base, &delta, &Decimal::one());
    let first_rate = preview.next().unwrap().rate();
    let second_rate = preview.next().unwrap().rate();
    let limit = (first_rate + second_rate) / Decimal::from_ratio(2u128, 1u128);

    let mut swapper = Swapper::new(
        CONTRACT_NAME,
        RangeContext::new(
            Addr::unchecked("fee"),
            Decimal::zero(),
            Denoms::new("base", "quote"),
            Tick::new(8),
        ),
        Uint128::new(1_000_000_000),
        SwapRequest::Limit {
            price: limit,
            to: None,
            callback: None,
        },
        Decimal::zero(),
    );
    let result = {
        let mut iter =
            RangeOfferIter::new(deps.as_ref().storage, &Side::Base, &delta, &Decimal::one());
        swapper.swap(&mut iter).unwrap()
    };

    assert_eq!(result.events.len(), 1, "test must accept exactly one tier");

    // Only the accepted tier may affect reserves. The rejected second tier is
    // evaluated against a cloned context and must be rolled back.
    let expected_base = range.base - Decimal::from_atomics(result.return_amount, 0).unwrap();
    let actual_base = swapper
        .context
        .ranges
        .get(&range.idx)
        .unwrap()
        .borrow()
        .base;
    assert_eq!(actual_base, expected_base);
}

/// One big swap consuming the whole range must produce the same result
/// as N independent tier-by-tier swaps. This validates that the feedback
/// channel keeps reserves fresh between tiers.
#[test]
fn single_swap_equals_n_tier_swaps() {
    let delta = Decimal::permille(5); // 0.5% steps — many tiers to amplify stale-data drift
    let side = Side::Quote;
    let offer_amount = Uint128::new(100_000_000);

    let make_ctx = || {
        RangeContext::new(
            Addr::unchecked("fee"),
            Decimal::zero(),
            Denoms::new("base", "quote"),
            Tick::new(8),
        )
    };

    let setup = |deps: &mut cosmwasm_std::OwnedDeps<_, _, _>| {
        let tick = Tick::new(8);
        let range = Range::new(
            deps.as_mut().storage,
            &tick,
            &Decimal::zero(),
            Addr::unchecked("owner"),
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
    };

    // Path A: single swap consuming the full offer in one go
    let mut deps_a = mock_dependencies();
    setup(&mut deps_a);
    let mut swapper_a = Swapper::new(
        CONTRACT_NAME,
        make_ctx(),
        offer_amount,
        SwapRequest::Yolo {
            to: None,
            callback: None,
        },
        Decimal::zero(),
    );
    let res_a = {
        let deps_mut = deps_a.as_mut();
        let mut iter = RangeOfferIter::new(deps_mut.storage, &side, &delta, &Decimal::one());
        swapper_a.swap(&mut iter).unwrap()
    };
    // Collect Path A per-tier results
    let path_a_events: Vec<_> = res_a
        .events
        .iter()
        .map(|e| {
            let offer: u128 = e
                .attributes
                .iter()
                .find(|a| a.key == "offer")
                .unwrap()
                .value
                .parse()
                .unwrap();
            let bid: u128 = e
                .attributes
                .iter()
                .find(|a| a.key == "bid")
                .unwrap()
                .value
                .parse()
                .unwrap();
            (offer, bid)
        })
        .collect();

    // Path B: tier-by-tier — each tier is a separate swap, committing between them
    let mut deps_b = mock_dependencies();
    setup(&mut deps_b);
    let mut total_consumed = Uint128::zero();
    let mut total_returned = Uint128::zero();
    let mut remaining = offer_amount;
    let mut tier_count = 0u64;

    while !remaining.is_zero() {
        let mut swapper_b = Swapper::new(
            CONTRACT_NAME,
            make_ctx(),
            remaining,
            SwapRequest::Yolo {
                to: None,
                callback: None,
            },
            Decimal::zero(),
        );
        let res_b = {
            let deps_mut = deps_b.as_mut();
            let mut iter = RangeOfferIter::new(deps_mut.storage, &side, &delta, &Decimal::one());
            // Take only one tier
            let mut one_tier = iter.next().into_iter();
            swapper_b.swap(&mut one_tier).unwrap()
        };
        swapper_b
            .context
            .commit(deps_b.as_mut().storage, Response::default())
            .unwrap();

        if res_b.consumed_offer.is_zero() {
            break;
        }
        tier_count += 1;
        let b_offer: u128 = res_b.consumed_offer.into();
        let b_bid: u128 = res_b.return_amount.into();
        if tier_count <= path_a_events.len() as u64 {
            let (a_offer, a_bid) = path_a_events[(tier_count - 1) as usize];
            if a_offer != b_offer || a_bid != b_bid {
                eprintln!("DIVERGE tier {tier_count}: A offer={a_offer} bid={a_bid} | B offer={b_offer} bid={b_bid}");
            }
        }
        total_consumed += res_b.consumed_offer;
        total_returned += res_b.return_amount;
        remaining -= res_b.consumed_offer;
    }

    // Divergence comes from price-step boundary differences: single swap iterates
    // continuously from the original bid, tier-by-tier restarts from the repriced bid.
    // The feedback keeps reserves fresh, so divergence is bounded by the delta step size.
    let returned_diff = res_a.return_amount.abs_diff(total_returned);
    let consumed_diff = res_a.consumed_offer.abs_diff(total_consumed);
    let max_returned = res_a.return_amount.max(total_returned);
    let max_consumed = res_a.consumed_offer.max(total_consumed);
    // Divergence must be < delta (0.5%) — well within acceptable range
    let max_pct = Decimal::permille(5); // same as delta
    let returned_pct = Decimal::from_ratio(returned_diff, max_returned);
    let consumed_pct = Decimal::from_ratio(consumed_diff, max_consumed);
    assert!(
        returned_pct < max_pct,
        "return_amount diverged: single={} vs tiers={}, diff={returned_diff} ({returned_pct}%), max_allowed={max_pct}%",
        res_a.return_amount, total_returned
    );
    assert!(
        consumed_pct < max_pct,
        "consumed_offer diverged: single={} vs tiers={}, diff={consumed_diff} ({consumed_pct}%), max_allowed={max_pct}%",
        res_a.consumed_offer, total_consumed
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

    assert_eq!(range.quote, Decimal::from_str("595").unwrap());
    assert_eq!(range.base, Decimal::from_str("100").unwrap());
    assert_eq!(
        range.fees,
        [Decimal::zero(), Decimal::from_str("5").unwrap()]
    );

    range.swap(
        &mut ctx,
        &Side::Quote,
        Decimal::from_str("50").unwrap(),
        Decimal::from_str("100").unwrap(),
    );

    assert_eq!(range.quote, Decimal::from_str("495").unwrap());
    assert_eq!(range.base, Decimal::from_str("148.75").unwrap());
    assert_eq!(
        range.fees,
        [
            Decimal::from_str("1.25").unwrap(),
            Decimal::from_str("5").unwrap()
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
        effective_rate: Decimal256::from_str("2").unwrap(),
        ranges: vec![
            (
                RangeHandle::new(Range {
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
                }),
                Decimal::from_str("400").unwrap(),
            ),
            (
                RangeHandle::new(Range {
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
                }),
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

    assert_eq!(range.base, Decimal::from_str("66666667").unwrap());
    assert_eq!(range.quote, Decimal::from_str("133333334").unwrap());
    assert_eq!(
        res,
        Some(NativeBalance(vec![
            coin(33333333, "base"),
            coin(66666666, "quote"),
        ]))
    )
}

#[test]
fn range_withdraw_zeroes_subatomic() {
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

    // Full withdraw
    let res = range.withdraw(&denoms, Decimal::one());
    assert!(res.is_some());
    assert_eq!(range.base, Decimal::zero());
    assert_eq!(range.quote, Decimal::zero());

    // Reprice, save, reload with zero reserves must not panic
    range.reprice(&tick, &Decimal::zero());
    range.save(deps.as_mut().storage).unwrap();
    Range::load(deps.as_ref().storage, range.idx).unwrap();
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

    for offer in RangeOfferIter::new(storage, &side, &delta, &Decimal::one()) {
        for (range, amount) in &offer.ranges {
            let range = range.borrow();
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

    /// Single swap must match N independent tier-by-tier swaps within delta %.
    /// The feedback channel keeps reserves fresh; residual divergence comes from
    /// price-step boundary differences (continuous vs restarted iteration).
    #[test]
    fn single_swap_equals_n_tier_swaps_prop(
        (low, high, base, quote, spread_permille) in range_inputs(),
        offer_pct in 1u64..100u64,
        side_base in proptest::bool::ANY,
    ) {
        let side = if side_base { Side::Base } else { Side::Quote };
        // 0.05% = 0.0005
        let delta = Decimal::from_ratio(5u64, 10_000u64);
        let scale = 1_000_000u128;

        let setup = |deps: &mut cosmwasm_std::OwnedDeps<_, _, _>| {
            let tick = Tick::new(6);
            let range = Range::new(
                deps.as_mut().storage,
                &tick,
                &Decimal::zero(),
                Addr::unchecked("owner"),
                RangeConfig {
                    high: Decimal::from_atomics(high as u128, 0).unwrap(),
                    low: Decimal::from_atomics(low as u128, 0).unwrap(),
                    skew: SignedDecimal::zero(),
                    spread: Decimal::permille(spread_permille),
                    fee: Decimal::zero(),
                },
                (
                    Decimal::from_atomics(base * scale, 0).unwrap(),
                    Decimal::from_atomics(quote * scale, 0).unwrap(),
                ),
            )
            .unwrap();
            range.save(deps.as_mut().storage).unwrap();
        };

        let make_ctx = || {
            RangeContext::new(
                Addr::unchecked("fee"),
                Decimal::zero(),
                Denoms::new("base", "quote"),
                Tick::new(6),
            )
        };

        // Scale reserves up to get meaningful divergence at Uint128 level
        let scale = 1_000_000u128;
        let reserve = match side {
            Side::Base => quote,
            Side::Quote => base,
        };
        let offer_amount = Uint128::new(reserve * scale * offer_pct as u128 / 100);
        if offer_amount.is_zero() {
            return Ok(());
        }

        // Path A: single swap
        let mut deps_a = mock_dependencies();
        setup(&mut deps_a);
        let mut swapper_a = Swapper::new(
            CONTRACT_NAME,
            make_ctx(),
            offer_amount,
            SwapRequest::Yolo { to: None, callback: None },
            Decimal::zero(),
        );
        let res_a = {
            let deps_mut = deps_a.as_mut();
            let mut iter = RangeOfferIter::new(deps_mut.storage, &side, &delta, &Decimal::one());
            swapper_a.swap(&mut iter).map_err(|e| TestCaseError::fail(format!("{e:?}")))?
        };

        // Path B: tier-by-tier
        let mut deps_b = mock_dependencies();
        setup(&mut deps_b);
        let mut total_consumed = Uint128::zero();
        let mut total_returned = Uint128::zero();
        let mut remaining = offer_amount;

        for _ in 0..1000 {
            if remaining.is_zero() {
                break;
            }
            let mut swapper_b = Swapper::new(
                CONTRACT_NAME,
                make_ctx(),
                remaining,
                SwapRequest::Yolo { to: None, callback: None },
                Decimal::zero(),
            );
            let res_b = {
                let deps_mut = deps_b.as_mut();
                let mut iter = RangeOfferIter::new(deps_mut.storage, &side, &delta, &Decimal::one());
                let mut one_tier = iter.next().into_iter();
                swapper_b.swap(&mut one_tier).map_err(|e| TestCaseError::fail(format!("{e:?}")))?
            };
            swapper_b
                .context
                .commit(deps_b.as_mut().storage, Response::default())
                .map_err(|e| TestCaseError::fail(format!("{e:?}")))?;

            if res_b.consumed_offer.is_zero() {
                break;
            }
            total_consumed += res_b.consumed_offer;
            total_returned += res_b.return_amount;
            remaining -= res_b.consumed_offer;
        }

        // Divergence from price-step boundary differences must be < delta
        let returned_diff = res_a.return_amount.abs_diff(total_returned);
        let consumed_diff = res_a.consumed_offer.abs_diff(total_consumed);
        let max_returned = res_a.return_amount.max(total_returned);
        let max_consumed = res_a.consumed_offer.max(total_consumed);
        if !max_returned.is_zero() {
            let returned_pct = Decimal::from_ratio(returned_diff, max_returned);
            prop_assert!(
                returned_pct < delta,
                "return_amount diverged: single={} vs tiers={}, diff={} ({}%), max={}%",
                res_a.return_amount, total_returned, returned_diff, returned_pct, delta
            );
        }
        if !max_consumed.is_zero() {
            let consumed_pct = Decimal::from_ratio(consumed_diff, max_consumed);
            prop_assert!(
                consumed_pct < delta,
                "consumed_offer diverged: single={} vs tiers={}, diff={} ({}%), max={}%",
                res_a.consumed_offer, total_consumed, consumed_diff, consumed_pct, delta
            );
        }
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

    let asks: Vec<RangeOffer> = RangeOfferIter::new(
        deps.as_ref().storage,
        &Side::Base,
        &Decimal::permille(50),
        &Decimal::one(),
    )
    .collect();
    let bids: Vec<RangeOffer> = RangeOfferIter::new(
        deps.as_ref().storage,
        &Side::Quote,
        &Decimal::permille(50),
        &Decimal::one(),
    )
    .collect();

    assert_eq!(
        range.price,
        Decimal::from_str("3.500000000130949811").unwrap()
    );
    assert_eq!(range.ask, Decimal::from_str("3.5175001").unwrap());
    assert_eq!(range.bid, Decimal::from_str("3.4825").unwrap());

    assert_eq!(asks.len(), 3);
    assert_eq!(bids.len(), 3);

    assert_eq!(asks[0].start, Decimal::from_str("3.5175001").unwrap());
    assert_eq!(
        asks[0].total,
        Decimal::from_str("37221736.822116691229457652").unwrap()
    );

    assert_eq!(bids[0].start, Decimal::from_str("3.4825").unwrap());
    assert_eq!(
        bids[0].total,
        Decimal::from_str("136872729.825035047756939994").unwrap()
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
        RangeOfferIter::new(deps.as_ref().storage, &Side::Base, &delta, &Decimal::one()).collect();

    Range::map()
        .remove(deps.as_mut().storage, range_uniform.idx.u128())
        .unwrap();

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
        RangeOfferIter::new(deps.as_ref().storage, &Side::Base, &delta, &Decimal::one()).collect();

    Range::map()
        .remove(deps.as_mut().storage, range_positive.idx.u128())
        .unwrap();

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
        RangeOfferIter::new(deps.as_ref().storage, &Side::Base, &delta, &Decimal::one()).collect();

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
        RangeOfferIter::new(deps.as_ref().storage, &Side::Base, &delta, &Decimal::one()).collect();

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
        let mut iter = RangeOfferIter::new(
            deps_mut.storage,
            &Side::Base,
            &Decimal::permille(50),
            &Decimal::one(),
        );
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

proptest! {
    #![proptest_config(ProptestConfig { cases: 128, ..ProptestConfig::default() })]
    #[test]
    fn range_swap_invariants(
        low in 1_000u64..900_000u64,
        width in 1u64..900_000u64,
        skew_milli in -990i64..991i64,
        spread_permille in 2u64..900u64,
        range_fee_permille in 0u64..450u64,
        base in 1_000_000u128..2_000_000_000_000_000u128,
        quote in 1_000_000u128..2_000_000_000_000_000u128,
        initial_offer_quote in 1u128..2_000_000_000_000_000u128,
        delta_permille in 1u64..200u64,
        swapper_fee_permille in 0u64..450u64,
    ) {
        prop_assume!(spread_permille > range_fee_permille + swapper_fee_permille);
        let high = low + width;

        let mut deps = mock_dependencies();
        let tick = Tick::new(8);
        let owner = Addr::unchecked("owner");

        let abs = skew_milli.unsigned_abs();
        let skew_str = format!(
            "{}{}.{:03}",
            if skew_milli < 0 { "-" } else { "" },
            abs / 1000,
            abs % 1000
        );
        let skew = SignedDecimal::from_str(&skew_str).unwrap();
        let spread = Decimal::permille(spread_permille);
        let range_fee = Decimal::permille(range_fee_permille);
        let swapper_fee = Decimal::permille(swapper_fee_permille);
        let delta = Decimal::permille(delta_permille);
        let low_price = Decimal::from_atomics(low as u128, 0).unwrap();
        let high_price = Decimal::from_atomics(high as u128, 0).unwrap();
        let mid_price = (high_price + low_price) * Decimal::from_ratio(1u128, 2u128);

        Range::new(
            deps.as_mut().storage,
            &tick,
            &Decimal::zero(),
            owner,
            RangeConfig {
                high: high_price,
                low: low_price,
                skew,
                spread,
                fee: range_fee,
            },
            (
                Decimal::from_atomics(base, 0).unwrap(),
                Decimal::from_atomics(quote, 0).unwrap(),
            ),
        )
        .unwrap().save(deps.as_mut().storage).unwrap();

        let initial_range = Range::load(deps.as_ref().storage, Uint128::one()).unwrap();
        let offer_quote = Uint128::new(initial_offer_quote.min(quote).max(1));

        let mut quote_swapper = Swapper::new(
            CONTRACT_NAME,
            RangeContext::new(
                Addr::unchecked("fee"),
                Decimal::zero(),
                Denoms::new("base", "quote"),
                tick.clone(),
            ),
            offer_quote,
            SwapRequest::Yolo {
                to: None,
                callback: None,
            },
            swapper_fee,
        );
        let quote_res = {
            let mut iter = RangeOfferIter::new(deps.as_mut().storage, &Side::Quote, &delta, &Decimal::one());
            quote_swapper.swap(&mut iter).unwrap()
        };
        // Both must be zero or both non-zero
        prop_assert_eq!(quote_res.consumed_offer.is_zero(), quote_res.return_amount.is_zero());
        // No fills when bid/ask are outside the range — valid, no swap to verify
        if quote_res.consumed_offer.is_zero() {
            return Ok(());
        }
        quote_swapper
            .context
            .commit(deps.as_mut().storage, Response::default())
            .unwrap();

        let mut base_swapper = Swapper::new(
            CONTRACT_NAME,
            RangeContext::new(
                Addr::unchecked("fee"),
                Decimal::zero(),
                Denoms::new("base", "quote"),
                tick,
            ),
            quote_res.return_amount,
            SwapRequest::Yolo {
                to: None,
                callback: None,
            },
            swapper_fee,
        );
        let base_res = {
            let mut iter = RangeOfferIter::new(deps.as_mut().storage, &Side::Base, &delta, &Decimal::one());
            base_swapper.swap(&mut iter).unwrap()
        };
        prop_assert_eq!(base_res.consumed_offer.is_zero(), base_res.return_amount.is_zero());
        if base_res.consumed_offer.is_zero() {
            return Ok(());
        }
        base_swapper
            .context
            .commit(deps.as_mut().storage, Response::default())
            .unwrap();

        let final_range = Range::load(deps.as_ref().storage, Uint128::one()).unwrap();
        let mid256 = Decimal256::from(mid_price);
        let initial_value_quote = Decimal256::from(initial_range.quote)
            + Decimal256::from(initial_range.base) * mid256;
        // Total equity includes reserves AND collected fees
        let final_value_quote = Decimal256::from(final_range.quote)
            + Decimal256::from(final_range.fees[1])
            + (Decimal256::from(final_range.base) + Decimal256::from(final_range.fees[0])) * mid256;

        prop_assert!(
            final_value_quote >= initial_value_quote,
            "total equity should not decrease; value {} -> {}, mid_price={}, base {} -> {}, quote {} -> {}, fees [{}, {}]",
            initial_value_quote,
            final_value_quote,
            mid_price,
            initial_range.base,
            final_range.base,
            initial_range.quote,
            final_range.quote,
            final_range.fees[0],
            final_range.fees[1],
        );

        // Principal (reserves only, no fees) should also be preserved when spread > fees
        let principal_value = Decimal256::from(final_range.quote)
            + Decimal256::from(final_range.base) * mid256;

        prop_assert!(
            principal_value >= initial_value_quote,
            "principal should not decrease; value {} -> {}, mid_price={}, base {} -> {}, quote {} -> {}, fees [{}, {}]",
            initial_value_quote,
            principal_value,
            mid_price,
            initial_range.base,
            final_range.base,
            initial_range.quote,
            final_range.quote,
            final_range.fees[0],
            final_range.fees[1],
        );
    }

    #[test]
    fn no_over_bidding_per_range_both_sides(
        low in 1_000u64..100_000u64,
        width in 1u64..50_000u64,
        base in 1u128..10_000_000u128,
        quote in 1u128..10_000_000u128,
        spread_permille in 1u64..900u64,
        skew_milli in -1_999i64..2_000i64,
        delta_permille in 1u64..200u64,
    ) {
        let mut deps = mock_dependencies();
        let tick = Tick::new(8);
        let owner = Addr::unchecked("owner");

        let high = low + width;
        let abs = skew_milli.unsigned_abs();
        let skew_str = format!(
            "{}{}.{:03}",
            if skew_milli < 0 { "-" } else { "" },
            abs / 1000,
            abs % 1000
        );
        let skew = SignedDecimal::from_str(&skew_str).unwrap();

        let range = Range::new(
            deps.as_mut().storage,
            &tick,
            &Decimal::zero(),
            owner,
            RangeConfig {
                high: Decimal::from_atomics(high as u128, 0).unwrap(),
                low: Decimal::from_atomics(low as u128, 0).unwrap(),
                skew,
                spread: Decimal::permille(spread_permille),
                fee: Decimal::zero(),
            },
            (
                Decimal::from_atomics(base, 0).unwrap(),
                Decimal::from_atomics(quote, 0).unwrap(),
            ),
        )
        .unwrap();
        range.save(deps.as_mut().storage).unwrap();

        let delta = Decimal::permille(delta_permille);
        let offers_base: Vec<RangeOffer> =
            RangeOfferIter::new(deps.as_ref().storage, &Side::Base, &delta, &Decimal::one()).collect();
        let offers_quote: Vec<RangeOffer> =
            RangeOfferIter::new(deps.as_ref().storage, &Side::Quote, &delta, &Decimal::one()).collect();

        let total_base: Decimal = offers_base
            .iter()
            .flat_map(|o| o.ranges.iter().map(|(_r, amount)| *amount))
            .sum();
        let total_quote: Decimal = offers_quote
            .iter()
            .flat_map(|o| o.ranges.iter().map(|(_r, amount)| *amount))
            .sum();

        prop_assert!(
            total_base <= range.base,
            "base over-offer: total={total_base}, cap={}",
            range.base
        );
        prop_assert!(
            total_quote <= range.quote,
            "quote over-offer: total={total_quote}, cap={}",
            range.quote
        );
    }
}

#[test]
fn scl_price_stays_in_bounds() {
    let high = Decimal::from_str("4").unwrap();
    let low = Decimal::from_str("3").unwrap();

    let scl = Scl::new(high, low, SignedDecimal::from_str("0.9").unwrap());
    let price = scl.price(
        Decimal::from_str("100").unwrap(),
        Decimal::from_str("1000000").unwrap(),
    );
    assert!(price >= low);
    assert!(price <= high);

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

    let scl = Scl::new(high, low, SignedDecimal::from_str("0.99").unwrap());
    let price = scl.price(
        Decimal::from_str("500000").unwrap(),
        Decimal::from_str("500000").unwrap(),
    );
    assert!(price >= low);
    assert!(price <= high);

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

proptest! {
    #![proptest_config(ProptestConfig { cases: 256, ..ProptestConfig::default() })]
    #[test]
    fn scl_slices_never_exceed_reserves(
        low in 1_000u64..100_000u64,
        width in 1u64..50_000u64,
        base in 1u128..10_000_000u128,
        quote in 1u128..10_000_000u128,
        skew_milli in -1_999i64..2_000i64,
        n_slices in 2u64..20u64,
    ) {
        let high_raw = low + width;
        let high = Decimal::from_atomics(high_raw as u128, 0).unwrap();
        let low = Decimal::from_atomics(low as u128, 0).unwrap();
        let base = Decimal::from_atomics(base, 0).unwrap();
        let quote = Decimal::from_atomics(quote, 0).unwrap();

        let abs = skew_milli.unsigned_abs();
        let skew_str = format!(
            "{}{}.{:03}",
            if skew_milli < 0 { "-" } else { "" },
            abs / 1000,
            abs % 1000
        );
        let skew = SignedDecimal::from_str(&skew_str).unwrap();
        let scl = Scl::new(high, low, skew);

        // Price at midpoint
        let price = (high + low) / Decimal::from_ratio(2u64, 1u64);
        let step = (high - price) / Decimal::from_atomics(n_slices as u128, 0).unwrap();

        // Sum base across ask-side slices (price → high)
        let mut total_base = Decimal::zero();
        for i in 0..n_slices {
            let s = price + step * Decimal::from_atomics(i as u128, 0).unwrap();
            let e = s + step;
            let amt = scl.amount_in_slice(&Side::Base, base, quote, price, s, e);
            total_base += amt;
        }

        // Sum quote across bid-side slices (price → low)
        let bid_step = (price - low) / Decimal::from_atomics(n_slices as u128, 0).unwrap();
        let mut total_quote = Decimal::zero();
        for i in 0..n_slices {
            let s = price - bid_step * Decimal::from_atomics(i as u128, 0).unwrap();
            let e = s - bid_step;
            let amt = scl.amount_in_slice(&Side::Quote, base, quote, price, s, e);
            total_quote += amt;
        }

        prop_assert!(
            total_base <= base,
            "base over-offer: total={total_base}, reserves={base}, skew={skew_str}"
        );
        prop_assert!(
            total_quote <= quote,
            "quote over-offer: total={total_quote}, reserves={quote}, skew={skew_str}"
        );
    }
}

/// Verify that skew values in (-2, 2) produce non-negative slice amounts,
/// and that higher positive σ increases concentration toward the high end.
#[test]
fn skew_distribution_stays_valid() {
    let high = Decimal::from_str("1.1").unwrap();
    let low = Decimal::from_str("1.0").unwrap();
    let base = Decimal::from_str("1000000").unwrap();
    let quote = Decimal::from_str("1000000").unwrap();
    let price = Decimal::from_str("1.05").unwrap();

    // Slice near the high end — positive skew should concentrate here
    let p_hi_start = Decimal::from_str("1.08").unwrap();
    let p_hi_end = Decimal::from_str("1.1").unwrap();

    // Subdivide the ask side into 10 slices to check every sub-slice is non-negative
    let n_slices = 10u64;
    let ask_lo = Decimal::from_str("1.05").unwrap();
    let ask_hi = high;
    let step = (ask_hi - ask_lo) / Decimal::from_atomics(n_slices as u128, 0).unwrap();

    let skews = [
        "0", "0.5", "1", "1.5", "1.999", "-0.5", "-1", "-1.5", "-1.999",
    ];

    let mut prev_positive_hi_amt = Decimal::zero();

    for skew_str in skews {
        let skew = SignedDecimal::from_str(skew_str).unwrap();
        let scl = Scl::new(high, low, skew);

        // Every sub-slice must produce a non-negative amount
        for i in 0..n_slices {
            let s = ask_lo + step * Decimal::from_atomics(i as u128, 0).unwrap();
            let e = s + step;
            let amt = scl.amount_in_slice(&Side::Base, base, quote, price, s, e);
            assert!(
                amt >= Decimal::zero(),
                "skew={skew_str}: negative base in slice [{s}, {e}]: {amt}"
            );
        }

        // Quote-side sub-slices
        let bid_lo = low;
        let bid_hi = Decimal::from_str("1.05").unwrap();
        let bid_step = (bid_hi - bid_lo) / Decimal::from_atomics(n_slices as u128, 0).unwrap();
        for i in 0..n_slices {
            let s = bid_hi - bid_step * Decimal::from_atomics(i as u128, 0).unwrap();
            let e = s - bid_step;
            let amt = scl.amount_in_slice(&Side::Quote, base, quote, price, s, e);
            assert!(
                amt >= Decimal::zero(),
                "skew={skew_str}: negative quote in slice [{s}, {e}]: {amt}"
            );
        }

        // Concentration check: for increasing positive skew, hi-slice base amount grows
        let hi_base = scl.amount_in_slice(&Side::Base, base, quote, price, p_hi_start, p_hi_end);

        if skew > SignedDecimal::zero() {
            assert!(
                hi_base >= prev_positive_hi_amt,
                "skew={skew_str}: hi-slice base {hi_base} < previous {prev_positive_hi_amt} — monotonicity broken"
            );
            prev_positive_hi_amt = hi_base;
        }

        eprintln!("skew={skew_str:>5}: hi_base={hi_base}");
    }
}

#[test]
fn arb_range_consumed_at_most_one_side() {
    // R is bracketed by two crossing limit orders. Since R.ask > R.bid and the arb is a
    // monotone inward merge, R can be consumed on at most one side in a single Arber::run.
    let mut deps = mock_dependencies();
    let owner = Addr::unchecked("owner");
    let config = Config {
        denoms: Denoms::new("base", "quote"),
        oracles: None,
        market_makers: MarketMakers::new(&deps.api, vec![]).unwrap(),
        range_delta: Decimal::permille(5),
        range_min: Decimal::one(),
        tick: Tick::new(8),
        fee_maker: Decimal::zero(),
        fee_taker: Decimal::zero(),
        fee_range: Decimal::zero(),
        fee_address: Addr::unchecked("fee"),
    };

    let range = Range::new(
        deps.as_mut().storage,
        &config.tick,
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

    // A bid above R.ask and an ask below R.bid, unequal sizes so leftover reaches R.
    let store = deps.as_mut().storage;
    Pool::load(
        store,
        &Price::Fixed(Decimal::from_str("3.6").unwrap()),
        &Side::Quote,
        &None::<Decimal>,
    )
    .unwrap()
    .create_order(
        store,
        &Timestamp::default(),
        &owner,
        Uint128::new(20_000_000),
    )
    .unwrap();
    Pool::load(
        store,
        &Price::Fixed(Decimal::from_str("3.4").unwrap()),
        &Side::Base,
        &None::<Decimal>,
    )
    .unwrap()
    .create_order(
        store,
        &Timestamp::default(),
        &owner,
        Uint128::new(3_000_000),
    )
    .unwrap();

    // Reachability: R must be available on BOTH sides, else the assertion is vacuous.
    let base_has = RangeOfferIter::new(
        deps.as_ref().storage,
        &Side::Base,
        &config.range_delta,
        &config.range_min,
    )
    .any(|o| o.ranges.iter().any(|(h, _)| h.idx == range.idx));
    let quote_has = RangeOfferIter::new(
        deps.as_ref().storage,
        &Side::Quote,
        &config.range_delta,
        &config.range_min,
    )
    .any(|o| o.ranges.iter().any(|(h, _)| h.idx == range.idx));
    assert!(base_has, "R must be available on the Base side");
    assert!(quote_has, "R must be available on the Quote side");

    let mq = MockQuerier::default();
    let swap_iter = SwapIter::new(QuerierWrapper::new(&mq), &config);
    let mut arb = Arber::new(create_context(&config), CONTRACT_NAME.to_string());
    let base_iter = swap_iter.iter(deps.as_ref().storage, &Side::Base, &None::<Decimal>);
    let quote_iter = swap_iter.iter(deps.as_ref().storage, &Side::Quote, &None::<Decimal>);
    let res = arb.run(base_iter, quote_iter).unwrap();
    arb.context
        .commit(deps.as_mut().storage, Response::default())
        .unwrap();
    assert!(!res.events.is_empty(), "arb did not execute");

    // Distinct sides on which R was traded must be at most one.
    let marker = format!("{}:", range.id());
    let mut sides = BTreeSet::new();
    for e in &res.events {
        let touches_r = e
            .attributes
            .iter()
            .any(|a| a.key == "ranges" && a.value.split(',').any(|s| s.starts_with(&marker)));
        if touches_r {
            if let Some(side) = e.attributes.iter().find(|a| a.key == "side") {
                sides.insert(side.value.clone());
            }
        }
    }
    assert!(sides.len() <= 1, "R consumed on multiple sides: {sides:?}");
}

#[test]
fn arb_probe_rollback_gap() {
    // A rejected arb probe (non-zero first leg, zero second leg -> SkipOther, no commit)
    // shares the Rc cell of a range already committed in context.ranges. It must not leak
    // into that committed state; the arb path's checkpoint/rollback guarantees this.
    let range = Range {
        idx: Uint128::new(7),
        owner: Addr::unchecked("owner"),
        high: Decimal::from_str("4").unwrap(),
        low: Decimal::from_str("3").unwrap(),
        skew: SignedDecimal::zero(),
        spread: Decimal::zero(),
        fee: Decimal::zero(),
        base: Decimal::from_str("1000").unwrap(),
        quote: Decimal::from_str("1000").unwrap(),
        price: Decimal::from_str("3.5").unwrap(),
        ask: Decimal::from_str("3.5").unwrap(),
        bid: Decimal::from_str("3.5").unwrap(),
        fees: [Decimal::zero(), Decimal::zero()],
    };
    let mut counterparty = range.clone();
    counterparty.idx = Uint128::new(8);

    let handle = RangeHandle::new(range.clone());
    let mut context = RangeContext::new(
        Addr::unchecked("fee"),
        Decimal::zero(),
        Denoms::new("base", "quote"),
        Tick::new(8),
    );
    // R is committed by a prior step; q shares its cell, b is a tiny counterparty.
    context.ranges.insert(range.idx, handle.clone());
    let before = handle.borrow().clone();

    let mut q = RangeOffer {
        side: Side::Quote,
        start: Decimal::from_str("1.5").unwrap(),
        end: Decimal::from_str("1.5").unwrap(),
        total: Decimal::from_str("1000").unwrap(),
        effective_rate: Decimal256::from_str("1.5").unwrap(),
        ranges: vec![(handle.clone(), Decimal::from_str("1000").unwrap())],
    };
    let mut b = RangeOffer {
        side: Side::Base,
        start: Decimal::from_str("3.5").unwrap(),
        end: Decimal::from_str("3.5").unwrap(),
        total: Decimal::from_str("2").unwrap(),
        effective_rate: Decimal256::from_str("3.5").unwrap(),
        ranges: vec![(
            RangeHandle::new(counterparty),
            Decimal::from_str("2").unwrap(),
        )],
    };

    let step = b.arbitrage(&mut context, CONTRACT_NAME, &mut q).unwrap();
    assert_eq!(
        format!("{step:?}"),
        "SkipOther",
        "expected a rejected probe"
    );

    assert_eq!(
        *context.ranges.get(&range.idx).unwrap().borrow(),
        before,
        "rejected probe corrupted committed range state"
    );
}
