use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Decimal, Deps, SignedDecimal, StdResult, Storage, Uint128};
use cw_storage_plus::Item;
use cw_utils::Duration;
use rujira_rs::brune::{ConfigResponse, ConfigUpdate, InstantiateMsg, RangeConfig};

use crate::ContractError;

static CONFIG: Item<Config> = Item::new("config");

pub static RUNE: &str = "rune";

#[cw_serde]
pub struct Config {
    pub target_utilization: Decimal,
    pub min_node_fee: Decimal,
    pub range: RangeConfig,
    pub permissionless: bool,
    pub fin_contract: Addr,
    pub stake_contract: Addr,
    pub revenue_smear: u32,
    pub quarantine: Duration,
    pub token_id: String,
    pub max_bond: Uint128,
    pub max_effective_bond: Uint128,
    pub mint_cap: Uint128,
}

impl Config {
    pub fn new(deps: Deps, msg: &InstantiateMsg) -> StdResult<Self> {
        Ok(Self {
            target_utilization: msg.target_utilization,
            min_node_fee: msg.min_node_fee,
            range: msg.range.clone(),
            permissionless: msg.permissionless,
            fin_contract: deps.api.addr_validate(&msg.fin_contract)?,
            stake_contract: deps.api.addr_validate(&msg.stake_contract)?,
            revenue_smear: msg.revenue_smear,
            quarantine: msg.quarantine,
            token_id: msg.token_id.clone(),
            max_bond: msg.max_bond,
            max_effective_bond: msg.max_effective_bond,
            mint_cap: msg.mint_cap,
        })
    }

    pub fn load(storage: &dyn Storage) -> StdResult<Self> {
        CONFIG.load(storage)
    }

    pub fn validate(&self) -> Result<(), ContractError> {
        if self.target_utilization <= Decimal::zero() || self.target_utilization >= Decimal::one() {
            return Err(ContractError::Invalid(
                "target_utilization must be between 0 and 1".to_string(),
            ));
        }
        if self.min_node_fee <= Decimal::zero() || self.min_node_fee >= Decimal::one() {
            return Err(ContractError::Invalid(
                "min_node_fee must be between 0 and 1".to_string(),
            ));
        }
        if self.range.high >= Decimal::one() {
            return Err(ContractError::Invalid(
                "range.high must be less than 1".to_string(),
            ));
        }
        if self.range.low >= self.range.high {
            return Err(ContractError::Invalid(
                "range.low must be less than range.high".to_string(),
            ));
        }
        let two = SignedDecimal::from_atomics(2i128, 0).unwrap();
        if self.range.skew <= -two || self.range.skew >= two {
            return Err(ContractError::Invalid(
                "range.skew must be between -2 and 2".to_string(),
            ));
        }
        if self.range.step <= Decimal::zero() || self.range.step >= Decimal::one() {
            return Err(ContractError::Invalid(
                "range.step must be between 0 and 1".to_string(),
            ));
        }
        if self.revenue_smear <= 100 {
            return Err(ContractError::Invalid(
                "revenue_smear must be greater than 100".to_string(),
            ));
        }
        if matches!(self.quarantine, Duration::Height(0) | Duration::Time(0)) {
            return Err(ContractError::Invalid(
                "quarantine must be greater than 0".to_string(),
            ));
        }
        if self.mint_cap.is_zero() {
            return Err(ContractError::Invalid(
                "mint_cap must be greater than 0".to_string(),
            ));
        }
        if self.max_effective_bond.is_zero() {
            return Err(ContractError::Invalid(
                "max_effective_bond must be greater than 0".to_string(),
            ));
        }
        Ok(())
    }

    pub fn save(&self, storage: &mut dyn Storage) -> StdResult<()> {
        CONFIG.save(storage, self)
    }

