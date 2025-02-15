use std::str::FromStr;

use crate::contract::{execute, instantiate, query};
use cosmwasm_std::{coin, coins, Addr, Decimal, Event, Uint128, Uint256};
use cw_multi_test::{ContractWrapper, Executor};
use rujira_bow::contract as bow_contract;
use rujira_rs::{
    bow::{self, Xyk},
    fin::{
        BookItemResponse, BookResponse, Denoms, ExecuteMsg, InstantiateMsg, OrderResponse,
        OrdersResponse, Price, QueryMsg, Side, SwapRequest, Tick,
    },
    Chain, Layer1Asset, TokenMetadata,
};
use rujira_rs_testing::{mock_rujira_app, RujiraApp};

fn setup(app: &mut RujiraApp, owner: &Addr, fees: &Addr) -> (Addr, Addr) {
    let fin_code = Box::new(ContractWrapper::new(execute, instantiate, query));
    let bow_code = Box::new(ContractWrapper::new(
        bow_contract::execute,
        bow_contract::instantiate,
        bow_contract::query,
    ));
    let fin_code_id = app.store_code(fin_code);
    let bow_code_id = app.store_code(bow_code);

    let min_k = Uint256::from(10u128).pow(19);
    let bow = app
        .instantiate_contract(
            bow_code_id,
            owner.clone(),
            &bow::InstantiateMsg {
                metadata: TokenMetadata {
                    description: "".to_string(),
                    display: "".to_string(),
                    name: "".to_string(),
                    symbol: "".to_string(),
                    uri: None,
                    uri_hash: None,
                },
                strategy: bow::Strategies::Xyk(Xyk::new(
                    "btc".to_string(),
                    "usdc".to_string(),
                    Decimal::from_ratio(1u128, 1000u128),
                    // Requires at least 1 BTC @ 100k, BTC - 8dp, USDC - 6dp
                    min_k,
                )),
            },
            &[],
            "bow",
            None,
        )
        .unwrap();

    let fin = app
        .instantiate_contract(
            fin_code_id,
            owner.clone(),
            &InstantiateMsg {
                denoms: Denoms::new("btc", "usdc"),
                oracles: Some([
                    Layer1Asset::new(Chain::Btc, "BTC"),
                    Layer1Asset::new(
                        Chain::Eth,
                        "USDC-0XA0B86991C6218B36C1D19D4A2E9EB0CE3606EB48",
                    ),
                ]),
                market_maker: Some(bow.to_string()),
                tick: Tick::new(8),
                fee_taker: Decimal::from_ratio(15u128, 1000u128),
                fee_maker: Decimal::from_ratio(30u128, 1000u128),
                fee_address: fees.to_string(),
            },
            &[],
            "fin",
            None,
        )
        .unwrap();
    (fin, bow)
}

