use anybuf::Bufany;
use anyhow::Result as AnyResult;
use cosmwasm_std::{Api, BlockInfo, CustomMsg, CustomQuery, Storage};
use cw_multi_test::{AppResponse, CosmosRouter, SudoMsg, WasmSudo};
use serde::de::DeserializeOwned;

pub fn handle_execute_any<ExecC, QueryC>(
    api: &dyn Api,
    storage: &mut dyn Storage,
    router: &dyn CosmosRouter<ExecC = ExecC, QueryC = QueryC>,
    block: &BlockInfo,
    msg: &cosmwasm_std::AnyMsg,
) -> AnyResult<Option<AppResponse>>
where
    ExecC: CustomMsg + DeserializeOwned + 'static,
    QueryC: CustomQuery + DeserializeOwned + 'static,
{
    if msg.type_url != "/cosmwasm.wasm.v1.MsgSudoContract" {
        return Ok(None);
    }
    let buf = Bufany::deserialize(msg.value.as_slice())?;
    let contract = buf.string(2).unwrap();
    let msg = buf.bytes(3).unwrap();
    let response = router.sudo(
        api,
        storage,
        block,
        SudoMsg::Wasm(WasmSudo {
            contract_addr: api.addr_validate(&contract)?,
            message: msg.into(),
        }),
    )?;
    Ok(Some(response))
}
