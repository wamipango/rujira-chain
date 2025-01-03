use cosmwasm_std::{coin, Addr, Coin, Decimal, Event, Storage, Timestamp, Uint128};
use cw_utils::NativeBalance;
use rujira_rs::fin::{Denoms, Price, Side};
use std::cmp::Ordering;

use crate::{
    events::{event_create_order, event_increase_order, event_retract_order, event_withdraw_order},
    order::Order,
    pool::Pool,
    ContractError,
};

pub struct OrderManager {
    denoms: Denoms,
    owner: Addr,
    timestamp: Timestamp,
    oracle: Decimal,
    // NativeBalance can't be negative. Store in and out separately and we'll validate
    // no negative balances at the end
    // What we receive from the user and withdrawn and retracted orders
    receive: NativeBalance,
    // What we spend creating and increasing orders
    send: NativeBalance,
    events: Vec<Event>,
}

impl OrderManager {
    pub fn new(
        denoms: Denoms,
        owner: Addr,
        timestamp: Timestamp,
        oracle: Decimal,
        funds: NativeBalance,
    ) -> Self {
        Self {
            denoms,
            owner,
            timestamp,
            oracle,
            receive: funds,
            send: NativeBalance::default(),
            events: vec![],
        }
    }

    pub fn execute_orders(
        &mut self,
        storage: &mut dyn Storage,
        o: Vec<(Side, Price, Uint128)>,
    ) -> Result<ExecutionResult, ContractError> {
        for (side, price, target) in o {
            let mut pool = Pool::load(storage, &price, &side, &self.oracle);
            match pool.load_order(storage, &self.owner) {
                Ok(mut order) => {
                    self.execute_existing_order(storage, &mut pool, &mut order, &side, target)?
                }
                Err(ContractError::NotFound {}) => {
                    self.execute_new_order(storage, &mut pool, &side, target)?
                }
                Err(err) => return Err(err),
            }
        }

        for x in self.send.clone().into_vec() {
            self.receive = (self.receive.clone() - x)?;
        }

        Ok(self.into())
    }

    fn execute_existing_order(
        &mut self,
        storage: &mut dyn Storage,
        pool: &mut Pool,
        order: &mut Order,
        side: &Side,
        target: Uint128,
    ) -> Result<(), ContractError> {
        let res = maybe_withdraw(storage, &self.denoms, pool, order)?;
        if let Some((event, amount)) = res {
            self.receive += amount;
            self.events.push(event);
        }
        let amount = Uint128::try_from(order.bid.amount()).unwrap();

        match amount.cmp(&target) {
            Ordering::Less => {
                let diff = target - amount;

                let amount = pool.increase_order(storage, order, &self.timestamp, diff)?;
                let coins = coin(amount.u128(), self.denoms.bid(side.clone()));
                self.send += coins;
                self.events.push(event_increase_order(&pool, &order, &diff));
            }
            Ordering::Greater => {
                let diff = amount - target;
                let amount = pool.retract_order(storage, order, &self.timestamp, Some(diff))?;
                let coins = coin(amount.u128(), self.denoms.bid(side.clone()));
                self.receive += coins;
                self.events.push(event_retract_order(&pool, &order, &diff));
            }
            Ordering::Equal => {}
        }
        Ok(())
    }

    fn execute_new_order(
        &mut self,
        storage: &mut dyn Storage,
        pool: &mut Pool,
        side: &Side,
        target: Uint128,
    ) -> Result<(), ContractError> {
        let order = pool.create_order(storage, &self.timestamp, &self.owner, target)?;
        let coins = coin(order.amount().u128(), self.denoms.bid(side.clone()));
        self.send += coins;
        self.events.push(event_create_order(&pool, &order));
        Ok(())
    }
}

impl From<&mut OrderManager> for ExecutionResult {
    fn from(e: &mut OrderManager) -> Self {
        Self {
            withdraw: e.receive.clone(),
            events: e.events.clone(),
        }
    }
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
    use cosmwasm_std::{
        coins,
        testing::{message_info, mock_dependencies, mock_env},
    };
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

        let mut e = OrderManager::new(
            Denoms::new("ruji", "usdc"),
            info.sender,
            env.block.time,
            oracle,
            funds,
        );

        let res = e
            .execute_orders(
                &mut deps.storage,
                vec![(
                    Side::Quote,
                    Price::Fixed(Decimal::from_str("1.0").unwrap()),
                    Uint128::from(1000u128),
                )],
            )
            .unwrap();

