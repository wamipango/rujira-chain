use cosmwasm_schema::cw_serde;
use cosmwasm_std::Decimal;

use crate::Layer1Asset;

#[allow(clippy::large_enum_variant)]
#[cw_serde]
pub enum SudoMsg {
    StartSchedule(u64),
    EndSchedule {},
    UpdateConfig(ConfigUpdate),
}

#[cw_serde]
pub struct ConfigUpdate {
    pub market_makers: Option<Vec<String>>,
    pub fee_taker: Option<Decimal>,
    pub fee_maker: Option<Decimal>,
    pub fee_amm: Option<Decimal>,
    pub range_delta: Option<Decimal>,
    pub fee_address: Option<String>,
    pub oracles: Option<[Layer1Asset; 2]>,
}
