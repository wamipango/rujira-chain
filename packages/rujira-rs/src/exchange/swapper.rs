use cosmwasm_std::{Attribute, Decimal, Event, Storage, Uint128};
use std::ops::Mul;

use super::{commit::SwapCommit, error::SwapError, Swappable};

/// Executes a swap over an Iterator<Swappable>, consuming the offer and returning the returned amount
#[derive(Debug)]
pub struct Swapper<T> {
    events: Vec<Event>,
    fee: Decimal,
    min_return: Option<Uint128>,
    offer: Uint128,
    returned: Uint128,
    pending: Vec<T>,
}

impl<T: Swappable> Swapper<T> {
    pub fn new(offer: Uint128, min_return: Option<Uint128>, fee: Decimal) -> Self {
        Self {
            events: vec![],
            fee,
            min_return,
            offer,
            returned: Uint128::zero(),
            pending: vec![],
        }
    }

    pub fn swap(&mut self, iter: &mut dyn Iterator<Item = T>) -> Result<SwapResult, SwapError> {
        for mut v in iter {
            let (offer, bids) = v.swap(self.offer)?;
            let attrs = v.attributes();
            self.events.push(event(&v, offer, bids, &attrs));
            self.pending.push(v);
            self.offer -= offer;
            self.returned += bids;
            if self.offer.is_zero() {
                break;
            }
        }

        let fee = Decimal::from_ratio(self.returned, 1u128)
            .mul(self.fee)
            .to_uint_ceil();

        self.returned -= fee;

        if let Some(min_return) = self.min_return {
            if self.returned < min_return {
                return Err(SwapError::InsufficientReturn {
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

    pub fn commit(&self, storage: &mut dyn Storage) -> Result<SwapCommit, SwapError> {
        let mut res = SwapCommit::default();
        for pool in self.pending.iter() {
            res += pool.commit(storage)?;
        }

        Ok(res)
    }
}

pub fn event<T: Swappable>(s: &T, offer: Uint128, bid: Uint128, attributes: &[Attribute]) -> Event {
    let prefix = s.prefix();
    Event::new(format!("{prefix}/trade"))
        .add_attribute("rate", s.rate().to_string())
        .add_attribute("offer", offer.to_string())
        .add_attribute("bid", bid.to_string())
        .add_attributes(attributes.to_owned())
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

    use crate::exchange::testing::TestIter;

    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_swap_execution() {
        let fee = Decimal::from_str("0.001").unwrap();
        let mut iter = TestIter::new(vec![
            (Decimal::from_str("1.0").unwrap(), Uint128::from(1000u128)),
            (Decimal::from_str("0.95").unwrap(), Uint128::from(1000u128)),
            (Decimal::from_str("0.9").unwrap(), Uint128::from(1000u128)),
            (Decimal::from_str("0.85").unwrap(), Uint128::from(1000u128)),
            (Decimal::from_str("0.8").unwrap(), Uint128::from(1000u128)),
            (Decimal::from_str("0.7").unwrap(), Uint128::from(1000u128)),
            (Decimal::from_str("0.6").unwrap(), Uint128::from(1000u128)),
        ]);

        let mut s = Swapper::new(Uint128::from(7500u128), None, fee);
        let res = s.swap(&mut iter).unwrap();
        assert_eq!(res.return_amount, Uint128::from(6283u128));
        assert_eq!(res.fee_amount, Uint128::from(7u128));
        assert_eq!(res.remaining_offer, Uint128::zero());

        let event = res.events[0].clone();
        assert_eq!(event.ty, "some-prefix/trade");
        assert_eq!(event.attributes[0].key, "rate");
        assert_eq!(event.attributes[0].value, "1");
        assert_eq!(event.attributes[1].key, "offer");
        assert_eq!(event.attributes[1].value, "1000");
        assert_eq!(event.attributes[2].key, "bid");
        assert_eq!(event.attributes[2].value, "1000");
        assert_eq!(event.attributes[3].key, "test");
        assert_eq!(event.attributes[3].value, "attr");

        let event = res.events[1].clone();
        assert_eq!(event.ty, "some-prefix/trade");
        assert_eq!(event.attributes[0].key, "rate");
        assert_eq!(event.attributes[0].value, "0.95");
        assert_eq!(event.attributes[1].key, "offer");
        assert_eq!(event.attributes[1].value, "1052");
        assert_eq!(event.attributes[2].key, "bid");
        assert_eq!(event.attributes[2].value, "1000");
        assert_eq!(event.attributes[3].key, "test");
        assert_eq!(event.attributes[3].value, "attr");

        let event = res.events[2].clone();
        assert_eq!(event.ty, "some-prefix/trade");
        assert_eq!(event.attributes[0].key, "rate");
        assert_eq!(event.attributes[0].value, "0.9");
        assert_eq!(event.attributes[1].key, "offer");
        assert_eq!(event.attributes[1].value, "1111");
        assert_eq!(event.attributes[2].key, "bid");
        assert_eq!(event.attributes[2].value, "1000");
        assert_eq!(event.attributes[3].key, "test");
        assert_eq!(event.attributes[3].value, "attr");

        let event = res.events[3].clone();
        assert_eq!(event.ty, "some-prefix/trade");
        assert_eq!(event.attributes[0].key, "rate");
        assert_eq!(event.attributes[0].value, "0.85");
        assert_eq!(event.attributes[1].key, "offer");
        assert_eq!(event.attributes[1].value, "1176");
        assert_eq!(event.attributes[2].key, "bid");
        assert_eq!(event.attributes[2].value, "1000");
        assert_eq!(event.attributes[3].key, "test");
        assert_eq!(event.attributes[3].value, "attr");

        let event = res.events[4].clone();
        assert_eq!(event.ty, "some-prefix/trade");
        assert_eq!(event.attributes[0].key, "rate");
        assert_eq!(event.attributes[0].value, "0.8");
        assert_eq!(event.attributes[1].key, "offer");
        assert_eq!(event.attributes[1].value, "1250");
        assert_eq!(event.attributes[2].key, "bid");
        assert_eq!(event.attributes[2].value, "1000");
        assert_eq!(event.attributes[3].key, "test");
        assert_eq!(event.attributes[3].value, "attr");

        let event = res.events[5].clone();
        assert_eq!(event.ty, "some-prefix/trade");
        assert_eq!(event.attributes[0].key, "rate");
        assert_eq!(event.attributes[0].value, "0.7");
        assert_eq!(event.attributes[1].key, "offer");
        assert_eq!(event.attributes[1].value, "1428");
        assert_eq!(event.attributes[2].key, "bid");
        assert_eq!(event.attributes[2].value, "1000");
        assert_eq!(event.attributes[3].key, "test");
        assert_eq!(event.attributes[3].value, "attr");

        let event = res.events[6].clone();
        assert_eq!(event.ty, "some-prefix/trade");
        assert_eq!(event.attributes[0].key, "rate");
        assert_eq!(event.attributes[0].value, "0.6");
        assert_eq!(event.attributes[1].key, "offer");
        assert_eq!(event.attributes[1].value, "483");
        assert_eq!(event.attributes[2].key, "bid");
        assert_eq!(event.attributes[2].value, "290");
        assert_eq!(event.attributes[3].key, "test");
        assert_eq!(event.attributes[3].value, "attr");
    }
}
