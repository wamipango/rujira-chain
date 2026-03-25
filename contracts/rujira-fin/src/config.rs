use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Api, Decimal, Deps, StdResult, Storage};
use cw_storage_plus::Item;
use rujira_rs::{
    fin::{ConfigResponse, ConfigUpdate, Denoms, InstantiateMsg, OracleUpdate, Tick},
    OraclePair,
};

use crate::{market_makers::MarketMakers, ContractError};

pub static CONFIG: Item<Config> = Item::new("config");

#[cw_serde]
pub struct Config {
    pub denoms: Denoms,
    pub oracles: Option<OraclePair>,
    pub market_makers: MarketMakers,
    #[serde(default)]
    pub range_delta: Decimal,
    pub tick: Tick,
    pub fee_maker: Decimal,
    pub fee_taker: Decimal,
    #[serde(default)]
    pub fee_range: Decimal,
    pub fee_address: Addr,
}

impl Config {
    pub fn new(api: &dyn Api, value: InstantiateMsg) -> StdResult<Self> {
        Ok(Self {
            denoms: value.denoms.clone(),
            oracles: value.oracles,
            market_makers: MarketMakers::new(api, value.market_makers)?,
            tick: value.tick,
            fee_taker: value.fee_taker,
            fee_maker: value.fee_maker,
            fee_range: value.fee_range,
            range_delta: value.range_delta,
            fee_address: api.addr_validate(value.fee_address.as_str())?,
        })
    }

    pub fn validate(&self, deps: Deps) -> Result<(), ContractError> {
        self.denoms.validate()?;
        if let Some(oracles) = self.oracles.clone() {
            oracles.price(deps.querier)?;
        }
        if self.fee_maker >= Decimal::one() {
            return Err(ContractError::Invalid("fee_maker >= 1".into()));
        }
        if self.fee_taker >= Decimal::one() {
            return Err(ContractError::Invalid("fee_take >= 1".into()));
        }
        if self.range_delta.is_zero() {
            return Err(ContractError::Invalid("range_delta = 0".into()));
        }
        self.tick.validate()?;
        Ok(())
    }

    pub fn save(&self, storage: &mut dyn Storage) -> StdResult<()> {
        CONFIG.save(storage, self)
    }

    pub fn update(&mut self, api: &dyn Api, update: &ConfigUpdate) -> StdResult<()> {
        if let Some(market_makers) = &update.market_makers {
            self.market_makers = MarketMakers::new(api, market_makers.to_vec())?;
        }
        if let Some(fee_taker) = update.fee_taker {
            self.fee_taker = fee_taker;
        }
        if let Some(fee_maker) = update.fee_maker {
            self.fee_maker = fee_maker;
        }
        if let Some(fee_range) = update.fee_range {
            self.fee_range = fee_range;
        }
        if let Some(range_delta) = update.range_delta {
            self.range_delta = range_delta;
        }

        if let Some(fee_address) = &update.fee_address {
            self.fee_address = api.addr_validate(fee_address)?;
        }
        if let Some(oracles) = &update.oracles {
            match oracles {
                OracleUpdate::Set(v) => self.oracles = Some(v.clone()),
                OracleUpdate::Unset {} => self.oracles = None,
            }
        }
        Ok(())
    }
}

impl From<Config> for ConfigResponse {
    fn from(value: Config) -> Self {
        Self {
            denoms: value.denoms,
            oracles: value.oracles,
            market_makers: value
                .market_makers
                .contracts
                .iter()
                .map(|x| x.to_string())
                .collect(),
            tick: value.tick,
            fee_maker: value.fee_maker,
            fee_taker: value.fee_taker,
            fee_range: value.fee_range,
            range_delta: value.range_delta,
            fee_address: value.fee_address.to_string(),
        }
    }
}
