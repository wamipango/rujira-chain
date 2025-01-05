use cosmwasm_std::{Decimal, Event, Storage, Uint128};
use itertools::EitherOrBoth;
use rujira_rs::fin::Side;

use crate::{events::event_trade, pool::Pool, ContractError};

/// Generic trait to manage iterating through local structs as well as external market making contracts
pub trait Swappable {
    fn distribute(&mut self, offer: Uint128) -> Result<(Uint128, Uint128), ContractError>;
}

#[derive(Debug)]
pub struct Swapper {
    events: Vec<Event>,
    min_return: Option<Uint128>,
    offer: Uint128,
    oracle: Decimal,
    returned: Uint128,
    side: Side,
    pending_pools: Vec<Pool>,
}

impl Swapper {
    pub fn new(
        // The side of the book that is being consumed
        side: Side,
        offer: Uint128,
        min_return: Option<Uint128>,
        oracle: Decimal,
    ) -> Self {
        Self {
            events: vec![],
            min_return,
            offer,
            oracle,
            returned: Uint128::zero(),
            side,
            pending_pools: vec![],
        }
    }

    pub fn swap(&mut self, storage: &mut dyn Storage) -> Result<SwapResult, ContractError> {
        let iter = Pool::side(storage, &self.side, &self.oracle);
        for v in iter {
            if self.offer.is_zero() {
                break;
            }
            let mut offer = Uint128::zero();
            let mut returned = Uint128::zero();
            let price: Decimal;
            match v {
                EitherOrBoth::Both(mut a, mut b) => {
                    price = a.rate;

                    let offer_a =
                        Decimal::from_ratio(a.total() * self.offer, b.total() + a.total())
                            .to_uint_floor();
                    let offer_b = self.offer - offer_a;

                    let (consumed_offer, consumed_bids) = a.distribute(offer_a)?;
                    self.pending_pools.push(a);
                    offer += consumed_offer;
                    returned += consumed_bids;

                    let (consumed_offer, consumed_bids) = b.distribute(offer_b)?;
                    self.pending_pools.push(b);
                    offer += consumed_offer;
                    returned += consumed_bids;
                }
                EitherOrBoth::Left(mut x) => {
                    price = x.rate;
                    let (consumed_offer, consumed_bids) = x.distribute(self.offer)?;
                    self.pending_pools.push(x);
                    offer += consumed_offer;
                    returned += consumed_bids;
                }
                EitherOrBoth::Right(mut x) => {
                    price = x.rate;
                    let (consumed_offer, consumed_bids) = x.distribute(self.offer)?;
                    self.pending_pools.push(x);
                    offer += consumed_offer;
                    returned += consumed_bids;
                }
            }
            self.offer -= offer;
            self.returned += returned;
            self.events
                .push(event_trade(&self.side, &price, offer, returned));
        }

        if let Some(min_return) = self.min_return {
            if self.returned < min_return {
                return Err(ContractError::InsufficientReturn {
                    expected: min_return,
                    returned: self.returned,
                });
            }
        }

        for pool in self.pending_pools.iter_mut() {
            pool.save(storage)?;
        }

        Ok(self.into())
    }
}

#[derive(Debug)]
pub struct SwapResult {
    pub events: Vec<Event>,
    pub return_amount: Uint128,
    pub remaining_offer: Uint128,
}

impl From<&mut Swapper> for SwapResult {
    fn from(s: &mut Swapper) -> Self {
        Self {
            events: s.events.clone(),
            return_amount: s.returned,
            remaining_offer: s.offer,
        }
    }
}

#[cfg(test)]

mod tests {
    use crate::orders::OrderManager;

    use super::*;
    use cosmwasm_std::{
        coin,
        testing::{message_info, mock_env, MockStorage},
        Addr,
    };
    use cw_utils::NativeBalance;
    use rujira_rs::fin::{Denoms, Price, Tick};
    use std::str::FromStr;

