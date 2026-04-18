#[cfg(test)]
mod tests {
    use cosmwasm_std::{coins, Decimal, Uint128};
    use cw_multi_test::Executor;
    use rujira_bow::mock::Bow;
    use rujira_rs::{
        bow::SudoMsg as BowSudoMsg,
        fin::{
            BookResponse, ConfigUpdate, Denoms, ExecuteMsg, OracleUpdate, QueryMsg, SudoMsg,
            SwapRequest,
        },
        Oracle, OraclePair,
    };
    use rujira_rs_testing::{load_contract_state, mock_rujira_app};

    use crate::mock::Fin;

    #[test]
    fn height_25281348_underflow() {
        let mut app = mock_rujira_app();
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
            None,
            Decimal::zero(),
        );
        let bow = Bow::create(
            &mut app,
            fin.addr(),
            &owner,
            "btc-btc",
            "eth-usdc-0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
            Uint128::new(1000),
        );
        load_contract_state(
            &mut app,
            fin.addr(),
            "rujira-fin-btc-btc-eth-usdc-0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
            25281348,
        )
        .unwrap();

        load_contract_state(
            &mut app,
            bow.addr(),
            "rujira-bow-btc-btc-eth-usdc-0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48-xyk",
            25281348,
        )
        .unwrap();
        app.wasm_sudo(
            bow.0.clone(),
            &BowSudoMsg::SetMarket(fin.addr().to_string()),
        )
        .unwrap();
        app.init_modules(|router, _, storage| {
            router.stargate.with_prices(vec![
                ("BTC", Decimal::percent(6500000)),
                ("USDC", Decimal::one()),
            ]);
            router
                .bank
                .init_balance(storage, &owner, coins(1000000, "btc-btc"))
        })
        .unwrap();

        // Set mock addresses
        app.wasm_sudo(
            fin.0.clone(),
            &SudoMsg::UpdateConfig(ConfigUpdate {
                market_makers: Some(vec![bow.addr().to_string()]),
                oracles: Some(OracleUpdate::Set(OraclePair::new(
                    Oracle::Enshrined("BTC".to_string()),
                    Oracle::Enshrined("USDC".to_string()),
                ))),
                ..Default::default()
            }),
        )
        .unwrap();

        app.execute_contract(
            owner,
            fin.0.clone(),
            &ExecuteMsg::Swap(SwapRequest::Yolo {
                to: None,
                callback: None,
            }),
            &coins(1000, "btc-btc"),
        )
        .unwrap();

        let book: BookResponse = app
            .wrap()
            .query_wasm_smart(
                fin.0.clone(),
                &QueryMsg::Book {
                    limit: Some(1),
                    offset: None,
                },
            )
            .unwrap();
        assert!(book.base[0].price > book.quote[0].price);
    }
}
