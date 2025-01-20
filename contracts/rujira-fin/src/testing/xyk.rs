use std::str::FromStr;

use crate::contract::{execute, instantiate, query};
use cosmwasm_std::{coin, coins, Addr, Decimal, Event, Uint128, Uint256};
use cw_multi_test::{ContractWrapper, Executor};
use rujira_bow::contract as bow_contract;
use rujira_rs::{
    bow::{self, Xyk},
    fin::{
        BookItemResponse, BookResponse, Denoms, ExecuteMsg, InstantiateMsg, OrderResponse,
        OrdersResponse, Price, QueryMsg, Side, Tick,
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
                    exponent: 6,
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
        &bow::ExecuteMsg::Deposit {},
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
            &ExecuteMsg::Swap {
                min_return: None,
                to: None,
                callback: None,
            },
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
            &ExecuteMsg::Swap {
                min_return: None,
                to: None,
                callback: None,
            },
            &coins(500_000, "btc"),
        )
        .unwrap();

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
        ("bid", "99800599"),
        ("side", "quote"),
    ]));

    // BOW returns the USDC for the trade
    res.assert_event(&Event::new("transfer").add_attributes(vec![
        ("recipient", fin.to_string()),
        ("sender", bow.to_string()),
        ("amount", "499201597usdc".to_string()),
    ]));

    // Returns the amount less fees to the buyer
    res.assert_event(&Event::new("transfer").add_attributes(vec![
        ("recipient", owner.to_string()),
        ("sender", fin.to_string()),
        ("amount", "491713573usdc".to_string()),
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
        &ExecuteMsg::Swap {
            min_return: None,
            to: None,
            callback: None,
        },
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
                coin(1_000_000_000_000, "btc"),
                coin(1_000_000_000_000, "usdc"),
            ],
        )
    })
    .unwrap();

    app.execute_contract(
        owner.clone(),
        bow.clone(),
        &bow::ExecuteMsg::Deposit {},
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
    // Now let's add some user orders

    app.execute_contract(
        owner.clone(),
        fin.clone(),
        &ExecuteMsg::Order(vec![
            (
                Side::Quote,
                Price::Fixed(Decimal::from_str("997.00698").unwrap()),
                Uint128::from(5_000_000u128),
            ),
            (
                Side::Quote,
                Price::Fixed(Decimal::from_str("996").unwrap()),
                Uint128::from(5_000_000u128),
            ),
            (Side::Quote, Price::Oracle(0), Uint128::from(5_000_000u128)),
        ]),
        &coins(15_000_000, "usdc"),
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
            ("1000", 5_000_000u128),
            ("999.00099", 199_800_199u128),
            ("997.00698", 204_400_400u128),
            ("996", 5_000_000u128),
            ("995.01895", 199_000_806u128),
        ],
    );

    // Swap across them all
    let res = app
        .execute_contract(
            owner.clone(),
            fin.clone(),
            &ExecuteMsg::Swap {
                min_return: None,
                to: None,
                callback: None,
            },
            &coins(455_000, "btc"),
        )
        .unwrap();

    res.assert_event(&Event::new("wasm-rujira-fin/trade").add_attributes(vec![
        ("rate", "1000"),
        ("price", "oracle:0"),
        ("offer", "5000"),
        ("bid", "5000000"),
        ("side", "quote"),
    ]));

    res.assert_event(&Event::new("wasm-rujira-fin/trade").add_attributes(vec![
        ("rate", "999.00099"),
        ("price", "mm:999.000995"),
        ("offer", "200000"),
        ("bid", "199800199"),
        ("side", "quote"),
    ]));

    res.assert_event(&Event::new("wasm-rujira-fin/trade").add_attributes(vec![
        ("rate", "997.00698"),
        ("offer", "205015"),
        ("bid", "204400400"),
        ("price", "fixed:997.00698"),
        ("price", "mm:997.006985034925174625"),
        ("side", "quote"),
    ]));
    res.assert_event(&Event::new("wasm-rujira-fin/trade").add_attributes(vec![
        ("rate", "996"),
        ("price", "fixed:996"),
        ("offer", "5021"),
        ("bid", "5000000"),
        ("side", "quote"),
    ]));
    res.assert_event(&Event::new("wasm-rujira-fin/trade").add_attributes(vec![
        ("rate", "995.01895"),
        ("price", "mm:995.018955284329264938"),
        ("offer", "39964"),
        ("bid", "39764938"),
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
                Price::Fixed(Decimal::from_str("996").unwrap()),
            )),
        )
        .unwrap();
    assert_eq!(order.filled, Uint128::from(5020u128));
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
            &ExecuteMsg::Order(vec![(
                Side::Quote,
                Price::Fixed(Decimal::from_str("996").unwrap()),
                Uint128::zero(),
            )]),
            &coins(455_000, "btc"),
        )
        .unwrap();

    res.assert_event(
        &Event::new("wasm-rujira-fin/order.withdraw").add_attributes(vec![
            ("owner", owner.as_str()),
            ("side", "quote"),
            ("price", "fixed:996"),
            ("amount", "5020"),
        ]),
    );

    res.assert_event(&Event::new("transfer").add_attributes(vec![
        ("recipient", owner.as_str()),
        ("sender", fin.as_str()),
        ("amount", "459869btc"),
    ]));

    res.assert_event(&Event::new("transfer").add_attributes(vec![
        ("recipient", fees.as_str()),
        ("sender", fin.as_str()),
        ("amount", "151btc"),
    ]));
}

#[test]
fn test_arbitrage() {
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
        &bow::ExecuteMsg::Deposit {},
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

    // let res = app
    //     .execute_contract(
    //         owner.clone(),
    //         fin.clone(),
    //         &ExecuteMsg::Order(vec![(
    //             Side::Base,
    //             Price::Fixed(Decimal::from_str("998").unwrap()),
    //             Uint128::from(10_000u128),
    //         )]),
    //         &coins(10_000, "btc"),
    //     )
    //     .unwrap();

    // dbg!(&res);
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
