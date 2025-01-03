use cosmwasm_std::{coin, Addr, Coin, Decimal, Event, Storage, Timestamp, Uint128};
use cw_utils::NativeBalance;
use rujira_rs::fin::{Denoms, Price, Side};
use std::cmp::Ordering;

use crate::{
    config::Config,
    events::{event_create_order, event_increase_order, event_retract_order, event_withdraw_order},
    order::Order,
    pool::Pool,
    ContractError,
};

pub fn execute_orders(
    storage: &mut dyn Storage,
    denoms: &Denoms,
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
                let res = maybe_withdraw(storage, denoms, &mut pool, &mut order)?;
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
                        let amount =
                            pool.increase_order(storage, &mut order, timestamp, amount.clone())?;
                        let coins = coin(amount.u128(), denoms.bid(side));
                        send += coins;
                        events.push(event_increase_order(&pool, &mut order, &amount));
                    }
                    Ordering::Greater => {
                        let amount = pool.retract_order(
                            storage,
                            &mut order,
                            timestamp,
                            Some(amount.clone()),
                        )?;
                        let coins = coin(amount.u128(), denoms.bid(side));
                        receive += coins;
                        events.push(event_retract_order(&pool, &mut order, &amount));
                    }
                    Ordering::Equal => {}
                }
            }
            Err(ContractError::NotFound {}) => {
                let order = pool.create_order(storage, timestamp, owner, target)?;
                let coins = coin(order.amount().u128(), denoms.bid(side));
                send += coins;
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

#[derive(Debug)]
pub struct ExecutionResult {
    pub withdraw: NativeBalance,
    pub events: Vec<Event>,
}

fn maybe_withdraw(
    storage: &mut dyn Storage,
    denoms: &Denoms,
    pool: &mut Pool,
    order: &mut Order,
) -> Result<Option<(Event, Coin)>, ContractError> {
    if order.bid.filled().is_zero() {
        return Ok(None);
    }
    let amount = pool.claim_order(storage, order)?;
    let coins = coin(amount.u128(), denoms.ask(pool.side.clone()));

    Ok(Some((event_withdraw_order(pool, order, &amount), coins)))
}

#[cfg(test)]

mod tests {
    use super::*;
    use cosmwasm_std::testing::{message_info, mock_dependencies, mock_env};
    use rujira_rs::fin::Price;
    use std::str::FromStr;

    #[test]
    fn test_simple_success() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        let info = message_info(&Addr::unchecked("addr0000"), &[]);
        let oracle = Decimal::from_str("1.0").unwrap();
        let mut funds = NativeBalance::default();
        funds += coin(1000, "usdc");

        let res = execute_orders(
            &mut deps.storage,
            &Denoms::new("ruji", "uusd"),
            &info.sender,
            &env.block.time,
            &oracle,
            &funds,
            vec![(
                Side::Quote,
                Price::Fixed(Decimal::from_str("1.0").unwrap()),
                Uint128::from(1000u128),
            )],
        )
        .unwrap();

        assert_eq!(res.withdraw, NativeBalance::default());

        dbg!(res);
    }

    #[test]
    fn test_multiple_orders() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        let info = message_info(&Addr::unchecked("addr0000"), &[]);

        let oracle = Decimal::from_str("1.0").unwrap();
        let mut funds = NativeBalance::default();
        funds += coin(10000, "usdc");
        funds += coin(10000, "ruji");

        let res = execute_orders(
            &mut deps.storage,
            &Denoms::new("ruji", "usdc"),
            &info.sender,
            &env.block.time,
            &oracle,
            &funds,
            vec![
                (
                    Side::Quote,
                    Price::Fixed(Decimal::from_str("1.0").unwrap()),
                    Uint128::from(1000u128),
                ),
                (Side::Quote, Price::Oracle(0), Uint128::from(2000u128)),
                (Side::Base, Price::Oracle(0), Uint128::from(1200u128)),
                (
                    Side::Base,
                    Price::Fixed(Decimal::from_str("1.3").unwrap()),
                    Uint128::from(1300u128),
                ),
            ],
        )
        .unwrap();
        dbg!(res);
    }
}
