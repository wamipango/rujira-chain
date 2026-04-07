use cosmwasm_std::{coins, to_json_binary, Decimal, Event, Uint128};
use cw_multi_test::{ContractWrapper, Executor};

use rujira_fin::mock::Fin;
use rujira_rs::{
    brune::{InstantiateMsg, SudoMsg},
    fin::{self, Denoms, SwapRequest},
    proto::types::{NodeBondProvider, NodeBondProviders, QueryNodeResponse},
    TokenMetadata,
};
use rujira_rs_testing::mock_rujira_app;
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
                mint_cap: Uint128::new(1_000_000),
                spread: Decimal::permille(5),
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
