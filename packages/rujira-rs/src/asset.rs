use std::fmt::{Display, Formatter, Result};

use crate::{chain::Chain, memoed::Memoed};

#[derive(Clone)]
pub enum Asset {
    Native(NativeAsset),
    Bridge(BridgeAsset),
}

impl Display for Asset {
    fn fmt(&self, f: &mut Formatter) -> Result {
        match self {
            Asset::Native(native) => native.fmt(f),
            Asset::Bridge(bridge) => bridge.fmt(f),
        }
    }
}

#[derive(Clone)]
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

#[derive(Clone)]
pub struct BridgeAsset {
    chain: Chain,
    symbol: String,
}

impl BridgeAsset {
    pub fn new(chain: Chain, symbol: &str) -> Self {
        Self {
            chain,
            symbol: symbol.to_string(),
        }
    }

    pub fn denom_string(&self) -> String {
        format!("{}-{}", self.chain, self.symbol).to_ascii_lowercase()
    }

    pub fn ticker(&self) -> String {
        self.symbol
            .split("-")
            .next()
            .unwrap_or_default()
            .to_string()
    }
}

impl Display for BridgeAsset {
    fn fmt(&self, f: &mut Formatter) -> Result {
        write!(f, "{}-{}", self.chain, self.symbol)
    }
}

impl Memoed for Asset {
    fn to_memo(&self) -> String {
        match self {
            Asset::Native(native) => native.denom_string(),
            Asset::Bridge(bridge) => bridge.denom_string(),
        }
    }
}

impl From<BridgeAsset> for Asset {
    fn from(value: BridgeAsset) -> Self {
        Self::Bridge(value)
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
        let native = BridgeAsset::new(Chain::Btc, "BTC");
        assert_eq!(native.to_string(), "BTC-BTC");
        assert_eq!(native.denom_string(), "btc-btc");
        assert_eq!(format!("{}", native), "BTC-BTC");

        let native = BridgeAsset::new(
            Chain::Eth,
            "RUNE-0x3155ba85d5f96b2d030a4966af206230e46849cb",
        );
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
    fn memo() {
        let asset: Asset = NativeAsset::new("RUNE").into();
        assert_eq!(asset.to_memo(), "rune");

        let asset: Asset = BridgeAsset::new(Chain::Btc, "BTC").into();
        assert_eq!(asset.to_memo(), "btc-btc");
    }
}
