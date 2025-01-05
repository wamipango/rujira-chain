use cosmwasm_std::{Decimal, StdError, StdResult, Storage};
use cw_utils::NativeBalance;
use itertools::EitherOrBoth;
use rujira_rs::fin::Side;
use std::cmp::Ordering;

use crate::{pool::Pool, swappable::Swappable};

/// Utility to capture arbitrage from overlapping orders
pub struct Arb {
    oracle: Decimal,
    pending: Vec<EitherOrBoth<Pool>>,
}

impl Arb {
    pub fn new(oracle: &Decimal) -> Self {
        Self {
            oracle: *oracle,
            pending: vec![],
        }
    }
    pub fn run(&mut self, storage: &mut dyn Storage) -> StdResult<NativeBalance> {
        let mut base_iter = Pool::side(storage, &Side::Base, &self.oracle);
        let mut quote_iter = Pool::side(storage, &Side::Quote, &self.oracle);
        let mut base: Option<EitherOrBoth<Pool>> = base_iter.next();
        let mut quote: Option<EitherOrBoth<Pool>> = quote_iter.next();
        let mut profit: NativeBalance = NativeBalance::default();
        loop {
            match (&mut base, &mut quote) {
                (Some(b), Some(q)) => {
                    let res = b.arbitrage(q)?;
                    if res.is_empty() {
                        break;
                    };
                    profit += res;
                    if b.total().is_zero() {
                        base = base_iter.next()
                    }

                    if q.total().is_zero() {
                        quote = quote_iter.next()
                    }
                }
                _ => break,
            }
        }

        Ok(NativeBalance::default())
    }

    pub fn commit(&mut self, storage: &mut dyn Storage) -> StdResult<()> {
        for x in self.pending.iter_mut() {
            x.save(storage)?;
        }

        Ok(())
    }
}

// Utility trait to make handling EitherOrBoth<Pool>'s easier
pub trait Arbitrage: Swappable + Sized {
    // Execute arbitrage between the two price points
    // Returns the pool(s) with remaining funds.
    // If both are returned, there was no arbitrage opportunity and the iterator should break
    fn arbitrage(&mut self, other: &mut Self) -> StdResult<NativeBalance>;
}

impl Arbitrage for EitherOrBoth<Pool> {
    fn arbitrage(&mut self, other: &mut Self) -> StdResult<NativeBalance> {
        if self.clone().into_left().side == Side::Quote {
            return Err(StdError::generic_err("wrong side"));
        }

        match self.rate().cmp(&other.rate()) {
            Ordering::Less => {
                // ...execute
                todo!();
            }
            Ordering::Equal => {
                match self.total().cmp(&other.total()) {
                    Ordering::Less => {
                        // Fully consume self (base)

                        todo!();
                    }
                    Ordering::Equal => {
                        // Fully consume both
                        Ok(NativeBalance::default())
                    }
                    Ordering::Greater => {
                        // Fully consume other (quote)
                        todo!();
                    }
                }
                // ...execute
            }
            // No opportunity
            Ordering::Greater => Ok(NativeBalance::default()),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use cosmwasm_std::{
        coin,
        testing::{message_info, mock_env, MockStorage},
        Addr, Uint128,
    };
    use rujira_rs::fin::{Denoms, Price, Tick};

    use crate::order_manager::OrderManager;

    use super::*;
    #[test]
    fn test_arbitrage() {
        let mut storage = MockStorage::new();
        let env = mock_env();
        let info = message_info(&Addr::unchecked("addr0000"), &[]);
        let oracle = Decimal::from_str("1.0").unwrap();
        let funds = NativeBalance(vec![coin(1_000_000, "ruji"), coin(1_000_000, "usdc")]);
        let denoms = Denoms::new("ruji", "usdc");
        let tick = Tick::new(4);
        let fee = Decimal::from_str("0.001").unwrap();
        let mut e = OrderManager::new(
            denoms,
            fee,
            tick,
            info.sender,
            env.block.time,
            oracle,
            funds,
        );

        e.execute_orders(
            &mut storage,
            vec![
                (
                    Side::Base,
                    Price::Fixed(Decimal::from_str("1.1").unwrap()),
                    Uint128::from(500u128),
                ),
                (Side::Base, Price::Oracle(100), Uint128::from(400u128)),
                (
                    Side::Base,
                    Price::Fixed(Decimal::from_str("0.95").unwrap()),
                    Uint128::from(500u128),
                ),
                (
                    Side::Quote,
                    Price::Fixed(Decimal::from_str("1.0").unwrap()),
                    Uint128::from(1000u128),
                ),
                (Side::Quote, Price::Oracle(-1000), Uint128::from(1000u128)),
                (
                    Side::Quote,
                    Price::Fixed(Decimal::from_str("0.9").unwrap()),
                    Uint128::from(1000u128),
                ),
                (Side::Quote, Price::Oracle(-1500), Uint128::from(1000u128)),
                (
                    Side::Quote,
                    Price::Fixed(Decimal::from_str("0.8").unwrap()),
                    Uint128::from(1000u128),
                ),
                (
                    Side::Quote,
                    Price::Fixed(Decimal::from_str("0.7").unwrap()),
                    Uint128::from(1000u128),
                ),
                (
                    Side::Quote,
                    Price::Fixed(Decimal::from_str("0.6").unwrap()),
                    Uint128::from(1000u128),
                ),
            ],
        )
        .unwrap();

        Arb::new(&oracle).run(&mut storage).unwrap();
    }
}
