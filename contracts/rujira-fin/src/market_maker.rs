use cosmwasm_std::{Addr, Decimal, QuerierWrapper, Uint128};
use rujira_rs::fin::{BookItemResponse, Denoms, MarketMakerQuery, Side, Tick};
use std::{ops::Mul, str::FromStr};

pub struct MarketMakerIterator<'a> {
    q: QuerierWrapper<'a>,
    denoms: Denoms,
    side: Side,
    contract: Addr,
    tick: Tick,
    price: Decimal,
    bid: Uint128,
    ask: Uint128,
}

impl<'a> MarketMakerIterator<'a> {
    pub fn new(
        q: QuerierWrapper<'a>,
        denoms: Denoms,
        side: Side,
        contract: Addr,
        tick: Tick,
        price: Decimal,
    ) -> Self {
        Self {
            q,
            denoms,
            side,
            contract,
            tick,
            price,
            bid: Uint128::zero(),
            ask: Uint128::zero(),
        }
    }
}

impl<'a> MarketMakerIterator<'a> {
    pub fn totals(&self) -> (Uint128, Uint128) {
        (self.bid, self.ask)
    }
}

impl Iterator for MarketMakerIterator<'_> {
    type Item = BookItemResponse;

    fn next(&mut self) -> Option<Self::Item> {
        let price = self.price;
        let res: Option<Uint128> = self
            .q
            .query_wasm_smart(
                self.contract.to_string(),
                &MarketMakerQuery {
                    ask_denom: self.denoms.ask(&self.side).to_string(),
                    ask_total: self.ask,
                    bid_denom: self.denoms.bid(&self.side).to_string(),
                    bid_total: self.bid,
                    price,
                },
            )
            .ok();

        match res {
            Some(res) => {
                self.bid += Decimal::from_ratio(res, 1u128)
                    .mul(self.price)
                    .to_uint_ceil();
                self.ask += res;
                self.price += Decimal::from_str("0.25").unwrap();
                Some(BookItemResponse { price, total: res })
            }
            None => None,
        }
    }
}

#[cfg(test)]

mod test {
    use super::*;
    use cosmwasm_std::{
        from_json, testing::MockQuerier, to_json_binary, ContractResult, SystemError, SystemResult,
        WasmQuery,
    };

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

                SystemResult::Ok(ContractResult::Ok(to_json_binary(&size).unwrap()))
            }
            _ => SystemResult::Err(SystemError::Unknown {}),
        });
        let mut iter = MarketMakerIterator::new(
            QuerierWrapper::new(&q),
            Denoms::new("btc", "usdc"),
            Side::Quote,
            Addr::unchecked("bow"),
            Tick::new(4),
            Decimal::one(),
        );

        let items: Vec<BookItemResponse> = iter.by_ref().take(5).collect();

        assert_eq!(
            iter.totals(),
            (Uint128::from(15200u128), Uint128::from(8100u128))
        );

        assert_eq!(
            items,
            vec![
                BookItemResponse {
                    price: Decimal::from_str("1").unwrap(),
                    total: Uint128::from(100u128)
                },
                BookItemResponse {
                    price: Decimal::from_str("1.25").unwrap(),
                    total: Uint128::from(200u128)
                },
                BookItemResponse {
                    price: Decimal::from_str("1.5").unwrap(),
                    total: Uint128::from(600u128)
                },
                BookItemResponse {
                    price: Decimal::from_str("1.75").unwrap(),
                    total: Uint128::from(1800u128)
                },
                BookItemResponse {
                    price: Decimal::from_str("2").unwrap(),
                    total: Uint128::from(5400u128)
                }
            ]
        );
    }
}
