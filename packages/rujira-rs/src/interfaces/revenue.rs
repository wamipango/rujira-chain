use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Binary, Timestamp, Uint128};

#[cw_serde]
pub struct InstantiateMsg {
    pub owner: String,
    pub executor: String,
    pub target_denoms: Vec<(String, Uint128)>,
    pub target_addresses: Vec<(String, u8)>,
}

#[cw_serde]
pub enum ExecuteMsg {
    Run {},
    Distribute {},
}

#[cw_serde]
pub enum SudoMsg {
    Run {},
    SetOwner(String),
    SetExecutor(String),
    SetAction {
        denom: String,
        /// The target contract for swapping
        contract: String,
        /// The minimum amount of the token that must be included in any one execution of the Action
        min: Uint128,
        /// The maximum amount of the token that can be included in any one execution of the Action
        max: Uint128,
        /// The msg executed on the contract to swap to the target token
        msg: Binary,
    },
    UnsetAction(String),
    SetSchedule(u64),
    UnsetSchedule(),
    SetTargetDenom((String, Uint128)),
    SetTargetAddress((String, u8)),
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(ConfigResponse)]
    Config {},
    #[returns(ActionsResponse)]
    Actions {},
    #[returns(StatusResponse)]
    Status {},
}

#[cw_serde]
pub struct ConfigResponse {
    pub owner: String,
    pub executor: String,
    pub target_denoms: Vec<(String, Uint128)>,
    pub target_addresses: Vec<(String, u8)>,
    pub schedule: Option<u64>,
    pub last_executed: Timestamp,
}

#[cw_serde]
pub struct ActionsResponse {
    pub actions: Vec<ActionResponse>,
}
#[cw_serde]
pub struct ActionResponse {
    pub denom: String,
    pub contract: String,
    pub min: Uint128,
    pub max: Uint128,
    pub msg: Binary,
}

#[cw_serde]
pub struct StatusResponse {
    pub last: Option<String>,
}
