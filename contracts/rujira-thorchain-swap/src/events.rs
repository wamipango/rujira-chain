use cosmwasm_std::{Coin, Event};

pub fn event_swap(
    amount: &Coin,
    quote_return: &Coin,
    min_return: &Coin,
    reserve_fee: &Coin,
    amm_fee: &Coin,
    returned: &Coin,
    memo: &String,
) -> Event {
    Event::new(format!("{}/swap", env!("CARGO_PKG_NAME")))
        .add_attribute("amount", amount.to_string())
        .add_attribute("quote_return", quote_return.to_string())
        .add_attribute("min_return", min_return.to_string())
        .add_attribute("reserve_fee", reserve_fee.to_string())
        .add_attribute("amm_fee", amm_fee.to_string())
        .add_attribute("returned", returned.to_string())
        .add_attribute("memo", memo)
}
