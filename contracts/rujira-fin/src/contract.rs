use crate::config::{Config, CONFIG};
use crate::error::ContractError;
use crate::order_pool::execute::execute_order;
use crate::order_pool::{query as order_query, OrderManager};
use crate::ranges::execute::execute_range;
use crate::ranges::query as range_query;
use crate::swap_iter::{create_context, RootItem, SwapIter};
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    coin, ensure, ensure_eq, to_json_binary, Addr, BankMsg, Binary, CosmosMsg, Deps, DepsMut,
    Empty, Env, MessageInfo, Response, WasmMsg,
};
use cw2::{ensure_from_older_version, set_contract_version};
use cw_storage_plus::Item;
use cw_utils::{one_coin, NativeBalance};
use rujira_rs::exchange::{Arber, SwapContext, SwapItem, Swapper};
use rujira_rs::fin::{
    BookItemResponse, BookResponse, ConfigResponse, ExecuteMsg, InstantiateMsg, QueryMsg, Side,
    SimulationResponse, SudoMsg, SwapRequest,
};
use rujira_rs::proto::scheduler::MsgScheduleExecuteContract;
use rujira_rs::utils::module_address;
use rujira_rs::Premiumable;

// version info for migration info
pub const CONTRACT_NAME: &str = env!("CARGO_PKG_NAME");
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");
static SCHEDULE: Item<u64> = Item::new("s");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    let config = Config::new(deps.api, msg)?;
    config.validate(deps.as_ref())?;
    config.save(deps.storage)?;
    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let oracle = config
        .oracles
        .clone()
        .and_then(|x| x.price(deps.querier).ok());

    match msg {
        ExecuteMsg::Swap(req) => {
            let msg = WasmMsg::Execute {
                contract_addr: env.contract.address.to_string(),
                msg: to_json_binary(&ExecuteMsg::Arb {
                    then: Some(to_json_binary(&ExecuteMsg::DoSwap((info.sender, req)))?),
                })?,
                funds: info.funds,
            };

            Ok(Response::default().add_message(msg))
        }
        ExecuteMsg::Order(req) => {
            let msg = WasmMsg::Execute {
                contract_addr: env.contract.address.to_string(),
                msg: to_json_binary(&ExecuteMsg::Arb {
                    then: Some(to_json_binary(&ExecuteMsg::DoOrder((info.sender, req)))?),
                })?,
                funds: info.funds,
            };

            Ok(Response::default().add_message(msg))
        }
        ExecuteMsg::Range(msg) => {
            let msg = WasmMsg::Execute {
                contract_addr: env.contract.address.to_string(),
                msg: to_json_binary(&ExecuteMsg::Arb {
                    then: Some(to_json_binary(&ExecuteMsg::DoRange((info.sender, msg)))?),
                })?,
                funds: info.funds,
            };

            Ok(Response::default().add_message(msg))
        }
        ExecuteMsg::Arb { then } => {
            let scheduler = deps.api.addr_humanize(&module_address("scheduler"))?;

            let swap_iter = SwapIter::new(deps.querier, &config);
            let mut messages: Vec<CosmosMsg> = vec![];
            let mut fees = NativeBalance::default();
            let mut arb = Arber::new(create_context(&config), CONTRACT_NAME.to_string());
            let base_iter = swap_iter.iter(deps.storage, &Side::Base, &oracle);
            let quote_iter = swap_iter.iter(deps.storage, &Side::Quote, &oracle);
            let res = arb.run(base_iter, quote_iter)?;
            let response = arb
                .context
                .commit(deps.storage, Response::default().add_events(res.events))?;

            fees += coin(res.profit_quote.u128(), config.denoms.bid(&Side::Quote));
            fees += coin(res.profit_base.u128(), config.denoms.bid(&Side::Base));
            fees.normalize();

            if !fees.is_empty() {
                messages.push(CosmosMsg::Bank(BankMsg::Send {
                    to_address: config.fee_address.to_string(),
                    amount: fees.into_vec(),
                }))
            }

            if let Some(msg) = then {
                ensure!(
                    info.sender == env.contract.address,
                    ContractError::Unauthorized {}
                );

                messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: env.contract.address.to_string(),
                    msg,
                    funds: info.funds,
                }))
            }

            match SCHEDULE.load(deps.storage) {
                Ok(after) => {
                    if info.sender == scheduler {
                        Ok(response.add_messages(messages).add_message(
                            MsgScheduleExecuteContract {
                                sender: env.contract.address.to_string(),
                                after,
                                msg: to_json_binary(&ExecuteMsg::Arb { then: None })?.to_vec(),
                            },
                        ))
                    } else {
                        Ok(response.add_messages(messages))
                    }
                }
                Err(_) => Ok(response.add_messages(messages)),
            }
        }

        ExecuteMsg::DoSwap((sender, req)) => {
            ensure_eq!(
                info.sender,
                env.contract.address,
                ContractError::Unauthorized {}
            );

            execute_swap(deps, info, sender, req, &config, &oracle)
        }
        ExecuteMsg::DoOrder((recipient, (vec, callback))) => {
            ensure_eq!(
                info.sender,
                env.contract.address,
                ContractError::Unauthorized {}
            );
            let mut manager = OrderManager::new(
                &config,
                recipient.clone(),
                env.block.time,
                NativeBalance(info.funds),
            );
            execute_order(
                deps,
                &mut manager,
                recipient,
                vec,
                callback,
                &config,
                &oracle,
            )
        }
        ExecuteMsg::DoRange((sender, msg)) => {
            ensure_eq!(
                info.sender,
                env.contract.address,
                ContractError::Unauthorized {}
            );
            let swap_iter = SwapIter::new(deps.querier, &config);
            let price = swap_iter.mid_price(deps.storage, &oracle);

            execute_range(deps, env, info, sender, msg, price)
        }
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn sudo(deps: DepsMut, env: Env, msg: SudoMsg) -> Result<Response, ContractError> {
    match msg {
        SudoMsg::UpdateConfig(update) => {
            let mut config = CONFIG.load(deps.storage)?;
            config.update(deps.api, &update)?;
            config.validate(deps.as_ref())?;
            config.save(deps.storage)?;
            Ok(Response::default())
        }
        SudoMsg::StartSchedule(after) => {
            SCHEDULE.save(deps.storage, &after)?;
            Ok(Response::default().add_message(MsgScheduleExecuteContract {
                sender: env.contract.address.to_string(),
                after,
                msg: to_json_binary(&ExecuteMsg::Arb { then: None })?.to_vec(),
            }))
        }
        SudoMsg::EndSchedule {} => {
            SCHEDULE.remove(deps.storage);
            Ok(Response::default())
        }
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> Result<Binary, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let oracle = config
        .oracles
        .clone()
        .map(|x| x.price(deps.querier))
        .transpose()?;
    let swap_iter = SwapIter::new(deps.querier, &config);

    match msg {
        QueryMsg::Config {} => to_json_binary(&ConfigResponse::from(config)),
        QueryMsg::Order((owner, side, price)) => to_json_binary(&order_query::query_order(
            deps, &owner, side, price, &oracle,
        )?),
        QueryMsg::Orders {
            owner,
            side,
            start_after,
            limit,
        } => to_json_binary(&order_query::query_orders(
            deps,
            owner.as_deref(),
            side,
            start_after,
            limit,
            &oracle,
        )?),
        QueryMsg::Book { limit, offset } => {
            let limit = limit.unwrap_or(100) as usize;
            let offset = offset.unwrap_or(0) as usize;

            to_json_binary(&BookResponse {
                base: swap_iter
                    .iter(deps.storage, &Side::Base, &oracle)
                    .skip(offset)
                    .take(limit)
                    .map(|v| BookItemResponse {
                        price: v.rate(),
                        total: v.total(),
                    })
                    .collect(),
                quote: swap_iter
                    .iter(deps.storage, &Side::Quote, &oracle)
                    .skip(offset)
                    .take(limit)
                    .map(|v| BookItemResponse {
                        price: v.rate(),
                        total: v.total(),
                    })
                    .collect(),
            })
        }
        QueryMsg::Simulate(offer) => {
            let side = config.denoms.ask_side(&offer)?;
            let mut swapper = Swapper::new(
                CONTRACT_NAME,
                create_context(&config),
                offer.amount,
                SwapRequest::Yolo {
                    to: None,
                    callback: None,
                },
                config.fee_taker,
            );
            let mut iter = swap_iter.iter(deps.storage, &side, &oracle);
            let res = swapper.swap(&mut iter)?;
            to_json_binary(&SimulationResponse {
                returned: res.return_amount,
                fee: res.fee_amount,
            })
        }
        QueryMsg::Range(idx) => to_json_binary(&range_query::query_range(deps, idx)?),
        QueryMsg::Ranges {
            cursor,
            limit,
            owner,
        } => to_json_binary(&range_query::query_ranges(
            deps,
            cursor,
            limit,
            owner.map(|x| deps.api.addr_validate(&x)).transpose()?,
        )?),
    }
    .map_err(ContractError::Std)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, _msg: ()) -> Result<Response, ContractError> {
    ensure_from_older_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    Ok(Response::default())
}

fn execute_swap(
    deps: DepsMut,
    info: MessageInfo,
    sender: Addr,
    req: SwapRequest,
    config: &Config,
    oracle: &impl Premiumable,
) -> Result<Response, ContractError> {
    let swap_iter = SwapIter::new(deps.querier, config);

    let to = req.to().map(|x| deps.api.addr_validate(&x)).transpose()?;
    let funds = one_coin(&info)?;
    let side = config.denoms.ask_side(&funds)?;
    let mut swapper: Swapper<RootItem> = Swapper::new(
        CONTRACT_NAME,
        create_context(config),
        funds.amount,
        req.clone(),
        config.fee_taker,
    );
    let res = {
        let mut iter = swap_iter.iter(deps.storage, &side, oracle);
        swapper.swap(&mut iter)?
    };
    let response = swapper
        .context
        .commit(deps.storage, Response::default().add_events(res.events))?;
    let mut payout = NativeBalance(vec![
        coin(res.return_amount.u128(), config.denoms.bid(&side)),
        coin(res.remaining_offer.u128(), config.denoms.ask(&side)),
    ]);

    let recipient = to.unwrap_or(sender);

    let mut messages: Vec<CosmosMsg> = vec![];
    payout.normalize();
    if !payout.is_empty() {
        match req.callback() {
            None => messages.push(CosmosMsg::Bank(BankMsg::Send {
                to_address: recipient.to_string(),
                amount: payout.into_vec(),
            })),
            Some(cb) => messages.push(
                cb.to_message(&recipient, Empty {}, payout.into_vec())?
                    .into(),
            ),
        }
    };

    let mut fees = NativeBalance::default();
    fees += coin(res.fee_amount.u128(), config.denoms.bid(&side));
    fees.normalize();
    if !fees.is_empty() {
        messages.push(CosmosMsg::Bank(BankMsg::Send {
            to_address: config.fee_address.to_string(),
            amount: fees.into_vec(),
        }))
    }

    Ok(response.add_messages(messages))
}

#[cfg(test)]
mod tests {

    use std::str::FromStr;

    use super::*;
    use cosmwasm_std::{coin, coins, Addr, Decimal, Event, Uint128};
    use cw_multi_test::{ContractWrapper, Executor};
    use rujira_rs::{
        fin::{ConfigUpdate, Denoms, OracleUpdate, OrdersResponse, Price, SwapRequest, Tick},
        Layer1Asset, OraclePair,
    };
    use rujira_rs_testing::{mock_rujira_app, RujiraApp};

    fn setup() -> (RujiraApp, Addr) {
        let mut app = mock_rujira_app();
        app.init_modules(|a, _, _| {
            a.stargate.with_prices(vec![
                ("BTC", Decimal::from_str("100100.00000000").unwrap()),
                ("USDC", Decimal::from_str("1.00100000").unwrap()),
            ]);
        });

        let owner = app.api().addr_make("owner");

        let code = Box::new(ContractWrapper::new(execute, instantiate, query).with_sudo(sudo));
        let code_id = app.store_code(code);
        let contract = app
            .instantiate_contract(
                code_id,
                owner,
                &InstantiateMsg {
                    denoms: Denoms::new("btc-btc", "eth-usdc"),
                    market_makers: vec![],
                    oracles: Some(OraclePair::new(
                        Layer1Asset::try_from("BTC.BTC").unwrap(),
                        Layer1Asset::try_from(
                            "ETH.USDC-0XA0B86991C6218B36C1D19D4A2E9EB0CE3606EB48",
                        )
                        .unwrap(),
                    )),
                    tick: Tick::new(6u8),
                    fee_taker: Decimal::zero(),
                    fee_maker: Decimal::zero(),
                    fee_range: Decimal::zero(),
                    fee_address: app.api().addr_make("fee").to_string(),
                    range_delta: Decimal::permille(50),
                    range_min: Decimal::one(),
                },
                &[],
                "template",
                None,
            )
            .unwrap();

        (app, contract)
    }

    #[test]
    fn test_limit_order_and_oracle_order_same_price_different_direction() {
        let (mut app, contract) = setup();
        let owner = app.api().addr_make("owner");
        let user = app.api().addr_make("user");

        let funds = vec![
            coin(500_000_000_000_000, "btc-btc"),
            coin(500_000_000_000_000, "eth-usdc"),
        ];
        app.init_modules(|router, _, storage| {
            router
                .bank
                .init_balance(storage, &user, funds.clone())
                .unwrap();
        });

        app.init_modules(|router, _, storage| {
            router
                .bank
                .init_balance(storage, &owner, funds.clone())
                .unwrap();
        });

        // limit order 100_000
        // i am selling 100_000 price the quantity of 1000 BTC
        app.execute_contract(
            owner.clone(),
            contract.clone(),
            &ExecuteMsg::Order((
                vec![(
                    Side::Base,
                    Price::Fixed(Decimal::from_str("100100").unwrap()),
                    Some(Uint128::from(1_000u128)),
                )],
                None,
            )),
            &funds,
        )
        .unwrap();

        // limit order 100_000
        // i am buying 100_000 price the quantity of 1000 USDC
        app.execute_contract(
            user.clone(),
            contract.clone(),
            &ExecuteMsg::Order((
                vec![(
                    Side::Quote,
                    Price::Fixed(Decimal::from_str("100100").unwrap()),
                    Some(Uint128::from(100_000_000_000u128)),
                )],
                None,
            )),
            &funds,
        )
        .unwrap();

        let book: BookResponse = app
            .wrap()
            .query_wasm_smart(
                contract.clone(),
                &QueryMsg::Book {
                    limit: None,
                    offset: None,
                },
            )
            .unwrap();

        // the base order should be fully consumed
        assert_eq!(book.base.len(), 0);
        assert_eq!(book.quote.len(), 1);
    }

    #[test]
    fn instantiation() {
        let mut app = mock_rujira_app();
        let owner = app.api().addr_make("owner");

        let code = Box::new(ContractWrapper::new(execute, instantiate, query));
        let code_id = app.store_code(code);
        app.instantiate_contract(
            code_id,
            owner,
            &InstantiateMsg {
                denoms: Denoms::new("ruji", "eth-usdc"),
                oracles: Some(OraclePair::new(
                    Layer1Asset::try_from("BTC.BTC").unwrap(),
                    Layer1Asset::try_from("ETH.USDC-0XA0B86991C6218B36C1D19D4A2E9EB0CE3606EB48")
                        .unwrap(),
                )),
                market_makers: vec![],
                tick: Tick::new(4u8),
                fee_taker: Decimal::zero(),
                fee_maker: Decimal::zero(),
                fee_range: Decimal::zero(),
                fee_address: app.api().addr_make("fee").to_string(),
                range_delta: Decimal::permille(50),
                range_min: Decimal::one(),
            },
            &[],
            "template",
            None,
        )
        .unwrap();
    }

    #[test]
    fn query_book() {
        let (mut app, contract) = setup();
        let owner = app.api().addr_make("owner");
        let funds = vec![coin(500_000_000, "btc-btc"), coin(500_000_000, "eth-usdc")];
        app.init_modules(|router, _, storage| {
            router
                .bank
                .init_balance(storage, &owner, funds.clone())
                .unwrap();
        });

        app.execute_contract(
            owner.clone(),
            contract.clone(),
            &ExecuteMsg::Order((
                vec![
                    (
                        Side::Base,
                        Price::Oracle(0),
                        Some(Uint128::from(1000000u128)),
                    ),
                    (
                        Side::Base,
                        Price::Fixed(Decimal::from_str("100000").unwrap()),
                        Some(Uint128::from(1000000u128)),
                    ),
                    (
                        Side::Base,
                        Price::Fixed(Decimal::from_str("93317").unwrap()),
                        Some(Uint128::from(1000000u128)),
                    ),
                    (
                        Side::Base,
                        Price::Fixed(Decimal::from_str("93219").unwrap()),
                        Some(Uint128::from(2100000u128)),
                    ),
                    (
                        Side::Base,
                        Price::Fixed(Decimal::from_str("91219").unwrap()),
                        Some(Uint128::from(5100000u128)),
                    ),
                    (
                        Side::Quote,
                        Price::Oracle(1),
                        Some(Uint128::from(1000000u128)),
                    ),
                    (
                        Side::Quote,
                        Price::Fixed(Decimal::from_str("100010").unwrap()),
                        Some(Uint128::from(1250000u128)),
                    ),
                    (
                        Side::Quote,
                        Price::Fixed(Decimal::from_str("87900").unwrap()),
                        Some(Uint128::from(5100000u128)),
                    ),
                ],
                None,
            )),
            &funds,
        )
        .unwrap();

        let book: BookResponse = app
            .wrap()
            .query_wasm_smart(
                contract,
                &QueryMsg::Book {
                    limit: None,
                    offset: None,
                },
            )
            .unwrap();
        assert_eq!(book.base.len(), 4);
        let entry = book.base[0].clone();
        assert_eq!(entry.price, Decimal::from_str("91219").unwrap());
        assert_eq!(entry.total, Uint128::from(5099977u128));
        let entry = book.base[1].clone();
        assert_eq!(entry.price, Decimal::from_str("93219").unwrap());
        assert_eq!(entry.total, Uint128::from(2100000u128));
        let entry = book.base[2].clone();
        assert_eq!(entry.price, Decimal::from_str("93317").unwrap());
        assert_eq!(entry.total, Uint128::from(1000000u128));
        let entry = book.base[3].clone();
        assert_eq!(entry.price, Decimal::from_str("100000").unwrap());
        assert_eq!(entry.total, Uint128::from(2000000u128));

        assert_eq!(book.quote.len(), 1);
        let entry = book.quote[0].clone();
        assert_eq!(entry.price, Decimal::from_str("87900").unwrap());
        assert_eq!(entry.total, Uint128::from(5100000u128));

        #[allow(deprecated)]
        let balance = app.wrap().query_all_balances(owner.to_string()).unwrap();
        // Total 102000 btc submitted on base side, swapped inc fee
        // Total 73500 usdc submitted on quote side
        assert_eq!(
            balance,
            vec![
                coin(500_000_000 - 10199977, "btc-btc"),
                coin(500_000_000 - 7350000, "eth-usdc")
            ]
        );
    }

    #[test]
    fn swap() {
        let (mut app, contract) = setup();
        let owner = app.api().addr_make("owner");
        let funds = vec![
            coin(500_000_000_000_000, "btc-btc"),
            coin(500_000_000_000_000, "eth-usdc"),
        ];
        app.init_modules(|router, _, storage| {
            router
                .bank
                .init_balance(storage, &owner, funds.clone())
                .unwrap();
        });

        app.execute_contract(
            owner.clone(),
            contract.clone(),
            &ExecuteMsg::Order((
                vec![
                    (Side::Base, Price::Oracle(0), Some(Uint128::from(10000u128))),
                    (
                        Side::Base,
                        Price::Fixed(Decimal::from_str("100000").unwrap()),
                        Some(Uint128::from(10000u128)),
                    ),
                    (
                        Side::Base,
                        Price::Fixed(Decimal::from_str("93317").unwrap()),
                        Some(Uint128::from(10000u128)),
                    ),
                    (
                        Side::Base,
                        Price::Fixed(Decimal::from_str("93219").unwrap()),
                        Some(Uint128::from(21000u128)),
                    ),
                    (
                        Side::Base,
                        Price::Fixed(Decimal::from_str("91219").unwrap()),
                        Some(Uint128::from(51000u128)),
                    ),
                    (
                        Side::Quote,
                        Price::Oracle(-1000),
                        Some(Uint128::from(1000000000u128)),
                    ),
                    (
                        Side::Quote,
                        Price::Fixed(Decimal::from_str("90000").unwrap()),
                        Some(Uint128::from(1250000000u128)),
                    ),
                    (
                        Side::Quote,
                        Price::Fixed(Decimal::from_str("87900").unwrap()),
                        Some(Uint128::from(5100000000u128)),
                    ),
                ],
                None,
            )),
            &funds,
        )
        .unwrap();
        let swap_amount = coin(100_000, "eth-usdc");

        let sim: SimulationResponse = app
            .wrap()
            .query_wasm_smart(contract.clone(), &QueryMsg::Simulate(swap_amount.clone()))
            .unwrap();

        let res = app
            .execute_contract(
                owner.clone(),
                contract.clone(),
                &ExecuteMsg::Swap(SwapRequest::Yolo {
                    to: None,
                    callback: None,
                }),
                &[swap_amount],
            )
            .unwrap();

        // Best price = 91_219
        // 100_000 / 91_219 = 1.09
        res.assert_event(&Event::new("wasm-rujira-fin/trade").add_attributes(vec![
            ("side", "base"),
            ("offer", "100000"),
            ("bid", "1"),
            ("price", "fixed:91219"),
        ]));
        res.assert_event(&Event::new("transfer").add_attributes(vec![
            ("recipient", owner.as_str()),
            ("sender", contract.as_str()),
            ("amount", "1btc-btc"),
        ]));

        assert_eq!(sim.returned, Uint128::from(1u128));

        let res = app
            .execute_contract(
                owner.clone(),
                contract.clone(),
                &ExecuteMsg::Swap(SwapRequest::Yolo {
                    to: None,
                    callback: None,
                }),
                &coins(30_000, "btc-btc"),
            )
            .unwrap();

        // Selling 30_000 BTC
        res.assert_event(&Event::new("wasm-rujira-fin/trade").add_attributes(vec![
            ("side", "quote"),
            ("offer", "24999"),
            ("bid", "2250000000"),
            ("price", "oracle:-1000"),
            ("price", "fixed:90000"),
        ]));
        res.assert_event(&Event::new("wasm-rujira-fin/trade").add_attributes(vec![
            ("side", "quote"),
            ("offer", "5001"),
            ("bid", "439587900"),
            ("price", "fixed:87900"),
        ]));
        res.assert_event(&Event::new("transfer").add_attributes(vec![
            ("recipient", owner.as_str()),
            ("sender", contract.as_str()),
            ("amount", "2689587900eth-usdc"),
        ]));

        let res = app
            .execute_contract(
                owner.clone(),
                contract.clone(),
                &ExecuteMsg::Swap(SwapRequest::Yolo {
                    to: None,
                    callback: None,
                }),
                &coins(100000, "btc-btc"),
            )
            .unwrap();

        // Check that any unused funds are returned
        res.assert_event(&Event::new("wasm-rujira-fin/trade").add_attributes(vec![
            ("side", "quote"),
            ("offer", "53019"),
            ("bid", "4660412100"),
            ("price", "fixed:87900"),
        ]));
        res.assert_event(&Event::new("transfer").add_attributes(vec![
            ("recipient", owner.as_str()),
            ("sender", contract.as_str()),
            ("amount", "46981btc-btc,4660412100eth-usdc"),
        ]));
    }

    #[test]
    fn update_config() {
        let (mut app, contract) = setup();
        app.wasm_sudo(
            contract.clone(),
            &SudoMsg::UpdateConfig(ConfigUpdate {
                fee_taker: Some(Decimal::from_ratio(1u128, 100u128)),
                fee_maker: Some(Decimal::from_ratio(2u128, 100u128)),
                fee_address: Some(app.api().addr_make("fees2").to_string()),
                market_makers: Some(vec![app.api().addr_make("mm2").to_string()]),
                oracles: Some(OracleUpdate::Set(OraclePair::new(
                    Layer1Asset::try_from("ETH.USDC-0XA0B86991C6218B36C1D19D4A2E9EB0CE3606EB48")
                        .unwrap(),
                    Layer1Asset::try_from("BTC.BTC").unwrap(),
                ))),
                fee_range: Some(Decimal::from_ratio(3u128, 100u128)),
                range_delta: Some(Decimal::from_ratio(4u128, 100u128)),
                range_min: None,
            }),
        )
        .unwrap();

        let config: ConfigResponse = app
            .wrap()
            .query_wasm_smart(contract.clone(), &QueryMsg::Config {})
            .unwrap();

        assert_eq!(config.fee_taker, Decimal::from_ratio(1u128, 100u128));
        assert_eq!(config.fee_maker, Decimal::from_ratio(2u128, 100u128));
        assert_eq!(config.fee_range, Decimal::from_ratio(3u128, 100u128));
        assert_eq!(config.range_delta, Decimal::from_ratio(4u128, 100u128));
        assert_eq!(config.fee_address, app.api().addr_make("fees2").to_string());
        assert_eq!(
            config.market_makers[0],
            app.api().addr_make("mm2").to_string()
        );
        assert_eq!(
            config.oracles,
            Some(OraclePair::new(
                Layer1Asset::try_from("ETH.USDC-0XA0B86991C6218B36C1D19D4A2E9EB0CE3606EB48",)
                    .unwrap(),
                Layer1Asset::try_from("BTC.BTC").unwrap(),
            ))
        );
    }

    #[test]
    fn test_only_full_distribution() {
        let (mut app, contract) = setup();
        let owner = app.api().addr_make("owner");
        let funds = vec![
            coin(500_000_000_000_000, "btc-btc"),
            coin(500_000_000_000_000, "eth-usdc"),
        ];
        app.init_modules(|router, _, storage| {
            router
                .bank
                .init_balance(storage, &owner, funds.clone())
                .unwrap();
        });

        let user = app.api().addr_make("user");
        app.init_modules(|router, _, storage| {
            router
                .bank
                .init_balance(storage, &user, funds.clone())
                .unwrap();
        });

        let user2 = app.api().addr_make("user2");
        app.init_modules(|router, _, storage| {
            router
                .bank
                .init_balance(storage, &user2, funds.clone())
                .unwrap();
        });

        let user3 = app.api().addr_make("user3");
        app.init_modules(|router, _, storage| {
            router
                .bank
                .init_balance(storage, &user3, funds.clone())
                .unwrap();
        });

        app.execute_contract(
            owner.clone(),
            contract.clone(),
            &ExecuteMsg::Order((
                vec![(
                    Side::Base,
                    Price::Fixed(Decimal::from_str("1000.00").unwrap()),
                    Some(Uint128::from(100u128)),
                )],
                None,
            )),
            &funds,
        )
        .unwrap();

        app.execute_contract(
            user2.clone(),
            contract.clone(),
            &ExecuteMsg::Order((
                vec![(
                    Side::Base,
                    Price::Fixed(Decimal::from_str("1000.00").unwrap()),
                    Some(Uint128::from(1000u128)),
                )],
                None,
            )),
            &funds,
        )
        .unwrap();

        app.execute_contract(
            user3.clone(),
            contract.clone(),
            &ExecuteMsg::Order((
                vec![(
                    Side::Base,
                    Price::Fixed(Decimal::from_str("1000.00").unwrap()),
                    Some(Uint128::from(500u128)),
                )],
                None,
            )),
            &funds,
        )
        .unwrap();

        let orders_owner: OrdersResponse = app
            .wrap()
            .query_wasm_smart(
                contract.clone(),
                &QueryMsg::Orders {
                    owner: Some(owner.clone().to_string()),
                    side: None,
                    start_after: None,
                    limit: None,
                },
            )
            .unwrap();

        let order = orders_owner.orders[0].clone();
        assert_eq!(order.remaining, Uint128::from(100u128));
        assert_eq!(order.filled, Uint128::zero());

        let orders_user2: OrdersResponse = app
            .wrap()
            .query_wasm_smart(
                contract.clone(),
                &QueryMsg::Orders {
                    owner: Some(user2.clone().to_string()),
                    side: None,
                    start_after: None,
                    limit: None,
                },
            )
            .unwrap();
        let order = orders_user2.orders[0].clone();
        assert_eq!(order.remaining, Uint128::from(1000u128));
        assert_eq!(order.filled, Uint128::zero());

        let orders_user3: OrdersResponse = app
            .wrap()
            .query_wasm_smart(
                contract.clone(),
                &QueryMsg::Orders {
                    owner: Some(user3.clone().to_string()),
                    side: None,
                    start_after: None,
                    limit: None,
                },
            )
            .unwrap();
        let order = orders_user3.orders[0].clone();
        assert_eq!(order.remaining, Uint128::from(500u128));
        assert_eq!(order.filled, Uint128::zero());

        let swap_amount = coin(1_600_000, "eth-usdc");

        let res = app
            .execute_contract(
                user.clone(),
                contract.clone(),
                &ExecuteMsg::Swap(SwapRequest::Yolo {
                    to: None,
                    callback: None,
                }),
                &[swap_amount],
            )
            .unwrap();

        res.assert_event(&Event::new("wasm-rujira-fin/trade").add_attributes(vec![
            ("rate", "1000"),
            ("price", "fixed:1000"),
            ("offer", "1600000"),
            ("bid", "1600"),
            ("side", "base"),
        ]));

        let orders_owner: OrdersResponse = app
            .wrap()
            .query_wasm_smart(
                contract.clone(),
                &QueryMsg::Orders {
                    owner: Some(owner.clone().to_string()),
                    side: None,
                    start_after: None,
                    limit: None,
                },
            )
            .unwrap();

        let order = orders_owner.orders[0].clone();
        assert_eq!(order.remaining, Uint128::zero());
        assert_eq!(order.filled, Uint128::from(100000u128));

        let orders_user2: OrdersResponse = app
            .wrap()
            .query_wasm_smart(
                contract.clone(),
                &QueryMsg::Orders {
                    owner: Some(user2.clone().to_string()),
                    side: None,
                    start_after: None,
                    limit: None,
                },
            )
            .unwrap();
        let order = orders_user2.orders[0].clone();
        assert_eq!(order.remaining, Uint128::zero());
        assert_eq!(order.filled, Uint128::from(1000000u128));

        let orders_user3: OrdersResponse = app
            .wrap()
            .query_wasm_smart(
                contract.clone(),
                &QueryMsg::Orders {
                    owner: Some(user3.clone().to_string()),
                    side: None,
                    start_after: None,
                    limit: None,
                },
            )
            .unwrap();
        let order = orders_user3.orders[0].clone();
        assert_eq!(order.remaining, Uint128::zero());
        assert_eq!(order.filled, Uint128::from(500000u128));
    }

    #[test]
    fn test_first_partial_then_full() {
        let (mut app, contract) = setup();
        let owner = app.api().addr_make("owner");
        let funds = vec![
            coin(500_000_000_000_000, "btc-btc"),
            coin(500_000_000_000_000, "eth-usdc"),
        ];
        app.init_modules(|router, _, storage| {
            router
                .bank
                .init_balance(storage, &owner, funds.clone())
                .unwrap();
        });

        let user = app.api().addr_make("user");
        app.init_modules(|router, _, storage| {
            router
                .bank
                .init_balance(storage, &user, funds.clone())
                .unwrap();
        });

        let user2 = app.api().addr_make("user2");
        app.init_modules(|router, _, storage| {
            router
                .bank
                .init_balance(storage, &user2, funds.clone())
                .unwrap();
        });

        let user3 = app.api().addr_make("user3");
        app.init_modules(|router, _, storage| {
            router
                .bank
                .init_balance(storage, &user3, funds.clone())
                .unwrap();
        });

        app.execute_contract(
            owner.clone(),
            contract.clone(),
            &ExecuteMsg::Order((
                vec![(
                    Side::Base,
                    Price::Fixed(Decimal::from_str("1000.00").unwrap()),
                    Some(Uint128::from(100u128)),
                )],
                None,
            )),
            &funds,
        )
        .unwrap();

        app.execute_contract(
            user2.clone(),
            contract.clone(),
            &ExecuteMsg::Order((
                vec![(
                    Side::Base,
                    Price::Fixed(Decimal::from_str("1000.00").unwrap()),
                    Some(Uint128::from(1000u128)),
                )],
                None,
            )),
            &funds,
        )
        .unwrap();

        app.execute_contract(
            user3.clone(),
            contract.clone(),
            &ExecuteMsg::Order((
                vec![(
                    Side::Base,
                    Price::Fixed(Decimal::from_str("1000.00").unwrap()),
                    Some(Uint128::from(500u128)),
                )],
                None,
            )),
            &funds,
        )
        .unwrap();

        // First swap with same amount as price
        let swap_amount = coin(1_000_000, "eth-usdc");

        let res = app
            .execute_contract(
                user.clone(),
                contract.clone(),
                &ExecuteMsg::Swap(SwapRequest::Yolo {
                    to: None,
                    callback: None,
                }),
                &[swap_amount],
            )
            .unwrap();

        res.assert_event(&Event::new("wasm-rujira-fin/trade").add_attributes(vec![
            ("rate", "1000"),
            ("price", "fixed:1000"),
            ("offer", "1000000"),
            ("bid", "1000"),
            ("side", "base"),
        ]));

        let orders_owner: OrdersResponse = app
            .wrap()
            .query_wasm_smart(
                contract.clone(),
                &QueryMsg::Orders {
                    owner: Some(owner.clone().to_string()),
                    side: None,
                    start_after: None,
                    limit: None,
                },
            )
            .unwrap();

        let order = orders_owner.orders[0].clone();
        assert_eq!(order.remaining, Uint128::from(37u128));
        assert_eq!(order.filled, Uint128::from(62500u128));

        let orders_user2: OrdersResponse = app
            .wrap()
            .query_wasm_smart(
                contract.clone(),
                &QueryMsg::Orders {
                    owner: Some(user2.clone().to_string()),
                    side: None,
                    start_after: None,
                    limit: None,
                },
            )
            .unwrap();
        let order = orders_user2.orders[0].clone();
        assert_eq!(order.remaining, Uint128::from(375u128));
        assert_eq!(order.filled, Uint128::from(625000u128));

        let orders_user3: OrdersResponse = app
            .wrap()
            .query_wasm_smart(
                contract.clone(),
                &QueryMsg::Orders {
                    owner: Some(user3.clone().to_string()),
                    side: None,
                    start_after: None,
                    limit: None,
                },
            )
            .unwrap();
        let order = orders_user3.orders[0].clone();
        assert_eq!(order.remaining, Uint128::from(187u128));
        assert_eq!(order.filled, Uint128::from(312500u128));

        //Second swap with enough amount to cover all the bids
        let swap_amount = coin(1_600_000, "eth-usdc");

        let res = app
            .execute_contract(
                user.clone(),
                contract.clone(),
                &ExecuteMsg::Swap(SwapRequest::Yolo {
                    to: None,
                    callback: None,
                }),
                &[swap_amount],
            )
            .unwrap();

        res.assert_event(&Event::new("wasm-rujira-fin/trade").add_attributes(vec![
            ("rate", "1000"),
            ("price", "fixed:1000"),
            ("offer", "600000"),
            ("bid", "600"),
            ("side", "base"),
        ]));

        let orders_owner: OrdersResponse = app
            .wrap()
            .query_wasm_smart(
                contract.clone(),
                &QueryMsg::Orders {
                    owner: Some(owner.clone().to_string()),
                    side: None,
                    start_after: None,
                    limit: None,
                },
            )
            .unwrap();

        let order = orders_owner.orders[0].clone();
        assert_eq!(order.remaining, Uint128::zero());
        assert_eq!(order.filled, Uint128::from(100000u128));

        let orders_user2: OrdersResponse = app
            .wrap()
            .query_wasm_smart(
                contract.clone(),
                &QueryMsg::Orders {
                    owner: Some(user2.clone().to_string()),
                    side: None,
                    start_after: None,
                    limit: None,
                },
            )
            .unwrap();
        let order = orders_user2.orders[0].clone();
        assert_eq!(order.remaining, Uint128::zero());
        assert_eq!(order.filled, Uint128::from(1000000u128));

        let orders_user3: OrdersResponse = app
            .wrap()
            .query_wasm_smart(
                contract.clone(),
                &QueryMsg::Orders {
                    owner: Some(user3.clone().to_string()),
                    side: None,
                    start_after: None,
                    limit: None,
                },
            )
            .unwrap();
        let order = orders_user3.orders[0].clone();
        assert_eq!(order.remaining, Uint128::zero());
        assert_eq!(order.filled, Uint128::from(500000u128));
    }
}