#[test]
fn mm_book() {
    let mut app = mock_rujira_app();
    let owner = app.api().addr_make("owner");
    let fees = app.api().addr_make("fees");
    let (fin, bow) = setup(&mut app, &owner, &fees);
    let book: BookResponse = app
        .wrap()
        .query_wasm_smart(
            fin.clone(),
            &QueryMsg::Book {
                limit: None,
                offset: None,
            },
        )
        .unwrap();

    assert_eq!(book.base.len(), 0);
    assert_eq!(book.quote.len(), 0);

    app.init_modules(|router, _, storage| {
        router.bank.init_balance(
            storage,
            &owner,
            vec![
                coin(1_000_000_000_000, "btc"),
                coin(1_000_000_000_000, "usdc"),
            ],
        )
    })
    .unwrap();

    // First deposit
    app.execute_contract(
        owner.clone(),
        bow.clone(),
        &bow::ExecuteMsg::Deposit { callback: None },
        // 2 BTC @ 100k USDC
        &[coin(200_000_000, "btc"), coin(200_000_000_000, "usdc")],
    )
    .unwrap();

    let book: BookResponse = app
        .wrap()
        .query_wasm_smart(
            fin.clone(),
            &QueryMsg::Book {
                limit: None,
                offset: None,
            },
        )
        .unwrap();

    assert_eq!(book.base.len(), 100);
    assert_eq!(book.quote.len(), 100);

    // Just assert spread and direction
    // Quoting accuracy tested in Strategy

    assert_side(
        book.quote,
        vec![
            // 2 decimal delta, so 999 price = 99,900
            ("999.00099", 199_800_199u128),
            ("997.00698", 199_400_400u128),
            ("995.01895", 199_000_806u128),
        ],
    );

    assert_side(
        book.base,
        vec![
            ("1001.0011", 199_800u128),
            ("1003.0061", 199_201u128),
            ("1005.0018", 197_810u128),
        ],
    );

    let res = app
        .execute_contract(
            owner.clone(),
            fin.clone(),
            &ExecuteMsg::Swap(SwapRequest {
                min_return: None,
                to: None,
                callback: None,
            }),
            &coins(100_000_000, "usdc"),
        )
        .unwrap();

    // Single trade in first MM pool
    res.assert_event(&Event::new("wasm-rujira-fin/trade").add_attributes(vec![
        ("rate", "1001.0011"),
        ("price", "mm:1001.001001001001001001"),
        ("offer", "100000000"),
        ("bid", "99900"),
        ("side", "base"),
    ]));

    // BOW returns the BTC for the trade
    res.assert_event(&Event::new("transfer").add_attributes(vec![
        ("recipient", fin.to_string()),
        ("sender", bow.to_string()),
        ("amount", "99900btc".to_string()),
    ]));

    // Returns the amount less fees to the buyer
    res.assert_event(&Event::new("transfer").add_attributes(vec![
        ("recipient", owner.to_string()),
        ("sender", fin.to_string()),
        ("amount", "98401btc".to_string()),
    ]));

    // And the fees to the fee address
    res.assert_event(&Event::new("transfer").add_attributes(vec![
        ("recipient", fees.to_string()),
        ("sender", fin.to_string()),
        ("amount", "1499btc".to_string()),
    ]));

    let balances = app.wrap().query_all_balances(bow.clone()).unwrap();
    assert_eq!(
        balances,
        vec![coin(199900100, "btc"), coin(200100000000, "usdc"),]
    );

    let book: BookResponse = app
        .wrap()
        .query_wasm_smart(
            fin.clone(),
            &QueryMsg::Book {
                limit: Some(10),
                offset: None,
            },
        )
        .unwrap();

    assert_eq!(book.base.len(), 10);
    assert_eq!(book.quote.len(), 10);
    assert_side(
        book.quote,
        vec![
            // 2 decimal delta, so 999 price = 99,900
            ("1000.0000", 199_900_000u128),
            ("0998.00399", 199_500_998u128),
            ("0996.01395", 199_102_194u128),
        ],
    );

    assert_side(
        book.base,
        vec![
            ("1002.0031", 199_700u128),
            ("1004.0080", 198_902u128),
            ("1006.0017", 197_515u128),
        ],
    );

    // Larger swap, opposite direction
    let res = app
        .execute_contract(
            owner.clone(),
            fin.clone(),
            &ExecuteMsg::Swap(SwapRequest {
                min_return: None,
                to: None,
                callback: None,
            }),
            &coins(500_000, "btc"),
        )
        .unwrap();
    dbg!(&res);

    // 3 trade events
    res.assert_event(&Event::new("wasm-rujira-fin/trade").add_attributes(vec![
        ("rate", "1000"),
        ("price", "mm:1000"),
        ("offer", "199900"),
        ("bid", "199900000"),
        ("side", "quote"),
    ]));

    res.assert_event(&Event::new("wasm-rujira-fin/trade").add_attributes(vec![
        ("rate", "998.00399"),
        ("price", "mm:998.003991995997998999"),
        ("offer", "199900"),
        ("bid", "199500998"),
        ("side", "quote"),
    ]));
    res.assert_event(&Event::new("wasm-rujira-fin/trade").add_attributes(vec![
        ("rate", "996.01395"),
        ("price", "mm:996.013957048309396245"),
        ("offer", "100200"),
        ("bid", "99800598"),
        ("side", "quote"),
    ]));

    // BOW returns the USDC for the trade
    res.assert_event(&Event::new("transfer").add_attributes(vec![
        ("recipient", fin.to_string()),
        ("sender", bow.to_string()),
        ("amount", "499201596usdc".to_string()),
    ]));

    // Returns the amount less fees to the buyer
    res.assert_event(&Event::new("transfer").add_attributes(vec![
        ("recipient", owner.to_string()),
        ("sender", fin.to_string()),
        ("amount", "491713572usdc".to_string()),
    ]));

    // And the fees to the fee address
    res.assert_event(&Event::new("transfer").add_attributes(vec![
        ("recipient", fees.to_string()),
        ("sender", fin.to_string()),
        ("amount", "7488024usdc".to_string()),
    ]));

    // Check large in first dfirection succeeds
    app.execute_contract(
        owner.clone(),
        fin.clone(),
        &ExecuteMsg::Swap(SwapRequest {
            min_return: None,
            to: None,
            callback: None,
        }),
        &coins(600_000_000, "usdc"),
    )
    .unwrap();
}

