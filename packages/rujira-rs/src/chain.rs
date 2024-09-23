use std::fmt::{Display, Formatter, Result};

use cosmwasm_schema::cw_serde;

#[cw_serde]
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
