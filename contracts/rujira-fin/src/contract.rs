#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_json_binary, BankMsg, Binary, CosmosMsg, Decimal, Deps, DepsMut, Env, MessageInfo,
    QuerierWrapper, Response,
};
use cw2::set_contract_version;
use cw_utils::NativeBalance;
use rujira_rs::fin::{
    BookResponse, ConfigResponse, ExecuteMsg, InstantiateMsg, OrderResponse, OrdersResponse, Price,
    QueryMsg, Side, StatusResponse, SudoMsg,
};
use rujira_rs::query;

use crate::config::{Config, CONFIG};
use crate::error::ContractError;
use crate::order::Order;
use crate::orders::execute_orders;
use crate::pool::Pool;

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
        ExecuteMsg::Swap { .. } => todo!(),
        ExecuteMsg::Order(vec) => {
            let res = execute_orders(
                deps.storage,
                &info.sender,
                &env.block.time,
                &oracle,
                &NativeBalance(info.funds),
                vec,
            )?;
            let withdraw_message = CosmosMsg::Bank(BankMsg::Send {
                to_address: info.sender.to_string(),
                amount: res.withdraw.into_vec(),
            });
            Ok(Response::default()
                .add_message(withdraw_message)
                .add_events(res.events))
        }
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn sudo(_deps: DepsMut, _env: Env, _msg: SudoMsg) -> Result<Response, ContractError> {
    Ok(Response::default())
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
                        Ok(order_response(&order, &k.side, &k.price, &oracle))
                    })
                    .collect();

            to_json_binary(&OrdersResponse { orders: orders? })
        }
        QueryMsg::Book { limit, offset } => {
            let limit = limit.unwrap_or(100);
            let offset = offset.unwrap_or(0);

            to_json_binary(&BookResponse {
                base: todo!(),
                quote: todo!(),
            })
        }
        QueryMsg::Simulate(offer) => todo!(),
    }
    .map_err(ContractError::Std)
}

fn load_oracle_price(q: QuerierWrapper, config: &Config) -> Result<Decimal, ContractError> {
    let a = query::Pool::load(q, &config.oracles[0])?.asset_tor_price;
    let b = query::Pool::load(q, &config.oracles[1])?.asset_tor_price;
    Ok(a * b)
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
    use cosmwasm_std::Decimal;
    use cw_multi_test::{BasicApp, ContractWrapper, Executor};
    use rujira_rs::{Layer1Asset, Tick};
    use rujira_rs_testing::mock_rujira_app;

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
                denoms: ["ruji".to_string(), "eth-usdc".to_string()],
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
                    denoms: ["btc-btc".to_string(), "eth-usdc".to_string()],
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
                price: Decimal::from_str("93319.1524675569444582").unwrap()
            }
        );
    }
}
