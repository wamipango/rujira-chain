use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, StdResult, Storage};
use cw_storage_plus::{Index, IndexList, IndexedMap, MultiIndex};
use rujira_rs::ghost_registry::ContractConfigResponse;

#[cw_serde]
pub struct ContractConfig {
    pub contract: Addr,
    pub collateral_denom: String,
    pub debt_denom: String,
    pub vault_addr: String,
    pub orca_addr: String,
}

pub struct ContractConfigIndexes<'a> {
    pub collateral_denom: MultiIndex<'a, String, ContractConfig, String>,
    pub debt_denom: MultiIndex<'a, String, ContractConfig, String>,
    pub vault_addr: MultiIndex<'a, Addr, ContractConfig, String>,
    pub orca_addr: MultiIndex<'a, Addr, ContractConfig, String>,
}

impl<'a> IndexList<ContractConfig> for ContractConfigIndexes<'a> {
    fn get_indexes(&'_ self) -> Box<dyn Iterator<Item = &'_ dyn Index<ContractConfig>> + '_> {
        let v: Vec<&dyn Index<ContractConfig>> = vec![
            &self.collateral_denom,
            &self.debt_denom,
            &self.vault_addr,
            &self.orca_addr,
        ];
        Box::new(v.into_iter())
    }
}

impl ContractConfig {
    pub fn load(storage: &dyn Storage, addr: Addr) -> StdResult<Self> {
        Self::store().load(storage, &addr)
    }

    fn store<'a>() -> IndexedMap<&'a Addr, Self, ContractConfigIndexes<'a>> {
        let indexes = ContractConfigIndexes {
            collateral_denom: MultiIndex::new(
                |_k, d: &Self| d.collateral_denom.clone(),
                "contracts",
                "contracts__collateral_denom",
            ),
            debt_denom: MultiIndex::new(
                |_k, d: &Self| d.debt_denom.clone(),
                "contracts",
                "contracts__debt_denom",
            ),
            vault_addr: MultiIndex::new(
                |_k, d: &Self| Addr::unchecked(d.vault_addr.clone()),
                "contracts",
                "contracts__vault_addr",
            ),
            orca_addr: MultiIndex::new(
                |_k, d: &Self| Addr::unchecked(d.orca_addr.clone()),
                "contracts",
                "contracts__orca_addr",
            ),
        };

        IndexedMap::new("contracts", indexes)
    }
}

impl From<ContractConfig> for ContractConfigResponse {
    fn from(value: ContractConfig) -> Self {
        Self {
            contract: value.contract.to_string(),
            collateral_denom: value.collateral_denom,
            debt_denom: value.debt_denom,
            vault_addr: value.vault_addr,
            orca_addr: value.orca_addr,
        }
    }
}
