use std::str::FromStr;

use cosmwasm_std::{Addr, Coins, Env, Event, Reply, SubMsgResult, Uint128};

pub fn event_account_bond(owner: Addr, amount: Uint128) -> Event {
    Event::new(format!("{}/account.bond", env!("CARGO_PKG_NAME")))
        .add_attribute("owner", owner)
        .add_attribute("amount", amount)
}

pub fn event_account_claim(owner: Addr, reward_amount: Uint128) -> Event {
    Event::new(format!("{}/account.claim", env!("CARGO_PKG_NAME")))
        .add_attribute("owner", owner)
        .add_attribute("amount", reward_amount)
}

pub fn event_account_withdraw(owner: Addr, amount: Uint128, rewards: Uint128) -> Event {
    Event::new(format!("{}/account.withdraw", env!("CARGO_PKG_NAME")))
        .add_attribute("owner", owner)
        .add_attribute("amount", amount)
        .add_attribute("rewards", rewards)
}

pub fn event_liquid_bond(owner: Addr, amount: Uint128, shares: Uint128) -> Event {
    Event::new(format!("{}/liquid.bond", env!("CARGO_PKG_NAME")))
        .add_attribute("owner", owner)
        .add_attribute("amount", amount)
        .add_attribute("shares", shares)
}

pub fn event_liquid_unbond(owner: Addr, shares: Uint128, returned: Uint128) -> Event {
    Event::new(format!("{}/liquid.unbond", env!("CARGO_PKG_NAME")))
        .add_attribute("owner", owner)
        .add_attribute("shares", shares)
        .add_attribute("returned", returned)
}

pub fn extract_transfer_amount_from_reply(msg: &Reply, env: &Env, denom: &str) -> Option<Uint128> {
    if let SubMsgResult::Ok(result) = &msg.result {
        for event in &result.events {
            if event.ty != "transfer" {
                continue;
            }

            let mut recipient: Option<String> = None;
            let mut amount: Option<String> = None;

            for attr in &event.attributes {
                match attr.key.as_str() {
                    "recipient" => recipient = Some(attr.value.clone()),
                    "amount" => amount = Some(attr.value.clone()),
                    _ => {}
                }
            }

            if let (Some(r), Some(a)) = (recipient, amount) {
                let coins = Coins::from_str(&a).ok().unwrap();
                let denom_value = coins.amount_of(denom);
                if r == env.contract.address.as_str() && denom_value > Uint128::zero() {
                    return Some(denom_value);
                }
            }
        }
    }

    None
}
