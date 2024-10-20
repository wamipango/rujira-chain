use cosmwasm_std::{Addr, Timestamp};
use cw_storage_plus::Map;

/// Record of the last time each address claimed from this contract
pub static CLAIMS: Map<Addr, Timestamp> = Map::new("claims");
