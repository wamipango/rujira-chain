use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Api, Decimal, StdResult, Storage};
use cw_storage_plus::Item;
use rujira_rs::{
    fin::{ConfigResponse, Denoms, InstantiateMsg},
    Layer1Asset, Tick,
};

pub static CONFIG: Item<Config> = Item::new("config");

#[cw_serde]
pub struct Config {
    pub denoms: Denoms,
    pub oracles: [Layer1Asset; 2],
    pub tick: Tick,
    pub fee_maker: Decimal,
    pub fee_taker: Decimal,
    pub fee_address: Addr,
}

impl Config {
    pub fn new(api: &dyn Api, value: InstantiateMsg) -> StdResult<Self> {
        Ok(Self {
            denoms: value.denoms.clone(),
            oracles: value.oracles,
            tick: value.tick,
            fee_taker: value.fee_taker,
            fee_maker: value.fee_maker,
            fee_address: api.addr_validate(value.fee_address.as_str())?,
        })
    }

    pub fn validate(&self) -> StdResult<()> {
        Ok(())
    }

    pub fn save(&self, storage: &mut dyn Storage) -> StdResult<()> {
        CONFIG.save(storage, self)
    }
}

impl From<Config> for ConfigResponse {
    fn from(value: Config) -> Self {
        Self {
            denoms: value.denoms,
            oracles: value.oracles,
            tick: value.tick,
            fee_maker: value.fee_maker,
            fee_taker: value.fee_taker,
            fee_address: value.fee_address.to_string(),
        }
    }
}
