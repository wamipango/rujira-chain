use cosmwasm_std::{Addr, Coin, Coins, Event};

pub fn event_deposit(owner: Addr, amount: Coins, minted: Coin) -> Event {
    Event::new(format!("{}/deposit", env!("CARGO_PKG_NAME")))
        .add_attribute("owner", owner.clone())
        .add_attribute("deposit", amount.to_string())
        .add_attribute("minted", minted.to_string())
}

pub fn event_withdraw(owner: Addr, share: Coin, withdrawn: Coins) -> Event {
    Event::new(format!("{}/withdraw", env!("CARGO_PKG_NAME")))
        .add_attribute("owner", owner.clone())
        .add_attribute("share", share.to_string())
        .add_attribute("withdrawn", withdrawn.to_string())
}
