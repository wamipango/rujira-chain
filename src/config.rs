use cosmwasm_schema::cw_serde;
use cosmwasm_std::{StdResult, Storage, Timestamp, Uint128};
use cw_storage_plus::Item;

use crate::msg::InstantiateMsg;

static CONFIG: Item<Config> = Item::new("config");

#[cw_serde]
pub struct Config {
    pub merge_denom: String,
    pub merge_supply: Uint128,
    pub ruji_denom: String,
    pub ruji_allocation: Uint128,
    pub decay_starts_at: Timestamp,
    pub decay_ends_at: Timestamp,
}

impl From<InstantiateMsg> for Config {
    fn from(value: InstantiateMsg) -> Self {
        Self {
            merge_denom: value.merge_denom,
            merge_supply: value.merge_supply,
            ruji_denom: value.ruji_denom,
            ruji_allocation: value.ruji_allocation,
            decay_starts_at: value.decay_starts_at,
            decay_ends_at: value.decay_ends_at,
        }
    }
}

impl Config {
    pub fn load(storage: &dyn Storage) -> StdResult<Self> {
        CONFIG.load(storage)
    }

    pub fn validate(&self) -> StdResult<()> {
        Ok(())
    }

    pub fn save(&self, storage: &mut dyn Storage) -> StdResult<()> {
        CONFIG.save(storage, self)
    }
}
