use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Api, Decimal, StdResult};
use cw_storage_plus::Item;
use rujira_rs::{
    fin::{ConfigResponse, InstantiateMsg},
    Tick,
};

pub static CONFIG: Item<Config> = Item::new("config");

#[cw_serde]
pub struct Config {
    pub denoms: [String; 2],
    // pub oracles: Oracles,
    pub tick: Tick,
    pub fee_maker: Decimal,
    pub fee_taker: Decimal,
    pub fee_address: Addr,
}

impl Config {
    pub fn new(api: &dyn Api, value: InstantiateMsg) -> StdResult<Self> {
        Ok(Self {
            denoms: value.denoms.clone(),
            tick: value.tick,
            fee_taker: value.fee_taker,
            fee_maker: value.fee_maker,
            fee_address: api.addr_validate(value.fee_address.as_str())?,
            // oracles: value.oracles,
        })
    }
}

impl From<Config> for ConfigResponse {
    fn from(value: Config) -> Self {
        Self {
            denoms: value.denoms,
            tick: value.tick,
            fee_maker: value.fee_maker.into(),
            fee_taker: value.fee_taker.into(),
            fee_address: value.fee_address.to_string(),
            // oracles: value.oracles,
        }
    }
}
