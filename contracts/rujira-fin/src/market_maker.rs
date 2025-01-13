use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Attribute, Binary, Decimal, QuerierWrapper, Storage, Uint128};
use rujira_rs::{
    exchange::{SwapCommit, SwapError, Swappable},
    fin::{Denoms, MarketMakerQuery, MarketMakerResponse, Side, Tick},
};
use std::{
    cmp::Ordering,
    ops::{Div, Mul},
};

pub struct MarketMaker<'a> {
    q: QuerierWrapper<'a>,
    denoms: Denoms,
    side: Side,
    contract: Option<Addr>,
    tick: Tick,
    last_price: Option<Decimal>,
    bid: Uint128,
    ask: Uint128,
    data: Option<Binary>,
}

impl<'a> MarketMaker<'a> {
    pub fn new(
        q: QuerierWrapper<'a>,
        denoms: Denoms,
        side: Side,
        contract: Option<Addr>,
        tick: Tick,
    ) -> Self {
        Self {
            q,
            denoms,
            side,
            contract,
            tick,
            last_price: None,
            bid: Uint128::zero(),
            ask: Uint128::zero(),
            data: None,
        }
    }
}

#[cw_serde]
pub struct MarketMakerOffer {
    pub price: Decimal,
    pub total: Uint128,
    side: Side,
    commitment: (Uint128, Uint128),
}

impl Swappable for MarketMakerOffer {
    fn rate(&self) -> Decimal {
        self.price
    }

    fn attributes(&self) -> Vec<Attribute> {
        vec![]
    }

    fn total(&self) -> Uint128 {
        self.total
    }

    fn swap(&mut self, offer: Uint128) -> Result<(Uint128, Uint128), SwapError> {
        let res = match self.side {
            Side::Base => {
                let offer_value = Decimal::from_ratio(offer, 1u128)
                    .div(self.price)
                    .to_uint_floor();
                let pool_value = Decimal::from_ratio(self.total, 1u128)
                    .mul(self.price)
                    .to_uint_floor();
                match self.total.cmp(&offer_value) {
                    // Partial fill
                    Ordering::Greater => (offer_value, offer),
                    // Complete fill
                    _ => (self.total, pool_value),
                }
            }
            Side::Quote => {
                let offer_value = Decimal::from_ratio(offer, 1u128)
                    .mul(self.price)
                    .to_uint_floor();
                let pool_value = Decimal::from_ratio(self.total, 1u128)
                    .div(self.price)
                    .to_uint_floor();
                match self.total.cmp(&offer_value) {
                    // Partial fill
                    Ordering::Greater => (offer_value, offer),
                    // Complete fill
                    _ => (self.total, pool_value),
                }
            }
        };
        self.commitment = res;
        Ok(res)
    }

    fn commit(&self, _storage: &mut dyn Storage) -> Result<SwapCommit, SwapError> {
        Ok(SwapCommit {
            market_maker: self.commitment,
        })
    }

    fn prefix(&self) -> &str {
        todo!()
    }
}

impl<'a> MarketMaker<'a> {
    pub fn totals(&self) -> (Uint128, Uint128) {
        (self.bid, self.ask)
    }

    fn query_next(&mut self, contract: Addr) -> Option<MarketMakerOffer> {
        let price = self.last_price;
        let res: Option<MarketMakerResponse> = self
            .q
            .query_wasm_smart(
                contract.to_string(),
                &MarketMakerQuery {
                    ask_denom: self.denoms.ask(&self.side).to_string(),
                    ask_total: self.ask,
                    bid_denom: self.denoms.bid(&self.side).to_string(),
                    bid_total: self.bid,
                    last_price: price,
                    tick: self.tick.clone(),
                    data: self.data.clone(),
                },
            )
            .ok();

        match res {
            Some(res) => {
                self.bid += Decimal::from_ratio(res.size, 1u128)
                    .mul(res.price)
                    .to_uint_ceil();
                self.ask += res.size;
                self.last_price = Some(res.price);
                self.data = res.data;
                Some(MarketMakerOffer {
                    price: res.price,
                    total: res.size,
                    side: self.side.clone(),
                    commitment: Default::default(),
                })
            }
            None => None,
        }
    }
}

impl Iterator for MarketMaker<'_> {
    type Item = MarketMakerOffer;

    fn next(&mut self) -> Option<Self::Item> {
        self.contract.clone().map(|x| self.query_next(x)).flatten()
    }
}

#[cfg(test)]