    #[test]
    fn test_swap_execution() {
        let mut storage = MockStorage::new();
        let env = mock_env();
        let info = message_info(&Addr::unchecked("addr0000"), &[]);
        let oracle = Decimal::from_str("1.0").unwrap();
        let funds = NativeBalance(vec![coin(1_000_000, "ruji"), coin(1_000_000, "usdc")]);
        let denoms = Denoms::new("ruji", "usdc");
        let tick = Tick::new(4);
        let mut e = OrderManager::new(denoms, tick, info.sender, env.block.time, oracle, funds);

        e.execute_orders(
            &mut storage,
            // ruji: 7500
            // usdc: 0
            vec![
                // sell 1000 ruji for 1000 usdc
                // ruji: 6500
                // usdc: 1000
                (
                    Side::Quote,
                    Price::Fixed(Decimal::from_str("1.0").unwrap()),
                    Uint128::from(1000u128),
                ),
                // sell 1112 ruji for 1000 usdc
                // ruji: 5388
                // usdc: 2000
                (Side::Quote, Price::Oracle(-1000), Uint128::from(1000u128)),
                // sell 1112 ruji for 1000 usdc
                // ruji: 4276
                // usdc: 3000
                (
                    Side::Quote,
                    Price::Fixed(Decimal::from_str("0.9").unwrap()),
                    Uint128::from(1000u128),
                ),
                // Sell 1176 ruji for 1000 usdc
                // ruji: 3100
                // usdc: 4000
                (Side::Quote, Price::Oracle(-1500), Uint128::from(1000u128)),
                // Sell 1250 ruji for 1000 usdc
                // ruji: 1850
                // usdc: 5000
                (
                    Side::Quote,
                    Price::Fixed(Decimal::from_str("0.8").unwrap()),
                    Uint128::from(1000u128),
                ),
                // Sell 1429 ruji for 1000 usdc
                // ruji: 421
                // usdc: 6000
                (
                    Side::Quote,
                    Price::Fixed(Decimal::from_str("0.7").unwrap()),
                    Uint128::from(1000u128),
                ),
                // Sell 421 ruji for 252 usdc
                // ruji: 0
                // usdc: 6252
                (
                    Side::Quote,
                    Price::Fixed(Decimal::from_str("0.6").unwrap()),
                    Uint128::from(1000u128),
                ),
            ],
        )
        .unwrap();

        let mut s = Swapper::new(Side::Quote, Uint128::from(7500u128), None, oracle);
        let res = s.swap(&mut storage).unwrap();
        assert_eq!(res.return_amount, Uint128::from(6252u128));
        assert_eq!(res.remaining_offer, Uint128::zero());

        let event = res.events[0].clone();
        assert_eq!(event.ty, "rujira-fin/trade");
        assert_eq!(event.attributes[0].key, "side");
        assert_eq!(event.attributes[0].value, "quote");
        assert_eq!(event.attributes[1].key, "rate");
        assert_eq!(event.attributes[1].value, "1");
        assert_eq!(event.attributes[2].key, "offer");
        assert_eq!(event.attributes[2].value, "1000");
        assert_eq!(event.attributes[3].key, "bid");
        assert_eq!(event.attributes[3].value, "1000");

        let event = res.events[1].clone();
        assert_eq!(event.ty, "rujira-fin/trade");
        assert_eq!(event.attributes[0].key, "side");
        assert_eq!(event.attributes[0].value, "quote");
        assert_eq!(event.attributes[1].key, "rate");
        assert_eq!(event.attributes[1].value, "0.9");
        assert_eq!(event.attributes[2].key, "offer");
        assert_eq!(event.attributes[2].value, "2224");
        assert_eq!(event.attributes[3].key, "bid");
        assert_eq!(event.attributes[3].value, "2000");

        let event = res.events[2].clone();
        assert_eq!(event.ty, "rujira-fin/trade");
        assert_eq!(event.attributes[0].key, "side");
        assert_eq!(event.attributes[0].value, "quote");
        assert_eq!(event.attributes[1].key, "rate");
        assert_eq!(event.attributes[1].value, "0.85");
        assert_eq!(event.attributes[2].key, "offer");
        assert_eq!(event.attributes[2].value, "1177");
        assert_eq!(event.attributes[3].key, "bid");
        assert_eq!(event.attributes[3].value, "1000");

        let event = res.events[3].clone();
        assert_eq!(event.ty, "rujira-fin/trade");
        assert_eq!(event.attributes[0].key, "side");
        assert_eq!(event.attributes[0].value, "quote");
        assert_eq!(event.attributes[1].key, "rate");
        assert_eq!(event.attributes[1].value, "0.8");
        assert_eq!(event.attributes[2].key, "offer");
        assert_eq!(event.attributes[2].value, "1250");
        assert_eq!(event.attributes[3].key, "bid");
        assert_eq!(event.attributes[3].value, "1000");

        let event = res.events[4].clone();
        assert_eq!(event.ty, "rujira-fin/trade");
        assert_eq!(event.attributes[0].key, "side");
        assert_eq!(event.attributes[0].value, "quote");
        assert_eq!(event.attributes[1].key, "rate");
        assert_eq!(event.attributes[1].value, "0.7");
        assert_eq!(event.attributes[2].key, "offer");
        assert_eq!(event.attributes[2].value, "1429");
        assert_eq!(event.attributes[3].key, "bid");
        assert_eq!(event.attributes[3].value, "1000");

        let event = res.events[5].clone();
        assert_eq!(event.ty, "rujira-fin/trade");
        assert_eq!(event.attributes[0].key, "side");
        assert_eq!(event.attributes[0].value, "quote");
        assert_eq!(event.attributes[1].key, "rate");
        assert_eq!(event.attributes[1].value, "0.6");
        assert_eq!(event.attributes[2].key, "offer");
        assert_eq!(event.attributes[2].value, "420");
        assert_eq!(event.attributes[3].key, "bid");
        assert_eq!(event.attributes[3].value, "252");
    }
}
