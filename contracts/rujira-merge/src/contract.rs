#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    ensure_eq, Binary, Deps, DepsMut, Env, Event, MessageInfo, Response, StdResult,
};
use cw2::set_contract_version;
use cw_utils::must_pay;
use rujira_rs::merge::{ExecuteMsg, InstantiateMsg, QueryMsg};

// use cw2::set_contract_version;

use crate::config::Config;
use crate::error::ContractError;
use crate::state::{execute_deposit, execute_withdraw, init};

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:ruji";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    let config = Config::from(msg);
    config.validate()?;
    config.save(deps.storage)?;
    ensure_eq!(
        must_pay(&info, config.ruji_denom.as_str())?,
        config.ruji_allocation,
        ContractError::InsufficientFunds {}
    );
    init(deps.storage)?;
    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    let config = Config::load(deps.storage)?;
    match msg {
        ExecuteMsg::Deposit {} => {
            let amount = must_pay(&info, &config.merge_denom)?;
            let shares =
                execute_deposit(deps.storage, &config, env.block.time, &info.sender, amount)?;
            Ok(Response::default().add_event(
                Event::new("merge/deposit")
                    .add_attribute("account", info.sender)
                    .add_attribute("amount", amount)
                    .add_attribute("shares", shares),
            ))
        }
        ExecuteMsg::Withdraw { share_amount } => {
            let amount = execute_withdraw(
                deps.storage,
                &config,
                env.block.time,
                &info.sender,
                share_amount,
            )?;
            Ok(Response::default().add_event(
                Event::new("merge/withdraw")
                    .add_attribute("account", info.sender)
                    .add_attribute("shares", share_amount)
                    .add_attribute("amount", amount),
            ))
        }
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(_deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Status {} => todo!(),
        QueryMsg::Account { addr } => todo!(),
    }
}

#[cfg(test)]
mod tests {}
