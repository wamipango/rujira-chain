use crate::exchange::swap::SwapContext;

use super::{SwapError, SwapItem};
use cosmwasm_std::{Attribute, Decimal, Fraction, Response, Storage, Uint128};
use std::cmp::Ordering;
use std::ops::{Div, Mul};
use std::str::FromStr;

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct TestItem {
    pub price: Decimal,
    pub amount: Uint128,
    pub commitment: (Uint128, Uint128),
}

impl TestItem {
    pub fn new(price: &str, amount: u128, is_sell: bool) -> Self {
        let mut price = Decimal::from_str(price).unwrap();
        if is_sell {
            price = price.inv().unwrap();
        }
        Self {
            price,
            amount: Uint128::from(amount),
            commitment: Default::default(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct FloorItem {
    pub price: Decimal,
    pub amount: Uint128,
}

impl FloorItem {
    pub fn new(price: &str, amount: u128) -> Self {
        Self {
            price: Decimal::from_str(price).unwrap(),
            amount: Uint128::from(amount),
        }
    }
}

#[derive(Default, Clone)]
pub struct TestIterContext {
    items: Vec<TestItem>,
}

impl SwapContext for TestIterContext {
    fn commit(
        &mut self,
        _storage: &mut dyn Storage,
        response: Response,
    ) -> Result<Response, SwapError> {
        Ok(response)
    }
}

impl SwapItem for TestItem {
    type Context = TestIterContext;
    fn rate(&self) -> Decimal {
        self.price
    }

    fn total(&self) -> Uint128 {
        self.amount
    }

    fn swap(
        &mut self,
        context: &mut Self::Context,
        offer: Uint128,
    ) -> Result<(Uint128, Uint128, Vec<Attribute>), SwapError> {
        let offer_value = Decimal::from_ratio(offer, 1u128)
            .mul(self.price)
            .to_uint_ceil();
        let pool_value = Decimal::from_ratio(self.amount, 1u128)
            .div(self.price)
            .to_uint_floor();
        let (offer, ask) = match self.amount.cmp(&offer_value) {
            // Partial fill
            Ordering::Greater => {
                self.amount -= offer_value;
                (offer, offer_value)
            }
            // Complete fill
            _ => {
                let size = self.amount;
                self.amount = Uint128::zero();
                (pool_value, size)
            }
        };
        context.items.push(self.clone());
        // Increment commitment as the same offer may be used multiple times, particularly during arbitrage
        self.commitment = (offer, ask);
        Ok((offer, ask, vec![Attribute::new("test", "attr")]))
    }
}

impl SwapItem for FloorItem {
    type Context = ();

    fn rate(&self) -> Decimal {
        self.price
    }

    fn total(&self) -> Uint128 {
        self.amount
    }

    fn swap(
        &mut self,
        _context: &mut Self::Context,
        offer: Uint128,
    ) -> Result<(Uint128, Uint128, Vec<Attribute>), SwapError> {
        let offer_value = Decimal::from_ratio(offer, 1u128)
            .mul(self.price)
            .to_uint_floor();
        let pool_value = Decimal::from_ratio(self.amount, 1u128)
            .div(self.price)
            .to_uint_floor();
        let (offer, ask) = match self.amount.cmp(&offer_value) {
            Ordering::Greater => {
                self.amount -= offer_value;
                (offer, offer_value)
            }
            _ => {
                let size = self.amount;
                self.amount = Uint128::zero();
                (pool_value, size)
            }
        };

        Ok((offer, ask, vec![]))
    }
}

pub struct TestIter {
    pub items: Vec<(Decimal, Uint128)>,
}

impl TestIter {
    pub fn new(items: Vec<(Decimal, Uint128)>) -> Self {
        let mut items = items;
        items.reverse();
        Self { items }
    }
}

impl Iterator for TestIter {
    type Item = TestItem;

    fn next(&mut self) -> Option<Self::Item> {
        self.items.pop().map(|(price, amount)| TestItem {
            price,
            amount,
            commitment: Default::default(),
        })
    }
}
