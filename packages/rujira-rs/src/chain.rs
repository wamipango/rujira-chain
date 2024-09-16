use std::fmt::{Display, Formatter, Result};

#[derive(Debug, Clone)]
pub enum Chain {
    Bsc,
    Eth,
    Btc,
    Ltc,
    Bch,
    Doge,
    Thor,
    Gaia,
    Avax,
}

impl Display for Chain {
    fn fmt(&self, f: &mut Formatter) -> Result {
        write!(f, "{}", format!("{:?}", self).to_uppercase())
    }
}
