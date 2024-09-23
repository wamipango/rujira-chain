use anybuf::Anybuf;
use cosmwasm_std::{AnyMsg, CosmosMsg};

use crate::coin::Coin;

pub(crate) struct MsgDeposit {
    memo: String,
    coins: Vec<Coin>,
}

impl MsgDeposit {
    pub fn new(coins: Vec<Coin>, memo: String) -> Self {
        Self { memo, coins }
    }
}

impl From<MsgDeposit> for CosmosMsg {
    fn from(value: MsgDeposit) -> Self {
        let coins: Vec<Anybuf> = value.coins.iter().map(Anybuf::from).collect();
        let value = Anybuf::new()
            .append_repeated_message(1, &coins)
            .append_string(2, value.memo);

        CosmosMsg::Any(AnyMsg {
            type_url: "/types.MsgDeposit".to_string(),
            value: value.as_bytes().into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use cosmwasm_std::{Binary, Uint256};

    use crate::asset::NativeAsset;

    use super::*;

    #[test]
    fn encoding() {
        let msg: CosmosMsg = MsgDeposit::new(
            vec![Coin::new(NativeAsset::new("uruji"), Uint256::from(100u128))],
            "foo".to_string(),
        )
        .into();
        assert_eq!(
            msg,
            CosmosMsg::Any(AnyMsg {
                type_url: "/types.MsgDeposit".to_string(),
                value: Binary::from(vec![
                    0x0a, 0x11, 0x0a, 0x0a, 0x52, 0x55, 0x4e, 0x45, 0x2e, 0x75, 0x72, 0x75, 0x6a,
                    0x69, 0x12, 0x03, 0x31, 0x30, 0x30, 0x12, 0x03, 0x66, 0x6f, 0x6f
                ]),
            })
        )
    }
}
