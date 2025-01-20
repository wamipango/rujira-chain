use std::str::FromStr;

use crate::contract::{execute, instantiate, query};
use cosmwasm_std::{coin, coins, Decimal, Event, Uint128, Uint256};
use cw_multi_test::{ContractWrapper, Executor};
use rujira_bow::contract as bow_contract;
use rujira_rs::{
    bow::{self, Xyk},
    fin::{BookItemResponse, BookResponse, Denoms, ExecuteMsg, InstantiateMsg, QueryMsg, Tick},
    Chain, Layer1Asset, TokenMetadata,
};
use rujira_rs_testing::mock_rujira_app;

#[test]
fn complete_book_query() {
    let mut app = mock_rujira_app();
    let owner = app.api().addr_make("owner");
    let fees = app.api().addr_make("fees");
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
                    min_k.clone(),
                )),
            },
            &vec![],
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
                oracles: [
                    Layer1Asset::new(Chain::Btc, "BTC"),
                    Layer1Asset::new(
                        Chain::Eth,
                        "USDC-0XA0B86991C6218B36C1D19D4A2E9EB0CE3606EB48",
                    ),
                ],
                market_maker: Some(bow.to_string()),
                tick: Tick::new(4),
                fee_taker: Decimal::from_ratio(15u128, 1000u128),
                fee_maker: Decimal::from_ratio(30u128, 1000u128),
                fee_address: fees.to_string(),
            },
            &vec![],
            "fin",
            None,
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

    app.execute_contract(
        owner.clone(),
        bow.clone(),
        &bow::ExecuteMsg::Deposit {},
        // 2 BTC @ 100k USDC
        &vec![coin(200_000_000, "btc"), coin(200_000_000_000, "usdc")],
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

    assert_side(
        book.quote,
        vec![
            // 2 decimal delta, so 999 price = 99,900
            ("999.000995000000000000", 199_800_199u128),
            ("997.006985034925174625", 199_400_400u128),
            ("995.018955284329264938", 199_000_806u128),
            ("993.036880737614752295", 198_603_404u128),
            ("991.060736518412960324", 198_207_192u128),
            ("989.090487714631438943", 197_812_163u128),
            ("987.126124414354502407", 197_418_315u128),
        ],
    );

    assert_side(
        book.base,
        vec![
            ("1001.001001001001001001", 199_800u128),
            ("1003.006003985923796638", 199_201u128),
            ("1005.001769374653359524", 197_810u128),
            ("1006.988640697219657966", 196_667u128),
            ("1008.980940389654648705", 196_174u128),
            ("1010.962344642774847137", 194_793u128),
            ("1012.942249490861694194", 193_955u128),
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
        ("rate", "1001.001001001001001001"),
        ("offer", "100000000"),
        ("bid", "99900"),
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
    dbg!(balances);

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
            ("1000.000000000000000000", 199_900_000u128),
            ("0998.003991995997998999", 199_500_998u128),
            ("0996.013957048309396245", 199_102_194u128),
            ("0994.029885241473151307", 198_704_586u128),
            ("0992.051741646948178311", 198_308_167u128),
            ("0990.079506343298515227", 197_912_933u128),
            ("0988.113154406063183171", 197_518_879u128),
        ],
    );

    assert_side(
        book.base,
        vec![
            ("1002.003004506761018781", 199_700u128),
            ("1004.007993886437265411", 198_902u128),
            ("1006.001635318837081206", 197_515u128),
            ("1007.997169614773054236", 196_793u128),
            ("1009.980085144705366549", 195_432u128),
            ("1011.966830939328810336", 195_001u128),
            ("1013.948523767574090187", 194_109u128),
        ],
    );
}

fn assert_side(side: Vec<BookItemResponse>, items: Vec<(&str, u128)>) {
    for ((price, amount), item) in items.iter().zip(side) {
        let price = Decimal::from_str(price).unwrap();
        let amount = Uint128::from(*amount);
        assert_eq!(price, item.price);
        assert_eq!(amount, item.total);
    }
}
