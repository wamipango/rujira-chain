use std::fmt::{Display, Formatter, Result};

use cosmwasm_schema::cw_serde;

use crate::{chain::Chain, memoed::Memoed};

#[cw_serde]
pub enum Asset {
    Native(NativeAsset),
    Bridge(BridgeAsset),
    L1(L1Asset),
}

impl Display for Asset {
    fn fmt(&self, f: &mut Formatter) -> Result {
        match self {
            Asset::Native(native) => native.fmt(f),
            Asset::Bridge(bridge) => bridge.fmt(f),
            Asset::L1(l1) => l1.fmt(f),
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

#[cw_serde]
pub struct L1Asset {
    chain: Chain,
    symbol: String,
}

impl L1Asset {
    pub fn new(chain: Chain, symbol: &str) -> Self {
        Self {
            chain,
            symbol: symbol.to_string(),
        }
    }

    pub fn denom_string(&self) -> String {
        format!("{}.{}", self.chain, self.symbol).to_ascii_lowercase()
    }

    pub fn ticker(&self) -> String {
        self.symbol
            .split("-")
            .next()
            .unwrap_or_default()
            .to_string()
    }
}

impl Display for L1Asset {
    fn fmt(&self, f: &mut Formatter) -> Result {
        write!(f, "{}.{}", self.chain, self.symbol)
    }
}

impl Memoed for Asset {
    fn to_memo(&self) -> String {
        match self {
            Asset::Native(native) => native.denom_string(),
            Asset::Bridge(bridge) => bridge.denom_string(),
            Asset::L1(l1) => l1.denom_string(),
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
    fn l1() {
        let l1 = L1Asset::new(Chain::Btc, "BTC");
        assert_eq!(l1.to_string(), "BTC.BTC");
        assert_eq!(l1.denom_string(), "btc.btc");
        assert_eq!(format!("{}", l1), "BTC.BTC");

        let l1 = L1Asset::new(
            Chain::Eth,
            "RUNE-0x3155ba85d5f96b2d030a4966af206230e46849cb",
        );
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

        let asset: Asset = BridgeAsset::new(Chain::Btc, "BTC").into();
        assert_eq!(asset.to_memo(), "btc-btc");
    }
}
