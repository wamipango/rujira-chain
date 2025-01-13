use cosmwasm_std::{Decimal, Event, StdResult, Storage, Uint128};
use itertools::{EitherOrBoth, Itertools};
use rujira_rs::{SwapCommit, Swappable};
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
    pending: Vec<EitherOrBoth<Pool, MarketMakerOffer>>,
}

impl Swapper {
    pub fn new(offer: Uint128, min_return: Option<Uint128>, oracle: Decimal, fee: Decimal) -> Self {
        Self {
            events: vec![],
            fee,
            min_return,
            offer,
            oracle,
            returned: Uint128::zero(),
            pending: vec![],
        }
    }

    pub fn swap(
        &mut self,
        storage: &dyn Storage,
        market_maker: &mut MarketMaker,
    ) -> Result<SwapResult, ContractError> {
        let iter = Pool::iter(storage, &self.oracle)
            .merge_join_by(market_maker, |x, y| x.rate().cmp(&y.rate()));
        for mut v in iter {
            if self.offer.is_zero() {
                break;
            }
            let (offer, bids) = v.swap(self.offer)?;
            let attrs = v.attributes();
            self.events.push(event_trade(&v, offer, bids, &attrs));

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
    use rujira_rs::orca::Denoms;
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
        let fee = Decimal::from_str("0.001").unwrap();
        let mut mm = MarketMaker::new(q, denoms.clone(), None);

        let mut e = OrderManager::new(denoms, fee, 30, info.sender, env.block.time, oracle, funds);

        e.execute_orders(
            &mut storage,
            vec![
                (0, Uint128::from(1000u128)),
                (1, Uint128::from(1000u128)),
                (4, Uint128::from(1000u128)),
                (5, Uint128::from(1000u128)),
                (7, Uint128::from(1000u128)),
                (9, Uint128::from(1000u128)),
                (12, Uint128::from(1000u128)),
                (15, Uint128::from(1000u128)),
            ],
        )
        .unwrap();

        let mut s = Swapper::new(
            Uint128::from(7500u128),
            None,
            oracle,
            Decimal::from_str("0.001").unwrap(),
        );
        let res = s.swap(&mut storage, &mut mm).unwrap();
        assert_eq!(res.return_amount, Uint128::from(7061u128));
        assert_eq!(res.fee_amount, Uint128::from(8u128));
        assert_eq!(res.remaining_offer, Uint128::zero());

        let event = res.events[0].clone();
        assert_eq!(event.ty, "rujira-orca/trade");
        assert_eq!(event.attributes[0].key, "rate");
        assert_eq!(event.attributes[0].value, "1");
        assert_eq!(event.attributes[1].key, "offer");
        assert_eq!(event.attributes[1].value, "1000");
        assert_eq!(event.attributes[2].key, "bid");
        assert_eq!(event.attributes[2].value, "1000");
        assert_eq!(event.attributes[3].key, "premium");
        assert_eq!(event.attributes[3].value, "0");

        let event = res.events[1].clone();
        assert_eq!(event.ty, "rujira-orca/trade");
        assert_eq!(event.attributes[0].key, "rate");
        assert_eq!(event.attributes[0].value, "0.99");
        assert_eq!(event.attributes[1].key, "offer");
        assert_eq!(event.attributes[1].value, "1011");
        assert_eq!(event.attributes[2].key, "bid");
        assert_eq!(event.attributes[2].value, "1000");
        assert_eq!(event.attributes[3].key, "premium");
        assert_eq!(event.attributes[3].value, "1");

        let event = res.events[2].clone();
        assert_eq!(event.ty, "rujira-orca/trade");
        assert_eq!(event.attributes[0].key, "rate");
        assert_eq!(event.attributes[0].value, "0.96");
        assert_eq!(event.attributes[1].key, "offer");
        assert_eq!(event.attributes[1].value, "1042");
        assert_eq!(event.attributes[2].key, "bid");
        assert_eq!(event.attributes[2].value, "1000");
        assert_eq!(event.attributes[3].key, "premium");
        assert_eq!(event.attributes[3].value, "4");

        let event = res.events[3].clone();
        assert_eq!(event.ty, "rujira-orca/trade");
        assert_eq!(event.attributes[0].key, "rate");
        assert_eq!(event.attributes[0].value, "0.95");
        assert_eq!(event.attributes[1].key, "offer");
        assert_eq!(event.attributes[1].value, "1053");
        assert_eq!(event.attributes[2].key, "bid");
        assert_eq!(event.attributes[2].value, "1000");
        assert_eq!(event.attributes[3].key, "premium");
        assert_eq!(event.attributes[3].value, "5");

        let event = res.events[4].clone();
        assert_eq!(event.ty, "rujira-orca/trade");
        assert_eq!(event.attributes[0].key, "rate");
        assert_eq!(event.attributes[0].value, "0.93");
        assert_eq!(event.attributes[1].key, "offer");
        assert_eq!(event.attributes[1].value, "1076");
        assert_eq!(event.attributes[2].key, "bid");
        assert_eq!(event.attributes[2].value, "1000");
        assert_eq!(event.attributes[3].key, "premium");
        assert_eq!(event.attributes[3].value, "7");

        let event = res.events[5].clone();
        assert_eq!(event.ty, "rujira-orca/trade");
        assert_eq!(event.attributes[0].key, "rate");
        assert_eq!(event.attributes[0].value, "0.91");
        assert_eq!(event.attributes[1].key, "offer");
        assert_eq!(event.attributes[1].value, "1099");
        assert_eq!(event.attributes[2].key, "bid");
        assert_eq!(event.attributes[2].value, "1000");
        assert_eq!(event.attributes[3].key, "premium");
        assert_eq!(event.attributes[3].value, "9");

        let event = res.events[6].clone();
        assert_eq!(event.ty, "rujira-orca/trade");
        assert_eq!(event.attributes[0].key, "rate");
        assert_eq!(event.attributes[0].value, "0.88");
        assert_eq!(event.attributes[1].key, "offer");
        assert_eq!(event.attributes[1].value, "1137");
        assert_eq!(event.attributes[2].key, "bid");
        assert_eq!(event.attributes[2].value, "1000");
        assert_eq!(event.attributes[3].key, "premium");
        assert_eq!(event.attributes[3].value, "12");

        let event = res.events[7].clone();
        assert_eq!(event.ty, "rujira-orca/trade");
        assert_eq!(event.attributes[0].key, "rate");
        assert_eq!(event.attributes[0].value, "0.85");
        assert_eq!(event.attributes[1].key, "offer");
        assert_eq!(event.attributes[1].value, "82");
        assert_eq!(event.attributes[2].key, "bid");
        assert_eq!(event.attributes[2].value, "69");
        assert_eq!(event.attributes[3].key, "premium");
        assert_eq!(event.attributes[3].value, "15");
    }
}
