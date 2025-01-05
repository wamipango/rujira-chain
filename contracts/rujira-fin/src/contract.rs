#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    coin, coins, to_json_binary, BankMsg, Binary, CosmosMsg, Decimal, Deps, DepsMut, Env,
    MessageInfo, QuerierWrapper, Response,
};
use cw2::set_contract_version;
use cw_utils::{one_coin, NativeBalance};
use rujira_rs::fin::{
    BookResponse, ConfigResponse, ExecuteMsg, InstantiateMsg, OrderResponse, OrdersResponse, Price,
    QueryMsg, Side, StatusResponse, SudoMsg,
};
use rujira_rs::query;

use crate::config::{Config, CONFIG};
use crate::error::ContractError;
use crate::order::Order;
use crate::orders::OrderManager;
use crate::pool::Pool;
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

    match msg {
        ExecuteMsg::Swap { min_return, to, .. } => {
            let to = to.map(|x| deps.api.addr_validate(&x)).transpose()?;
            let funds = one_coin(&info)?;
            let side = config.denoms.ask_side(&funds)?;
            let res = Swapper::new(
                side.clone(),
                funds.amount,
                min_return,
                oracle,
                config.fee_taker,
            )
            .swap(deps.storage)?;

            let mut funds = NativeBalance(vec![
                coin(res.return_amount.u128(), config.denoms.bid(&side)),
                coin(res.remaining_offer.u128(), config.denoms.ask(&side)),
            ]);

            funds.normalize();

            let mut messages: Vec<CosmosMsg> = vec![];

            if !funds.is_empty() {
                messages.push(CosmosMsg::Bank(BankMsg::Send {
                    to_address: to.unwrap_or(info.sender).to_string(),
                    amount: funds.into_vec(),
                }))
            }

            if !res.fee_amount.is_zero() {
                messages.push(CosmosMsg::Bank(BankMsg::Send {
                    to_address: config.fee_address.to_string(),
                    amount: coins(res.fee_amount.u128(), config.denoms.bid(&side)),
                }))
            }

            Ok(Response::default()
                .add_messages(messages)
                .add_events(res.events))
        }
        ExecuteMsg::Order(vec) => {
            let mut e = OrderManager::new(
                config.denoms,
                config.fee_maker,
                config.tick,
                info.sender.clone(),
                env.block.time,
                oracle,
                NativeBalance(info.funds),
            );

            let res = e.execute_orders(deps.storage, vec)?;

            let mut messages: Vec<CosmosMsg> = vec![];

            if !res.withdraw.is_empty() {
                messages.push(CosmosMsg::Bank(BankMsg::Send {
                    to_address: info.sender.to_string(),
                    amount: res.withdraw.into_vec(),
                }))
            }

            if !res.fees.is_empty() {
                messages.push(CosmosMsg::Bank(BankMsg::Send {
                    to_address: config.fee_address.to_string(),
                    amount: res.fees.into_vec(),
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
        SudoMsg::Pause {} => todo!(),
        SudoMsg::Resume {} => todo!(),
        SudoMsg::UpdateConfig {
            tick,
            fee_taker,
            fee_maker,
            fee_address,
        } => {
            let fee_address = fee_address
                .map(|x| deps.api.addr_validate(&x))
                .transpose()?;
            let mut config = CONFIG.load(deps.storage)?;
            config.update(tick, fee_taker, fee_maker, fee_address);
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
        QueryMsg::Order((owner, side, price)) => {
            let addr = deps.api.addr_validate(&owner)?;
            let pool = Pool::load(deps.storage, &price, &side, &oracle);
            let order = pool.load_order(deps.storage, &addr)?;
            to_json_binary(&order_response(&order, &pool.side, &pool.price, &oracle))
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
                        let pool = Pool::load(deps.storage, &k.price, &k.side, &oracle);
                        pool.sync_order(deps.storage, order)?;
                        Ok(order_response(order, &k.side, &k.price, &oracle))
                    })
                    .collect();

            to_json_binary(&OrdersResponse { orders: orders? })
        }
        QueryMsg::Book { limit, offset } => {
            let limit = limit.unwrap_or(100);
            let offset = offset.unwrap_or(0);

            let base = Pool::side(deps.storage, &Side::Base, &oracle)
                .skip(offset as usize)
                .take(limit as usize)
                .map(Pool::combine)
                .collect();

            let quote = Pool::side(deps.storage, &Side::Quote, &oracle)
                .skip(offset as usize)
                .take(limit as usize)
                .map(Pool::combine)
                .collect();

            to_json_binary(&BookResponse { base, quote })
        }
        QueryMsg::Simulate(_offer) => todo!(),
    }
    .map_err(ContractError::Std)
}

fn load_oracle_price(q: QuerierWrapper, config: &Config) -> Result<Decimal, ContractError> {
    let a = query::Pool::load(q, &config.oracles[0])?.asset_tor_price;
    let b = query::Pool::load(q, &config.oracles[1])?.asset_tor_price;
    Ok(a / b)
}

fn order_response(order: &Order, side: &Side, price: &Price, oracle: &Decimal) -> OrderResponse {
    OrderResponse {
        owner: order.owner.to_string(),
        side: side.clone(),
        price: price.clone(),
        rate: price.to_rate(oracle),
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
    use rujira_rs::{
        fin::{Denoms, Tick},
        Layer1Asset,
    };
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
                    oracles: [
                        Layer1Asset::try_from("BTC.BTC").unwrap(),
                        Layer1Asset::try_from(
                            "ETH.USDC-0XA0B86991C6218B36C1D19D4A2E9EB0CE3606EB48",
                        )
                        .unwrap(),
                    ],
                    tick: Tick::new(6u8),
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
                tick: Tick::new(4u8),
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
                    tick: Tick::new(4u8),
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
                (Side::Base, Price::Oracle(0), Uint128::from(10000u128)),
                (
                    Side::Base,
                    Price::Fixed(Decimal::from_str("100000").unwrap()),
                    Uint128::from(10000u128),
                ),
                (
                    Side::Base,
                    Price::Fixed(Decimal::from_str("93317").unwrap()),
                    Uint128::from(10000u128),
                ),
                (
                    Side::Base,
                    Price::Fixed(Decimal::from_str("93219").unwrap()),
                    Uint128::from(21000u128),
                ),
                (
                    Side::Base,
                    Price::Fixed(Decimal::from_str("91219").unwrap()),
                    Uint128::from(51000u128),
                ),
                (Side::Quote, Price::Oracle(1), Uint128::from(10000u128)),
                (
                    Side::Quote,
                    Price::Fixed(Decimal::from_str("100010").unwrap()),
                    Uint128::from(12500u128),
                ),
                (
                    Side::Quote,
                    Price::Fixed(Decimal::from_str("87900").unwrap()),
                    Uint128::from(51000u128),
                ),
            ]),
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
        assert_eq!(entry.total, Uint128::from(51000u128));
        let entry = book.base[1].clone();
        assert_eq!(entry.price, Decimal::from_str("93219").unwrap());
        assert_eq!(entry.total, Uint128::from(21000u128));
        let entry = book.base[2].clone();
        assert_eq!(entry.price, Decimal::from_str("93317").unwrap());
        assert_eq!(entry.total, Uint128::from(10000u128));
        let entry = book.base[3].clone();
        assert_eq!(entry.price, Decimal::from_str("100000").unwrap());
        assert_eq!(entry.total, Uint128::from(20000u128));

        assert_eq!(book.quote.len(), 2);
        let entry = book.quote[0].clone();
        assert_eq!(entry.price, Decimal::from_str("100010").unwrap());
        assert_eq!(entry.total, Uint128::from(22500u128));
        let entry = book.quote[1].clone();
        assert_eq!(entry.price, Decimal::from_str("87900").unwrap());
        assert_eq!(entry.total, Uint128::from(51000u128));

        let balance = app.wrap().query_all_balances(owner.to_string()).unwrap();
        // Total 102000 btc submitted on base side
        // Total 73500 usdc submitted on quote side
        assert_eq!(
            balance,
            vec![
                coin(500_000_000 - 102000, "btc-btc"),
                coin(500_000_000 - 73500, "eth-usdc")
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
            &ExecuteMsg::Order(vec![
                (Side::Base, Price::Oracle(0), Uint128::from(10000u128)),
                (
                    Side::Base,
                    Price::Fixed(Decimal::from_str("100000").unwrap()),
                    Uint128::from(10000u128),
                ),
                (
                    Side::Base,
                    Price::Fixed(Decimal::from_str("93317").unwrap()),
                    Uint128::from(10000u128),
                ),
                (
                    Side::Base,
                    Price::Fixed(Decimal::from_str("93219").unwrap()),
                    Uint128::from(21000u128),
                ),
                (
                    Side::Base,
                    Price::Fixed(Decimal::from_str("91219").unwrap()),
                    Uint128::from(51000u128),
                ),
                (Side::Quote, Price::Oracle(1), Uint128::from(10000000u128)),
                (
                    Side::Quote,
                    Price::Fixed(Decimal::from_str("100010").unwrap()),
                    Uint128::from(12500000u128),
                ),
                (
                    Side::Quote,
                    Price::Fixed(Decimal::from_str("87900").unwrap()),
                    Uint128::from(51000000u128),
                ),
            ]),
            &funds,
        )
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
                &coins(10_000_000, "eth-usdc"),
            )
            .unwrap();
        // Buying 10m USDC-worth of BTC
        // Best price = 91219
        // 10_000_000 / 91219 = 109
        res.assert_event(&Event::new("wasm-rujira-fin/trade").add_attributes(vec![
            ("side", "base"),
            ("rate", "91219"),
            ("offer", "10000000"),
            ("bid", "109"),
        ]));
        res.assert_event(&Event::new("transfer").add_attributes(vec![
            ("recipient", owner.as_str()),
            ("sender", contract.as_str()),
            ("amount", "109btc-btc"),
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
                &coins(300, "btc-btc"),
            )
            .unwrap();
        // Buying 10m USDC-worth of BTC
        // Best price = 91219
        // 10_000_000 / 91219 = 109
        res.assert_event(&Event::new("wasm-rujira-fin/trade").add_attributes(vec![
            ("side", "quote"),
            ("rate", "100010"),
            ("offer", "225"),
            ("bid", "22500000"),
        ]));
        res.assert_event(&Event::new("wasm-rujira-fin/trade").add_attributes(vec![
            ("side", "quote"),
            ("rate", "87900"),
            ("offer", "75"),
            ("bid", "6592500"),
        ]));
        res.assert_event(&Event::new("transfer").add_attributes(vec![
            ("recipient", owner.as_str()),
            ("sender", contract.as_str()),
            ("amount", "29092500eth-usdc"),
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
            ("rate", "87900"),
            ("offer", "506"),
            ("bid", "44407500"),
        ]));
        res.assert_event(&Event::new("transfer").add_attributes(vec![
            ("recipient", owner.as_str()),
            ("sender", contract.as_str()),
            ("amount", "494btc-btc,44407500eth-usdc"),
        ]));
    }
}
