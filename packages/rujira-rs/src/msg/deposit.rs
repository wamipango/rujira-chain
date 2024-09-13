use anybuf::Anybuf;
use cosmwasm_std::{AnyMsg, CosmosMsg};

use crate::coin::Coin;

pub struct MsgDeposit {
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