#[test]
fn combined_book() {
    let mut app = mock_rujira_app();
    let owner = app.api().addr_make("owner");
    let fees = app.api().addr_make("fees");
    let (fin, bow) = setup(&mut app, &owner, &fees);
    app.init_modules(|router, _, storage| {
        router.bank.init_balance(
            storage,
            &owner,
            vec![
                coin(1_000_000_000_00, "btc"),
                coin(1_000_000_000_0000, "usdc"),
            ],
        )
    })
    .unwrap();

    app.execute_contract(
        owner.clone(),
        bow.clone(),
        &bow::ExecuteMsg::Deposit { callback: None },
        // 2 BTC @ 100k USDC
        &[coin(200_000_00, "btc"), coin(200_000_000_0000, "usdc")],
    )
    .unwrap();

    let book: BookResponse = app
        .wrap()
        .query_wasm_smart(
            fin.clone(),
            &QueryMsg::Book {
                limit: Some(10),
                offset: None,
            },
        )
        .unwrap();

    assert_side(
        book.quote,
        vec![
            ("99900.099", 1_998_001_998u128),
            ("99700.703", 1_993_914_368u128),
            ("99501.913", 1_989_839_263u128),
        ],
    );
    // Now let's add some user orders

    app.execute_contract(
        owner.clone(),
        fin.clone(),
        &ExecuteMsg::Order((
            vec![
                (
                    Side::Quote,
                    Price::Fixed(Decimal::from_str("99700.703").unwrap()),
                    Uint128::from(50_000_000u128),
                ),
                (
                    Side::Quote,
                    Price::Fixed(Decimal::from_str("99600").unwrap()),
                    Uint128::from(50_000_000u128),
                ),
                (Side::Quote, Price::Oracle(0), Uint128::from(50_000_000u128)),
            ],
            None,
        )),
        &coins(150_000_000, "usdc"),
    )
    .unwrap();

    let book: BookResponse = app
        .wrap()
        .query_wasm_smart(
            fin.clone(),
            &QueryMsg::Book {
                limit: Some(5),
                offset: None,
            },
        )
        .unwrap();
    assert_side(
        book.quote,
        vec![
            ("100000", 50_000_000u128),
            ("99900.099", 1_998_001_998u128),
            ("99700.703", 2_043_914_368u128),
            ("99600", 50_000_000u128),
            ("99501.913", 1_989_839_263u128),
        ],
    );

    // Swap across them all
    let res = app
        .execute_contract(
            owner.clone(),
            fin.clone(),
            &ExecuteMsg::Swap(SwapRequest {
                min_return: None,
                to: None,
                callback: None,
            }),
            &coins(45_500, "btc"),
        )
        .unwrap();

    res.assert_event(&Event::new("wasm-rujira-fin/trade").add_attributes(vec![
        ("rate", "100000"),
        ("price", "oracle:0"),
        ("offer", "500"),
        ("bid", "50000000"),
        ("side", "quote"),
    ]));

    res.assert_event(&Event::new("wasm-rujira-fin/trade").add_attributes(vec![
        ("rate", "99900.099"),
        ("price", "mm:99900.0999"),
        ("offer", "20000"),
        ("bid", "1998001998"),
        ("side", "quote"),
    ]));

    res.assert_event(&Event::new("wasm-rujira-fin/trade").add_attributes(vec![
        ("rate", "99700.703"),
        ("offer", "20501"),
        ("bid", "2043914368"),
        ("price", "fixed:99700.703"),
        ("price", "mm:99700.703435171758587929"),
        ("side", "quote"),
    ]));
    res.assert_event(&Event::new("wasm-rujira-fin/trade").add_attributes(vec![
        ("rate", "99600"),
        ("price", "fixed:99600"),
        ("offer", "503"),
        ("bid", "50000000"),
        ("side", "quote"),
    ]));
    res.assert_event(&Event::new("wasm-rujira-fin/trade").add_attributes(vec![
        ("rate", "99501.913"),
        ("price", "mm:99501.913341334133413341"),
        ("offer", "3996"),
        ("bid", "397609645"),
        ("side", "quote"),
    ]));

    // Check orders and withdraw
    let order: OrderResponse = app
        .wrap()
        .query_wasm_smart(
            fin.to_string(),
            &QueryMsg::Order((
                owner.to_string(),
                Side::Quote,
                Price::Fixed(Decimal::from_str("99600").unwrap()),
            )),
        )
        .unwrap();
    assert_eq!(order.filled, Uint128::from(503u128));
    assert_eq!(order.remaining, Uint128::zero());
    let orders: OrdersResponse = app
        .wrap()
        .query_wasm_smart(
            fin.to_string(),
            &QueryMsg::Orders {
                owner: owner.to_string(),
                side: None,
                offset: None,
                limit: None,
            },
        )
        .unwrap();
    assert_eq!(orders.orders[0].clone(), order);

    let res = app
        .execute_contract(
            owner.clone(),
            fin.clone(),
            &ExecuteMsg::Order((
                vec![(
                    Side::Quote,
                    Price::Fixed(Decimal::from_str("99600").unwrap()),
                    Uint128::zero(),
                )],
                None,
            )),
            &coins(45_500, "btc"),
        )
        .unwrap();

    dbg!(&res);

    res.assert_event(
        &Event::new("wasm-rujira-fin/order.withdraw").add_attributes(vec![
            ("owner", owner.as_str()),
            ("side", "quote"),
            ("price", "fixed:99600"),
            ("amount", "503"),
        ]),
    );

    res.assert_event(&Event::new("transfer").add_attributes(vec![
        ("recipient", owner.as_str()),
        ("sender", fin.as_str()),
        ("amount", "45987btc"),
    ]));

    res.assert_event(&Event::new("transfer").add_attributes(vec![
        ("recipient", fees.as_str()),
        ("sender", fin.as_str()),
        ("amount", "16btc"),
    ]));
}

