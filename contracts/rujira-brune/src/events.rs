use std::ops::Add;

use cosmwasm_std::{Addr, Decimal, Env, Event, Order, StdResult, Storage, Timestamp, Uint128};
use cw_storage_plus::{Bound, Map};
use rujira_rs::brune::LoggedEvent;

/// Create an event log so we can cleanly see what the
static LOG: Map<u128, (u64, Timestamp, Event)> = Map::new("events");

fn store_event(storage: &mut dyn Storage, env: &Env, event: Event) -> StdResult<Event> {
    let last = LOG
        .keys(storage, None, None, Order::Descending)
        .next()
        .transpose()?
        .unwrap_or_default();
    LOG.save(
        storage,
        last.add(1),
        &(env.block.height, env.block.time, event.clone()),
    )?;
    Ok(event)
}

pub fn event_node_register(storage: &mut dyn Storage, env: &Env, node: &Addr) -> StdResult<Event> {
    store_event(
        storage,
        env,
        Event::new(format!("{}/node.register", env!("CARGO_PKG_NAME")))
            .add_attribute("node", node.to_string()),
    )
}

pub fn event_node_deregister(
    storage: &mut dyn Storage,
    env: &Env,
    node: &Addr,
) -> StdResult<Event> {
    store_event(
        storage,
        env,
        Event::new(format!("{}/node.deregister", env!("CARGO_PKG_NAME")))
            .add_attribute("node", node.to_string()),
    )
}

pub fn event_bond(
    storage: &mut dyn Storage,
    env: &Env,
    node: &Addr,
    amount: Uint128,
) -> StdResult<Event> {
    store_event(
        storage,
        env,
        Event::new(format!("{}/node.bond", env!("CARGO_PKG_NAME")))
            .add_attribute("node", node.to_string())
            .add_attribute("amount", amount),
    )
}

pub fn event_leave(storage: &mut dyn Storage, env: &Env, node: &Addr) -> StdResult<Event> {
    store_event(
        storage,
        env,
        Event::new(format!("{}/node.leave", env!("CARGO_PKG_NAME")))
            .add_attribute("node", node.to_string()),
    )
}

pub fn event_unbond(
    storage: &mut dyn Storage,
    env: &Env,
    node: &Addr,
    amount: Uint128,
) -> StdResult<Event> {
    store_event(
        storage,
        env,
        Event::new(format!("{}/node.unbond", env!("CARGO_PKG_NAME")))
            .add_attribute("node", node.to_string())
            .add_attribute("amount", amount),
    )
}

pub fn event_mint(recipient: &Addr, amount: Uint128, denom: &str) -> Event {
    Event::new(format!("{}/mint", env!("CARGO_PKG_NAME")))
        .add_attribute("recipient", recipient.to_string())
        .add_attribute("amount", amount)
        .add_attribute("denom", denom)
}

pub fn event_burn(sender: &Addr, amount: Uint128, denom: &str) -> Event {
    Event::new(format!("{}/burn", env!("CARGO_PKG_NAME")))
        .add_attribute("sender", sender.to_string())
        .add_attribute("amount", amount)
        .add_attribute("denom", denom)
}

pub fn event_warn(v: &str) -> Event {
    Event::new(format!("{}/warn", env!("CARGO_PKG_NAME"))).add_attribute("message", v)
}

pub fn event_fee_allocate(
    storage: &mut dyn Storage,
    env: &Env,
    amount: Decimal,
) -> StdResult<Event> {
    store_event(
        storage,
        env,
        Event::new(format!("{}/fee.allocate", env!("CARGO_PKG_NAME")))
            .add_attribute("amount", amount.to_string()),
    )
}

pub fn event_fee_distribute(
    storage: &mut dyn Storage,
    env: &Env,
    amount: Uint128,
) -> StdResult<Event> {
    store_event(
        storage,
        env,
        Event::new(format!("{}/fee.distribute", env!("CARGO_PKG_NAME")))
            .add_attribute("amount", amount),
    )
}

pub fn query_events(
    storage: &dyn Storage,
    start_after: Option<u128>,
    limit: Option<u32>,
) -> StdResult<Vec<LoggedEvent>> {
    let limit = limit.unwrap_or(50).min(100) as usize;
    let start = start_after.map(Bound::exclusive);
    LOG.range(storage, start, None, Order::Descending)
        .take(limit)
        .map(|item| {
            let (id, (height, time, event)) = item?;
            Ok(LoggedEvent {
                id,
                event,
                height,
                time,
            })
        })
        .collect()
}
