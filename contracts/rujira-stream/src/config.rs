use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Api, Coin, Decimal, StdResult, Storage};
use cw_storage_plus::Item;
use rujira_rs::stream::{InstantiateMsg, Schedule};

use crate::ContractError;

static CONFIG: Item<Config> = Item::new("config");
#[cw_serde]
pub struct Recipients(Vec<(Addr, u8)>);

impl Recipients {
    pub fn allocation_for(&self, recipient: Addr, coins: Vec<Coin>) -> Vec<Coin> {
        match self.0.iter().find(|x| x.0.eq(&recipient)) {
            None => vec![],
            Some((_, weight)) => {
                let total = self.0.iter().fold(0u8, |a, v| a + v.1);
                let ratio = Decimal::from_ratio(*weight, total);
                coins
                    .iter()
                    .map(|x| Coin {
                        amount: x.amount.mul_floor(ratio),
                        ..x.clone()
                    })
                    .collect()
            }
        }
    }
}

#[cw_serde]
pub struct Config {
    pub schedule: Schedule,
    pub recipients: Recipients,
}

impl Config {
    pub fn from_instantiate(api: &dyn Api, value: InstantiateMsg) -> StdResult<Self> {
        Ok(Self {
            recipients: Recipients::from_input(api, value.recipients)?,
            schedule: value.schedule,
        })
    }
}

impl Recipients {
    fn from_input(api: &dyn Api, recipients: Vec<(String, u8)>) -> StdResult<Self> {
        let inner = recipients
            .iter()
            .map(|(a, w)| Ok((api.addr_validate(a)?, *w)))
            .collect::<StdResult<Vec<(Addr, u8)>>>()?;

        Ok(Self(inner))
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
    use cosmwasm_std::Timestamp;

    use super::*;

    #[test]
    fn validation() {
        Config {
            recipients: Recipients(vec![(Addr::unchecked("recipient1"), 1u8)]),
            schedule: Schedule::Continuous {
                starts: Timestamp::default(),
                amount: vec![],
                period: 1000u64,
            },
        }
        .validate()
        .unwrap();
    }
}