    pub fn update(&mut self, deps: Deps, update: ConfigUpdate) -> StdResult<()> {
        if let Some(fin_contract) = update.fin_contract {
            self.fin_contract = deps.api.addr_validate(&fin_contract)?;
        };
        if let Some(stake_contract) = update.stake_contract {
            self.stake_contract = deps.api.addr_validate(&stake_contract)?;
        };
        if let Some(target_utilization) = update.target_utilization {
            self.target_utilization = target_utilization;
        };
        if let Some(min_node_fee) = update.min_node_fee {
            self.min_node_fee = min_node_fee;
        };
        if let Some(range) = update.range {
            self.range = range;
        };
        if let Some(permissionless) = update.permissionless {
            self.permissionless = permissionless;
        };
        if let Some(revenue_smear) = update.revenue_smear {
            self.revenue_smear = revenue_smear;
        };
        if let Some(quarantine) = update.quarantine {
            self.quarantine = quarantine;
        };
        if let Some(max_bond) = update.max_bond {
            self.max_bond = max_bond;
        };
        if let Some(max_effective_bond) = update.max_effective_bond {
            self.max_effective_bond = max_effective_bond;
        };
        if let Some(mint_cap) = update.mint_cap {
            self.mint_cap = mint_cap;
        };
        Ok(())
    }
}

