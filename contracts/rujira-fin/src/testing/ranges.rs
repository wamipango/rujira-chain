use std::str::FromStr;

use cosmwasm_std::{coin, Decimal, Event, Uint128};
use rujira_rs::fin::{BookResponse, Denoms};
use rujira_rs_testing::mock_rujira_app;

use crate::mock::Fin;

#[test]
fn test_composite_range_lifecycle() {
    let mut app = mock_rujira_app();
    let owner = app.api().addr_make("owner");
    let fees = app.api().addr_make("fees");
    let user_a = app.api().addr_make("user_a");
    let user_b = app.api().addr_make("user_b");
    let user_c = app.api().addr_make("user_c");

    app.init_modules(|router, _, storage| {
        let amount = vec![
            coin(1000000000000, "btc-btc"),
            coin(100000000000000000, "rune"),
        ];
        router
            .bank
            .init_balance(storage, &user_a, amount.clone())
            .unwrap();
        router
            .bank
            .init_balance(storage, &user_b, amount.clone())
            .unwrap();
        router.bank.init_balance(storage, &user_c, amount).unwrap();
    });

    let fin = Fin::create(
        &mut app,
        &owner,
        &fees,
        &[],
        Denoms::new("rune", "btc-btc"),
        None,
        Decimal::permille(0),
    );
    let res = fin
        .create_range(
            &mut app,
            &user_a,
            ("0.000012", "0.0000035"),
            "0.001",
            "0.0001",
            "0",
            &[coin(5000000, "btc-btc"), coin(666300000000, "rune")],
        )
        .unwrap();
    res.assert_event(
        &Event::new("wasm-rujira-fin/range.create").add_attributes(vec![
            ("owner", user_a.as_str()),
            ("idx", "1"),
            ("high", "0.000012"),
            ("low", "0.0000035"),
            ("skew", "0"),
            ("spread", "0.001"),
            ("fee", "0.0001"),
            ("base", "666300000000"),
            ("quote", "5000000"),
        ]),
    );

    let mut book = fin.query_book(&app, None, None).unwrap();
    assert_book(
        &mut book,
        vec![
            ("0.000011765003187221", 39776696102),
            ("0.000011256625113553", 50160095979),
            ("0.000010720595346242", 51398803391),
            ("0.000010210090805943", 52668100778),
            ("0.000009723896005662", 53968743560),
            ("0.000009260853338729", 55301505816),
            ("0.000008819860322594", 56667180739),
            ("0.000008399866973902", 58066581107),
            ("0.000007999873308482", 59500539775),
            ("0.000007618926960452", 60969910163),
            ("0.000007256120914722", 62475566764),
            ("0.000006910591347351", 64018405671),
        ],
        vec![
            ("0.000006560150013223", 453157),
            ("0.000006232142512561", 441682),
            ("0.000005920535386932", 430499),
            ("0.00000562450861759", 419598),
            ("0.000005343283186711", 408974),
            ("0.000005076119027366", 398618),
            ("0.000004822313076011", 388525),
            ("0.000004581197422201", 378687),
            ("0.000004352137551093", 369099),
            ("0.000004134530673543", 359753),
            ("0.00000392780413986", 350644),
            ("0.000003731413932877", 341765),
            ("0.000003567809338599", 250037),
        ],
    );

    let res = fin
        .create_range(
            &mut app,
            &user_a,
            ("0.000012", "0.0000025"),
            "0.001",
            "0.0001",
            "0",
            &[coin(5000000, "btc-btc"), coin(416000000000, "rune")],
        )
        .unwrap();

    // Check the new range was created at the correct ratio and the overage refunded
    res.assert_event(
        &Event::new("wasm-rujira-fin/range.create").add_attributes(vec![
            ("owner", user_a.as_str()),
            ("idx", "2"),
            ("high", "0.000012"),
            ("low", "0.0000025"),
            ("skew", "0"),
            ("spread", "0.001"),
            ("fee", "0.0001"),
            ("base", "416000000000"),
            ("quote", "4365291"),
        ]),
    );

    res.assert_event(&Event::new("transfer").add_attributes(vec![
        ("recipient", user_a.as_str()),
        ("sender", fin.addr().as_str()),
        ("amount", "634708btc-btc"),
    ]));

    let mut book = fin.query_book(&app, None, None).unwrap();
    // New book that extends down to the new low range

    assert_book(
        &mut book,
        vec![
            ("0.000011765003187221", 64596767448),
            ("0.000011256625113553", 81459255610),
            ("0.000010720595346242", 83470898167),
            ("0.000010210090805943", 85532218391),
            ("0.000009723896005662", 87644443075),
            ("0.000009260853338729", 89808829310),
            ("0.000008819860322594", 92026665229),
            ("0.000008399866973902", 94299270776),
            ("0.000007999873308482", 96627998491),
            ("0.000007618926960452", 99014234315),
            ("0.000007256120914722", 101459398415),
            ("0.000006910591347351", 103964946031),
            ("0.000006743076279641", 238517510),
        ],
        vec![
            ("0.000006729603529889", 2574),
            ("0.000006558262449032", 735814),
            ("0.000006230349326574", 717183),
            ("0.00000591883186025", 699023),
            ("0.000005622890267243", 681324),
            ("0.000005341745753874", 664072),
            ("0.000005074658466181", 647257),
            ("0.000004820925542875", 630869),
            ("0.000004579879265733", 614895),
            ("0.000004350885302449", 599325),
            ("0.000004133341037318", 584150),
            ("0.000003926673985457", 569359),
            ("0.000003730340286181", 554942),
            ("0.000003567296015465", 402983),
            ("0.000003411378020685", 203906),
            ("0.000003240809119647", 198743),
            ("0.000003078768663665", 193711),
            ("0.000002924830230485", 188806),
            ("0.000002778588718951", 184025),
            ("0.000002639659283007", 179366),
            ("0.000002536149452021", 98413),
        ],
    );

    let res = fin
        .swap(&mut app, &user_b, 1000000, "btc-btc", None)
        .unwrap();
    // Effective range pricing now inserts a small leading level before the prior best ask.
    res.assert_event(&Event::new("wasm-rujira-fin/trade").add_attributes(vec![
        ("rate", "0.000006743076279641"),
        ("price", "ccl:0.000006743076279641"),
        ("offer", "1609"),
        ("bid", "238517510"),
        ("ranges", "2:0.0000025-0.000012:415761482489.877528544730489081:4366899.803568529390791933:238517510.122471455269510919:1608.180930609390791933::0.160834176478586937"),
        ("side", "base"),
    ]));

    res.assert_event(&Event::new("wasm-rujira-fin/trade").add_attributes(vec![
        ("rate", "0.000006910591347351"),
        ("price", "ccl:0.000006910591347351"),
        ("offer", "718460"),
        ("bid", "103964946031"),
        ("ranges", "2:0.0000025-0.000012:375814942130.285851436013565818:4642926.414312514578564072:39946540359.591677108716923263:276026.610743985187772139::27.605421616560174794,1:0.0000035-0.000012:602281594328.5326341593886499:5442360.799800418118705342:64018405671.4673658406113501:442360.799800418118705342::44.240504030444856356"),
        ("side", "base"),
    ]));

    res.assert_event(&Event::new("wasm-rujira-fin/trade").add_attributes(vec![
        ("rate", "0.000007256120914722"),
        ("price", "ccl:0.000007256120914722"),
        ("offer", "279931"),
        ("bid", "38578601885"),
        ("ranges", "2:0.0000025-0.000012:360991852848.214396265538192709:4750473.786660128737110471:14823089282.071455170475373109:107547.372347614158546399::10.755812816043020156,1:0.0000035-0.000012:578526081725.394896764460462759:5614716.434352803960158943:23755512603.137737394928187141:172355.634552385841453601::17.237287183956979843"),
        ("side", "base"),
    ]));

    // Updated transfers
    res.assert_event(&Event::new("transfer").add_attributes(vec![
        ("recipient", user_b.as_str()),
        ("sender", fin.addr().as_str()),
        ("amount", "140640334444rune"),
    ]));

    res.assert_event(&Event::new("transfer").add_attributes(vec![
        ("recipient", fees.as_str()),
        ("sender", fin.addr().as_str()),
        ("amount", "2141730982rune"),
    ]));

    let range_1 = fin.query_range(&app, &Uint128::one()).unwrap();
    let range_2 = fin.query_range(&app, &Uint128::new(2)).unwrap();

    assert!(range_1.bid < range_2.ask);
    assert!(range_1.ask > range_1.bid);

    assert_eq!(
        range_1.base,
        Decimal::from_str("578526081725.394896764460462759").unwrap()
    );
    assert_eq!(
        range_1.quote,
        Decimal::from_str("5614716.434352803960158943").unwrap()
    );

    assert_eq!(range_1.fees[0], Decimal::zero());
    assert_eq!(
        range_1.fees[1],
        Decimal::from_str("61.477791214401836199").unwrap()
    );

    assert_eq!(
        range_2.base,
        Decimal::from_str("360991852848.214396265538192709").unwrap()
    );
    assert_eq!(
        range_2.quote,
        Decimal::from_str("4750473.786660128737110471").unwrap()
    );

    assert_eq!(range_2.fees[0], Decimal::zero());
    assert_eq!(
        range_2.fees[1],
        Decimal::from_str("38.522068609081781887").unwrap()
    );

    fin.claim_range(&mut app, &user_b, &range_1.idx)
        .unwrap_err();
    let res = fin.claim_range(&mut app, &user_a, &range_1.idx).unwrap();
    res.assert_event(
        &Event::new("wasm-rujira-fin/range.claim").add_attributes(vec![
            ("owner", user_a.as_str()),
            ("idx", "1"),
            ("base", "0"),
            ("quote", "61"),
        ]),
    );

    res.assert_event(&Event::new("transfer").add_attributes(vec![
        ("recipient", user_a.as_str()),
        ("sender", fin.addr().as_str()),
        ("amount", "61btc-btc"),
    ]));

    let res = fin.close_range(&mut app, &user_a, &range_1.idx).unwrap();
    // Fees were already claimed above, so fee_base and fee_quote are 0
    res.assert_event(
        &Event::new("wasm-rujira-fin/range.close").add_attributes(vec![
            ("owner", user_a.as_str()),
            ("idx", "1"),
            ("base", "578526081725"),
            ("quote", "5614716"),
            ("fee_base", "0"),
            ("fee_quote", "0"),
        ]),
    );

    res.assert_event(&Event::new("transfer").add_attributes(vec![
        ("recipient", user_a.as_str()),
        ("sender", fin.addr().as_str()),
        ("amount", "5614716btc-btc,578526081725rune"),
    ]));

    let res = fin.close_range(&mut app, &user_a, &range_2.idx).unwrap();
    res.assert_event(
        &Event::new("wasm-rujira-fin/range.close").add_attributes(vec![
            ("owner", user_a.as_str()),
            ("idx", "2"),
            ("base", "360991852848"),
            ("quote", "4750473"),
            ("fee_base", "0"),
            ("fee_quote", "38"),
        ]),
    );

    res.assert_event(&Event::new("transfer").add_attributes(vec![
        ("recipient", user_a.as_str()),
        ("sender", fin.addr().as_str()),
        ("amount", "4750511btc-btc,360991852848rune"),
    ]));
}

