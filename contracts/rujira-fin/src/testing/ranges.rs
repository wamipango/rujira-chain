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
            ("0.000011762064468153", 40284180268),
            ("0.000011251002347983", 50172628339),
            ("0.000010715240331416", 51411645239),
            ("0.000010204990791822", 52681259756),
            ("0.000009719038849355", 53982227501),
            ("0.000009256227475577", 55315322744),
            ("0.000008815454738645", 56681338876),
            ("0.000008395671179664", 58081088881),
            ("0.000007995877313964", 59515405820),
            ("0.000007615121251391", 60985143325),
            ("0.000007252496429904", 62491176111),
            ("0.000006907139457045", 64034400491),
        ],
        vec![
            ("0.000006563433318363", 453270),
            ("0.000006235261652447", 441793),
            ("0.000005923498569827", 430606),
            ("0.000005627323641337", 419703),
            ("0.000005345957459271", 409076),
            ("0.000005078659586306", 398718),
            ("0.000004824726606983", 388622),
            ("0.000004583490276642", 378782),
            ("0.000004354315762808", 369191),
            ("0.000004136599974673", 359843),
            ("0.00000392976997593", 350732),
            ("0.000003733281477141", 341851),
            ("0.000003568702057628", 253329),
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
            ("quote", "4365112"),
        ]),
    );

    res.assert_event(&Event::new("transfer").add_attributes(vec![
        ("recipient", user_a.as_str()),
        ("sender", fin.addr().as_str()),
        ("amount", "634887btc-btc"),
    ]));

    let mut book = fin.query_book(&app, None, None).unwrap();
    // New book that extends down to the new low range

    assert_book(
        &mut book,
        vec![
            ("0.000011762064468153", 65420287126),
            ("0.000011251002347983", 81478826923),
            ("0.000010715240331416", 83490952795),
            ("0.000010204990791822", 85552768269),
            ("0.000009719038849355", 87665500434),
            ("0.000009256227475577", 89830406683),
            ("0.000008815454738645", 92048775456),
            ("0.000008395671179664", 94321927017),
            ("0.000007995877313964", 96651214229),
            ("0.000007615121251391", 99038023368),
            ("0.000007252496429904", 101483774941),
            ("0.000006907139457045", 103989924536),
            ("0.000006739665923786", 248938994),
        ],
        vec![
            ("0.000006732929524608", 2687),
            ("0.00000656146271148", 735987),
            ("0.000006233389575916", 717351),
            ("0.000005921720097117", 699187),
            ("0.000005625634092254", 681483),
            ("0.000005344352387647", 664228),
            ("0.000005077134768272", 647409),
            ("0.000004823278029848", 631016),
            ("0.000004582114128363", 615039),
            ("0.000004353008421938", 599466),
            ("0.000004135358000847", 584287),
            ("0.000003928590100807", 569492),
            ("0.000003732160595766", 555072),
            ("0.000003568166283399", 408190),
            ("0.000003411378020685", 203901),
            ("0.000003240809119647", 198738),
            ("0.000003078768663665", 193706),
            ("0.000002924830230485", 188801),
            ("0.000002778588718951", 184021),
            ("0.000002639659283007", 179361),
            ("0.000002536149452021", 98411),
        ],
    );

    let res = fin
        .swap(&mut app, &user_b, 1000000, "btc-btc", None)
        .unwrap();
    // Effective range pricing now inserts a small leading level before the prior best ask.
    res.assert_event(&Event::new("wasm-rujira-fin/trade").add_attributes(vec![
        ("rate", "0.000006739665923786"),
        ("price", "ccl:0.000006739665923786"),
        ("offer", "1678"),
        ("bid", "248938994"),
        (
            "ranges",
            "2:0.0000025-0.000012:415751061006:4366790.729660096:248938994:1677.9161::0.0839",
        ),
        ("side", "base"),
    ]));

    res.assert_event(&Event::new("wasm-rujira-fin/trade").add_attributes(vec![
        ("rate", "0.000006907139457045"),
        ("price", "ccl:0.000006907139457045"),
        ("offer", "718273"),
        ("bid", "103989930049"),
        ("ranges", "2:0.0000025-0.000012:375795531448.590681320139518603:4642755.365075878152825647:39955529557.409318679860481397:275964.635415782152825647::13.798921716874951388,1:0.0000035-0.000012:602265599508.409318679860481397:5442272.450934217847174354:64034400491.590681320139518603:442272.450934217847174354::22.114728283125048611"),
        ("side", "base"),
    ]));

    res.assert_event(&Event::new("wasm-rujira-fin/trade").add_attributes(vec![
        ("rate", "0.000007252496429904"),
        ("price", "ccl:0.000007252496429904"),
        ("offer", "280049"),
        ("bid", "38614152065"),
        ("ranges", "2:0.0000025-0.000012:360959041789.221416253751098009:4750351.573285197858818558:14836489659.369265066388420594:107596.208209319705992911::5.380079414436707135,1:0.0000035-0.000012:578487937102.778583746248901991:5614711.240274898141181444:23777662405.630734933611579406:172438.78934068029400709::8.622370585563292864"),
        ("side", "base"),
    ]));

    res.assert_event(&Event::new("transfer").add_attributes(vec![
        ("recipient", user_b.as_str()),
        ("sender", fin.addr().as_str()),
        ("amount", "140710225791rune"),
    ]));

    res.assert_event(&Event::new("transfer").add_attributes(vec![
        ("recipient", fees.as_str()),
        ("sender", fin.addr().as_str()),
        ("amount", "2142795317rune"),
    ]));

    let range_1 = fin.query_range(&app, &Uint128::one()).unwrap();
    let range_2 = fin.query_range(&app, &Uint128::new(2)).unwrap();

    assert!(range_1.bid < range_2.ask);
    assert!(range_1.ask > range_1.bid);

    assert_eq!(
        range_1.base,
        Decimal::from_str("578487937102.778583746248901991").unwrap()
    );
    // quote updated due to halved range_fee — more goes to reserves
    assert_eq!(
        range_1.quote,
        Decimal::from_str("5614711.240274898141181444").unwrap()
    );

    assert_eq!(range_1.fees[0], Decimal::zero());
    assert_eq!(
        range_1.fees[1],
        Decimal::from_str("30.737098868688341475").unwrap()
    );

    assert_eq!(
        range_2.base,
        Decimal::from_str("360959041789.221416253751098009").unwrap()
    );
    assert_eq!(
        range_2.quote,
        Decimal::from_str("4750351.573285197858818558").unwrap()
    );

    assert_eq!(range_2.fees[0], Decimal::zero());
    assert_eq!(
        range_2.fees[1],
        Decimal::from_str("19.262901131311658523").unwrap()
    );

    fin.claim_range(&mut app, &user_b, &range_1.idx)
        .unwrap_err();
    let res = fin.claim_range(&mut app, &user_a, &range_1.idx).unwrap();
    res.assert_event(
        &Event::new("wasm-rujira-fin/range.claim").add_attributes(vec![
            ("owner", user_a.as_str()),
            ("idx", "1"),
            ("base", "0"),
            ("quote", "30"),
        ]),
    );

    res.assert_event(&Event::new("transfer").add_attributes(vec![
        ("recipient", user_a.as_str()),
        ("sender", fin.addr().as_str()),
        ("amount", "30btc-btc"),
    ]));

    let res = fin.close_range(&mut app, &user_a, &range_1.idx).unwrap();
    // Fees were already claimed above, so fee_base and fee_quote are 0
    res.assert_event(
        &Event::new("wasm-rujira-fin/range.close").add_attributes(vec![
            ("owner", user_a.as_str()),
            ("idx", "1"),
            ("base", "578487937102"),
            ("quote", "5614711"),
            ("fee_base", "0"),
            ("fee_quote", "0"),
        ]),
    );

    res.assert_event(&Event::new("transfer").add_attributes(vec![
        ("recipient", user_a.as_str()),
        ("sender", fin.addr().as_str()),
        ("amount", "5614711btc-btc,578487937102rune"),
    ]));

    let res = fin.close_range(&mut app, &user_a, &range_2.idx).unwrap();
    res.assert_event(
        &Event::new("wasm-rujira-fin/range.close").add_attributes(vec![
            ("owner", user_a.as_str()),
            ("idx", "2"),
            ("base", "360959041789"),
            ("quote", "4750351"),
            ("fee_base", "0"),
            ("fee_quote", "19"),
        ]),
    );

    res.assert_event(&Event::new("transfer").add_attributes(vec![
        ("recipient", user_a.as_str()),
        ("sender", fin.addr().as_str()),
        ("amount", "4750370btc-btc,360959041789rune"),
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
    assert_book(
        &mut book,
        vec![
            ("0.000011964704659884", 9682823166),
            ("0.000011644119039658", 79160679698),
            ("0.000011089630946209", 77486692636),
            ("0.000010561547083056", 75858821743),
            ("0.000010058610118468", 74276098194),
            ("0.000009579622567129", 72737580030),
            ("0.000009123443989591", 71242351604),
            ("0.000008688988252226", 69789523032),
            ("0.000008275220926996", 68378229662),
            ("0.000007881156855634", 67007631564),
        ],
        vec![
            ("0.000007489118684758", 516622),
            ("0.000007114655989874", 480655),
            ("0.000006758916491407", 447294),
            ("0.000006420964044862", 416347),
            ("0.000006099909285542", 387637),
            ("0.000005794907337109", 360999),
            ("0.000005505155564733", 336281),
            ("0.000005229891452537", 313343),
            ("0.000004968390641109", 292054),
            ("0.000004719964939232", 272292),
            ("0.000004483960615018", 253947),
            ("0.00000425975659649", 236915),
            ("0.000004046762869497", 221098),
            ("0.000003844418918039", 206410),
            ("0.00000365219226667", 192766),
            ("0.000003529479027207", 60108),
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
    assert_book(
        &mut book,
        vec![
            ("0.000011707049092047", 23559727289),
            ("0.000011152403079275", 27302050864),
            ("0.000010621971698586", 30894884726),
            ("0.000010116642332981", 34506105665),
            ("0.000009635266885282", 38137862896),
            ("0.000009176735127757", 41792317860),
            ("0.000008739979780992", 45471645502),
            ("0.000008323978063", 49178035574),
            ("0.000007927751851559", 52913693931),
            ("0.000007550366860196", 56680843849),
            ("0.000007190931405869", 60481727345),
            ("0.000006848594914423", 64318606514),
            ("0.000006522546363328", 68193764872),
            ("0.000006212012647788", 72109508718),
        ],
        vec![
            ("0.000005902882936847", 473199),
            ("0.000005607792361196", 474381),
            ("0.000005327448549384", 474557),
            ("0.000005061115435776", 473827),
            ("0.000004808093536739", 472279),
            ("0.000004567718135591", 469997),
            ("0.000004339357615148", 467056),
            ("0.00000412241180044", 463526),
            ("0.000003916310442462", 459470),
            ("0.00000372051172157", 454949),
            ("0.000003562604245684", 312150),
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
