use cosmwasm_std::{Coin, Event, Uint128};
use rujira_rs::new_event;

use crate::{order::Order, pool::Pool};

pub fn event_create_order(pool: &Pool, order: &Order) -> Event {
    new_event("order.create")
        .add_attribute("owner", order.owner.clone())
        .add_attribute("side", pool.side.to_string())
        .add_attribute("price", pool.price.to_string())
        .add_attribute("offer", order.offer)
}

pub fn event_withdraw_order(pool: &Pool, order: &Order, amount: &Coin) -> Event {
    new_event("order.withdraw")
        .add_attribute("owner", order.owner.clone())
        .add_attribute("side", pool.side.to_string())
        .add_attribute("price", pool.price.to_string())
        .add_attribute("amount", amount.to_string())
}

pub fn event_increase_order(pool: &Pool, order: &Order, amount: &Coin) -> Event {
    new_event("order.increase")
        .add_attribute("owner", order.owner.clone())
        .add_attribute("side", pool.side.to_string())
        .add_attribute("price", pool.price.to_string())
        .add_attribute("amount", amount.to_string())
}

pub fn event_retract_order(pool: &Pool, order: &Order, amount: &Coin) -> Event {
    new_event("order.decrease")
        .add_attribute("owner", order.owner.clone())
        .add_attribute("side", pool.side.to_string())
        .add_attribute("price", pool.price.to_string())
        .add_attribute("amount", amount.to_string())
}
