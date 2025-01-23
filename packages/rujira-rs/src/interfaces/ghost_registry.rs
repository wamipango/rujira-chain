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
    /// Creates and registers a new debt market
    Create {
        /// Lending vault to debt funds from
        vault_addr: String,
        /// Initial hard-cap for debting for this contract from the vault
        vault_limit: Uint128,
        /// The liquidation queue that will be used to liquidate collateral_denom for debt_denom
        orca_addr: String,
        /// Collateral denom string
        collateral_denom: String,
        /// Debt token denom string
        debt_denom: String,
    },
    UpdateConfig {
        code: Option<(u64, HexBinary)>,
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

    /// Query the status of a registered debt market
    #[returns(ContractConfigResponse)]
    ContractConfig(String),

    /// Query the status of a registered debt market
    #[returns(ContractStatusResponse)]
    ContractStatus(String),
}

#[cw_serde]
pub struct ConfigResponse {
    /// The Code ID used to deploy and register new ghost-debt markets
    pub code_id: u64,
    pub checksum: HexBinary,
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
    pub max_ltv: Decimal,
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

    /// Current debt rate
    pub debt_rate: Decimal,

    /// Current debt limit
    pub debt_limit: Uint128,

    /// Maximum LTV value before the contract is allowed to begin liqudiating collateral
    pub max_ltv: Decimal,

    /// Current collateral price denmincated in debt token
    /// Adjusted for decimal delta between tokens
    pub price: Decimal,
}

#[cw_serde]
pub struct CollateralStatus {
    /// The total amount of collateral deposited across all contracts
    amount: Uint128,
    /// The collateral token
    denom: String,
    /// A breakdown of the collateral on the contracts where the debt token is CollateralStatus::denom
    contracts: Vec<ContractStatusResponse>,
}

#[cw_serde]
pub struct DebtStatus {
    /// The total amount of collateral deposited across all contracts
    amount: Uint128,
    /// The debted token
    denom: String,
    /// A breakdown of the debt on the contracts where the debt token is DebtStatus::denom
    contracts: Vec<ContractStatusResponse>,
}
