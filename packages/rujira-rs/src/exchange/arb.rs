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
                Step::Captured(base_profit, quote_profit, mut events) => {
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
                Step::SkipSelf => {
                    base = base_iter.next();
                }
                Step::SkipOther => {
                    quote = quote_iter.next();
                }
                Step::Done => break,
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

#[derive(Debug, PartialEq)]
pub enum Step {
    Captured(Uint128, Uint128, Vec<Event>),
    SkipSelf,
    SkipOther,
    Done,
}

pub trait Arbitrage: SwapItem + Sized {
    /// Must be called as bid token sell.arbitrage(buy) with respect to Swappable::rate()
    /// Returns either captured profit, a signal to discard one side, or Done when no
    /// further arbitrage is possible at the current or worse prices.
    fn arbitrage(
        &mut self,
        context: &mut Self::Context,
        prefix: &str,
        other: &mut Self,
    ) -> Result<Step, SwapError>;
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
    ) -> Result<Step, SwapError> {
        let float = self.total();
        let other_total = other.total();
        let mut next_context = context.clone();
        let a = other.swap(&mut next_context, float)?;
        if a.0.is_zero() || a.1.is_zero() {
            // A zero-leg fill can mean either the quote item was exhausted by rounding,
            // or the current base item is too small to execute against this still-usable quote.
            return Ok(if other.total().is_zero() {
                Step::SkipOther
            } else {
                Step::SkipSelf
            });
        }
        let b = self.swap(&mut next_context, a.1)?;
        if b.0.is_zero() || b.1.is_zero() {
            // Symmetrically, if the base item still has quantity after a zero-leg second
            // hop, keep it and try the next quote item instead.
            return Ok(if self.total().is_zero() {
                Step::SkipSelf
            } else {
                Step::SkipOther
            });
        }
        let mut events = vec![
            trade_event(other, prefix, a.0, a.1, &a.2),
            trade_event(self, prefix, b.0, b.1, &b.2),
        ];
        match (b.1.checked_sub(a.0), a.1.checked_sub(b.0)) {
            (Ok(a), Ok(b)) => {
                // Captured steps must consume liquidity. If neither side advanced, terminate
                // defensively rather than risking an infinite arb loop.
                if self.total() == float && other.total() == other_total {
                    return Ok(Step::Done);
                }

                *context = next_context;
                events.push(event(prefix, a, b));
                Ok(Step::Captured(a, b, events))
            }
            _ => Ok(Step::Done),
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

    use crate::exchange::testing::{FloorItem, TestItem, TestIter, TestIterContext};

    use super::{Arber, Arbitrage, Step};

    // TODO: Test when rounding of b

    #[test]

    fn test_aribtrage() {
        let mut a = TestItem::new("0.625", 1000u128, true);
        let mut b = TestItem::new("0.8", 3000u128, false);
        let mut context = TestIterContext::default();

        let res = a.arbitrage(&mut context, "prefix", &mut b).unwrap();
        assert_eq!(
            res,
            Step::Captured(
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
                ],
            )
        );

        let mut a = TestItem::new("1.6", 1000u128, false);
        let mut b = TestItem::new("1.25", 3000u128, true);

        let res = b.arbitrage(&mut context, "prefix", &mut a).unwrap();
        assert_eq!(
            res,
            Step::Captured(
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
                ],
            )
        );

        // Test when profit token is on the opposite side
        let mut a = TestItem::new("0.625", 5000u128, true);
        let mut b = TestItem::new("0.8", 1000u128, false);

        let res = a.arbitrage(&mut context, "prefix", &mut b).unwrap();
        assert_eq!(
            res,
            Step::Captured(
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
                ],
            )
        );

        let mut a = TestItem::new("1.6", 5000u128, false);
        let mut b = TestItem::new("1.25", 1000u128, true);

        let res = b.arbitrage(&mut context, "prefix", &mut a).unwrap();
        assert_eq!(
            res,
            Step::Captured(
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
                ],
            )
        );

        // Test no opportunity
        let mut a = TestItem::new("0.5", 1000u128, false);
        let mut b = TestItem::new("1", 3000u128, true);

        assert_eq!(
            b.arbitrage(&mut context, "prefix", &mut a).unwrap(),
            Step::Done
        );

        // Test zero-profit opportunity is still captured
        let mut a = TestItem::new("1", 1000u128, false);
        let mut b = TestItem::new("1", 3000u128, true);

        let res = b.arbitrage(&mut context, "prefix", &mut a).unwrap();
        match res {
            Step::Captured(base, quote, events) => {
                assert_eq!(base, Uint128::zero());
                assert_eq!(quote, Uint128::zero());
                assert_eq!(
                    events.last().unwrap(),
                    &Event::new("prefix/arb").add_attributes(vec![("base", "0"), ("quote", "0")])
                );
            }
            _ => panic!("expected zero-profit opportunity to be captured"),
        }

        // And when profit token is on the opposite side
        let mut a = TestItem::new("2", 5000u128, true);
        let mut b = TestItem::new("1", 1000u128, false);

        assert_eq!(
            b.arbitrage(&mut context, "prefix", &mut a).unwrap(),
            Step::Done
        );
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

    #[test]
    fn test_arber_zero_profit_terminates() {
        let base_iter = TestIter::new(vec![
            (Decimal::one(), Uint128::from(1000u128)),
            (Decimal::one(), Uint128::from(1000u128)),
        ]);

        let quote_iter = TestIter::new(vec![
            (Decimal::one(), Uint128::from(1000u128)),
            (Decimal::one(), Uint128::from(1000u128)),
            (Decimal::one(), Uint128::from(1000u128)),
        ]);

        let mut arb = Arber::new(Default::default(), "prefix".to_string());
        let res = arb.run(base_iter, quote_iter).unwrap();
        assert_eq!(res.profit_base, Uint128::zero());
        assert_eq!(res.profit_quote, Uint128::zero());
        assert_eq!(res.events.len(), 6);
        assert_eq!(
            res.events.iter().filter(|e| e.ty == "prefix/arb").count(),
            2
        );
    }

    #[test]
    fn test_arber_skips_zero_value_items() {
        let base_iter = TestIter::new(vec![
            (
                Decimal::from_str("0.5").unwrap().inv().unwrap(),
                Uint128::from(1u128),
            ),
            (
                Decimal::from_str("0.625").unwrap().inv().unwrap(),
                Uint128::from(1000u128),
            ),
        ]);

        let quote_iter = TestIter::new(vec![
            (Decimal::from_str("2").unwrap(), Uint128::from(1u128)),
            (Decimal::from_str("0.8").unwrap(), Uint128::from(3000u128)),
        ]);

        let mut arb = Arber::default();
        let res = arb.run(base_iter, quote_iter).unwrap();
        assert_eq!(res.profit_base, Uint128::zero());
        assert_eq!(res.profit_quote, Uint128::from(175u128));
    }

    #[test]
    fn test_arber_keeps_quote_after_zero_first_leg_on_small_base() {
        let base_iter = vec![FloorItem::new("1.6", 1), FloorItem::new("1.6", 1000)].into_iter();
        let quote_iter = vec![FloorItem::new("0.8", 3000)].into_iter();

        let mut arb = Arber::new((), "prefix".to_string());
        let res = arb.run(base_iter, quote_iter).unwrap();

        assert_eq!(res.profit_base, Uint128::zero());
        assert_eq!(res.profit_quote, Uint128::from(175u128));
    }

    #[test]
    fn test_arber_keeps_base_after_zero_second_leg_on_small_quote() {
        let base_iter = vec![FloorItem::new("0.6", 1000)].into_iter();
        let quote_iter = vec![FloorItem::new("1", 1), FloorItem::new("2", 3000)].into_iter();

        let mut arb = Arber::new((), "prefix".to_string());
        let res = arb.run(base_iter, quote_iter).unwrap();

        assert_eq!(res.profit_base, Uint128::zero());
        assert_eq!(res.profit_quote, Uint128::from(334u128));
    }
}
