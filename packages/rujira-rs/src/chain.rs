use cosmwasm_schema::cw_serde;
use std::{
    fmt::{Display, Formatter, Result},
    str::FromStr,
};
use thiserror::Error;

#[cw_serde]
pub enum Chain {
    Avax,
    Bch,
    Bsc,
    Btc,
    Doge,
    Eth,
    Gaia,
    Ltc,
    Thor,
}

impl Display for Chain {
    fn fmt(&self, f: &mut Formatter) -> Result {
        write!(f, "{}", format!("{:?}", self).to_uppercase())
    }
}

impl TryFrom<String> for Chain {
    type Error = ChainParseError;

    fn try_from(value: String) -> std::result::Result<Self, Self::Error> {
        value.as_str().try_into()
    }
}

impl TryFrom<&String> for Chain {
    type Error = ChainParseError;

    fn try_from(value: &String) -> std::result::Result<Self, Self::Error> {
        value.as_str().try_into()
    }
}

impl TryFrom<&str> for Chain {
    type Error = ChainParseError;

    fn try_from(value: &str) -> std::result::Result<Self, Self::Error> {
        match value {
            "AVAX" => Ok(Self::Avax),
            "BCH" => Ok(Self::Bch),
            "BSC" => Ok(Self::Bsc),
            "BTC" => Ok(Self::Btc),
            "DOGE" => Ok(Self::Doge),
            "ETH" => Ok(Self::Eth),
            "GAIA" => Ok(Self::Gaia),
            "LTC" => Ok(Self::Ltc),
            "THOR" => Ok(Self::Thor),
            _ => Err(ChainParseError::UnknownChain(value.to_string())),
        }
    }
}

impl FromStr for Chain {
    type Err = ChainParseError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        s.try_into()
    }
}

#[derive(Error, Debug)]
pub enum ChainParseError {
    #[error("Unknown chain: {0}")]
    UnknownChain(String),
}
