use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Api, Decimal, StdResult, Storage};
use cw_storage_plus::Item;
use rujira_rs::thorchain_swap::{ConfigResponse, ConfigUpdate, InstantiateMsg};

use crate::ContractError;

static CONFIG: Item<Config> = Item::new("config");

#[cw_serde]
pub struct Config {
    pub max_stream_length: u32,
    pub stream_step_ratio: Decimal,
    pub max_borrow_ratio: Decimal,
    pub reserve_fee: Decimal,
    pub fee: (Decimal, Addr),
}

impl Config {
    pub fn new(msg: InstantiateMsg, api: &dyn Api) -> StdResult<Self> {
        let addr = api.addr_validate(&msg.fee.1)?;
        Ok(Self {
            max_stream_length: msg.max_stream_length,
            stream_step_ratio: msg.stream_step_ratio,
            max_borrow_ratio: msg.max_borrow_ratio,
            reserve_fee: msg.reserve_fee,
            fee: (msg.fee.0, addr),
        })
    }

    pub fn load(storage: &dyn Storage) -> StdResult<Self> {
        CONFIG.load(storage)
    }

    pub fn validate(&self) -> Result<(), ContractError> {
        Ok(())
    }

    pub fn update(&mut self, update: &ConfigUpdate, api: &dyn Api) -> StdResult<()> {
        if let Some(max_stream_length) = update.max_stream_length {
            self.max_stream_length = max_stream_length;
        }
        if let Some(stream_step_ratio) = update.stream_step_ratio {
            self.stream_step_ratio = stream_step_ratio;
        }
        if let Some(max_borrow_ratio) = update.max_borrow_ratio {
            self.max_borrow_ratio = max_borrow_ratio;
        }
        if let Some(reserve_fee) = update.reserve_fee {
            self.reserve_fee = reserve_fee;
        }
        if let Some(fee) = update.fee.clone() {
            let addr = api.addr_validate(&fee.1)?;
            self.fee = (fee.0, addr);
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
            max_stream_length: value.max_stream_length,
            stream_step_ratio: value.stream_step_ratio,
            max_borrow_ratio: value.max_borrow_ratio,
            reserve_fee: value.reserve_fee,
            fee: value.fee,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::testing::mock_dependencies;

    #[test]
    fn validation() {
        let deps = mock_dependencies();
        Config {
            max_stream_length: 1,
            max_borrow_ratio: Decimal::one(),
            reserve_fee: Decimal::from_ratio(10u128, 500u128),
            stream_step_ratio: Decimal::one(),
            fee: (Decimal::zero(), deps.api.addr_make("fee")),
        }
        .validate()
        .unwrap();
    }
}
