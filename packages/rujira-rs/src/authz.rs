use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::Binary;

#[cw_serde]
pub struct InstantiateMsg {}

#[cw_serde]
pub enum ExecuteMsg {}

#[cw_serde]
pub enum SudoMsg {}

/// The address of the actor
type Actor = String;

/// The address of the target contract
type Target = String;

/// The requested payload
type Msg = Binary;

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(bool)]
    Can((Actor, Target, Msg)),
}
