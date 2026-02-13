use cosmwasm_std::{Event, Uint128};
use cw_utils::NativeBalance;
use rujira_rs::fin::Denoms;

pub fn event_market_maker_fee(denoms: &Denoms, fees: &NativeBalance) -> Event {
    let mut base = Uint128::zero();
    let mut quote = Uint128::zero();
    for coin in fees.clone().into_vec() {
        if coin.denom == denoms.base() {
            base = coin.amount;
        } else if coin.denom == denoms.quote() {
            quote = coin.amount;
        }
    }
    Event::new(format!("{}/market-maker.fee", env!("CARGO_PKG_NAME")))
        .add_attribute("base", base)
        .add_attribute("quote", quote)
}
