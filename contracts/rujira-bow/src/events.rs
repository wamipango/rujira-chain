use cosmwasm_std::{Addr, Coin, Coins, Event};

pub fn event_deposit(owner: Addr, amount: Coins, minted: Coin) -> Event {
    Event::new("rujira-bow/deposit")
        .add_attribute("owner", owner.clone())
        .add_attribute("deposit", amount.to_string())
        .add_attribute("minted", minted.to_string())
}

pub fn event_withdraw(owner: Addr, share: Coin, withdrawn: Coins) -> Event {
    Event::new("rujira-bow/withdraw")
        .add_attribute("owner", owner.clone())
        .add_attribute("share", share.to_string())
        .add_attribute("withdrawn", withdrawn.to_string())
}
