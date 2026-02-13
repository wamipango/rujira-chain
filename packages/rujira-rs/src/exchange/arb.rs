use std::fmt::Debug;

use cosmwasm_std::{Event, Uint128};

use super::{trade_event, SwapError, SwapItem};

/// Utility to capture arbitrage from overlapping orders
#[derive(Debug, Default)]
pub struct Arber<T: SwapItem> {
    pub context: T::Context,
    events: Vec<Event>,
    prefix: String,
}

impl<T: Arbitrage + Clone> Arber<T> {
    pub fn new(context: T::Context, prefix: String) -> Self {
        Self {
            context,
            events: vec![],
            prefix,
        }
    }

    pub fn run(
        &mut self,
        mut base_iter: impl Iterator<Item = T>,
        mut quote_iter: impl Iterator<Item = T>,
    ) -> Result<ArbitrageResult, SwapError> {
        let mut base = base_iter.next();
        let mut quote = quote_iter.next();
        let mut profit_base = Uint128::zero();
        let mut profit_quote = Uint128::zero();
        while let (Some(b), Some(q)) = (&mut base, &mut quote) {
            match b.arbitrage(&mut self.context, &self.prefix, q)? {
                Some((base_profit, quote_profit, mut events)) => {
                    self.events.append(&mut events);
                    profit_base += base_profit;
                    profit_quote += quote_profit;

                    if b.total().is_zero() {
                        base = base_iter.next()
                    }

                    if q.total().is_zero() {
                        quote = quote_iter.next()
                    }
                }
                None => break,
            }
        }
        Ok(ArbitrageResult {
            profit_base,
            profit_quote,
            events: self.events.clone(),
        })
    }
}

#[derive(Debug)]
pub struct ArbitrageResult {
    pub profit_base: Uint128,
    pub profit_quote: Uint128,
    pub events: Vec<Event>,
}

pub trait Arbitrage: SwapItem + Sized {
    /// Must be called as bid token sell.arbitrage(buy) with respect to Swappable::rate()
    /// Returns arbitrage profit in the (`self`, `other`) bid tokens
    fn arbitrage(
        &mut self,
        context: &mut Self::Context,
        prefix: &str,
        other: &mut Self,
    ) -> Result<Option<(Uint128, Uint128, Vec<Event>)>, SwapError>;
}

impl<T> Arbitrage for T
where
    T: SwapItem + Clone + Debug,
{
    fn arbitrage(
        &mut self,
        context: &mut T::Context,
        prefix: &str,
        other: &mut Self,
    ) -> Result<Option<(Uint128, Uint128, Vec<Event>)>, SwapError> {
        let float = self.total();
        let mut next_context = context.clone();
        let a = other.swap(&mut next_context, float)?;
        let b = self.swap(&mut next_context, a.1)?;
        let mut events = vec![
            trade_event(other, prefix, a.0, a.1, &a.2),
            trade_event(self, prefix, b.0, b.1, &b.2),
        ];
        match (b.1.checked_sub(a.0), a.1.checked_sub(b.0)) {
            (Ok(a), Ok(b)) => {
                *context = next_context;
                events.push(event(prefix, a, b));
                Ok(Some((a, b, events)))
            }
            _ => Ok(None),
        }
    }
}

pub fn event(prefix: &str, base: Uint128, quote: Uint128) -> Event {
    Event::new(format!("{prefix}/arb"))
        .add_attribute("base", base.to_string())
        .add_attribute("quote", quote.to_string())
}

#[cfg(test)]
mod tests {

    use std::str::FromStr;

    use cosmwasm_std::{Decimal, Event, Fraction, Uint128};

    use crate::exchange::testing::{TestItem, TestIter, TestIterContext};

    use super::{Arber, Arbitrage};

    // TODO: Test when rounding of b

    #[test]

