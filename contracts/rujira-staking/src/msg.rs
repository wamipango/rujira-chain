use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Binary, Uint128};

#[cw_serde]
pub struct InstantiateMsg {
    pub bond_denom: String,
    pub revenue_denom: String,
    /// Configuration to convert `revenue_denom` into `bond_denom`
    ///
    /// `(contract_address, execute_msg, threshold_limit)`
    pub revenue_converter: (Addr, Binary, Uint128),
}

#[cw_serde]
pub enum ExecuteMsg {
    /// Actions relating to account-based yield
    Account(AccountMsg),
    /// Actions relating to liquid staking
    Liquid(LiquidMsg),
}

#[cw_serde]
pub enum AccountMsg {
    /// Bond [InstantiateMsg::bond_denom] tokens to an account
    Bond {},
    /// Claim earned revenue for all bonded tokens
    Claim {},
    /// Withdraw `amount` or all of [InstantiateMsg::bond_denom] from the sender's account
    /// This will also claim all revenue
    Withdraw { amount: Option<Uint128> },
}

#[cw_serde]
pub enum LiquidMsg {
    /// Bond [InstantiateMsg::bond_denom] tokens, retuning minted Share tokens
    Bond {},
    /// Burn the provided Share tokens, returning the share of the underlying pooled assets
    Withdraw {},
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(StatusResponse)]
    Status {},
}

#[cw_serde]
pub struct StatusResponse {
    /// The amount of [InstantiateMsg::bond_denom] bonded in Accounts
    account_bond: Uint128,

    /// The total amount of [InstantiateMsg::revenue_denom] available for Account staking to claim
    account_revenue: Uint128,

    /// The total size of the Share Pool of liquid bonded tokens
    liquid_bond: Uint128,

    /// The amount of [InstantiateMsg::revenue_denom] pending conversion to [InstantiateMsg::bond_denom]
    pending_liquid_revenue: Uint128,
}
