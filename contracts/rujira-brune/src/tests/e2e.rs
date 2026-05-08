use cosmwasm_std::{coins, testing::MockApi, to_json_binary, Decimal, Event, Uint128};
use cw_multi_test::{BasicAppBuilder, ContractWrapper, Executor};

use rujira_fin::mock::Fin;
use rujira_rs::{
    brune::{InstantiateMsg, RangeConfig, SudoMsg},
    fin::{self, Denoms, SwapRequest},
    proto::types::{NodeBondProvider, NodeBondProviders, QueryNodeResponse},
    TokenMetadata,
};
use rujira_rs_testing::{load_contract_state, mock_rujira_app, ThorchainStargate};
use rujira_staking::mock::Staking;

use crate::contract::{execute, instantiate, query, reply, sudo};

#[test]
fn brune_fin_staking_e2e() {
    let mut app = mock_rujira_app();
    let owner = app.api().addr_make("owner");
    let fin_fees = app.api().addr_make("fin-fees");
    let staking_fees = app.api().addr_make("staking-fees");
    let user = app.api().addr_make("user");

    app.init_modules(|router, _, storage| {
        router
            .bank
            .init_balance(storage, &user, coins(100000, "rune"))
    })
    .unwrap();

    let fin = Fin::create(
        &mut app,
        &owner,
        &fin_fees,
        &[owner.clone()],
        Denoms::new("x/brune", "rune"),
        None,
        Decimal::permille(0),
    );

    let staking = Staking::create(
        &mut app,
        &owner,
        "x/brune",
        "rune",
        Some((Decimal::percent(10), staking_fees.to_string())),
        fin.addr().clone(),
        to_json_binary(&fin::ExecuteMsg::Swap(SwapRequest::Yolo {
            to: None,
            callback: None,
        }))
        .unwrap(),
    );

    let brune_code = Box::new(
        ContractWrapper::new(execute, instantiate, query)
            .with_reply(reply)
            .with_sudo(sudo),
    );
    let brune_code_id = app.store_code(brune_code);
    let brune = app
        .instantiate_contract(
            brune_code_id,
            owner.clone(),
            &InstantiateMsg {
                target_utilization: Decimal::percent(75),
                min_node_fee: Decimal::percent(5),
                stake_contract: staking.addr().to_string(),
                revenue_smear: 1000,
                token_id: "brune".to_string(),
                token_metadata: TokenMetadata {
                    description: "BRUNE".to_string(),
                    display: "BRUNE".to_string(),
                    name: "BRUNE".to_string(),
                    symbol: "BRUNE".to_string(),
                    uri: None,
                    uri_hash: None,
                },
                fin_contract: fin.addr().to_string(),
                max_bond: Uint128::new(10_000),
                max_effective_bond: Uint128::new(1_000_000),
                mint_cap: Uint128::new(1_000_000),
                range: RangeConfig {
                    high: Decimal::percent(99),
                    low: Decimal::percent(80),
                    skew: cosmwasm_std::SignedDecimal::zero(),
                    step: Decimal::permille(5),
                },
                permissionless: false,
            },
            &[],
            "brune",
            None,
        )
        .unwrap();

    app.wasm_sudo(
        fin.addr().clone(),
        &fin::SudoMsg::UpdateConfig(fin::ConfigUpdate {
            fee_taker: None,
            fee_maker: None,
            fee_address: None,
            market_makers: Some(vec![brune.to_string()]),
            oracles: None,
            fee_range: None,
            range_delta: None,
            range_min: None,
        }),
    )
    .unwrap();

    let nodes = vec![
        app.api().addr_make("node-1"),
        app.api().addr_make("node-2"),
        app.api().addr_make("node-3"),
        app.api().addr_make("node-4"),
    ];
    app.init_modules(|router, _, _| {
        for node in &nodes {
            router.stargate.with_node(
                node.clone(),
                QueryNodeResponse {
                    node_address: node.to_string(),
                    status: "Active".to_string(),
                    bond_providers: Some(NodeBondProviders {
                        node_operator_fee: "500".to_string(),
                        providers: vec![NodeBondProvider {
                            bond_address: brune.to_string(),
                            bond: "0".to_string(),
                        }],
                    }),
                    ..Default::default()
                },
            );
        }
    });
    for node in &nodes {
        app.wasm_sudo(
            brune.clone(),
            &SudoMsg::ProcessNode {
                addr: node.to_string(),
            },
        )
        .unwrap();
    }

    fin.swap(&mut app, &user, 1_000, "rune", None).unwrap();

    // Stake most of the BRUNE, keep a little for later.
    staking
        .account_create(&mut app, &user, Uint128::new(900))
        .unwrap();

    let book = fin.query_book(&app, Some(5), None).unwrap();
    assert_eq!(book.base[0].price, Decimal::one());
    // We have max capacity of 10_000 per node. 4 nodes, and a deposit of 1000 with a 75% target utilization.
    // 750 used up across 1 node, 39250 capacity available. will require 39250 / 0.75 rune to fill it = 52333
    assert_eq!(book.base[0].total, Uint128::new(52333));

    // Simulate a node accruing bond for the contract.
    app.init_modules(|router, _, _| {
        router.stargate.with_node(
            nodes[0].clone(),
            QueryNodeResponse {
                node_address: nodes[0].to_string(),
                status: "Active".to_string(),
                bond_providers: Some(NodeBondProviders {
                    node_operator_fee: "500".to_string(),
                    providers: vec![NodeBondProvider {
                        bond_address: brune.to_string(),
                        bond: "2000".to_string(),
                    }],
                }),
                ..Default::default()
            },
        );
    });
    fin.swap(&mut app, &user, 1000, "rune", None).unwrap();

    app.update_block(|block| {
        block.time = block.time.plus_seconds(200);
    });

    // Run another swap to trigger allocate and revenue distribution.
    fin.swap(&mut app, &user, 1000, "rune", None).unwrap();
    let statu = staking.query_status(&app);
    assert_eq!(statu.undistributed_revenue, Uint128::new(200));

    let res = staking.account_claim(&mut app, &user).unwrap();

    res.assert_event(&Event::new("transfer").add_attributes(vec![
        ("recipient", user.as_str()),
        ("sender", staking.addr().as_str()),
        ("amount", "180rune"),
    ]));

    res.assert_event(&Event::new("transfer").add_attributes(vec![
        ("recipient", staking_fees.as_str()),
        ("sender", staking.addr().as_str()),
        ("amount", "20rune"),
    ]));
}

