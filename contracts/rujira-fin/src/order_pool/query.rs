use cosmwasm_std::{Deps, StdResult};
use rujira_rs::fin::{OrderResponse, OrdersResponse, Price, Side};
use rujira_rs::Premiumable;

use crate::ContractError;

use super::{Order, Pool};

pub fn query_order(
    deps: Deps,
    owner: &str,
    side: Side,
    price: Price,
    oracle: &impl Premiumable,
) -> Result<OrderResponse, ContractError> {
    let addr = deps.api.addr_validate(owner)?;
    let pool = Pool::load(deps.storage, &price, &side, oracle)?;
    let order = pool.load_order(deps.storage, &addr)?;
    Ok(order_response(&order, &pool.side, &pool.price, oracle)?)
}

pub fn query_orders(
    deps: Deps,
    owner: &str,
    side: Option<Side>,
    offset: Option<u8>,
    limit: Option<u8>,
    oracle: &impl Premiumable,
) -> Result<OrdersResponse, ContractError> {
    let addr = deps.api.addr_validate(owner)?;
    let orders: Result<Vec<OrderResponse>, ContractError> =
        Order::by_owner(deps.storage, &addr, side, offset, limit)?
            .iter_mut()
            .map(|(k, order)| {
                let pool = Pool::load(deps.storage, &k.price, &k.side, oracle)?;
                pool.sync_order(deps.storage, order)?;
                Ok(order_response(order, &k.side, &k.price, oracle)?)
            })
            .collect();

    Ok(OrdersResponse { orders: orders? })
}

fn order_response(
    order: &Order,
    side: &Side,
    price: &Price,
    oracle: &impl Premiumable,
) -> StdResult<OrderResponse> {
    Ok(OrderResponse {
        owner: order.owner.to_string(),
        side: side.clone(),
        price: price.clone(),
        rate: price.to_rate(oracle)?,
        updated_at: order.updated_at,
        offer: order.offer,
        remaining: order.bid.amount().try_into().unwrap(),
        filled: order.bid.filled().try_into().unwrap(),
    })
}
