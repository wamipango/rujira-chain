use std::ops::Mul;

use cosmwasm_std::{Addr, Binary, Decimal, Fraction, QuerierWrapper, StdResult, Uint128};
use rujira_rs::fin::{
    market_maker::{MarketMakerQueryMsg, QuoteRequest, QuoteResponse},
    Denoms, Side, Tick,
};

use super::MarketMakerOffer;

pub struct MarketMakerIter<'a> {
    q: QuerierWrapper<'a>,
    denoms: Denoms,
    side: Side,
    contract: Addr,
    tick: Tick,
    bid: Uint128,
    ask: Uint128,
    last_price: Option<Decimal>,
    data: Option<Binary>,
}

impl<'a> MarketMakerIter<'a> {
    pub fn new(
        q: QuerierWrapper<'a>,
        denoms: Denoms,
        side: Side,
        contract: Addr,
        tick: Tick,
    ) -> Self {
        Self {
            q,
            denoms,
            side,
            contract,
            tick,
            bid: Uint128::zero(),
            ask: Uint128::zero(),
            last_price: None,
            data: None,
        }
    }

    fn query_next(&mut self, contract: Addr) -> Option<MarketMakerOffer> {
        let res: StdResult<Option<QuoteResponse>> = self.q.query_wasm_smart(
            contract.to_string(),
            &MarketMakerQueryMsg::Quote(QuoteRequest {
                min_price: self.last_price,
                ask_denom: self.denoms.bid(&self.side).to_string(),
                offer_denom: self.denoms.ask(&self.side).to_string(),
                data: self.data.clone(),
            }),
        );

        match res {
            Ok(Some(res)) => {
                self.bid += Decimal::from_ratio(res.size, 1u128)
                    .mul(res.price)
                    .to_uint_ceil();
                self.ask += res.size;
                self.data = res.data;
                let price = res.price;
                self.last_price = Some(price);
                // Rate is used for presentation, and for sorting and grouping `Iter<impl Swappable>`
                // Truncate to "worse" price so that execution price is fractionally better than
                // the price rendered
                let rate = match self.side {
                    Side::Base => self.tick.truncate_ceil(&res.price.inv().unwrap()),
                    Side::Quote => self.tick.truncate_floor(&res.price),
                };

                Some(MarketMakerOffer {
                    contract,
                    price,
                    rate,
                    total: res.size,
                    side: self.side.clone(),
                    denoms: self.denoms.clone(),
                })
            }
            _ => None,
        }
    }
}

impl Iterator for MarketMakerIter<'_> {
    type Item = MarketMakerOffer;

    fn next(&mut self) -> Option<Self::Item> {
        self.query_next(self.contract.clone())
    }
}
