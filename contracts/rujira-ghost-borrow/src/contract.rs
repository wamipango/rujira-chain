#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_json_binary, BankMsg, Binary, Decimal, Deps, DepsMut, Env, MessageInfo, Response,
};
use cw2::set_contract_version;
use cw_utils::NativeBalance;
use rujira_rs::ghost_borrow::{
    ConfigResponse, ExecuteMsg, InstantiateMsg, PositionResponse, QueryMsg, StatusResponse, SudoMsg,
};

use crate::config::{Config, Status, REGISTRY};
use crate::error::ContractError;
use crate::position::Position;

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
    REGISTRY.save(deps.storage, &deps.api.addr_validate(&msg.registry)?)?;
    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    let status = Status::load(deps.as_ref(), &env)?;
    match msg {
        ExecuteMsg::Manage {
            borrowed,
            deposited,
        } => {
            let funds = NativeBalance(info.funds);
            let mut position = Position::load(deps.storage, &info.sender);
            let mut returned = position.adjust(status, deposited, borrowed, funds)?;
            returned.normalize();
            position.save(deps.storage)?;

            if returned.is_empty() {
                Ok(Response::default())
            } else {
                Ok(Response::default().add_message(BankMsg::Send {
                    to_address: position.owner.to_string(),
                    amount: returned.into_vec(),
                }))
            }
        }
        ExecuteMsg::Transfer { .. } => todo!(),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn sudo(_deps: DepsMut, _env: Env, _msg: SudoMsg) -> Result<Response, ContractError> {
    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> Result<Binary, ContractError> {
    match msg {
        QueryMsg::Config {} => Ok(to_json_binary(&ConfigResponse::from(Config::load(
            deps, &env,
        )?))?),
        QueryMsg::Status {} => Ok(to_json_binary(&StatusResponse::from(Status::load(
            deps, &env,
        )?))?),
        QueryMsg::Position(addr) => {
            let position = Position::load(deps.storage, &deps.api.addr_validate(&addr)?);
            let res = PositionResponse {
                owner: position.owner.to_string(),
                collateral: position.collateral,
                debt: position.debt,
                ltv: Decimal::zero(),
            };
            Ok(to_json_binary(&res)?)
        }
    }
}
