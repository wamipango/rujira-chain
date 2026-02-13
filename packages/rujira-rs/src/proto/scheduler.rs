use cosmwasm_std::{AnyMsg, CosmosMsg};
use prost::Message;

static TYPE_URL: &str = "/thorchain.scheduler.v1.MsgScheduleExecuteContract";

#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MsgScheduleExecuteContract {
    #[prost(string, tag = "1")]
    pub sender: String,
    #[prost(uint64, tag = "2")]
    pub after: u64,
    #[prost(bytes = "vec", tag = "3")]
    pub msg: Vec<u8>,
}

impl From<MsgScheduleExecuteContract> for CosmosMsg {
    fn from(value: MsgScheduleExecuteContract) -> Self {
        CosmosMsg::Any(AnyMsg {
            type_url: TYPE_URL.to_string(),
            value: value.encode_to_vec().into(),
        })
    }
}