#[test]
fn test_range_positive_skew() {
    let mut app = mock_rujira_app();
    let owner = app.api().addr_make("owner");
    let fees = app.api().addr_make("fees");
    let user_a = app.api().addr_make("user_a");

    app.init_modules(|router, _, storage| {
        let amount = vec![
            coin(1000000000000, "btc-btc"),
            coin(100000000000000000, "rune"),
        ];
        router.bank.init_balance(storage, &user_a, amount).unwrap();
    });

    let fin = Fin::create(
        &mut app,
        &owner,
        &fees,
        &[],
        Denoms::new("rune", "btc-btc"),
        None,
        Decimal::permille(0),
    );

    // Create range with skew=+1: concentrates liquidity toward the spread edge (near current price)
    // With positive skew, more liquidity is placed near the current price (bid/ask),
    // and less near the range limits (high/low bounds)
    fin.create_range(
        &mut app,
        &user_a,
        ("0.000012", "0.0000035"),
        "0.001",
        "0.0001",
        "1",
        &[coin(5000000, "btc-btc"), coin(666300000000, "rune")],
    )
    .unwrap();

    let mut book = fin.query_book(&app, None, None).unwrap();
    // Positive skew: base (ask) amounts are larger near the current price and decrease toward high
    // Quote (bid) amounts are larger near the current price and decrease toward low
    assert_book(
        &mut book,
        vec![
            ("0.000011967688684155", 8863372291),
            ("0.00001164993826316", 79178063808),
            ("0.000011095173067798", 77503599395),
            ("0.000010566825293992", 75875261215),
            ("0.000010063636982187", 74292080162),
            ("0.000009584410057121", 72753114006),
            ("0.000009128003505013", 71257446834),
            ("0.000008693330645899", 69804188498),
            ("0.000008279356542208", 68392474093),
            ("0.000007885095532065", 67021463438),
        ],
        vec![
            ("0.000007485372177368", 516257),
            ("0.000007111096805716", 480317),
            ("0.000006755535272595", 446981),
            ("0.000006417751881875", 416057),
            ("0.000006096857732367", 387367),
            ("0.000005792008366352", 360749),
            ("0.000005502401538882", 336049),
            ("0.00000522727513894", 313128),
            ("0.000004965905129708", 291854),
            ("0.000004717603710251", 272107),
            ("0.000004481717453845", 253775),
            ("0.000004257625585119", 236755),
            ("0.000004044738414816", 220950),
            ("0.000003842495688214", 206272),
            ("0.000003650365195422", 192638),
            ("0.000003528592099126", 58292),
        ],
    );
}

