use std::ops::{Add, AddAssign};

use cosmwasm_std::Uint128;

#[derive(Debug, Default)]
pub struct SwapCommit {
    /// The (offer, ask) totals committed to by the MarketMaker
    pub market_maker: (Uint128, Uint128),
}

impl Add for SwapCommit {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self {
            market_maker: (
                self.market_maker.0.add(rhs.market_maker.0),
                self.market_maker.1.add(rhs.market_maker.1),
            ),
        }
    }
}

impl AddAssign for SwapCommit {
    fn add_assign(&mut self, rhs: Self) {
        self.market_maker.0 += rhs.market_maker.0;
        self.market_maker.1 += rhs.market_maker.1;
    }
}
