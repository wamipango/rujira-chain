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
            ("0.000011532086266528", 39998793197),
            ("0.000010982939301456", 50165580683),
            ("0.000010459942191863", 51404423541),
            ("0.000009961849706537", 52673859718),
            ("0.000009487475910988", 53974644718),
            ("0.000009035691343799", 55307552703),
            ("0.000008605420327428", 56673376954),
            ("0.000008195638407075", 58072930339),
            ("0.0000078053699115", 59507045802),
            ("0.00000743368563", 60976576856),
            ("0.0000070797006", 62482398092),
            ("0.000006742572", 64025405699),
        ],
        vec![
            ("0.0000067340719", 453274),
            ("0.000006397368305", 441797),
            ("0.00000607749988975", 430611),
            ("0.000005773624895263", 419707),
            ("0.0000054849436505", 409080),
            ("0.000005210696467975", 398722),
            ("0.000004950161644577", 388626),
            ("0.000004702653562349", 378786),
            ("0.000004467520884232", 369195),
            ("0.000004244144840021", 359847),
            ("0.00000403193759802", 350735),
            ("0.000003830340718119", 341854),
            ("0.000003638823682214", 253457),
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
            ("quote", "4371556"),
        ]),
    );

    res.assert_event(&Event::new("transfer").add_attributes(vec![
        ("recipient", user_a.as_str()),
        ("sender", fin.addr().as_str()),
        ("amount", "628443btc-btc"),
    ]));

    let mut book = fin.query_book(&app, None, None).unwrap();
    // New book that extends down to the new low range

    assert_book(
        &mut book,
        vec![
            ("0.000011533816616856", 64731708932),
            ("0.000010984587254149", 81489407288),
            ("0.000010461511670619", 83501794443),
            ("0.000009963344448209", 85563877652),
            ("0.000009488899474485", 87676884165),
            ("0.000009037047118558", 89842071535),
            ("0.000008606711541484", 92060728373),
            ("0.000008196868134747", 94334175112),
            ("0.000007806541080712", 96663764792),
            ("0.00000743480102925", 99050883867),
            ("0.000007080762885", 101496953031),
            ("0.0000067435837", 104003428061),
            ("0.000006742572", 199289263),
        ],
        vec![
            ("0.0000067340836", 9),
            ("0.0000067340719", 736358),
            ("0.000006397368305", 717713),
            ("0.00000607749988975", 699541),
            ("0.000005773624895263", 681828),
            ("0.0000054849436505", 664563),
            ("0.000005210696467975", 647736),
            ("0.000004950161644577", 631335),
            ("0.000004702653562349", 615349),
            ("0.000004467520884232", 599768),
            ("0.000004244144840021", 584582),
            ("0.00000403193759802", 569780),
            ("0.000003830340718119", 555353),
            ("0.000003638823682214", 411749),
            ("0.0000035", 204084),
            ("0.000003325", 198917),
            ("0.00000315875", 193880),
            ("0.0000030008125", 188971),
            ("0.000002850771875", 184186),
            ("0.00000270823328125", 179522),
            ("0.000002572821617188", 98499),
        ],
    );

    let res = fin
        .swap(&mut app, &user_b, 1000000, "btc-btc", None)
        .unwrap();
    // Now check trade has used 4 trades (and both ranges appear from trade #2 onwards)
    res.assert_event(&Event::new("wasm-rujira-fin/trade").add_attributes(vec![
        ("rate", "0.000006742572"),
        ("price", "ccl:0.000006742572"),
        ("offer", "1344"),
        ("bid", "199289263"),
        ("ranges", "1:0.0000035-0.000012:666100710736.201726123804042143:5001343.587837765856400462:199289263.798273876195957857:1343.587837765856400462::0.134372220998685508"),
        ("side", "base"),
    ]));

    res.assert_event(&Event::new("wasm-rujira-fin/trade").add_attributes(vec![
        ("rate", "0.0000067435837"),
        ("price", "ccl:0.0000067435837"),
        ("offer", "701356"),
        ("bid", "104003428061"),
        ("ranges", "1:0.0000035-0.000012:602080107893.894229456423685103:5433028.708799945205463921:64020602842.30749666738035704:431685.120962179349063459::43.172829379155850491,2:0.0000025-0.000012:376017174780.994086903019715913:4641156.751901354522043837:39982825219.005913096980284087:269600.565673994522043837::26.962752842683720576"),
        ("side", "base"),
    ]));

    res.assert_event(&Event::new("wasm-rujira-fin/trade").add_attributes(vec![
        ("rate", "0.000007080762885"),
        ("price", "ccl:0.000007080762885"),
        ("offer", "297300"),
        ("bid", "41987001235"),
        ("ranges", "1:0.0000035-0.000012:576234487410.637596802991721787:5616017.118386558769953578:25845620483.256632653431963316:182988.409586613564489657::18.300671025763932842,2:0.0000025-0.000012:359875794028.860530381941729129:4755438.612314740957554179:16141380752.133556521077986784:114281.860413386435510342::11.429328974236067157"),
        ("side", "base"),
    ]));

    // Updated transfers
    res.assert_event(&Event::new("transfer").add_attributes(vec![
        ("recipient", user_b.as_str()),
        ("sender", fin.addr().as_str()),
        ("amount", "143996872780rune"),
    ]));

    res.assert_event(&Event::new("transfer").add_attributes(vec![
        ("recipient", fees.as_str()),
        ("sender", fin.addr().as_str()),
        ("amount", "2192845779rune"),
    ]));

    let range_1 = fin.query_range(&app, &Uint128::one()).unwrap();
    let range_2 = fin.query_range(&app, &Uint128::new(2)).unwrap();

    assert!(range_1.bid < range_2.ask);
    assert!(range_1.ask > range_1.bid);

    assert_eq!(
        range_1.base,
        Decimal::from_str("576234487410.637596802991721787").unwrap()
    );
    assert_eq!(
        range_1.quote,
        Decimal::from_str("5616017.118386558769953578").unwrap()
    );

    assert_eq!(range_1.fees[0], Decimal::zero());
    assert_eq!(
        range_1.fees[1],
        Decimal::from_str("61.607872625918468841").unwrap()
    );

    assert_eq!(
        range_2.base,
        Decimal::from_str("359875794028.860530381941729129").unwrap()
    );
    assert_eq!(
        range_2.quote,
        Decimal::from_str("4755438.612314740957554179").unwrap()
    );

    assert_eq!(range_2.fees[0], Decimal::zero());
    assert_eq!(
        range_2.fees[1],
        Decimal::from_str("38.392081816919787733").unwrap()
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
            ("base", "576234487410"),
            ("quote", "5616017"),
            ("fee_base", "0"),
            ("fee_quote", "0"),
        ]),
    );

    res.assert_event(&Event::new("transfer").add_attributes(vec![
        ("recipient", user_a.as_str()),
        ("sender", fin.addr().as_str()),
        ("amount", "5616017btc-btc,576234487410rune"),
    ]));

    let res = fin.close_range(&mut app, &user_a, &range_2.idx).unwrap();
    res.assert_event(
        &Event::new("wasm-rujira-fin/range.close").add_attributes(vec![
            ("owner", user_a.as_str()),
            ("idx", "2"),
            ("base", "359875794028"),
            ("quote", "4755438"),
            ("fee_base", "0"),
            ("fee_quote", "38"),
        ]),
    );

    res.assert_event(&Event::new("transfer").add_attributes(vec![
        ("recipient", user_a.as_str()),
        ("sender", fin.addr().as_str()),
        ("amount", "4755476btc-btc,359875794028rune"),
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
            ("0.000011930178913938", 9582357222),
            ("0.000011362075156132", 79162810957),
            ("0.000010821023958221", 77488765371),
            ("0.000010305737103068", 75860837188),
            ("0.000009814987717208", 74278057548),
            ("0.000009347607349722", 72739484459),
            ("0.000008902483190212", 71244202242),
            ("0.00000847855541925", 69791320980),
            ("0.000008074814685", 68379975990),
            ("0.0000076902997", 67009327312),
        ],
        vec![
            ("0.0000076817996", 516589),
            ("0.00000729770962", 480625),
            ("0.000006932824139", 447266),
            ("0.00000658618293205", 416321),
            ("0.000006256873785448", 387613),
            ("0.000005944030096176", 360977),
            ("0.000005646828591368", 336260),
            ("0.0000053644871618", 313324),
            ("0.00000509626280371", 292036),
            ("0.000004841449663525", 272276),
            ("0.000004599377180349", 253932),
            ("0.000004369408321332", 236900),
            ("0.000004150937905266", 221085),
            ("0.000003943391010003", 206397),
            ("0.000003746221459503", 192755),
            ("0.000003558910386528", 59946),
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
            ("0.00001143826850006", 23309011053),
            ("0.000010893589047677", 27266801459),
            ("0.000010374846712074", 30859465129),
            ("0.000009880806392452", 34470494795),
            ("0.000009410291802336", 38102039561),
            ("0.000008962182668892", 41756260738),
            ("0.000008535412065612", 45435333135),
            ("0.000008128963872012", 49141446350),
            ("0.000007741870354298", 52876806074),
            ("0.000007373209861237", 56643635405),
            ("0.00000702210462975", 60444176169),
            ("0.000006687718695", 64280690258),
            ("0.0000063692559", 68155460971),
            ("0.000006065958", 72070794374),
        ],
        vec![
            ("0.0000060574579", 473197),
            ("0.000005754585005", 474380),
            ("0.00000546685575475", 474558),
            ("0.000005193512967013", 473829),
            ("0.000004933837318663", 472282),
            ("0.00000468714545273", 470001),
            ("0.000004452788180094", 467061),
            ("0.00000423014877109", 463531),
            ("0.000004018641332536", 459477),
            ("0.00000381770926591", 454956),
            ("0.000003626823802615", 312834),
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