#[test]
fn test_range_negative_skew() {
    let mut app = mock_rujira_app();
    let owner = app.api().addr_make("owner");
    let fees = app.api().addr_make("fees");
    let user_a = app.api().addr_make("user_a");

    app.init_modules(|router, _, storage| {
        let amount = vec![
            coin(1000000000000, "btc-btc"),
            coin(100000000000000000, "rune"),
        ];
        router.bank.init_balance(storage, &user_a, amount).unwrap();
    });

    let fin = Fin::create(
        &mut app,
        &owner,
        &fees,
        &[],
        Denoms::new("rune", "btc-btc"),
        None,
        Decimal::permille(0),
    );

    // Create range with skew=-1: concentrates liquidity toward the limit edges (high/low bounds)
    // With negative skew, more liquidity is placed near the range limits (high/low),
    // and less near the current price (bid/ask)
    fin.create_range(
        &mut app,
        &user_a,
        ("0.000012", "0.0000035"),
        "0.001",
        "0.0001",
        "-1",
        &[coin(5000000, "btc-btc"), coin(666300000000, "rune")],
    )
    .unwrap();

    let mut book = fin.query_book(&app, None, None).unwrap();
    // Negative skew: base (ask) amounts increase away from current price toward high
    // Quote (bid) amounts are relatively flat, slightly increasing toward low
    assert_book(
        &mut book,
        vec![
            ("0.000011710090174485", 23298673283),
            ("0.000011157968734782", 27265347011),
            ("0.000010627274245703", 30858003663),
            ("0.000010121693725606", 34469025442),
            ("0.00000964007866039", 38100561445),
            ("0.000009181318475471", 41754772980),
            ("0.000008744345383616", 45433834849),
            ("0.000008328136178468", 49139936644),
            ("0.000007931712274775", 52875284051),
            ("0.000007554138931311", 56642100158),
            ("0.000007194524044194", 60442626785),
            ("0.000006852016635069", 64279125815),
            ("0.00000652580527155", 68153880537),
            ("0.000006215116472929", 72069197010),
        ],
        vec![
            ("0.000005899930606077", 473216),
            ("0.000005604987568084", 474387),
            ("0.000005324783922076", 474554),
            ("0.000005058583983189", 473815),
            ("0.000004805688608401", 472260),
            ("0.000004565433415278", 469971),
            ("0.000004337187094208", 467024),
            ("0.000004120349774826", 463488),
            ("0.000003914351492405", 459428),
            ("0.000003718650697457", 454903),
            ("0.000003561716016166", 307735),
        ],
    );
}

fn assert_book(res: &mut BookResponse, base: Vec<(&str, u128)>, quote: Vec<(&str, u128)>) {
    res.base.reverse();
    for (idx, b) in res.base.iter().enumerate() {
        let (p, a) = base[idx];
        assert_eq!(b.price, Decimal::from_str(p).unwrap());
        assert_eq!(b.total, Uint128::new(a));
    }
    for (idx, b) in res.quote.iter().enumerate() {
        let (p, a) = quote[idx];
        assert_eq!(b.price, Decimal::from_str(p).unwrap());
        assert_eq!(b.total, Uint128::new(a));
    }
}