        assert_eq!(res.withdraw, NativeBalance::default());
        let event = res.events[0].clone();
        assert_eq!(event.ty, "rujira-fin/order.create");
        assert_eq!(event.attributes[0].key, "owner");
        assert_eq!(event.attributes[0].value, "addr0000");
        assert_eq!(event.attributes[1].key, "side");
        assert_eq!(event.attributes[1].value, "Quote");
        assert_eq!(event.attributes[2].key, "price");
        assert_eq!(event.attributes[2].value, "fixed:1");
        assert_eq!(event.attributes[3].key, "offer");
        assert_eq!(event.attributes[3].value, "1000");
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

        let mut e = OrderManager::new(
            Denoms::new("ruji", "usdc"),
            info.sender,
            env.block.time,
            oracle,
            funds,
        );

        let res = e
            .execute_orders(
                &mut deps.storage,
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
        let returned = NativeBalance(vec![coin(7500, "ruji"), coin(7000, "usdc")]);
        assert_eq!(res.withdraw, returned);
        let event = res.events[0].clone();
        assert_eq!(event.ty, "rujira-fin/order.create");
        assert_eq!(event.attributes[0].key, "owner");
        assert_eq!(event.attributes[0].value, "addr0000");
        assert_eq!(event.attributes[1].key, "side");
        assert_eq!(event.attributes[1].value, "Quote");
        assert_eq!(event.attributes[2].key, "price");
        assert_eq!(event.attributes[2].value, "fixed:1");
        assert_eq!(event.attributes[3].key, "offer");
        assert_eq!(event.attributes[3].value, "1000");

        let event = res.events[1].clone();
        assert_eq!(event.ty, "rujira-fin/order.create");
        assert_eq!(event.attributes[0].key, "owner");
        assert_eq!(event.attributes[0].value, "addr0000");
        assert_eq!(event.attributes[1].key, "side");
        assert_eq!(event.attributes[1].value, "Quote");
        assert_eq!(event.attributes[2].key, "price");
        assert_eq!(event.attributes[2].value, "oracle:0");
        assert_eq!(event.attributes[3].key, "offer");
        assert_eq!(event.attributes[3].value, "2000");

        let event = res.events[2].clone();
        assert_eq!(event.ty, "rujira-fin/order.create");
        assert_eq!(event.attributes[0].key, "owner");
        assert_eq!(event.attributes[0].value, "addr0000");
        assert_eq!(event.attributes[1].key, "side");
        assert_eq!(event.attributes[1].value, "Base");
        assert_eq!(event.attributes[2].key, "price");
        assert_eq!(event.attributes[2].value, "oracle:0");
        assert_eq!(event.attributes[3].key, "offer");
        assert_eq!(event.attributes[3].value, "1200");

        let event = res.events[3].clone();
        assert_eq!(event.ty, "rujira-fin/order.create");
        assert_eq!(event.attributes[0].key, "owner");
        assert_eq!(event.attributes[0].value, "addr0000");
        assert_eq!(event.attributes[1].key, "side");
        assert_eq!(event.attributes[1].value, "Base");
        assert_eq!(event.attributes[2].key, "price");
        assert_eq!(event.attributes[2].value, "fixed:1.3");
        assert_eq!(event.attributes[3].key, "offer");
        assert_eq!(event.attributes[3].value, "1300");
    }

    #[test]
    fn test_out_of_funds() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        let info = message_info(&Addr::unchecked("addr0000"), &[]);

        let oracle = Decimal::from_str("1.0").unwrap();
        let funds = NativeBalance::default();
        let mut e = OrderManager::new(
            Denoms::new("ruji", "usdc"),
            info.sender,
            env.block.time,
            oracle,
            funds,
        );