mod test {
    use super::*;
    use cosmwasm_std::{
        from_json, testing::MockQuerier, to_json_binary, ContractResult, SystemError, SystemResult,
        WasmQuery,
    };
    use std::ops::Add;
    use std::str::FromStr;

    #[test]
    fn test_iterator() {
        let mut q = MockQuerier::default();
        q.update_wasm(|x| match x {
            WasmQuery::Smart { msg, .. } => {
                let query: MarketMakerQuery = from_json(msg).unwrap();
                // Double the order size until we run out of funds
                if query.ask_total.gt(&Uint128::from(1000000u128)) {
                    return SystemResult::Err(SystemError::Unknown {});
                }

                let size = if query.ask_total.is_zero() {
                    Uint128::from(100u128)
                } else {
                    query.ask_total * Uint128::from(2u128)
                };

                SystemResult::Ok(ContractResult::Ok(
                    to_json_binary(&MarketMakerResponse {
                        size,
                        data: None,
                        price: query
                            .last_price
                            .unwrap_or(Decimal::one())
                            .add(Decimal::from_ratio(1u128, 4u128)),
                    })
                    .unwrap(),
                ))
            }
            _ => SystemResult::Err(SystemError::Unknown {}),
        });
        let mut iter = MarketMaker::new(
            QuerierWrapper::new(&q),
            Denoms::new("btc", "usdc"),
            Side::Quote,
            Some(Addr::unchecked("bow")),
            Tick::new(2),
        );

        let items: Vec<MarketMakerOffer> = iter.by_ref().take(5).collect();

        assert_eq!(
            iter.totals(),
            (Uint128::from(17225u128), Uint128::from(8100u128))
        );

        assert_eq!(
            items,
            vec![
                MarketMakerOffer {
                    price: Decimal::from_str("1.25").unwrap(),
                    total: Uint128::from(100u128),
                    side: Side::Quote,
                    commitment: Default::default()
                },
                MarketMakerOffer {
                    price: Decimal::from_str("1.5").unwrap(),
                    total: Uint128::from(200u128),
                    side: Side::Quote,
                    commitment: Default::default()
                },
                MarketMakerOffer {
                    price: Decimal::from_str("1.75").unwrap(),
                    total: Uint128::from(600u128),
                    side: Side::Quote,
                    commitment: Default::default()
                },
                MarketMakerOffer {
                    price: Decimal::from_str("2").unwrap(),
                    total: Uint128::from(1800u128),
                    side: Side::Quote,
                    commitment: Default::default()
                },
                MarketMakerOffer {
                    price: Decimal::from_str("2.25").unwrap(),
                    total: Uint128::from(5400u128),
                    side: Side::Quote,
                    commitment: Default::default()
                }
            ]
        );
    }
    #[test]
    fn test_market_market_item_swap() {
        // Quote partial fill
        let mut o = MarketMakerOffer {
            price: Decimal::from_str("1.25").unwrap(),
            total: Uint128::from(10000u128),
            side: Side::Quote,
            commitment: Default::default(),
        };
        let (bid_returned, offer_consumed) = o.swap(Uint128::from(500u128)).unwrap();
        assert_eq!(bid_returned, Uint128::from(625u128));
        assert_eq!(offer_consumed, Uint128::from(500u128));

        // Quote full fill
        let mut o = MarketMakerOffer {
            price: Decimal::from_str("1.25").unwrap(),
            total: Uint128::from(10000u128),
            side: Side::Quote,
            commitment: Default::default(),
        };
        let (bid_returned, offer_consumed) = o.swap(Uint128::from(500000u128)).unwrap();
        assert_eq!(bid_returned, Uint128::from(10000u128));
        assert_eq!(offer_consumed, Uint128::from(8000u128));

        // Base partial fill
        let mut o = MarketMakerOffer {
            price: Decimal::from_str("1.25").unwrap(),
            total: Uint128::from(10000u128),
            side: Side::Base,
            commitment: Default::default(),
        };
        let (bid_returned, offer_consumed) = o.swap(Uint128::from(500u128)).unwrap();
        assert_eq!(bid_returned, Uint128::from(400u128));
        assert_eq!(offer_consumed, Uint128::from(500u128));

        // Base full fill
        let mut o = MarketMakerOffer {
            price: Decimal::from_str("1.25").unwrap(),
            total: Uint128::from(10000u128),
            side: Side::Base,
            commitment: Default::default(),
        };
        let (bid_returned, offer_consumed) = o.swap(Uint128::from(500000u128)).unwrap();
        assert_eq!(bid_returned, Uint128::from(10000u128));
        assert_eq!(offer_consumed, Uint128::from(12500u128));
    }
}
