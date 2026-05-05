use cosmwasm_std::Deps;
use rujira_rs::fin::{OrderResponse, OrdersResponse, Price, Side};
use rujira_rs::Premiumable;

use crate::ContractError;

use super::{Order, Pool};

pub fn query_order(
    deps: Deps,
    owner: &str,
    side: Side,
    price: Price,
    oracle: &Option<impl Premiumable>,
) -> Result<OrderResponse, ContractError> {
    let addr = deps.api.addr_validate(owner)?;
    let pool = Pool::load(deps.storage, &price, &side, oracle)?;
    let order = pool.load_order(deps.storage, &addr)?;
    order_response(&order, &pool.side, &pool.price, oracle)
}

pub fn query_orders(
    deps: Deps,
    owner: Option<&str>,
    side: Option<Side>,
    start_after: Option<(String, Side, Price)>,
    limit: Option<u8>,
    oracle: &Option<impl Premiumable>,
) -> Result<OrdersResponse, ContractError> {
    let orders: Result<Vec<OrderResponse>, ContractError> = match owner {
        Some(owner) => {
            let addr = deps.api.addr_validate(owner)?;
            let cursor = start_after.map(|(_, s, p)| (s, p));
            Order::by_owner(deps.storage, &addr, side, cursor, limit)?
        }
        None => {
            let cursor = start_after
                .map(|(a, s, p)| Ok::<_, ContractError>((deps.api.addr_validate(&a)?, s, p)))
                .transpose()?;
            Order::all(deps.storage, side, cursor, limit)?
        }
    }
    .iter_mut()
    .map(|(k, order)| {
        let pool = Pool::load(deps.storage, &k.price, &k.side, oracle)?;
        pool.sync_order(deps.storage, order)?;
        order_response(order, &k.side, &k.price, oracle)
    })
    .collect();

    Ok(OrdersResponse { orders: orders? })
}

fn order_response(
    order: &Order,
    side: &Side,
    price: &Price,
    oracle: &Option<impl Premiumable>,
) -> Result<OrderResponse, ContractError> {
    Ok(OrderResponse {
        owner: order.owner.to_string(),
        side: side.clone(),
        price: price.clone(),
        rate: price.to_rate(oracle),
        updated_at: order.updated_at,
        offer: order.offer,
        remaining: order.bid.amount().try_into().unwrap(),
        filled: order.bid.filled().try_into().unwrap(),
    })
}
