use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Attribute, Binary, Decimal, QuerierWrapper, StdResult, Storage, Uint128};
use rujira_rs::{
    bow,
    exchange::{SwapCommit, SwapError, Swappable},
    orca::Denoms,
};
use std::{
    cmp::Ordering,
    ops::{Div, Mul},
};

pub struct MarketMaker<'a> {
    q: QuerierWrapper<'a>,
    denoms: Denoms,
    contract: Option<Addr>,
    bid: Uint128,
    ask: Uint128,
    last_price: Option<Decimal>,
    data: Option<Binary>,
}

impl<'a> MarketMaker<'a> {
    pub fn new(q: QuerierWrapper<'a>, contract: Option<Addr>, denoms: Denoms) -> Self {
        Self {
            q,
            denoms,
            contract,
            bid: Uint128::zero(),
            ask: Uint128::zero(),
            last_price: None,
            data: None,
        }
    }
}

#[cw_serde]
pub struct MarketMakerOffer {
    /// The premium bucket
    pub premium: u8,
    pub total: Uint128,
    /// The price quoted
    price: Decimal,
    /// The effective rate at bucketed premium
    rate: Decimal,
    commitment: (Uint128, Uint128),
}

impl Swappable for MarketMakerOffer {
    fn rate(&self) -> Decimal {
        self.rate
    }

    fn attributes(&self) -> Vec<Attribute> {
        vec![Attribute::new("price", format!("mm:{}", self.price))]
    }

    fn total(&self) -> Uint128 {
        self.total
    }

    fn swap(&mut self, offer: Uint128) -> Result<(Uint128, Uint128), SwapError> {
        let offer_value = Decimal::from_ratio(offer, 1u128)
            .mul(self.price)
            .to_uint_ceil();
        let pool_value = Decimal::from_ratio(self.total, 1u128)
            .div(self.price)
            .to_uint_floor();
        let res = match self.total.cmp(&offer_value) {
            // Partial fill
            Ordering::Greater => (offer, offer_value),
            // Complete fill
            _ => (pool_value, self.total),
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
        env!("CARGO_PKG_NAME")
    }
}

impl<'a> MarketMaker<'a> {
    fn query_next(&mut self, contract: Addr) -> Option<MarketMakerOffer> {
        let res: StdResult<Option<bow::QuoteResponse>> = self.q.query_wasm_smart(
            contract.to_string(),
            &bow::QueryMsg::Quote(bow::QuoteRequest {
                // TODO: Calculate price of next premium
                min_price: self.last_price,
                ask_denom: self.denoms.ask().to_string(),
                offer_denom: self.denoms.bid().to_string(),
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

                let rate = Decimal::one();
                let premium = 1;

                Some(MarketMakerOffer {
                    price,
                    rate,
                    total: res.size,
                    commitment: Default::default(),
                    premium,
                })
            }
            _ => None,
        }
    }
}

impl Iterator for MarketMaker<'_> {
    type Item = MarketMakerOffer;

    fn next(&mut self) -> Option<Self::Item> {
        self.contract.clone().and_then(|x| self.query_next(x))
    }
}

#[cfg(test)]

mod test {

    #[test]
    fn test_iterator() {}
}
