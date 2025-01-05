use cosmwasm_schema::cw_serde;
use cosmwasm_std::Decimal;

use crate::Tick;

#[cw_serde]
pub enum SudoMsg {
    Pause {},
    Resume {},
    UpdateConfig {
        tick: Option<Tick>,
        fee_taker: Option<Decimal>,
        fee_maker: Option<Decimal>,
        fee_address: Option<String>,
    },
}
