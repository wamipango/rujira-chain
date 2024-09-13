use std::fmt::{Display, Formatter, Result};

#[derive(Debug, Clone)]
pub enum Chain {
    BSC,
    ETH,
    BTC,
    LTC,
    BCH,
    DOGE,
    THOR,
    GAIA,
    AVAX,
}

impl Display for Chain {
    fn fmt(&self, f: &mut Formatter) -> Result {
        write!(f, "{:?}", self)
    }
}
