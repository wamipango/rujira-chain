use cosmwasm_std::{coin, Addr, Coin, Decimal, Event, Storage, Timestamp, Uint128};
use cw_utils::NativeBalance;
use rujira_rs::fin::{Price, Side};
use std::cmp::Ordering;

use crate::{
    events::{event_create_order, event_increase_order, event_retract_order, event_withdraw_order},
    order::Order,
    pool::Pool,
    ContractError,
};

pub fn execute_orders(
    storage: &mut dyn Storage,
    owner: &Addr,
    timestamp: &Timestamp,
    oracle: &Decimal,
    funds: &NativeBalance,
    o: Vec<(Side, Price, Uint128)>,
) -> Result<ExecutionResult, ContractError> {
    // NativeBalance can't be negative. Store in and out separately and we'll validate
    // no negative balances at the end
    // What we receive from the user and withdrawn and retracted orders
    let mut receive = funds.clone();
    // What we spend creating and increasing orders
    let mut send = NativeBalance::default();
    let mut events: Vec<Event> = vec![];

    for (side, price, target) in o {
        let mut pool = Pool::load(storage, &price, &side, oracle);
        match pool.load_order(storage, owner) {
            Ok(mut order) => {
                let res = maybe_withdraw(storage, &mut pool, &mut order)?;
                match res {
                    Some((event, amount)) => {
                        receive += amount;
                        events.push(event);
                    }
                    None => {}
                }
                let amount = Uint128::try_from(order.bid.amount()).unwrap();

                match amount.cmp(&target) {
                    Ordering::Less => {
                        let amount = coin(
                            pool.increase_order(storage, &mut order, timestamp, amount.clone())?
                                .u128(),
                            "usdc",
                        );
                        send += amount.clone();
                        events.push(event_increase_order(&pool, &mut order, &amount));
                    }
                    Ordering::Greater => {
                        let amount = coin(
                            pool.retract_order(
                                storage,
                                &mut order,
                                timestamp,
                                Some(amount.clone()),
                            )?
                            .u128(),
                            "usdc",
                        );
                        receive += amount.clone();
                        events.push(event_retract_order(&pool, &mut order, &amount));
                    }
                    Ordering::Equal => {}
                }
            }
            Err(ContractError::NotFound {}) => {
                let order = pool.create_order(storage, timestamp, owner, target)?;
                let amount = coin(order.amount().u128(), "usdc");
                send += amount.clone();
                events.push(event_create_order(&pool, &order));
            }
            Err(_) => {}
        }
    }

    for x in send.into_vec() {
        receive = (receive - x)?;
    }

    Ok(ExecutionResult {
        events,
        withdraw: receive.clone(),
    })
}

pub struct ExecutionResult {
    pub withdraw: NativeBalance,
    pub events: Vec<Event>,
}

fn maybe_withdraw(
    storage: &mut dyn Storage,
    pool: &mut Pool,
    order: &mut Order,
) -> Result<Option<(Event, Coin)>, ContractError> {
    if order.bid.filled().is_zero() {
        return Ok(None);
    }
    let amount = coin(pool.claim_order(storage, order)?.u128(), "ruji");

    Ok(Some((event_withdraw_order(pool, order, &amount), amount)))
}
