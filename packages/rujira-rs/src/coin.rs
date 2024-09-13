use crate::asset::Asset;
use anybuf::Anybuf;
use cosmwasm_std::Uint256;

#[derive(Clone)]
pub struct Coin {
    asset: Asset,
    amount: Uint256,
}

impl Coin {
    pub fn new<T: Into<Asset>>(asset: T, amount: Uint256) -> Self {
        Self {
            asset: asset.into(),
            amount,
        }
    }
}

impl From<Coin> for Anybuf {
    fn from(value: Coin) -> Self {
        Anybuf::new()
            .append_string(1, value.asset.to_string())
            .append_string(2, value.amount.to_string())
    }
}

impl From<&Coin> for Anybuf {
    fn from(value: &Coin) -> Self {
        value.clone().into()
    }
}
