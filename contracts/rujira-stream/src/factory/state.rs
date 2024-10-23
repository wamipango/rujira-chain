use cosmwasm_std::Uint128;
use cw_storage_plus::Item;

pub static COUNTER: Item<Uint128> = Item::new("counter");
