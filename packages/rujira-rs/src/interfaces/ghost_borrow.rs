use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Binary, Decimal, Uint128};

#[cw_serde]
pub struct InstantiateMsg {
    pub registry: String,
}

#[cw_serde]
pub enum ExecuteMsg {
    /// Create a new, update or close an existing position
    /// Set the target amounts. If set to zero, position will be closed
    /// Surplus is returned to the sender
    Manage {
        borrowed: Uint128,
        deposited: Uint128,
        // TODO: How do we support a recipient with a one-tx solution to withdraw secured assets to another L1
    },

    /// Transfer debt to another borrow position, optionally swapping collateral and moving it at the same time
    Transfer {
        /// Amount of the Vault's debt shares to transfer.
        /// N.b: query the vault status to get the current obligation (value) of the debt shares
        debt_amount: Uint128,
        /// ghost-borrow contract address destination
        /// this address must be registered on ghost-registry for the transfer to be valid
        destination: String,
        /// An optional execution step that will deduct collateral from the current position,
        /// exchange it for the collateral of the destination, and transfer them all together
        collateral: Option<CollateralTransfer>,
    },
}

#[cw_serde]
pub struct CollateralTransfer {
    contract: String,
    msg: Binary,
    amount: Uint128,
}

#[cw_serde]
pub enum SudoMsg {}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(ConfigResponse)]
    Config {},
    #[returns(StatusResponse)]
    Status {},
}

#[cw_serde]
pub struct ConfigResponse {
    pub collateral_denom: String,
    pub debt_denom: String,
    pub vault_addr: String,
    pub orca_addr: String,
}

#[cw_serde]
pub struct StatusResponse {
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
