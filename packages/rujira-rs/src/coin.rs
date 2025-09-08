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
            .append_message(1, &Anybuf::from(value.asset))
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
            vec![
                10, 20, 10, 4, 84, 72, 79, 82, 18, 5, 117, 114, 117, 106, 105, 26, 5, 117, 114,
                117, 106, 105, 18, 3, 49, 48, 48
            ]
        );
    }
}