        e.execute_orders(
            &mut deps.storage,
            vec![(
                Side::Quote,
                Price::Fixed(Decimal::from_str("1.0").unwrap()),
                Uint128::from(1000u128),
            )],
        )
        .unwrap_err();
    }

    #[test]
    fn test_moving_orders() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        let info = message_info(&Addr::unchecked("addr0000"), &[]);

        let oracle = Decimal::from_str("1.0").unwrap();
        let mut funds = NativeBalance::default();
        funds += coin(10000, "usdc");
        funds += coin(10000, "ruji");
        let mut e = OrderManager::new(
            Denoms::new("ruji", "usdc"),
            info.sender.clone(),
            env.block.time,
            oracle,
            funds,
        );

        // Same as above
        e.execute_orders(
            &mut deps.storage,
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

        let mut e = OrderManager::new(
            Denoms::new("ruji", "usdc"),
            info.sender.clone(),
            env.block.time,
            oracle,
            NativeBalance::default(),
        );

        let res = e
            .execute_orders(
                &mut deps.storage,
                vec![
                    (
                        Side::Quote,
                        Price::Fixed(Decimal::from_str("0.99").unwrap()),
                        Uint128::from(1000u128),
                    ),
                    (
                        Side::Quote,
                        Price::Fixed(Decimal::from_str("1.0").unwrap()),
                        Uint128::zero(),
                    ),
                    // Split 1200 ito 2 x 600
                    (Side::Base, Price::Oracle(0), Uint128::from(600u128)),
                    (Side::Base, Price::Oracle(10), Uint128::from(600u128)),
                ],
            )
            .unwrap();

        let returned = NativeBalance::default();
        assert_eq!(res.withdraw, returned);
        assert_eq!(res.events.len(), 4);

        let event = res.events[0].clone();
        assert_eq!(event.ty, "rujira-fin/order.create");
        assert_eq!(event.attributes[0].key, "owner");
        assert_eq!(event.attributes[0].value, "addr0000");
        assert_eq!(event.attributes[1].key, "side");
        assert_eq!(event.attributes[1].value, "Quote");
        assert_eq!(event.attributes[2].key, "price");
        assert_eq!(event.attributes[2].value, "fixed:0.99");
        assert_eq!(event.attributes[3].key, "offer");
        assert_eq!(event.attributes[3].value, "1000");

        let event = res.events[1].clone();
        assert_eq!(event.ty, "rujira-fin/order.retract");
        assert_eq!(event.attributes[0].key, "owner");
        assert_eq!(event.attributes[0].value, "addr0000");
        assert_eq!(event.attributes[1].key, "side");
        assert_eq!(event.attributes[1].value, "Quote");
        assert_eq!(event.attributes[2].key, "price");
        assert_eq!(event.attributes[2].value, "fixed:1");
        assert_eq!(event.attributes[3].key, "amount");
        assert_eq!(event.attributes[3].value, "1000");

        let event = res.events[2].clone();
        assert_eq!(event.ty, "rujira-fin/order.retract");
        assert_eq!(event.attributes[0].key, "owner");
        assert_eq!(event.attributes[0].value, "addr0000");
        assert_eq!(event.attributes[1].key, "side");
        assert_eq!(event.attributes[1].value, "Base");
        assert_eq!(event.attributes[2].key, "price");
        assert_eq!(event.attributes[2].value, "oracle:0");
        assert_eq!(event.attributes[3].key, "amount");
        assert_eq!(event.attributes[3].value, "600");

        let event = res.events[3].clone();
        assert_eq!(event.ty, "rujira-fin/order.create");
        assert_eq!(event.attributes[0].key, "owner");
        assert_eq!(event.attributes[0].value, "addr0000");
        assert_eq!(event.attributes[1].key, "side");
        assert_eq!(event.attributes[1].value, "Base");
        assert_eq!(event.attributes[2].key, "price");
        assert_eq!(event.attributes[2].value, "oracle:10");
        assert_eq!(event.attributes[3].key, "offer");
        assert_eq!(event.attributes[3].value, "600");

        let mut e = OrderManager::new(
            Denoms::new("ruji", "usdc"),
            info.sender.clone(),
            env.block.time,
            oracle,
            NativeBalance(coins(300, "ruji")),
        );

        // Move 300 over and increase by 300 more with fresh funds
        let res = e
            .execute_orders(
                &mut deps.storage,
                vec![
                    (Side::Base, Price::Oracle(0), Uint128::from(300u128)),
                    (Side::Base, Price::Oracle(10), Uint128::from(1200u128)),
                ],
            )
            .unwrap();

        let returned = NativeBalance::default();
        assert_eq!(res.withdraw, returned);
        assert_eq!(res.events.len(), 2);

        let event = res.events[0].clone();
        assert_eq!(event.ty, "rujira-fin/order.retract");
        assert_eq!(event.attributes[0].key, "owner");
        assert_eq!(event.attributes[0].value, "addr0000");
        assert_eq!(event.attributes[1].key, "side");
        assert_eq!(event.attributes[1].value, "Base");
        assert_eq!(event.attributes[2].key, "price");
        assert_eq!(event.attributes[2].value, "oracle:0");
        assert_eq!(event.attributes[3].key, "amount");
        assert_eq!(event.attributes[3].value, "300");

        let event = res.events[1].clone();
        assert_eq!(event.ty, "rujira-fin/order.increase");
        assert_eq!(event.attributes[0].key, "owner");
        assert_eq!(event.attributes[0].value, "addr0000");
        assert_eq!(event.attributes[1].key, "side");
        assert_eq!(event.attributes[1].value, "Base");
        assert_eq!(event.attributes[2].key, "price");
        assert_eq!(event.attributes[2].value, "oracle:10");
        assert_eq!(event.attributes[3].key, "amount");
        assert_eq!(event.attributes[3].value, "600");
    }
}
