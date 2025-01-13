use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::Decimal;

/// Interfaces that the executor of a Pilot sale must conform to
#[cw_serde]
#[derive(QueryResponses)]
pub enum ExecutorQueryMsg {
    #[returns(Decimal)]
    Price {},
}
