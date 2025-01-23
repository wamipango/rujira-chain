use cosmwasm_schema::cw_serde;
use cosmwasm_std::{HexBinary, StdResult, Storage};
use cw_storage_plus::Item;
use rujira_rs::ghost_registry::InstantiateMsg;

use crate::ContractError;

static CONFIG: Item<Config> = Item::new("config");

#[cw_serde]
pub struct Config {
    pub checksum: HexBinary,
    pub code_id: u64,
}

impl From<InstantiateMsg> for Config {
    fn from(value: InstantiateMsg) -> Self {
        Self {
            checksum: value.checksum,
            code_id: value.code_id,
        }
    }
}

impl Config {
    pub fn load(storage: &dyn Storage) -> StdResult<Self> {
        CONFIG.load(storage)
    }

    pub fn validate(&self) -> Result<(), ContractError> {
        Ok(())
    }

    pub fn save(&self, storage: &mut dyn Storage) -> StdResult<()> {
        CONFIG.save(storage, self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validation() {
        Config {
            checksum: HexBinary::default(),
            code_id: 1,
        }
        .validate()
        .unwrap();
    }
}
