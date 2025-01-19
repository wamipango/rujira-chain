use cosmwasm_std::{Addr, Coin, Coins, Event};
use cw_utils::NativeBalance;

pub fn event_deposit(owner: Addr, amount: Coins, minted: Coin) -> Event {
    Event::new("rujira-bow/deposit")
        .add_attribute("owner", owner.clone())
        .add_attribute("deposit", amount.to_string())
        .add_attribute("minted", minted.to_string())
}

pub fn event_withdraw(owner: Addr, share: Coin, withdrawn: NativeBalance) -> Event {
    Event::new("rujira-bow/deposit")
        .add_attribute("owner", owner.clone())
        .add_attribute("share", share.to_string())
        .add_attribute("withdrawn", withdrawn.to_string())
}
