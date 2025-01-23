use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Decimal, HexBinary, Uint128};

#[cw_serde]
pub struct InstantiateMsg {
    pub checksum: HexBinary,
    pub code_id: u64,
}

#[cw_serde]
pub enum ExecuteMsg {}

#[cw_serde]
pub enum SudoMsg {
    /// Creates and registers a new borrow market
    Create {
        /// Lending vault to borrow funds from
        vault_addr: String,
        /// The liquidation queue that will be used to liquidate collateral_denom for debt_denom
        orca_addr: String,
        /// Collateral denom string
        collateral_denom: String,
        /// Debt token denom string
        debt_denom: String,
    },
    UpdateConfig {
        code_id: Option<u32>,
    },
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    /// Query global config settings
    #[returns(ConfigResponse)]
    Config {},

    /// Query global status of lending market
    #[returns(StatusResponse)]
    Status {},

    /// Query the status of a registered borrow market
    #[returns(ContractConfigResponse)]
    ContractConfig(String),

    /// Query the status of a registered borrow market
    #[returns(ContractStatusResponse)]
    ContractStatus(String),
}

#[cw_serde]
pub struct ConfigResponse {
    /// The Code ID used to deploy and register new ghost-borrow markets
    pub code_id: u32,
}

#[cw_serde]
pub struct StatusResponse {
    collaterals: Vec<CollateralStatus>,
    debts: Vec<DebtStatus>,
}

#[cw_serde]
pub struct ContractConfigResponse {
    pub contract: String,
    pub collateral_denom: String,
    pub debt_denom: String,
    pub vault_addr: String,
    pub orca_addr: String,
}

#[cw_serde]
pub struct ContractStatusResponse {
    pub contract: String,

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

#[cw_serde]
pub struct CollateralStatus {
    /// The total amount of collateral deposited across all contracts
    deposited: Uint128,
    /// The collateral token
    denom: String,
    /// A breakdown of the collateral on the contracts where the borrow token is CollateralStatus::denom
    contracts: Vec<ContractStatusResponse>,
}

#[cw_serde]
pub struct DebtStatus {
    /// The total amount of collateral deposited across all contracts
    borrowed: Uint128,
    /// The borrowed token
    denom: String,
    /// A breakdown of the debt on the contracts where the borrow token is DebtStatus::denom
    contracts: Vec<ContractStatusResponse>,
}