#[test]
fn test_arbitrage_quote_single() {
    // Test arbitrage with a single match
    let mut app = mock_rujira_app();
    let owner = app.api().addr_make("owner");
    let fees = app.api().addr_make("fees");
    let (fin, bow) = setup(&mut app, &owner, &fees);
    app.init_modules(|router, _, storage| {
        router.bank.init_balance(
            storage,
            &owner,
            vec![
                coin(1_000_000_000_000, "btc"),
                coin(1_000_000_000_000, "usdc"),
            ],
        )
    })
    .unwrap();

    app.execute_contract(
        owner.clone(),
        bow.clone(),
        &bow::ExecuteMsg::Deposit { callback: None },
        // 2 BTC @ 100k USDC
        &[coin(200_000_000, "btc"), coin(200_000_000_000, "usdc")],
    )
    .unwrap();

    let book: BookResponse = app
        .wrap()
        .query_wasm_smart(
            fin.clone(),
            &QueryMsg::Book {
                limit: Some(10),
                offset: None,
            },
        )
        .unwrap();

    assert_side(
        book.quote,
        vec![
            ("999.00099", 199_800_199u128),
            ("997.00698", 199_400_400u128),
            ("995.01895", 199_000_806u128),
        ],
    );

    // Here we have an arbitrage profit that's captured in USDC, but an local orderbook that only has a
    // single BTC in local balances - no USDC
    // Options:
    // 1: Execute the mm swap for arbitrage and send fees in a callback
    // 2: Assume there is a float on the contract (including balances from user orders) and just check that the Arb MM swap is executed correctly
    // 3: Ensure arbitrage profit is captured in the local token, also ensure arb swap is executed

    // We have a maker maker order buying with
    // 199_800_199 at 999.00099  and a user order selling
    // 10_000 at 998

    // Buy 10000 BTC off the market maker for 9990009 USDC
    // Sell 10000 BTC to the buyer for 9,980,000 USDC
    // Profit 10,009 USDC
    // Request the full 10000 swap from the market maker
    // in exchange for 9990010 USDC - sell order value (9,980,000) + profit 10,009
    let res = app
        .execute_contract(
            owner.clone(),
            fin.clone(),
            &ExecuteMsg::Order((
                vec![(
                    Side::Base,
                    Price::Fixed(Decimal::from_str("998").unwrap()),
                    Uint128::from(10_000u128),
                )],
                None,
            )),
            &coins(10_000, "btc"),
        )
        .unwrap();

    // Ensure BOW has returned the funds
    res.assert_event(&Event::new("transfer").add_attributes(vec![
        ("recipient", fin.to_string()),
        ("sender", bow.to_string()),
        ("amount", "9990009usdc".to_string()),
    ]));

    // Ensure arb fees are sent
    res.assert_event(&Event::new("transfer").add_attributes(vec![
        ("recipient", fees.as_str()),
        ("sender", fin.as_str()),
        ("amount", "10008usdc"),
    ]));
}

