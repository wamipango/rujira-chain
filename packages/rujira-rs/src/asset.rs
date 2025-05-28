use std::{
    fmt::{Display, Formatter, Result},
    result,
    str::FromStr,
};

use cosmwasm_schema::cw_serde;
use thiserror::Error;

use crate::memoed::Memoed;

#[cw_serde]
pub enum Asset {
    Native(NativeAsset),
    Secured(SecuredAsset),
    Layer1(Layer1Asset),
}

impl Display for Asset {
    fn fmt(&self, f: &mut Formatter) -> Result {
        match self {
            Asset::Native(native) => native.fmt(f),
            Asset::Secured(bridge) => bridge.fmt(f),
            Asset::Layer1(l1) => l1.fmt(f),
        }
    }
}

#[cw_serde]
pub struct NativeAsset {
    symbol: String,
}

impl NativeAsset {
    pub fn new(symbol: &str) -> Self {
        Self {
            symbol: symbol.to_string(),
        }
    }

    pub fn denom_string(&self) -> String {
        self.symbol.to_ascii_lowercase()
    }
}

impl Display for NativeAsset {
    fn fmt(&self, f: &mut Formatter) -> Result {
        write!(f, "RUNE.{}", self.symbol)
    }
}

#[cw_serde]
pub struct SecuredAsset {
    chain: String,
    symbol: String,
}

impl SecuredAsset {
    pub fn new(chain: &str, symbol: &str) -> Self {
        Self {
            chain: chain.to_string(),
            symbol: symbol.to_string(),
        }
    }

    pub fn denom_string(&self) -> String {
        format!("{}-{}", self.chain, self.symbol).to_ascii_lowercase()
    }

    pub fn ticker(&self) -> String {
        self.symbol
            .split('-')
            .next()
            .unwrap_or_default()
            .to_string()
    }
}

impl Display for SecuredAsset {
    fn fmt(&self, f: &mut Formatter) -> Result {
        write!(f, "{}-{}", self.chain, self.symbol)
    }
}

#[cw_serde]
pub struct Layer1Asset {
    chain: String,
    symbol: String,
}

impl Layer1Asset {
    pub fn new(chain: &str, symbol: &str) -> Self {
        Self {
            chain: chain.to_string(),
            symbol: symbol.to_string(),
        }
    }

    pub fn denom_string(&self) -> String {
        format!("{}.{}", self.chain, self.symbol).to_ascii_lowercase()
    }

    pub fn ticker(&self) -> String {
        self.symbol
            .split('-')
            .next()
            .unwrap_or_default()
            .to_string()
    }

    pub fn from_native(denom: String) -> std::result::Result<Self, Layer1AssetError> {
        if denom == *"rune" {
            return Ok(Self::new("THOR", "rune"));
        }
        match denom.split('-').collect::<Vec<_>>().as_slice() {
            [chain, symbol] => Ok(Self::new(chain, symbol)),
            _ => Err(Layer1AssetError::InvalidNativeDenom(denom)),
        }
    }

    pub fn is_rune(&self) -> bool {
        self.chain == "THOR" && self.symbol == "RUNE"
    }
}

#[derive(Error, Debug)]
pub enum Layer1AssetError {
    #[error("Invalid layer 1 string {0}")]
    Invalid(String),

    #[error("Invalid native denom string {0}")]
    InvalidNativeDenom(String),
}

impl Display for Layer1Asset {
    fn fmt(&self, f: &mut Formatter) -> Result {
        write!(f, "{}.{}", self.chain, self.symbol)
    }
}

impl TryFrom<&String> for Layer1Asset {
    type Error = Layer1AssetError;

    fn try_from(value: &String) -> result::Result<Self, Self::Error> {
        value.as_str().try_into()
    }
}

impl TryFrom<String> for Layer1Asset {
    type Error = Layer1AssetError;

    fn try_from(value: String) -> result::Result<Self, Self::Error> {
        value.as_str().try_into()
    }
}

impl TryFrom<&str> for Layer1Asset {
    type Error = Layer1AssetError;

    fn try_from(value: &str) -> result::Result<Self, Self::Error> {
        match value.split(".").collect::<Vec<_>>().as_slice() {
            [chain, symbol] => Ok(Self::new(chain, symbol)),
            _ => Err(Layer1AssetError::Invalid(value.to_owned())),
        }
    }
}

impl FromStr for Layer1Asset {
    type Err = Layer1AssetError;

    fn from_str(s: &str) -> result::Result<Self, Self::Err> {
        s.try_into()
    }
}

impl Memoed for Asset {
    fn to_memo(&self) -> String {
        match self {
            Asset::Native(native) => native.denom_string(),
            Asset::Secured(bridge) => bridge.denom_string(),
            Asset::Layer1(l1) => l1.denom_string(),
        }
    }
}

impl From<SecuredAsset> for Asset {
    fn from(value: SecuredAsset) -> Self {
        Self::Secured(value)
    }
}

impl From<NativeAsset> for Asset {
    fn from(value: NativeAsset) -> Self {
        Self::Native(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native() {
        let native = NativeAsset::new("RUNE");
        assert_eq!(native.to_string(), "RUNE.RUNE");
        assert_eq!(native.denom_string(), "rune");
        assert_eq!(format!("{}", native), "RUNE.RUNE");
    }

    #[test]
    fn bridge() {
        let native = SecuredAsset::new("BTC", "BTC");
        assert_eq!(native.to_string(), "BTC-BTC");
        assert_eq!(native.denom_string(), "btc-btc");
        assert_eq!(format!("{}", native), "BTC-BTC");

        let native = SecuredAsset::new("ETH", "RUNE-0x3155ba85d5f96b2d030a4966af206230e46849cb");
        assert_eq!(
            native.to_string(),
            "ETH-RUNE-0x3155ba85d5f96b2d030a4966af206230e46849cb"
        );
        assert_eq!(native.ticker(), "RUNE");

        assert_eq!(
            format!("{}", native),
            "ETH-RUNE-0x3155ba85d5f96b2d030a4966af206230e46849cb"
        );
    }

    #[test]
    fn l1() {
        let l1 = Layer1Asset::new("BTC", "BTC");
        assert_eq!(l1.to_string(), "BTC.BTC");
        assert_eq!(l1.denom_string(), "btc.btc");
        assert_eq!(format!("{}", l1), "BTC.BTC");

        let l1 = Layer1Asset::new("ETH", "RUNE-0x3155ba85d5f96b2d030a4966af206230e46849cb");
        assert_eq!(
            l1.to_string(),
            "ETH.RUNE-0x3155ba85d5f96b2d030a4966af206230e46849cb"
        );
        assert_eq!(l1.ticker(), "RUNE");

        assert_eq!(
            format!("{}", l1),
            "ETH.RUNE-0x3155ba85d5f96b2d030a4966af206230e46849cb"
        );
    }

    #[test]
    fn memo() {
        let asset: Asset = NativeAsset::new("RUNE").into();
        assert_eq!(asset.to_memo(), "rune");

        let asset: Asset = SecuredAsset::new("BTC", "BTC").into();
        assert_eq!(asset.to_memo(), "btc-btc");
    }
}