impl From<Config> for ConfigResponse {
    fn from(value: Config) -> Self {
        Self {
            target_utilization: value.target_utilization,
            min_node_fee: value.min_node_fee,
            range: value.range,
            permissionless: value.permissionless,
            fin_contract: value.fin_contract.to_string(),
            stake_contract: value.stake_contract.to_string(),
            revenue_smear: value.revenue_smear,
            quarantine: value.quarantine,
            token_id: value.token_id,
            max_bond: value.max_bond,
            max_effective_bond: value.max_effective_bond,
            mint_cap: value.mint_cap,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn range() -> RangeConfig {
        RangeConfig {
            high: Decimal::percent(99),
            low: Decimal::percent(80),
            skew: SignedDecimal::zero(),
            step: Decimal::percent(1),
        }
    }

    #[test]
    fn validation_ok() {
        Config {
            target_utilization: Decimal::percent(90),
            min_node_fee: Decimal::percent(5),
            range: range(),
            permissionless: false,
            stake_contract: Addr::unchecked("stake"),
            revenue_smear: 101,
            quarantine: Duration::Height(10),
            token_id: "brune".to_string(),
            fin_contract: Addr::unchecked("fin"),
            max_bond: Uint128::new(100),
            max_effective_bond: Uint128::new(1_000_000),
            mint_cap: Uint128::new(1_000),
        }
        .validate()
        .unwrap();
    }

    #[test]
    fn validation_target_utilization_bounds() {
        let base = Config {
            target_utilization: Decimal::percent(90),
            min_node_fee: Decimal::percent(5),
            range: range(),
            permissionless: false,
            stake_contract: Addr::unchecked("stake"),
            revenue_smear: 101,
            quarantine: Duration::Height(10),
            token_id: "brune".to_string(),
            fin_contract: Addr::unchecked("fin"),
            max_bond: Uint128::new(100),
            max_effective_bond: Uint128::new(1_000_000),
            mint_cap: Uint128::new(1_000),
        };
        assert!(Config {
            target_utilization: Decimal::zero(),
            ..base.clone()
        }
        .validate()
        .is_err());
        assert!(Config {
            target_utilization: Decimal::one(),
            ..base.clone()
        }
        .validate()
        .is_err());
    }

    #[test]
    fn validation_min_node_fee_bounds() {
        let base = Config {
            target_utilization: Decimal::percent(90),
            min_node_fee: Decimal::percent(5),
            range: range(),
            permissionless: false,
            stake_contract: Addr::unchecked("stake"),
            revenue_smear: 101,
            quarantine: Duration::Height(10),
            token_id: "brune".to_string(),
            fin_contract: Addr::unchecked("fin"),
            max_bond: Uint128::new(100),
            max_effective_bond: Uint128::new(1_000_000),
            mint_cap: Uint128::new(1_000),
        };
        assert!(Config {
            min_node_fee: Decimal::zero(),
            ..base.clone()
        }
        .validate()
        .is_err());
        assert!(Config {
            min_node_fee: Decimal::one(),
            ..base
        }
        .validate()
        .is_err());
    }

    #[test]
    fn validation_revenue_smear_bounds() {
        let base = Config {
            target_utilization: Decimal::percent(90),
            min_node_fee: Decimal::percent(5),
            range: range(),
            permissionless: false,
            stake_contract: Addr::unchecked("stake"),
            revenue_smear: 101,
            quarantine: Duration::Height(10),
            token_id: "brune".to_string(),
            fin_contract: Addr::unchecked("fin"),
            max_bond: Uint128::new(100),
            max_effective_bond: Uint128::new(1_000_000),
            mint_cap: Uint128::new(1_000),
        };
        assert!(Config {
            revenue_smear: 100,
            ..base.clone()
        }
        .validate()
        .is_err());
        assert!(Config {
            revenue_smear: 0,
            ..base
        }
        .validate()
        .is_err());
    }

    #[test]
    fn validation_quarantine_bounds() {
        let base = Config {
            target_utilization: Decimal::percent(90),
            min_node_fee: Decimal::percent(5),
            range: range(),
            permissionless: false,
            stake_contract: Addr::unchecked("stake"),
            revenue_smear: 101,
            quarantine: Duration::Height(10),
            token_id: "brune".to_string(),
            fin_contract: Addr::unchecked("fin"),
            max_bond: Uint128::new(100),
            max_effective_bond: Uint128::new(1_000_000),
            mint_cap: Uint128::new(1_000),
        };
        assert!(Config {
            quarantine: Duration::Height(0),
            ..base.clone()
        }
        .validate()
        .is_err());
        assert!(Config {
            quarantine: Duration::Time(0),
            ..base
        }
        .validate()
        .is_err());
    }

    #[test]
    fn validation_range_bounds() {
        let base = Config {
            target_utilization: Decimal::percent(90),
            min_node_fee: Decimal::percent(5),
            range: range(),
            permissionless: false,
            stake_contract: Addr::unchecked("stake"),
            revenue_smear: 101,
            quarantine: Duration::Height(10),
            token_id: "brune".to_string(),
            fin_contract: Addr::unchecked("fin"),
            max_bond: Uint128::new(100),
            max_effective_bond: Uint128::new(1_000_000),
            mint_cap: Uint128::new(1_000),
        };
        assert!(Config {
            range: RangeConfig {
                high: Decimal::one(),
                ..range()
            },
            ..base.clone()
        }
        .validate()
        .is_err());
        assert!(Config {
            range: RangeConfig {
                low: Decimal::percent(99),
                high: Decimal::percent(90),
                ..range()
            },
            ..base.clone()
        }
        .validate()
        .is_err());
        assert!(Config {
            range: RangeConfig {
                step: Decimal::zero(),
                ..range()
            },
            ..base
        }
        .validate()
        .is_err());
    }

    #[test]
    fn validation_mint_cap_bounds() {
        let base = Config {
            target_utilization: Decimal::percent(90),
            min_node_fee: Decimal::percent(5),
            range: range(),
            permissionless: false,
            stake_contract: Addr::unchecked("stake"),
            revenue_smear: 101,
            quarantine: Duration::Height(10),
            token_id: "brune".to_string(),
            fin_contract: Addr::unchecked("fin"),
            max_bond: Uint128::new(100),
            max_effective_bond: Uint128::new(1_000_000),
            mint_cap: Uint128::new(1_000),
        };
        assert!(Config {
            mint_cap: Uint128::zero(),
            ..base
        }
        .validate()
        .is_err());
    }

    #[test]
    fn validation_max_effective_bond_bounds() {
        let base = Config {
            target_utilization: Decimal::percent(90),
            min_node_fee: Decimal::percent(5),
            range: range(),
            permissionless: false,
            stake_contract: Addr::unchecked("stake"),
            revenue_smear: 101,
            quarantine: Duration::Height(10),
            token_id: "brune".to_string(),
            fin_contract: Addr::unchecked("fin"),
            max_bond: Uint128::new(100),
            max_effective_bond: Uint128::new(1_000_000),
            mint_cap: Uint128::new(1_000),
        };
        assert!(Config {
            max_effective_bond: Uint128::zero(),
            ..base
        }
        .validate()
        .is_err());
    }
}
