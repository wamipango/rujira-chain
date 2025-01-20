use crate::config::{Config, CONFIG};
use crate::error::ContractError;
use crate::market_maker::{MarketMaker, MarketMakerOffer};
use crate::oracle::Oracle;
use crate::order::Order;
use crate::order_manager::OrderManager;
use crate::pool::Pool;
use crate::premium::Premium;
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    coin, coins, to_json_binary, BankMsg, Binary, CosmosMsg, Deps, DepsMut, Env, MessageInfo,
    Response, WasmMsg,
};
use cw2::set_contract_version;
use cw_utils::{must_pay, NativeBalance};
use itertools::{EitherOrBoth, Itertools};
use rujira_rs::bow;
use rujira_rs::exchange::{SwapError, SwapResult, Swappable, Swapper};
use rujira_rs::fin::OraclePrice;
use rujira_rs::orca::{
    ConfigResponse, ExecuteMsg, InstantiateMsg, OrderResponse, OrdersResponse, PoolResponse,
    PoolsResponse, QueryMsg, SimulationResponse, SudoMsg,
};

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
    let oracle = Oracle::load(deps.querier, &config.oracles)?;
    let mut fees = NativeBalance::default();
    let mut messages: Vec<CosmosMsg> = vec![];

    match msg {
        ExecuteMsg::Swap { min_return, to, .. } => {
            let to = to.map(|x| deps.api.addr_validate(&x)).transpose()?;
            let funds = must_pay(&info, config.denoms.ask())?;
            let mut swapper = Swapper::new(funds, min_return, config.fee_taker);
            let res = simulate_swap(&mut swapper, deps.as_ref(), &oracle, &config)?;
            let commit = swapper.commit(deps.storage)?;

            if let Some(mm) = config.market_maker {
                if !commit.market_maker.1.is_zero() {
                    messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
                        contract_addr: mm.to_string(),
                        msg: to_json_binary(&bow::ExecuteMsg::Swap {
                            min_return: coin(commit.market_maker.0.u128(), config.denoms.bid()),
                            to: None,
                            callback: None,
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
                NativeBalance(info.funds),
            );

            let res = e.execute_orders(deps.storage, vec, &oracle)?;
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
    let oracle = Oracle::load(deps.querier, &config.oracles)?;
    match msg {
        QueryMsg::Config {} => to_json_binary(&ConfigResponse::from(config)),
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
            let mut swapper = Swapper::new(offer.amount, None, config.fee_taker);
            let res = simulate_swap(&mut swapper, deps, &oracle, &config)?;
            to_json_binary(&SimulationResponse {
                returned: res.return_amount,
                fee: res.fee_amount,
            })
        }
    }
    .map_err(ContractError::Std)
}

fn simulate_swap(
    swapper: &mut Swapper<EitherOrBoth<Pool, MarketMakerOffer>>,
    deps: Deps,
    oracle: &impl OraclePrice,
    config: &Config,
) -> Result<SwapResult, SwapError> {
    let market_maker = MarketMaker::new(
        deps.querier,
        config.market_maker.clone(),
        config.denoms.clone(),
    );

    let mut iter = Pool::iter(deps.storage, oracle)
        .merge_join_by(market_maker, |x, y| x.premium.cmp(&y.premium));

    swapper.swap(&mut iter)
}

fn order_response(order: &Order, premium: &u8, oracle: &Oracle) -> OrderResponse {
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
    fn query_pools() {
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
        assert_eq!(entry.price, Decimal::from_str("990").unwrap());
        assert_eq!(entry.total, Uint128::from(10000u128));
        let entry = pools.pools[1].clone();
        assert_eq!(entry.price, Decimal::from_str("900").unwrap());
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
        let swap_amount = coin(100, "btc-btc");

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
        res.assert_event(&Event::new("wasm-rujira-orca/trade").add_attributes(vec![
            ("offer", "100"),
            ("bid", "90000"),
            ("rate", "900"),
            ("premium", "10"),
        ]));
        res.assert_event(&Event::new("transfer").add_attributes(vec![
            ("recipient", owner.as_str()),
            ("sender", contract.as_str()),
            ("amount", "90000eth-usdc"),
        ]));

        assert_eq!(sim.returned, Uint128::from(90000u128));

        let res = app
            .execute_contract(
                owner.clone(),
                contract.clone(),
                &ExecuteMsg::Swap {
                    min_return: None,
                    to: None,
                    callback: None,
                },
                &coins(30000, "btc-btc"),
            )
            .unwrap();
        res.assert_event(&Event::new("wasm-rujira-orca/trade").add_attributes(vec![
            ("offer", "11012"),
            ("bid", "9910000"),
            ("rate", "900"),
        ]));
        res.assert_event(&Event::new("wasm-rujira-orca/trade").add_attributes(vec![
            ("offer", "14706"),
            ("bid", "12500000"),
            ("rate", "850"),
        ]));
        res.assert_event(&Event::new("wasm-rujira-orca/trade").add_attributes(vec![
            ("offer", "4282"),
            ("bid", "3425600"),
            ("rate", "800"),
        ]));
        res.assert_event(&Event::new("transfer").add_attributes(vec![
            ("recipient", owner.as_str()),
            ("sender", contract.as_str()),
            ("amount", "25835600eth-usdc"),
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
                &coins(100000, "btc-btc"),
            )
            .unwrap();

        // Check that any unused funds are returned
        res.assert_event(&Event::new("wasm-rujira-orca/trade").add_attributes(vec![
            ("offer", "59468"),
            ("bid", "47574400"),
            ("rate", "800"),
        ]));
        res.assert_event(&Event::new("transfer").add_attributes(vec![
            ("recipient", owner.as_str()),
            ("sender", contract.as_str()),
            ("amount", "40532btc-btc,47574400eth-usdc"),
        ]));
    }
}
