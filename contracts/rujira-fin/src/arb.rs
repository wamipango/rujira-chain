use cosmwasm_std::{coin, Decimal, StdResult, Storage, Uint128};
use cw_utils::NativeBalance;
use itertools::EitherOrBoth;
use rujira_rs::fin::{Denoms, Side};
use std::fmt::Debug;
use std::ops::{Add, Sub};

use crate::{pool::Pool, swappable::Swappable, ContractError};

/// Utility to capture arbitrage from overlapping orders
pub struct Arb {
    oracle: Decimal,
    pending: Vec<EitherOrBoth<Pool>>,
    denoms: Denoms,
}

impl Arb {
    pub fn new(oracle: &Decimal, denoms: &Denoms) -> Self {
        Self {
            oracle: *oracle,
            denoms: denoms.clone(),
            pending: vec![],
        }
    }
    pub fn run(&mut self, storage: &dyn Storage) -> Result<NativeBalance, ContractError> {
        let mut base_iter = Pool::side(storage, &Side::Base, &self.oracle);
        let mut quote_iter = Pool::side(storage, &Side::Quote, &self.oracle);
        let mut base = base_iter.next();
        let mut quote = quote_iter.next();
        let mut profit = NativeBalance::default();
        loop {
            match (&mut base, &mut quote) {
                (Some(b), Some(q)) => match b.arbitrage(q) {
                    Ok(res) => {
                        profit += coin(res.0.u128(), self.denoms.bid(&Side::Base));
                        profit += coin(res.1.u128(), self.denoms.bid(&Side::Quote));

                        self.pending.push(b.clone());
                        self.pending.push(q.clone());
                        if b.total().is_zero() {
                            base = base_iter.next()
                        }

                        if q.total().is_zero() {
                            quote = quote_iter.next()
                        }
                    }
                    Err(ContractError::Overflow(_)) => break,
                    Err(err) => return Err(err),
                },
                _ => break,
            }
        }
        profit.normalize();
        Ok(profit)
    }

    pub fn commit(&mut self, storage: &mut dyn Storage) -> StdResult<()> {
        for x in self.pending.iter_mut() {
            x.save(storage)?;
        }

        Ok(())
    }
}

pub trait Arbitrage: Swappable + Sized {
    /// Returns arbitrage profit in (self, other) token
    fn arbitrage(&mut self, other: &mut Self) -> Result<(Uint128, Uint128), ContractError>;
}

impl<T> Arbitrage for T
where
    T: Swappable + Clone + Debug,
{
    fn arbitrage(&mut self, other: &mut Self) -> Result<(Uint128, Uint128), ContractError> {
        let float = self.total();
        let a = other.swap(float)?;
        let remaining = float - a.0;
        let b = self.swap(a.1)?;
        Ok((remaining.add(b.1).checked_sub(float)?, a.1.sub(b.0)))
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use cosmwasm_std::{
        coin, coins,
        testing::{message_info, mock_env, MockStorage},
        Addr, Timestamp, Uint128,
    };
    use cw_utils::NativeBalance;
    use rujira_rs::fin::{Denoms, Price, Tick};

    use crate::order_manager::OrderManager;

    use super::*;

    #[test]

    fn test_aribtrage() {
        let mut storage = MockStorage::new();
        let oracle = Decimal::one();
        let mut base = Pool::load(
            &mut storage,
            &Price::Fixed(Decimal::from_str("1.15").unwrap()),
            &Side::Base,
            &oracle,
        );
        base.create_order(
            &mut storage,
            &Timestamp::default(),
            &Addr::unchecked("owner"),
            Uint128::from(1000u128),
        )
        .unwrap();

        let mut quote = Pool::load(
            &mut storage,
            &Price::Fixed(Decimal::from_str("1.5").unwrap()),
            &Side::Quote,
            &oracle,
        );
        quote
            .create_order(
                &mut storage,
                &Timestamp::default(),
                &Addr::unchecked("owner"),
                Uint128::from(3000u128),
            )
            .unwrap();

        // This arbitrage should take the balance of the base (sell) pool (1000 eg RUJI)
        // And sell it to the quote pool which has 3000 eg USDC @ 1.5 = 1500
        // 1500 USDC will then be buy back from base pool at 1.15
        // clamped to max 1000 returned for 1150 spent
        // 350 USDC profit, zero RUJI profit
        // RUJI profit = -1000 float  + 1000 returned at end
        // USDC profit = 1500 returned from first swap - 1150 spend on second
        let (b, q) = base.arbitrage(&mut quote).unwrap();
        assert_eq!(b, Uint128::zero());
        assert_eq!(q, Uint128::from(349u128));

        let mut storage = MockStorage::new();
        let mut base = Pool::load(
            &mut storage,
            &Price::Fixed(Decimal::from_str("1.15").unwrap()),
            &Side::Base,
            &oracle,
        );
        base.create_order(
            &mut storage,
            &Timestamp::default(),
            &Addr::unchecked("owner"),
            Uint128::from(1000u128),
        )
        .unwrap();

        let mut quote = Pool::load(
            &mut storage,
            &Price::Fixed(Decimal::from_str("1.5").unwrap()),
            &Side::Quote,
            &oracle,
        );
        quote
            .create_order(
                &mut storage,
                &Timestamp::default(),
                &Addr::unchecked("owner"),
                Uint128::from(3000u128),
            )
            .unwrap();
        // In reverse, 3000 USDC will buy 1000 RUJI for 1150, leaving 1850
        // 1000 RUJI will be sold for 1500 USDC to the quote pool
        // RUJI profit = + 1000 bought and then sold = 0
        // USDC profit = -3000 float + 1850 unused + 1500 returned = 350
        let (q, b) = quote.arbitrage(&mut base).unwrap();
        assert_eq!(b, Uint128::zero());
        assert_eq!(q, Uint128::from(349u128));
    }

    #[test]
    fn test_arber() {
        let mut storage = MockStorage::new();
        let env = mock_env();
        let info = message_info(&Addr::unchecked("addr0000"), &[]);
        let oracle = Decimal::from_str("1.0").unwrap();
        let funds = NativeBalance(vec![coin(1_000_000, "ruji"), coin(1_000_000, "usdc")]);
        let denoms = Denoms::new("ruji", "usdc");
        let tick = Tick::new(4);
        let fee = Decimal::from_str("0.001").unwrap();
        let mut e = OrderManager::new(
            denoms.clone(),
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
                    Price::Fixed(Decimal::from_str("1.15").unwrap()),
                    Uint128::from(1000u128),
                ),
                (
                    Side::Base,
                    Price::Fixed(Decimal::from_str("1.25").unwrap()),
                    Uint128::from(1000u128),
                ),
                (
                    Side::Quote,
                    Price::Fixed(Decimal::from_str("1.5").unwrap()),
                    Uint128::from(3000u128),
                ),
            ],
        )
        .unwrap();
        let mut arb = Arb::new(&oracle, &denoms);
        let res = arb.run(&mut storage).unwrap();
        arb.commit(&mut storage).unwrap();
        // 350 (349 with rounding) profit from arb 1
        // 250 from arb 2
        assert_eq!(res, NativeBalance(coins(599u128, "usdc")));

        let base = Pool::load(
            &mut storage,
            &Price::Fixed(Decimal::from_str("1.15").unwrap()),
            &Side::Base,
            &oracle,
        );

        assert_eq!(base.total(), Uint128::zero());

        let base = Pool::load(
            &mut storage,
            &Price::Fixed(Decimal::from_str("1.25").unwrap()),
            &Side::Base,
            &oracle,
        );

        assert_eq!(base.total(), Uint128::zero());
    }
}
