#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_json_binary, Binary, Deps, DepsMut, Env, MessageInfo, Response, Uint128, WasmMsg,
};
use cw2::set_contract_version;
use rujira_rs::stream::factory::{ExecuteMsg, InstantiateMsg, QueryMsg};
use rujira_rs::stream::InstantiateMsg as StreamInstantiateMsg;
use std::ops::Add;

use crate::config::Config;
use crate::error::ContractError;
use crate::state::COUNTER;

const CONTRACT_NAME: &str = env!("CARGO_PKG_NAME");
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    let config = Config::from(msg);
    config.validate()?;
    config.save(deps.storage)?;
    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    let config = Config::load(deps.storage)?;
    let idx = COUNTER
        .load(deps.storage)
        .unwrap_or_default()
        .add(Uint128::one());
    COUNTER.save(deps.storage, &idx)?;
    let label = format!("rujira-stream:{}", idx);
    let init_msg = WasmMsg::Instantiate {
        admin: None,
        code_id: config.code_id,
        label,
        msg: to_json_binary(&StreamInstantiateMsg::from(msg))?,
        funds: info.funds,
    };

    Ok(Response::default().add_message(init_msg))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(_deps: Deps, _env: Env, _msg: QueryMsg) -> Result<Binary, ContractError> {
    Ok(to_json_binary(&())?)
}

#[cfg(test)]
mod tests {

    use super::*;
    use cw_multi_test::{BasicApp, ContractWrapper, Executor};

    #[test]
    fn instantiation() {
        let mut app = BasicApp::default();
        let owner = app.api().addr_make("owner");

        let code = Box::new(ContractWrapper::new(execute, instantiate, query));
        let code_id = app.store_code(code);
        app.instantiate_contract(
            code_id,
            owner,
            &InstantiateMsg { code_id: 1u64 },
            &[],
            "template",
            None,
        )
        .unwrap();
    }
}
