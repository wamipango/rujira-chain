use std::{
    cmp::Ordering,
    ops::{Div, Mul},
};

use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Attribute, Decimal, Fraction, Uint128};
use rujira_rs::{
    exchange::{SwapError, SwapItem},
    fin::{Denoms, Side},
};

use super::MarketMakerContext;

#[cw_serde]
pub struct MarketMakerOffer {
    pub price: Decimal,
    pub total: Uint128,
    pub(crate) side: Side,
    pub(crate) rate: Decimal,
    pub(crate) contract: Addr,
    pub(crate) denoms: Denoms,
}

impl SwapItem for MarketMakerOffer {
    type Context = MarketMakerContext;
    fn rate(&self) -> Decimal {
        self.rate
    }

    fn total(&self) -> Uint128 {
        self.total
    }

    fn swap(
        &mut self,
        context: &mut Self::Context,
        offer: Uint128,
    ) -> Result<(Uint128, Uint128, Vec<Attribute>), SwapError> {
        let offer_value = Decimal::from_ratio(offer, 1u128)
            .mul(self.price)
            .to_uint_floor();
        let pool_value = Decimal::from_ratio(self.total, 1u128)
            .div(self.price)
            .to_uint_floor();
        let (offer, ask) = match self.total.cmp(&offer_value) {
            Ordering::Greater => {
                self.total -= offer_value;
                (offer, offer_value)
            }
            // Complete fill
            _ => {
                let size = self.total;
                self.total = Uint128::zero();
                (pool_value, size)
            }
        };

        let rate = match self.side {
            Side::Base => self.price.inv().unwrap().to_string(),
            Side::Quote => self.price.to_string(),
        };

        // Commit to context
        if !offer.is_zero() && !ask.is_zero() {
            context.insert(&self.contract, &self.denoms, &self.side, offer, ask);
            Ok((
                offer,
                ask,
                vec![
                    Attribute::new("price", format!("{}:{rate}", self.contract)),
                    Attribute::new("side", self.side.to_string()),
                ],
            ))
        } else {
            Ok((Uint128::zero(), Uint128::zero(), vec![]))
        }
    }
}
