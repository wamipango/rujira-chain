use cosmwasm_std::{Decimal, Event, StdResult, Storage, Uint128};
use itertools::{EitherOrBoth, Itertools};
use rujira_rs::{fin::Side, SwapCommit, Swappable};
use std::ops::Mul;

use crate::{
    events::event_trade,
    market_maker::{MarketMaker, MarketMakerOffer},
    pool::Pool,
    ContractError,
};

#[derive(Debug)]
pub struct Swapper {
    events: Vec<Event>,
    fee: Decimal,
    min_return: Option<Uint128>,
    offer: Uint128,
    oracle: Decimal,
    returned: Uint128,
    side: Side,
    pending: Vec<EitherOrBoth<EitherOrBoth<Pool>, MarketMakerOffer>>,
}

impl Swapper {
    pub fn new(
        // The side of the book that is being consumed
        side: Side,
        offer: Uint128,
        min_return: Option<Uint128>,
        oracle: Decimal,
        fee: Decimal,
    ) -> Self {
        Self {
            events: vec![],
            fee,
            min_return,
            offer,
            oracle,
            returned: Uint128::zero(),
            side,
            pending: vec![],
        }
    }

    pub fn swap(
        &mut self,
        storage: &dyn Storage,
        market_maker: &mut MarketMaker,
    ) -> Result<SwapResult, ContractError> {
        let iter = Pool::side(storage, &self.side, &self.oracle)
            .merge_join_by(market_maker, |x, y| x.rate().cmp(&y.price));
        for mut v in iter {
            if self.offer.is_zero() {
                break;
            }
            let (offer, bids) = v.swap(self.offer)?;
            let attrs = v.attributes();
            self.events
                .push(event_trade(&v, &self.side, offer, bids, &attrs));

            self.pending.push(v);
            self.offer -= offer;
            self.returned += bids;
        }

        let fee = Decimal::from_ratio(self.returned, 1u128)
            .mul(self.fee)
            .to_uint_ceil();

        self.returned -= fee;

        if let Some(min_return) = self.min_return {
            if self.returned < min_return {
                return Err(ContractError::InsufficientReturn {
                    expected: min_return,
                    returned: self.returned,
                });
            }
        }

        Ok(SwapResult {
            events: self.events.clone(),
            fee_amount: fee,
            return_amount: self.returned,
            remaining_offer: self.offer,
        })
    }

    pub fn commit(&self, storage: &mut dyn Storage) -> StdResult<SwapCommit> {
        let mut res = SwapCommit::default();
        for pool in self.pending.iter() {
            res += pool.commit(storage)?;
        }

        Ok(res)
    }
}

#[derive(Debug)]
pub struct SwapResult {
    pub events: Vec<Event>,
    pub fee_amount: Uint128,
    pub return_amount: Uint128,
    pub remaining_offer: Uint128,
}

#[cfg(test)]

mod tests {
    use crate::order_manager::OrderManager;

    use super::*;
    use cosmwasm_std::{
        coin,
        testing::{message_info, mock_env, MockQuerier, MockStorage},
        Addr, QuerierWrapper,
    };
    use cw_utils::NativeBalance;
    use rujira_rs::fin::{Denoms, Price, Tick};
    use std::str::FromStr;

    #[test]
    fn test_swap_execution() {
        let mut storage = MockStorage::new();
        let q = MockQuerier::default();
        let q = QuerierWrapper::new(&q);
        let env = mock_env();
        let info = message_info(&Addr::unchecked("addr0000"), &[]);
        let oracle = Decimal::from_str("1.0").unwrap();
        let funds = NativeBalance(vec![coin(1_000_000, "ruji"), coin(1_000_000, "usdc")]);
        let denoms = Denoms::new("ruji", "usdc");
        let tick = Tick::new(4);
        let fee = Decimal::from_str("0.001").unwrap();
        let mut mm = MarketMaker::new(q, denoms.clone(), Side::Base, None, Tick::new(2));

        let mut e = OrderManager::new(
            denoms,
            fee,
            tick,
            info.sender,
            env.block.time,
            oracle,
            funds,
        );

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

        let mut s = Swapper::new(
            Side::Quote,
            Uint128::from(7500u128),
            None,
            oracle,
            Decimal::from_str("0.001").unwrap(),
        );
        let res = s.swap(&mut storage, &mut mm).unwrap();
        assert_eq!(res.return_amount, Uint128::from(6245u128));
        assert_eq!(res.fee_amount, Uint128::from(7u128));
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
        assert_eq!(event.attributes[4].key, "price");
        assert_eq!(event.attributes[4].value, "fixed:1");

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
        assert_eq!(event.attributes[4].key, "price");
        assert_eq!(event.attributes[4].value, "fixed:0.9");

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
        assert_eq!(event.attributes[4].key, "price");
        assert_eq!(event.attributes[4].value, "oracle:-1500");

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
        assert_eq!(event.attributes[4].key, "price");
        assert_eq!(event.attributes[4].value, "fixed:0.8");

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
        assert_eq!(event.attributes[4].key, "price");
        assert_eq!(event.attributes[4].value, "fixed:0.7");

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
        assert_eq!(event.attributes[4].key, "price");
        assert_eq!(event.attributes[4].value, "fixed:0.6");
    }
}