#[test]
fn oob_memory_26086832() {
    let api = MockApi::default().with_prefix("thor");
    let owner = api.addr_make("owner");
    let fees = api.addr_make("fees");
    let mut app = BasicAppBuilder::new()
        .with_api(MockApi::default().with_prefix("thor"))
        .with_stargate(ThorchainStargate::new("grpc.rujira.network:443".to_owned()))
        .build(|_, _, _| {});

    let fin = Fin::create(
        &mut app,
        &owner,
        &fees,
        &[owner.clone()],
        Denoms::new("x/brune", "rune"),
        None,
        Decimal::permille(0),
    );
    let brune_code = Box::new(
        ContractWrapper::new(execute, instantiate, query)
            .with_reply(reply)
            .with_sudo(sudo),
    );

    let brune_code_id = app.store_code(brune_code);
    let brune = app
        .instantiate_contract(
            brune_code_id,
            owner.clone(),
            &InstantiateMsg {
                target_utilization: Decimal::percent(75),
                min_node_fee: Decimal::percent(5),
                stake_contract: owner.to_string(),
                revenue_smear: 1000,
                token_id: "brune".to_string(),
                token_metadata: TokenMetadata {
                    description: "BRUNE".to_string(),
                    display: "BRUNE".to_string(),
                    name: "BRUNE".to_string(),
                    symbol: "BRUNE".to_string(),
                    uri: None,
                    uri_hash: None,
                },
                fin_contract: fin.addr().to_string(),
                max_bond: Uint128::new(10_000),
                max_effective_bond: Uint128::new(1_000_000),
                mint_cap: Uint128::new(1_000_000),
                range: RangeConfig {
                    high: Decimal::percent(99),
                    low: Decimal::percent(80),
                    skew: cosmwasm_std::SignedDecimal::zero(),
                    step: Decimal::permille(5),
                },
                permissionless: false,
            },
            &[],
            "brune",
            None,
        )
        .unwrap();

    load_contract_state(&mut app, fin.addr(), "rujira-fin-x-brune-rune", 26086832).unwrap();
    load_contract_state(&mut app, &brune, "rujira-brune", 26086832).unwrap();
    let book = fin.query_book(&app, None, None).unwrap();

    // Regression: this sparse range state used to recursively skip thousands of
    // zero-sized range offers and overflow the test thread stack.
    assert_eq!(book.base.len(), 51);
    assert_eq!(book.quote.len(), 100);

    let decimal = |value: &str| value.parse::<Decimal>().unwrap();
    let assert_base = |idx: usize, price: &str, total: u128| {
        assert_eq!(book.base[idx].price, decimal(price));
        assert_eq!(book.base[idx].total, Uint128::new(total));
    };
    let assert_quote = |idx: usize, price: &str, total: u128| {
        assert_eq!(book.quote[idx].price, decimal(price));
        assert_eq!(book.quote[idx].total, Uint128::new(total));
    };

    assert_base(0, "1.093989999954365616", 7_179_391);
    assert_base(1, "1.099989999954614516", 1_536_702);
    assert_base(2, "1.509137727028441529", 54_545);
    assert_base(3, "1.509213183914803559", 54_543);
    assert_base(4, "1.50928864457395225", 54_542);
    assert_base(5, "1.509364109006176503", 54_541);
    assert_base(6, "1.509439577211618729", 54_539);
    assert_base(7, "1.509515049190542832", 54_538);
    assert_base(8, "1.509590524942942872", 54_536);
    assert_base(9, "1.509666004469231133", 54_535);
    assert_base(10, "1.50974148776945084", 54_534);
    assert_base(11, "1.509816974843767601", 54_532);
    assert_base(12, "1.509892465692595692", 54_531);
    assert_base(13, "1.509965105863045562", 50_406);
    assert_base(14, "1.99902997387532194", 46_214);
    assert_base(15, "1.999129925374098053", 46_212);
    assert_base(16, "1.999229881870314223", 46_211);
    assert_base(17, "1.999329843364384035", 46_210);
    assert_base(18, "1.999429809856494869", 46_209);
    assert_base(19, "1.999529781347060409", 46_208);
    assert_base(20, "1.999629757836098428", 46_207);
    assert_base(21, "1.999729739324079129", 46_205);
    assert_base(22, "1.999829725810907203", 46_204);
    assert_base(23, "1.99992971729727915", 46_203);
    assert_base(24, "1.999989857432294162", 9_372);
    assert_base(25, "2.097452434344574209", 27_812);
    assert_base(26, "2.097557306966249185", 27_811);
    assert_base(27, "2.097662184831592862", 27_810);
    assert_base(28, "2.097767067940975696", 27_809);
    assert_base(29, "2.097871956294298989", 27_809);
    assert_base(30, "2.097976849891991117", 27_808);
    assert_base(31, "2.098081748734677034", 27_807);
    assert_base(32, "2.098186652822003558", 27_807);
    assert_base(33, "2.098291562154717267", 27_806);
    assert_base(34, "2.098396476732702373", 27_805);
    assert_base(35, "2.098501396556641925", 27_805);
    assert_base(36, "2.098606321626373563", 27_804);
    assert_base(37, "2.098711251942626963", 27_803);
    assert_base(38, "2.098816187505123835", 27_803);
    assert_base(39, "2.098921128314414322", 27_802);
    assert_base(40, "2.099026074370944195", 27_801);
    assert_base(41, "2.099131025674655469", 27_800);
    assert_base(42, "2.099235982225878034", 27_800);
    assert_base(43, "2.099340944025040508", 27_799);
    assert_base(44, "2.099445911072218014", 27_798);
    assert_base(45, "2.099550883367729021", 27_798);
    assert_base(46, "2.099655860912013736", 27_797);
    assert_base(47, "2.099760843704961851", 27_796);
    assert_base(48, "2.099865831747251236", 27_796);
    assert_base(49, "2.099959163471356919", 21_621);
    assert_base(50, "10", 9_670_877_901);

    assert_quote(0, "2.082707930349170458", 58_132);
    assert_quote(49, "2.077611414287328833", 58_060);
    assert_quote(99, "2.072423743349444925", 57_988);
}
