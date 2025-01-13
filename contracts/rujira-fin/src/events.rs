use cosmwasm_std::{Attribute, Event, Uint128};
use rujira_rs::{fin::Side, Swappable};

use crate::{order::Order, pool::Pool, ContractError};

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

pub fn event_trade<T: Swappable<ContractError>>(
    s: &T,
    side: &Side,
    offer: Uint128,
    bid: Uint128,
    attributes: &Vec<Attribute>,
) -> Event {
    Event::new("rujira-fin/trade")
        .add_attribute("side", side.to_string())
        .add_attribute("rate", s.rate().to_string())
        .add_attribute("offer", offer.to_string())
        .add_attribute("bid", bid.to_string())
        .add_attributes(attributes.clone())
}