#[test]
fn test_arbitrage_quote_multi() {
    // Now execute in the opposite direction, consuming:
    // - multiple market maker pools with a single order
    // - a single market maker pool with multiple orders

    let mut app = mock_rujira_app();
    let owner = app.api().addr_make("owner");
    let fees = app.api().addr_make("fees");
    let (fin, bow) = setup(&mut app, &owner, &fees);
    app.init_modules(|router, _, storage| {
        router.bank.init_balance(
            storage,
            &owner,
            vec![
                coin(1_000_000_000_000, "btc"),
                coin(1_000_000_000_000, "usdc"),
            ],
        )
    })
    .unwrap();

    app.execute_contract(
        owner.clone(),
        bow.clone(),
        &bow::ExecuteMsg::Deposit { callback: None },
        // 2 BTC @ 100k USDC
        &[coin(200_000_000, "btc"), coin(200_000_000_000, "usdc")],
    )
    .unwrap();

    let book: BookResponse = app
        .wrap()
        .query_wasm_smart(
            fin.clone(),
            &QueryMsg::Book {
                limit: Some(10),
                offset: None,
            },
        )
        .unwrap();

    assert_side(
        book.base,
        vec![
            ("1001.0011", 199_800u128),
            ("1003.0061", 199_201u128),
            ("1005.0018", 197_810u128),
            ("1006.9887", 196_667u128),
            ("1008.981", 196_174u128),
            ("1010.9624", 194_793u128),
        ],
    );

    let res = app
        .execute_contract(
            owner.clone(),
            fin.clone(),
            &ExecuteMsg::Order((
                vec![(
                    Side::Quote,
                    Price::Fixed(Decimal::from_str("1004").unwrap()),
                    Uint128::from(600_000_000u128),
                )],
                None,
            )),
            &coins(600_000_000u128, "usdc"),
        )
        .unwrap();

    // Ensure BOW has returned the funds
    res.assert_event(&Event::new("transfer").add_attributes(vec![
        ("recipient", fin.to_string()),
        ("sender", bow.to_string()),
        ("amount", "399001btc".to_string()),
    ]));

    // Ensure arb fees are sent
    res.assert_event(&Event::new("transfer").add_attributes(vec![
        ("recipient", fees.as_str()),
        ("sender", fin.as_str()),
        ("amount", "797205usdc"),
    ]));

    // Now a small one that should take profit in BTC

    let book: BookResponse = app
        .wrap()
        .query_wasm_smart(
            fin.clone(),
            &QueryMsg::Book {
                limit: Some(10),
                offset: None,
            },
        )
        .unwrap();

    assert_side(book.base, vec![("1005.009", 199_401u128)]);

    let res = app
        .execute_contract(
            owner.clone(),
            fin.clone(),
            &ExecuteMsg::Order((
                vec![(
                    Side::Quote,
                    Price::Fixed(Decimal::from_str("1006").unwrap()),
                    Uint128::from(10_000_000u128),
                )],
                None,
            )),
            &coins(10_000_000u128, "usdc"),
        )
        .unwrap();

    // Ensure BOW has returned the funds
    res.assert_event(&Event::new("transfer").add_attributes(vec![
        ("recipient", fin.to_string()),
        ("sender", bow.to_string()),
        ("amount", "9950btc".to_string()),
    ]));

    // Ensure arb fees are sent
    res.assert_event(&Event::new("transfer").add_attributes(vec![
        ("recipient", fees.as_str()),
        ("sender", fin.as_str()),
        ("amount", "9btc"),
    ]));
}

fn assert_side(side: Vec<BookItemResponse>, items: Vec<(&str, u128)>) {
    assert!(side.len().ge(&items.len()));
    for ((price, amount), item) in items.iter().zip(side) {
        let price = Decimal::from_str(price).unwrap();
        let amount = Uint128::from(*amount);
        assert_eq!(price, item.price);
        assert_eq!(amount, item.total);
    }
}
