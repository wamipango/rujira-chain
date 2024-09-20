use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Binary, StdResult, Storage, Uint128};
use cw_storage_plus::Item;
use rujira_rs::staking::InstantiateMsg;

use crate::ContractError;

static CONFIG: Item<Config> = Item::new("config");

#[cw_serde]
pub struct Config {
    pub bond_denom: String,
    pub revenue_denom: String,
    pub revenue_converter: (Addr, Binary, Uint128),
}

impl From<InstantiateMsg> for Config {
    fn from(value: InstantiateMsg) -> Self {
        Self {
            bond_denom: value.bond_denom,
            revenue_denom: value.revenue_denom,
            revenue_converter: value.revenue_converter,
        }
    }
}

impl Config {
    pub fn load(storage: &dyn Storage) -> StdResult<Self> {
        CONFIG.load(storage)
    }

    pub fn validate(&self) -> Result<(), ContractError> {
        if self.bond_denom == "" {
            return Err(ContractError::Invalid("ruji_denom".to_string()));
        }
        if self.revenue_denom == "" {
            return Err(ContractError::Invalid("ruji_denom".to_string()));
        }
        if self.revenue_converter.2.is_zero() {
            return Err(ContractError::Invalid("revenue_converter".to_string()));
        }

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
            bond_denom: "".to_string(),
            revenue_denom: "uusdc".to_string(),
            revenue_converter: (Addr::unchecked(""), Binary::default(), Uint128::one()),
        }
        .validate()
        .unwrap_err();

        Config {
            bond_denom: "uruji".to_string(),
            revenue_denom: "".to_string(),
            revenue_converter: (Addr::unchecked(""), Binary::default(), Uint128::one()),
        }
        .validate()
        .unwrap_err();

        Config {
            bond_denom: "uruji".to_string(),
            revenue_denom: "uusdc".to_string(),
            revenue_converter: (Addr::unchecked(""), Binary::default(), Uint128::zero()),
        }
        .validate()
        .unwrap_err();

        Config {
            bond_denom: "uruji".to_string(),
            revenue_denom: "uusdc".to_string(),
            revenue_converter: (Addr::unchecked(""), Binary::default(), Uint128::one()),
        }
        .validate()
        .unwrap();
    }
}
