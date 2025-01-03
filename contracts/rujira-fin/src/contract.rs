#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{to_json_binary, Binary, Deps, DepsMut, Env, MessageInfo, Response};
use cw2::set_contract_version;
use rujira_rs::fin::{
    BookResponse, ConfigResponse, ExecuteMsg, InstantiateMsg, OrderResponse, OrdersResponse, Price,
    QueryMsg, Side, StatusResponse, SudoMsg,
};

use crate::config::{Config, CONFIG};
use crate::error::ContractError;
use crate::order::Order;
use crate::pool::Pool;

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:rujira-fin";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    let config = Config::new(deps.api, msg);
    // config.validate()?;
    // config.save(deps.storage)?;
    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    _deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    _msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn sudo(_deps: DepsMut, _env: Env, _msg: SudoMsg) -> Result<Response, ContractError> {
    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> Result<Binary, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    // let rate = Rate::load(&deps.querier, &config)?;
    match msg {
        QueryMsg::Config {} => to_json_binary(&ConfigResponse::from(config)),
        QueryMsg::Status {} => to_json_binary(&StatusResponse::Active),
        QueryMsg::Order((owner, side, price)) => {
            let addr = deps.api.addr_validate(&owner)?;
            let oracle = todo!();
            let pool = Pool::load(deps.storage, &price, &side, &oracle);
            let order = pool.load_order(deps.storage, &addr)?;
            to_json_binary(&order_response(&order, &pool.side, &pool.price))
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
                    .map(|order| {
                        // TODO: Decode from key in by_owner
                        let side = todo!();
                        let price = todo!();
                        let oracle = todo!();
                        let pool = Pool::load(deps.storage, &price, &side, &oracle);
                        pool.sync_order(deps.storage, &mut order)?;
                        Ok(order_response(order, &side, &price))
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

fn order_response(order: &Order, side: &Side, price: &Price) -> OrderResponse {
    OrderResponse {
        owner: order.owner.to_string(),
        side: side.clone(),
        price: price.clone(),
        updated_at: order.updated_at,
        offer: order.offer,
        remaining: order.bid.amount().try_into().unwrap(),
        filled: order.bid.filled().try_into().unwrap(),
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use cosmwasm_std::Decimal;
    use cw_multi_test::{BasicApp, ContractWrapper, Executor};
    use rujira_rs::Tick;

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
}
