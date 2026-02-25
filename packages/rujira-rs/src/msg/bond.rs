use cosmwasm_std::{Addr, CanonicalAddr, CosmosMsg, Uint128};

use crate::{
    coin::Coin,
    memoed::{Memo, Memoed},
    Asset, Layer1Asset,
};

use super::deposit::MsgDeposit;

#[derive(Clone)]
pub struct MsgLeave {
    node: Addr,
    signer: CanonicalAddr,
    rune: Uint128,
}

impl MsgLeave {
    pub fn new(
        node: Addr,
        signer: &CanonicalAddr,
        // MsgDeposit requires exactly 1 coin. Send some RUNE so that it passes validation and is added to the node bond
        rune: Uint128,
    ) -> Self {
        Self {
            node,
            signer: signer.clone(),
            rune,
        }
    }
}

impl Memoed for MsgLeave {
    fn to_memo(&self) -> String {
        Memo::default().push(&"leave").push(&self.node).to_string()
    }
}

impl From<MsgLeave> for CosmosMsg {
    fn from(value: MsgLeave) -> Self {
        MsgDeposit::new(
            vec![Coin::new(
                Asset::Layer1(Layer1Asset::new("THOR", "RUNE")),
                value.rune.into(),
            )],
            value.to_memo(),
            value.signer,
        )
        .into()
    }
}

#[derive(Clone)]
pub struct MsgBond {
    node: Addr,
    amount: Uint128,
    signer: CanonicalAddr,
}

impl MsgBond {
    pub fn new(node: Addr, amount: Uint128, signer: CanonicalAddr) -> Self {
        Self {
            node,
            amount,
            signer,
        }
    }
}

impl Memoed for MsgBond {
    fn to_memo(&self) -> String {
        Memo::default().push(&"bond").push(&self.node).to_string()
    }
}

impl From<MsgBond> for CosmosMsg {
    fn from(value: MsgBond) -> Self {
        MsgDeposit::new(
            vec![Coin::new(
                Asset::Layer1(Layer1Asset::new("THOR", "RUNE")),
                value.amount.into(),
            )],
            value.to_memo(),
            value.signer,
        )
        .into()
    }
}

#[derive(Clone)]
pub struct MsgUnbond {
    node: Addr,
    amount: Uint128,
    provider: Option<Addr>,
    signer: CanonicalAddr,
}

impl MsgUnbond {
    pub fn new(
        node: Addr,
        amount: Uint128,
        provider: Option<Addr>,
        signer: &CanonicalAddr,
    ) -> Self {
        Self {
            node,
            amount,
            provider,
            signer: signer.clone(),
        }
    }
}

impl Memoed for MsgUnbond {
    fn to_memo(&self) -> String {
        let memo = Memo::default()
            .push(&"unbond")
            .push(&self.node)
            .push(&self.amount.to_string());
        match &self.provider {
            Some(provider) => memo.push(provider).to_string(),
            None => memo.to_string(),
        }
    }
}

impl From<MsgUnbond> for CosmosMsg {
    fn from(value: MsgUnbond) -> Self {
        MsgDeposit::new(vec![], value.to_memo(), value.signer).into()
    }
}
