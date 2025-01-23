#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{to_json_binary, Binary, Deps, DepsMut, Env, MessageInfo, Response};
use cw2::set_contract_version;
use rujira_rs::ghost_registry::{
    ContractConfigResponse, ExecuteMsg, InstantiateMsg, QueryMsg, SudoMsg,
};

use crate::config::Config;
use crate::error::ContractError;
use crate::state::ContractConfig;

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
    _deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn sudo(_deps: DepsMut, _env: Env, msg: SudoMsg) -> Result<Response, ContractError> {
    match msg {
        SudoMsg::Create { .. } => {
            // 1. Instantiate2, get contract address
            // 2. Whitelist contract with lending vault, TODO default limit
            // 3. Store contract config locally. borrow contracts should read their own config from the registry each time
            todo!()
        }
        SudoMsg::UpdateConfig { code_id } => todo!(),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> Result<Binary, ContractError> {
    match msg {
        QueryMsg::Config {} => todo!(),
        QueryMsg::Status() => todo!(),
        QueryMsg::ContractConfig(addr) => Ok(to_json_binary(&ContractConfigResponse::from(
            ContractConfig::load(deps.storage, deps.api.addr_validate(&addr)?)?,
        ))?),
        QueryMsg::ContractStatus(_) => todo!(),
    }
}