    fn test_aribtrage() {
        let mut a = TestItem::new("0.625", 1000u128, true);
        let mut b = TestItem::new("0.8", 3000u128, false);
        let mut context = TestIterContext::default();

        let res = a
            .arbitrage(&mut context, "prefix", &mut b)
            .unwrap()
            .unwrap();
        assert_eq!(
            res,
            (
                Uint128::zero(),
                Uint128::from(175u128),
                vec![
                    Event::new("prefix/trade").add_attributes(vec![
                        ("rate", "0.8"),
                        ("offer", "1000"),
                        ("bid", "800"),
                        ("test", "attr")
                    ]),
                    Event::new("prefix/trade").add_attributes(vec![
                        ("rate", "1.6"),
                        ("offer", "625"),
                        ("bid", "1000"),
                        ("test", "attr")
                    ]),
                    Event::new("prefix/arb").add_attributes(vec![("base", "0"), ("quote", "175"),])
                ]
            )
        );

        let mut a = TestItem::new("1.6", 1000u128, false);
        let mut b = TestItem::new("1.25", 3000u128, true);

        let res = b
            .arbitrage(&mut context, "prefix", &mut a)
            .unwrap()
            .unwrap();
        assert_eq!(
            res,
            (
                Uint128::from(175u128),
                Uint128::zero(),
                vec![
                    Event::new("prefix/trade").add_attributes(vec![
                        ("rate", "1.6"),
                        ("offer", "625"),
                        ("bid", "1000"),
                        ("test", "attr")
                    ]),
                    Event::new("prefix/trade").add_attributes(vec![
                        ("rate", "0.8"),
                        ("offer", "1000"),
                        ("bid", "800"),
                        ("test", "attr")
                    ]),
                    Event::new("prefix/arb").add_attributes(vec![("base", "175"), ("quote", "0"),])
                ]
            )
        );

        // Test when profit token is on the opposite side
        let mut a = TestItem::new("0.625", 5000u128, true);
        let mut b = TestItem::new("0.8", 1000u128, false);

        let res = a
            .arbitrage(&mut context, "prefix", &mut b)
            .unwrap()
            .unwrap();
        assert_eq!(
            res,
            (
                Uint128::from(350u128),
                Uint128::zero(),
                vec![
                    Event::new("prefix/trade").add_attributes(vec![
                        ("rate", "0.8"),
                        ("offer", "1250"),
                        ("bid", "1000"),
                        ("test", "attr")
                    ]),
                    Event::new("prefix/trade").add_attributes(vec![
                        ("rate", "1.6"),
                        ("offer", "1000"),
                        ("bid", "1600"),
                        ("test", "attr")
                    ]),
                    Event::new("prefix/arb").add_attributes(vec![("base", "350"), ("quote", "0"),])
                ]
            )
        );

        let mut a = TestItem::new("1.6", 5000u128, false);
        let mut b = TestItem::new("1.25", 1000u128, true);

        let res = b
            .arbitrage(&mut context, "prefix", &mut a)
            .unwrap()
            .unwrap();
        assert_eq!(
            res,
            (
                Uint128::zero(),
                Uint128::from(350u128),
                vec![
                    Event::new("prefix/trade").add_attributes(vec![
                        ("rate", "1.6"),
                        ("offer", "1000"),
                        ("bid", "1600"),
                        ("test", "attr")
                    ]),
                    Event::new("prefix/trade").add_attributes(vec![
                        ("rate", "0.8"),
                        ("offer", "1250"),
                        ("bid", "1000"),
                        ("test", "attr")
                    ]),
                    Event::new("prefix/arb").add_attributes(vec![("base", "0"), ("quote", "350"),])
                ]
            )
        );

        // Test no opportunity
        let mut a = TestItem::new("0.5", 1000u128, false);
        let mut b = TestItem::new("1", 3000u128, true);

        assert_eq!(b.arbitrage(&mut context, "prefix", &mut a).unwrap(), None);

        // And when profit token is on the opposite side
        let mut a = TestItem::new("2", 5000u128, true);
        let mut b = TestItem::new("1", 1000u128, false);

        assert_eq!(b.arbitrage(&mut context, "prefix", &mut a).unwrap(), None);
    }

    #[test]
    fn test_arber() {
        let base_iter = TestIter::new(vec![
            (
                Decimal::from_str("0.9").unwrap().inv().unwrap(),
                Uint128::from(1000u128),
            ),
            (
                Decimal::from_str("0.8").unwrap().inv().unwrap(),
                Uint128::from(1000u128),
            ),
            (
                Decimal::from_str("0.75").unwrap().inv().unwrap(),
                Uint128::from(1000u128),
            ),
        ]);

        let quote_iter = TestIter::new(vec![
            (Decimal::from_str("1.1").unwrap(), Uint128::from(1000u128)),
            (Decimal::from_str("0.9").unwrap(), Uint128::from(1000u128)),
            (Decimal::from_str("0.7").unwrap(), Uint128::from(1000u128)),
            (Decimal::from_str("0.6").unwrap(), Uint128::from(1000u128)),
        ]);

        let mut arb = Arber::default();
        let res = arb.run(base_iter, quote_iter).unwrap();
        assert_eq!(res.profit_base, Uint128::from(114u128));
        assert_eq!(res.profit_quote, Uint128::from(200u128));
    }
}
