use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Decimal, Deps, StdResult, Uint128};
use cw_storage_plus::Item;
use rujira_rs::{
    ghost_borrow::{ConfigResponse, StatusResponse},
    ghost_registry as registry,
};

pub static REGISTRY: Item<Addr> = Item::new("registry");

#[cw_serde]
pub struct Config {
    pub collateral_denom: String,
    pub debt_denom: String,
    pub vault_addr: String,
    pub orca_addr: String,
}

#[cw_serde]
pub struct Status {
    pub collateral_denom: String,

    pub debt_denom: String,

    /// Total amount of collateral deposited
    pub collateral_amount: Uint128,

    /// Total amount of debt issued
    pub debt_amount: Uint128,

    /// Current borrow rate
    pub borrow_rate: Decimal,

    /// Current borrow limit
    pub borrow_limit: Uint128,
}

impl Config {
    pub fn load(deps: Deps, addr: &Addr) -> StdResult<Self> {
        let res: registry::ContractConfigResponse = deps.querier.query_wasm_smart(
            REGISTRY.load(deps.storage)?,
            &registry::QueryMsg::ContractConfig(addr.to_string()),
        )?;
        Ok(res.into())
    }
}

impl From<registry::ContractConfigResponse> for Config {
    fn from(value: registry::ContractConfigResponse) -> Self {
        Self {
            collateral_denom: value.collateral_denom,
            debt_denom: value.debt_denom,
            vault_addr: value.vault_addr,
            orca_addr: value.orca_addr,
        }
    }
}

impl Status {
    pub fn load(deps: Deps, addr: &Addr) -> StdResult<Self> {
        let res: registry::ContractStatusResponse = deps.querier.query_wasm_smart(
            REGISTRY.load(deps.storage)?,
            &registry::QueryMsg::ContractStatus(addr.to_string()),
        )?;
        Ok(res.into())
    }
}

impl From<registry::ContractStatusResponse> for Status {
    fn from(value: registry::ContractStatusResponse) -> Self {
        Self {
            collateral_denom: value.collateral_denom,
            debt_denom: value.debt_denom,
            collateral_amount: value.collateral_amount,
            debt_amount: value.debt_amount,
            borrow_rate: value.borrow_rate,
            borrow_limit: value.borrow_limit,
        }
    }
}

impl From<Status> for StatusResponse {
    fn from(value: Status) -> Self {
        Self {
            collateral_denom: value.collateral_denom,
            debt_denom: value.debt_denom,
            collateral_amount: value.collateral_amount,
            debt_amount: value.debt_amount,
            borrow_rate: value.borrow_rate,
            borrow_limit: value.borrow_limit,
        }
    }
}

impl From<Config> for ConfigResponse {
    fn from(value: Config) -> Self {
        Self {
            collateral_denom: value.collateral_denom,
            debt_denom: value.debt_denom,
            vault_addr: value.vault_addr,
            orca_addr: value.orca_addr,
        }
    }
}
