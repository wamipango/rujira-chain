use cosmwasm_std::{Addr, CosmosMsg};

use crate::coin::Coin;
use crate::memoed::{Memo, Memoed};

use super::deposit::MsgDeposit;

#[derive(Clone)]
pub struct MsgBridgeAssetWithdraw {
    to: Addr,
    amount: Coin,
}

impl MsgBridgeAssetWithdraw {
    pub fn new(amount: Coin, to: Addr) -> Self {
        Self { amount, to }
    }
}

impl Memoed for MsgBridgeAssetWithdraw {
    fn to_memo(&self) -> String {
        Memo::default().push(&"bridge-").push(&self.to).to_string()
    }
}

impl From<MsgBridgeAssetWithdraw> for CosmosMsg {
    fn from(value: MsgBridgeAssetWithdraw) -> Self {
        MsgDeposit::new(vec![value.amount.clone()], value.to_memo()).into()
    }
}

#[cfg(test)]
mod tests {
    use cosmwasm_std::{testing::MockApi, Uint256};

    use crate::asset::NativeAsset;

    use super::*;

    #[test]
    fn encoding() {
        let msg = MsgBridgeAssetWithdraw::new(
            Coin::new(NativeAsset::new("uruji"), Uint256::from(100u128)),
            MockApi::default().addr_make("recipient"),
        );
        assert_eq!(
            msg.to_memo(),
            "bridge-:cosmwasm1vewsdxxmeraett7ztsaym88jsrv85kzm0xvjg09xqz8aqvjcja0syapxq9"
        );
    }
}
