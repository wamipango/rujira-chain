use cosmwasm_std::{Event, Uint128};

use crate::{order::Order, pool::Pool};

pub fn event_create_order(pool: &Pool, order: &Order) -> Event {
    Event::new("rujira-fin/order.create")
        .add_attribute("owner", order.owner.clone())
        .add_attribute("side", pool.side.to_string())
        .add_attribute("price", pool.price.to_string())
        .add_attribute("offer", order.offer)
}

pub fn event_withdraw_order(pool: &Pool, order: &Order, amount: &Uint128) -> Event {
    Event::new("rujira-fin/order.withdraw")
        .add_attribute("owner", order.owner.clone())
        .add_attribute("side", pool.side.to_string())
        .add_attribute("price", pool.price.to_string())
        .add_attribute("amount", amount.to_string())
}

pub fn event_increase_order(pool: &Pool, order: &Order, amount: &Uint128) -> Event {
    Event::new("rujira-fin/order.increase")
        .add_attribute("owner", order.owner.clone())
        .add_attribute("side", pool.side.to_string())
        .add_attribute("price", pool.price.to_string())
        .add_attribute("amount", amount.to_string())
}

pub fn event_retract_order(pool: &Pool, order: &Order, amount: &Uint128) -> Event {
    Event::new("rujira-fin/order.retract")
        .add_attribute("owner", order.owner.clone())
        .add_attribute("side", pool.side.to_string())
        .add_attribute("price", pool.price.to_string())
        .add_attribute("amount", amount.to_string())
}
