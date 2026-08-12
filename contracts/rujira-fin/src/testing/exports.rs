#[cfg(test)]
mod tests {
    use std::{str::FromStr, time::Instant};

    use cosmwasm_std::{coin, coins, testing::MockApi, Decimal, Uint128};
    use cw_multi_test::{BasicAppBuilder, Executor};
    use rujira_ghost_vault::mock::GhostVault;
    use rujira_rs::{
        fin::{ConfigUpdate, Denoms, ExecuteMsg, QueryMsg, SimulationResponse, SwapRequest},
        OraclePair,
    };
    use rujira_rs_testing::{load_contract_state, ThorchainStargate};
    use rujira_thorchain_swap::mock::ThorchainSwap;

    use crate::{
        mock::Fin,
        ranges::{range_iter_stats, reset_range_iter_stats},
    };

    #[test]
    fn height_27395277_btc_usdc_simulation() {
        let api = MockApi::default().with_prefix("thor");
        let mut app = BasicAppBuilder::new()
            .with_api(api)
            .with_stargate(rujira_rs_testing::RujiraStargate::default())
            .build(|_, _, _| {});
        app.init_modules(|router, _, _| {
            router.stargate.with_prices(vec![
                ("BTC", Decimal::from_str("63766.3429").unwrap()),
                ("USDC", Decimal::from_str("0.9998").unwrap()),
            ]);
        });
        let owner = app.api().addr_make("owner");
        let fees = app.api().addr_make("fees");
        let fin = Fin::create(
            &mut app,
            &owner,
            &fees,
            &[],
            Denoms::new(
                "btc-btc",
                "eth-usdc-0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
            ),
            Some(OraclePair::new("BTC", "USDC")),
            Decimal::zero(),
        );

        load_contract_state(
            &mut app,
            fin.addr(),
            "rujira-fin-btc-btc-eth-usdc-0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
            27395277,
        )
        .unwrap();
        // The live market maker is a separate contract. It is slower than a
        // local mock but does not win this small swap, so exclude it to isolate
        // the FIN order/range iteration exercised by CALC's can_swap node.
        fin.update_config(
            &mut app,
            ConfigUpdate {
                market_makers: Some(vec![]),
                ..Default::default()
            },
        )
        .unwrap();

        let iterations = std::env::var("RUJIRA_FIN_REPLAY_ITERATIONS")
            .ok()
            .and_then(|value| value.parse::<u32>().ok())
            .unwrap_or(1);
        let started = Instant::now();
        reset_range_iter_stats();
        for _ in 0..iterations {
            let response: SimulationResponse = app
                .wrap()
                .query_wasm_smart(
                    fin.addr(),
                    &QueryMsg::Simulate(coin(
                        10_000_000,
                        "eth-usdc-0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
                    )),
                )
                .unwrap();
            assert_eq!(response.returned, Uint128::new(155));
            assert_eq!(response.fee, Uint128::one());
        }
        let stats = range_iter_stats();
        assert_eq!(stats.offers_built, 2 * iterations as usize);
        assert_eq!(stats.range_contributions, 3 * iterations as usize);
        eprintln!(
            "height 27395277 simulation: {:?} / iteration; range stats: {:?}",
            started.elapsed() / iterations,
            stats,
        );
    }

    #[test]
    #[ignore = "requires live gRPC network access"]
    fn height_25952787_stall() {
        let api = MockApi::default().with_prefix("thor");
        let mut app = BasicAppBuilder::new()
            .with_api(api)
            .with_stargate(ThorchainStargate::new(
                "https://grpc.rujira.network:443".to_owned(),
            ))
            .build(|_, _, _| {});
        let owner = app.api().addr_make("owner");
        let fees = app.api().addr_make("fees");
        let eth_ghost = GhostVault::create(&mut app, &owner, "eth-eth");
        let usdc_ghost = GhostVault::create(
            &mut app,
            &owner,
            "eth-usdc-0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
        );

        // 2. Setup Thorchain Swap with both ghost vaults
        let tc: ThorchainSwap = ThorchainSwap::create(&mut app, &owner);

        let fin = Fin::create(
            &mut app,
            &owner,
            &fees,
            &[tc.addr().clone()],
            Denoms::new(
                "eth-eth",
                "eth-usdc-0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
            ),
            Some(OraclePair::new("ETH", "USDC")),
            Decimal::zero(),
        );

        load_contract_state(
            &mut app,
            fin.addr(),
            "thor1tnd06uswj8033d0kzd5d7zre73u3uc44r2vvez26z5m4kr68vtusf2snva",
            25952787,
        )
        .unwrap();

        load_contract_state(&mut app, tc.addr(), "rujira-thorchain-swap", 25952787).unwrap();
        load_contract_state(
            &mut app,
            eth_ghost.addr(),
            "rujira-ghost-vault-eth-eth",
            25952787,
        )
        .unwrap();
        load_contract_state(
            &mut app,
            usdc_ghost.addr(),
            "rujira-ghost-vault-eth-usdc-0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
            25952787,
        )
        .unwrap();

        // overwrite config values for contract addresses
        // Configure thorchain swap to use both vaults
        tc.set_vault(&mut app, "btc-btc", Some(&eth_ghost)).unwrap();
        tc.set_vault(&mut app, "rune", Some(&usdc_ghost)).unwrap();
        eth_ghost
            .set_borrower(&mut app, tc.addr().as_str(), Uint128::MAX)
            .unwrap();
        usdc_ghost
            .set_borrower(&mut app, tc.addr().as_str(), Uint128::MAX)
            .unwrap();
        fin.update_config(
            &mut app,
            ConfigUpdate {
                market_makers: Some(vec![tc.addr().to_string()]),
                ..Default::default()
            },
        )
        .unwrap();

        app.init_modules(|router, _, storage| {
            router
                .bank
                .init_balance(storage, &owner, coins(1000000, "eth-eth"))
        })
        .unwrap();
        app.execute_contract(
            owner,
            fin.addr().clone(),
            &ExecuteMsg::Swap(SwapRequest::Yolo {
                to: None,
                callback: None,
            }),
            &coins(1000, "eth-eth"),
        )
        .unwrap();
    }
}
