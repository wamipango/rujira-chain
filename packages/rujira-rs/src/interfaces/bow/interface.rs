use cosmwasm_schema::{cw_serde, QueryResponses};

use crate::fin::{MarketMakerExecute, MarketMakerQuery, MarketMakerResponse};

use super::strategy::Strategies;

#[cw_serde]
pub struct InstantiateMsg {
    pub denoms: [String; 2],
    pub strategy: Strategies,
}

#[cw_serde]
pub enum ExecuteMsg {
    Fin(MarketMakerExecute),
    Deposit {},
    Withdraw {},
}

#[cw_serde]
pub enum SudoMsg {}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(MarketMakerResponse)]
    Fin(MarketMakerQuery),
}
