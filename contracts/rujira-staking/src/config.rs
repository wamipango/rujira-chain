use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Api, Binary, StdResult, Storage, Uint128};
use cw_storage_plus::Item;
use rujira_rs::staking::{ConfigResponse, InstantiateMsg};

use crate::ContractError;

static CONFIG: Item<Config> = Item::new("config");

#[cw_serde]
pub struct Config {
    pub bond_denom: String,
    pub revenue_denom: String,
    pub revenue_converter: (Addr, Binary, Uint128),
}

impl Config {
    pub fn new(api: &dyn Api, value: InstantiateMsg) -> StdResult<Self> {
        Ok(Self {
            bond_denom: value.bond_denom,
            revenue_denom: value.revenue_denom,
            revenue_converter: (
                api.addr_validate(&value.revenue_converter.0)?,
                value.revenue_converter.1,
                value.revenue_converter.2,
            ),
        })
    }

    pub fn load(storage: &dyn Storage) -> StdResult<Self> {
        CONFIG.load(storage)
    }

    pub fn validate(&self) -> Result<(), ContractError> {
        if self.bond_denom.is_empty() {
            return Err(ContractError::Invalid("ruji_denom".to_string()));
        }
        if self.revenue_denom.is_empty() {
            return Err(ContractError::Invalid("ruji_denom".to_string()));
        }

        Ok(())
    }

    pub fn save(&self, storage: &mut dyn Storage) -> StdResult<()> {
        CONFIG.save(storage, self)
    }
}

impl From<Config> for ConfigResponse {
    fn from(value: Config) -> Self {
        Self {
            bond_denom: value.bond_denom,
            revenue_denom: value.revenue_denom,
            revenue_converter: (
                value.revenue_converter.0.to_string(),
                value.revenue_converter.1,
                value.revenue_converter.2,
            ),
        }
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
            revenue_converter: (Addr::unchecked(""), Binary::default(), Uint128::one()),
        }
        .validate()
        .unwrap();
    }
}
