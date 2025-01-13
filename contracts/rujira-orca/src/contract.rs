#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    coin, coins, ensure_eq, to_json_binary, BankMsg, Binary, CosmosMsg, Decimal, Deps, DepsMut,
    Env, MessageInfo, QuerierWrapper, Response, WasmMsg,
};
use cw2::set_contract_version;
use cw_utils::{must_pay, one_coin, NativeBalance};
use rujira_rs::orca::{
    ConfigResponse, ExecuteMsg, InstantiateMsg, MarketMakerCallback, MarketMakerExecute,
    OrderResponse, OrdersResponse, PoolResponse, PoolsResponse, QueryMsg, SimulationResponse,
    StatusResponse, SudoMsg,
};
use rujira_rs::{query, Swappable};

use crate::config::{Config, CONFIG};
use crate::error::ContractError;
use crate::market_maker::MarketMaker;
use crate::order::Order;
use crate::order_manager::OrderManager;
use crate::pool::Pool;
use crate::premium::Premium;
use crate::swapper::Swapper;

// version info for migration info
const CONTRACT_NAME: &str = env!("CARGO_PKG_NAME");
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    let config = Config::new(deps.api, msg)?;
    config.validate()?;
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
    let oracle = load_oracle_price(deps.querier, &config)?;
    let mut fees = NativeBalance::default();
    let mut messages: Vec<CosmosMsg> = vec![];

    match msg {
        ExecuteMsg::Swap { min_return, to, .. } => {
            let to = to.map(|x| deps.api.addr_validate(&x)).transpose()?;
            let funds = must_pay(&info, config.denoms.ask())?;
            let mut mm = MarketMaker::new(
                deps.querier,
                config.denoms.clone(),
                config.market_maker.clone(),
            );
            let mut swapper = Swapper::new(funds, min_return, oracle, config.fee_taker);
            let res = swapper.swap(deps.storage, &mut mm)?;
            let commit = swapper.commit(deps.storage)?;
            if let Some(mm) = config.market_maker {
                if !commit.market_maker.1.is_zero() {
                    messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
                        contract_addr: mm.to_string(),
                        msg: to_json_binary(&MarketMakerExecute {
                            min_return: coin(commit.market_maker.0.u128(), config.denoms.bid()),
                        })?,
                        funds: coins(commit.market_maker.1.u128(), config.denoms.ask()),
                    }))
                }
            }

            let mut funds = NativeBalance(vec![
                coin(res.return_amount.u128(), config.denoms.bid()),
                coin(res.remaining_offer.u128(), config.denoms.ask()),
            ]);

            funds.normalize();

            if !funds.is_empty() {
                messages.push(CosmosMsg::Bank(BankMsg::Send {
                    to_address: to.unwrap_or(info.sender).to_string(),
                    amount: funds.into_vec(),
                }))
            }

            fees += coin(res.fee_amount.u128(), config.denoms.bid());

            fees.normalize();

            if !fees.is_empty() {
                messages.push(CosmosMsg::Bank(BankMsg::Send {
                    to_address: config.fee_address.to_string(),
                    amount: fees.into_vec(),
                }))
            }

            Ok(Response::default()
                .add_messages(messages)
                .add_events(res.events))
        }
        ExecuteMsg::Order(vec) => {
            let mut e = OrderManager::new(
                config.denoms.clone(),
                config.fee_maker,
                config.max_premium,
                info.sender.clone(),
                env.block.time,
                oracle,
                NativeBalance(info.funds),
            );

            let res = e.execute_orders(deps.storage, vec)?;
            fees += res.fees;

            if !res.withdraw.is_empty() {
                messages.push(CosmosMsg::Bank(BankMsg::Send {
                    to_address: info.sender.to_string(),
                    amount: res.withdraw.into_vec(),
                }))
            }

            fees.normalize();

            if !fees.is_empty() {
                messages.push(CosmosMsg::Bank(BankMsg::Send {
                    to_address: config.fee_address.to_string(),
                    amount: fees.into_vec(),
                }))
            }

            Ok(Response::default()
                .add_messages(messages)
                .add_events(res.events))
        }

        ExecuteMsg::MarketMaker(MarketMakerCallback { min_return }) => match config.market_maker {
            Some(mm) => {
                ensure_eq!(mm, info.sender, ContractError::Unauthorized {});
                let amount = one_coin(&info)?;
                ensure_eq!(
                    min_return,
                    amount,
                    ContractError::InsufficientFunds {
                        expected: min_return,
                        returned: amount
                    }
                );
                Ok(Response::default())
            }
            None => Err(ContractError::Unauthorized {}),
        },
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn sudo(deps: DepsMut, _env: Env, msg: SudoMsg) -> Result<Response, ContractError> {
    match msg {
        SudoMsg::UpdateConfig {
            fee_taker,
            fee_maker,
            fee_address,
            market_maker,
        } => {
            let fee_address = fee_address
                .map(|x| deps.api.addr_validate(&x))
                .transpose()?;

            let market_maker = market_maker
                .map(|x| deps.api.addr_validate(&x))
                .transpose()?;
            let mut config = CONFIG.load(deps.storage)?;

            config.update(market_maker, fee_taker, fee_maker, fee_address);
            config.save(deps.storage)?;
            Ok(Response::default())
        }
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> Result<Binary, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let oracle = load_oracle_price(deps.querier, &config)?;
    match msg {
        QueryMsg::Config {} => to_json_binary(&ConfigResponse::from(config)),
        QueryMsg::Status {} => to_json_binary(&StatusResponse::Active { price: oracle }),
        QueryMsg::Order((owner, premium)) => {
            let addr = deps.api.addr_validate(&owner)?;
            let pool = Pool::load(deps.storage, &premium, &oracle);
            let order = pool.load_order(deps.storage, &addr)?;
            to_json_binary(&order_response(&order, &pool.premium, &oracle))
        }
        QueryMsg::Orders {
            owner,
            offset,
            limit,
        } => {
            let addr = deps.api.addr_validate(owner.as_str())?;
            let orders: Result<Vec<OrderResponse>, ContractError> =
                Order::by_owner(deps.storage, &addr, offset, limit)?
                    .iter_mut()
                    .map(|(k, order)| {
                        let pool = Pool::load(deps.storage, &k, &oracle);
                        pool.sync_order(deps.storage, order)?;
                        Ok(order_response(order, &k, &oracle))
                    })
                    .collect();

            to_json_binary(&OrdersResponse { orders: orders? })
        }
        QueryMsg::Pools { limit, offset } => {
            let limit = limit.unwrap_or(100);
            let offset = offset.unwrap_or(0);

            let pools = Pool::iter(deps.storage, &oracle)
                .skip(offset as usize)
                .take(limit as usize)
                .map(|v| PoolResponse {
                    premium: v.premium,
                    epoch: v.pool.epoch(),
                    price: v.rate(),
                    total: v.total(),
                })
                .collect();

            to_json_binary(&PoolsResponse { pools })
        }
        QueryMsg::Simulate(offer) => {
            let mut swapper = Swapper::new(offer.amount, None, oracle, config.fee_taker);
            let mut mm = MarketMaker::new(deps.querier, config.denoms, config.market_maker);
            let res = swapper.swap(deps.storage, &mut mm)?;
            to_json_binary(&SimulationResponse {
                returned: res.return_amount,
                fee: res.fee_amount,
            })
        }
    }
    .map_err(ContractError::Std)
}

fn load_oracle_price(q: QuerierWrapper, config: &Config) -> Result<Decimal, ContractError> {
    let a = query::Pool::load(q, &config.oracles[0])?.asset_tor_price;
    let b = query::Pool::load(q, &config.oracles[1])?.asset_tor_price;
    Ok(a / b)
}

fn order_response(order: &Order, premium: &u8, oracle: &Decimal) -> OrderResponse {
    OrderResponse {
        owner: order.owner.to_string(),
        premium: *premium,
        rate: premium.to_rate(oracle),
        updated_at: order.updated_at,
        offer: order.offer,
        remaining: order.bid.amount().try_into().unwrap(),
        filled: order.bid.filled().try_into().unwrap(),
    }
}

#[cfg(test)]
mod tests {

    use std::str::FromStr;

    use super::*;
    use cosmwasm_std::{coin, coins, Addr, Decimal, Event, Uint128};
    use cw_multi_test::{BasicApp, ContractWrapper, Executor};
    use rujira_rs::{orca::Denoms, Layer1Asset};
    use rujira_rs_testing::{mock_rujira_app, RujiraApp};

    fn setup() -> (RujiraApp, Addr) {
        let mut app = mock_rujira_app();

        let owner = app.api().addr_make("owner");

        let code = Box::new(ContractWrapper::new(execute, instantiate, query));
        let code_id = app.store_code(code);
        let contract = app
            .instantiate_contract(
                code_id,
                owner,
                &InstantiateMsg {
                    denoms: Denoms::new("btc-btc", "eth-usdc"),
                    market_maker: None,
                    max_premium: 30,
                    oracles: [
                        Layer1Asset::try_from("BTC.BTC").unwrap(),
                        Layer1Asset::try_from(
                            "ETH.USDC-0XA0B86991C6218B36C1D19D4A2E9EB0CE3606EB48",
                        )
                        .unwrap(),
                    ],
                    fee_taker: Decimal::zero(),
                    fee_maker: Decimal::zero(),
                    fee_address: app.api().addr_make("fee").to_string(),
                },
                &[],
                "template",
                None,
            )
            .unwrap();

        (app, contract)
    }

    #[test]
    fn instantiation() {
        let mut app = BasicApp::default();
        let owner = app.api().addr_make("owner");

        let code = Box::new(ContractWrapper::new(execute, instantiate, query));
        let code_id = app.store_code(code);
        app.instantiate_contract(
            code_id,
            owner,
            &InstantiateMsg {
                denoms: Denoms::new("ruji", "eth-usdc"),
                oracles: [
                    Layer1Asset::try_from("THOR.RUJI").unwrap(),
                    Layer1Asset::try_from("ETH.USDC").unwrap(),
                ],
                max_premium: 30,
                market_maker: None,
                fee_taker: Decimal::zero(),
                fee_maker: Decimal::zero(),
                fee_address: app.api().addr_make("fee").to_string(),
            },
            &[],
            "template",
            None,
        )
        .unwrap();
    }

    #[test]
    fn oracle_price() {
        let mut app = mock_rujira_app();
        let owner = app.api().addr_make("owner");

        let code = Box::new(ContractWrapper::new(execute, instantiate, query));
        let code_id = app.store_code(code);
        let contract = app
            .instantiate_contract(
                code_id,
                owner,
                &InstantiateMsg {
                    denoms: Denoms::new("btc-btc", "eth-usdc"),
                    oracles: [
                        Layer1Asset::try_from("BTC.BTC").unwrap(),
                        Layer1Asset::try_from(
                            "ETH.USDC-0XA0B86991C6218B36C1D19D4A2E9EB0CE3606EB48",
                        )
                        .unwrap(),
                    ],
                    max_premium: 30,
                    market_maker: None,
                    fee_taker: Decimal::zero(),
                    fee_maker: Decimal::zero(),
                    fee_address: app.api().addr_make("fee").to_string(),
                },
                &[],
                "template",
                None,
            )
            .unwrap();

        let status: StatusResponse = app
            .wrap()
            .query_wasm_smart(contract, &QueryMsg::Status {})
            .unwrap();

        assert_eq!(
            status,
            StatusResponse::Active {
                price: Decimal::from_str("100000").unwrap()
            }
        );
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
            &ExecuteMsg::Order(vec![
                (1, Uint128::from(10000u128)),
                (10, Uint128::from(12500u128)),
                (20, Uint128::from(51000u128)),
            ]),
            &funds,
        )
        .unwrap();

        let pools: PoolsResponse = app
            .wrap()
            .query_wasm_smart(
                contract,
                &QueryMsg::Pools {
                    limit: None,
                    offset: None,
                },
            )
            .unwrap();

        assert_eq!(pools.pools.len(), 3);
        let entry = pools.pools[0].clone();
        assert_eq!(entry.price, Decimal::from_str("99000").unwrap());
        assert_eq!(entry.total, Uint128::from(10000u128));
        let entry = pools.pools[1].clone();
        assert_eq!(entry.price, Decimal::from_str("90000").unwrap());
        assert_eq!(entry.total, Uint128::from(12500u128));

        let balance = app.wrap().query_all_balances(owner.to_string()).unwrap();
        assert_eq!(
            balance,
            vec![coin(500_000_000, "btc-btc"), coin(499_926_500, "eth-usdc")]
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
            &ExecuteMsg::Order(vec![
                (10, Uint128::from(10000000u128)),
                (15, Uint128::from(12500000u128)),
                (20, Uint128::from(51000000u128)),
            ]),
            &funds,
        )
        .unwrap();
        let swap_amount = coin(10_000_000, "btc-btc");

        let sim: SimulationResponse = app
            .wrap()
            .query_wasm_smart(contract.clone(), &QueryMsg::Simulate(swap_amount.clone()))
            .unwrap();

        let res = app
            .execute_contract(
                owner.clone(),
                contract.clone(),
                &ExecuteMsg::Swap {
                    min_return: None,
                    to: None,
                    callback: None,
                },
                &vec![swap_amount],
            )
            .unwrap();
        // Buying 10m USDC-worth of BTC
        // Best price = 91219
        // 10_000_000 / 91219 = 109
        res.assert_event(&Event::new("wasm-rujira-fin/trade").add_attributes(vec![
            ("side", "base"),
            ("offer", "10000000"),
            ("bid", "109"),
            ("price", "fixed:91219"),
        ]));
        res.assert_event(&Event::new("transfer").add_attributes(vec![
            ("recipient", owner.as_str()),
            ("sender", contract.as_str()),
            ("amount", "109btc-btc"),
        ]));

        assert_eq!(sim.returned, Uint128::from(109u128));

        let res = app
            .execute_contract(
                owner.clone(),
                contract.clone(),
                &ExecuteMsg::Swap {
                    min_return: None,
                    to: None,
                    callback: None,
                },
                &coins(300, "btc-btc"),
            )
            .unwrap();

        // Buying 10m USDC-worth of BTC
        // Best price = 91219
        // 10_000_000 / 91219 = 109
        res.assert_event(&Event::new("wasm-rujira-fin/trade").add_attributes(vec![
            ("side", "quote"),
            ("offer", "251"),
            ("bid", "22500000"),
            ("price", "oracle:-1000"),
            ("price", "fixed:90000"),
        ]));
        res.assert_event(&Event::new("wasm-rujira-fin/trade").add_attributes(vec![
            ("side", "quote"),
            ("offer", "49"),
            ("bid", "4307100"),
            ("price", "fixed:87900"),
        ]));
        res.assert_event(&Event::new("transfer").add_attributes(vec![
            ("recipient", owner.as_str()),
            ("sender", contract.as_str()),
            ("amount", "26807100eth-usdc"),
        ]));

        let res = app
            .execute_contract(
                owner.clone(),
                contract.clone(),
                &ExecuteMsg::Swap {
                    min_return: None,
                    to: None,
                    callback: None,
                },
                &coins(1000, "btc-btc"),
            )
            .unwrap();

        // Check that any unused funds are returned
        res.assert_event(&Event::new("wasm-rujira-fin/trade").add_attributes(vec![
            ("side", "quote"),
            ("offer", "532"),
            ("bid", "46692900"),
            ("price", "fixed:87900"),
        ]));
        res.assert_event(&Event::new("transfer").add_attributes(vec![
            ("recipient", owner.as_str()),
            ("sender", contract.as_str()),
            ("amount", "468btc-btc,46692900eth-usdc"),
        ]));
    }
}
