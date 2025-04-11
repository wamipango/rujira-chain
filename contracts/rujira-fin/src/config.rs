use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Api, Decimal, Deps, StdResult, Storage};
use cw_storage_plus::Item;
use rujira_rs::{
    fin::{ConfigResponse, Denoms, InstantiateMsg, Tick},
    Layer1Asset, Oracle,
};

use crate::ContractError;

pub static CONFIG: Item<Config> = Item::new("config");

#[cw_serde]
pub struct Config {
    pub denoms: Denoms,
    pub oracles: Option<[Layer1Asset; 2]>,
    pub market_maker: Option<Addr>,
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
            market_maker: value
                .market_maker
                .map(|x| api.addr_validate(x.as_str()))
                .transpose()?,
            tick: value.tick,
            fee_taker: value.fee_taker,
            fee_maker: value.fee_maker,
            fee_address: api.addr_validate(value.fee_address.as_str())?,
        })
    }

    pub fn validate(&self, deps: Deps) -> Result<(), ContractError> {
        self.denoms.validate()?;
        if let Some(oracles) = self.oracles.clone() {
            oracles[0].price(deps.querier)?;
            oracles[1].price(deps.querier)?;
        }
        if self.fee_maker >= Decimal::one() {
            return Err(ContractError::Invalid("fee_maker >= 1".into()));
        }
        if self.fee_taker >= Decimal::one() {
            return Err(ContractError::Invalid("fee_take >= 1".into()));
        }
        Ok(())
    }

    pub fn save(&self, storage: &mut dyn Storage) -> StdResult<()> {
        CONFIG.save(storage, self)
    }

    pub fn update(
        &mut self,
        tick: Option<Tick>,
        market_maker: Option<Addr>,
        fee_taker: Option<Decimal>,
        fee_maker: Option<Decimal>,
        fee_address: Option<Addr>,
        oracles: Option<[Layer1Asset; 2]>,
    ) {
        if let Some(tick) = tick {
            self.tick = tick;
        }
        if let Some(market_maker) = market_maker {
            self.market_maker = Some(market_maker);
        }
        if let Some(fee_taker) = fee_taker {
            self.fee_taker = fee_taker;
        }
        if let Some(fee_maker) = fee_maker {
            self.fee_maker = fee_maker;
        }
        if let Some(fee_address) = fee_address {
            self.fee_address = fee_address;
        }
        if let Some(oracles) = oracles {
            self.oracles = Some(oracles);
        }
    }
}

impl From<Config> for ConfigResponse {
    fn from(value: Config) -> Self {
        Self {
            denoms: value.denoms,
            oracles: value.oracles,
            market_maker: value.market_maker.map(|x| x.to_string()),
            tick: value.tick,
            fee_maker: value.fee_maker,
            fee_taker: value.fee_taker,
            fee_address: value.fee_address.to_string(),
        }
    }
}
