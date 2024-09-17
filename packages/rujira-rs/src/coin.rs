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

#[cfg(test)]
mod tests {
    use crate::asset::NativeAsset;

    use super::*;

    #[test]
    fn encoding() {
        let buf: Anybuf = Coin::new(NativeAsset::new("uruji"), Uint256::from(100u128)).into();
        assert_eq!(
            buf.into_vec(),
            vec![10, 10, 82, 85, 78, 69, 46, 117, 114, 117, 106, 105, 18, 3, 49, 48, 48,]
        );
    }
}
