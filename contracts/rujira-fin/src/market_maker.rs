use cosmwasm_schema::cw_serde;
use cosmwasm_std::{
    Addr, Attribute, Binary, Decimal, Fraction, QuerierWrapper, StdResult, Storage, Uint128,
};
use rujira_rs::fin::{Denoms, MarketMakerQuery, MarketMakerResponse, Side, Tick};
use std::ops::Mul;

use crate::{
    swappable::{SwapCommit, Swappable},
    ContractError,
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
    consumed: Uint128,
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

    fn swap(&mut self, offer: Uint128) -> Result<(Uint128, Uint128), ContractError> {
        todo!()
    }

    fn commit(&self, storage: &mut dyn Storage) -> StdResult<SwapCommit> {
        Ok(SwapCommit::default())
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
                    consumed: Uint128::zero(),
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
                    consumed: Uint128::zero()
                },
                MarketMakerOffer {
                    price: Decimal::from_str("1.5").unwrap(),
                    total: Uint128::from(200u128),
                    side: Side::Quote,
                    consumed: Uint128::zero()
                },
                MarketMakerOffer {
                    price: Decimal::from_str("1.75").unwrap(),
                    total: Uint128::from(600u128),
                    side: Side::Quote,
                    consumed: Uint128::zero()
                },
                MarketMakerOffer {
                    price: Decimal::from_str("2").unwrap(),
                    total: Uint128::from(1800u128),
                    side: Side::Quote,
                    consumed: Uint128::zero()
                },
                MarketMakerOffer {
                    price: Decimal::from_str("2.25").unwrap(),
                    total: Uint128::from(5400u128),
                    side: Side::Quote,
                    consumed: Uint128::zero()
                }
            ]
        );
    }
}
