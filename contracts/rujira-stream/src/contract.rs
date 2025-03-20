#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    ensure_eq, to_json_binary, BankMsg, Binary, Deps, DepsMut, Env, MessageInfo, Response,
};
use cw2::set_contract_version;
use cw_utils::NativeBalance;
use rujira_rs::stream::{ConfigResponse, ExecuteMsg, InstantiateMsg, QueryMsg, SudoMsg};

use crate::config::Config;
use crate::error::ContractError;
use crate::state::CLAIMS;

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
    let config = Config::from_instantiate(deps.api, msg)?;
    config.validate()?;
    config.save(deps.storage)?;
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
        ExecuteMsg::Claim {} => {
            let claimant = info.sender;
            let last_claim = CLAIMS
                .may_load(deps.storage, claimant.clone())?
                .unwrap_or(env.block.time);
            let total_allocation = config.schedule.allocation(&last_claim, &env.block.time);
            let mut allocation = NativeBalance(
                config
                    .recipients
                    .allocation_for(claimant.clone(), total_allocation),
            );
            allocation.normalize();
            if allocation.is_empty() {
                return Ok(Response::default());
            }
            // No need to save this claimant if nothing was earned in this period
            CLAIMS.save(deps.storage, claimant.clone(), &env.block.time)?;
            Ok(Response::default().add_message(BankMsg::Send {
                to_address: claimant.to_string(),
                amount: allocation.into_vec(),
            }))
        }
        ExecuteMsg::Update(msg) => {
            let owner = config.owner.ok_or(ContractError::Unauthorized {})?;
            ensure_eq!(info.sender, owner, ContractError::Unauthorized {});
            let config = Config::from_instantiate(deps.api, msg)?;
            config.validate()?;
            config.save(deps.storage)?;
            Ok(Response::default())
        }
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn sudo(_deps: DepsMut, _env: Env, _msg: SudoMsg) -> Result<Response, ContractError> {
    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> Result<Binary, ContractError> {
    let config = Config::load(deps.storage)?;
    match msg {
        QueryMsg::Pending { address } => {
            let claimant = deps.api.addr_validate(address.as_str())?;
            let last_claim = CLAIMS
                .may_load(deps.storage, claimant.clone())?
                .unwrap_or(env.block.time);
            let total_allocation = config.schedule.allocation(&last_claim, &env.block.time);
            let mut allocation = NativeBalance(
                config
                    .recipients
                    .allocation_for(claimant.clone(), total_allocation),
            );
            allocation.normalize();
            Ok(to_json_binary(&allocation)?)
        }
        QueryMsg::Config {} => Ok(to_json_binary(&ConfigResponse::from(config))?),
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use cosmwasm_std::{coins, Timestamp};
    use cw_multi_test::{BasicApp, ContractWrapper, Executor};
    use rujira_rs::stream::Schedule;

    #[test]
    fn instantiation() {
        let mut app = BasicApp::default();
        let owner = app.api().addr_make("owner");

        let code = Box::new(ContractWrapper::new(execute, instantiate, query));
        let code_id = app.store_code(code);
        app.instantiate_contract(
            code_id,
            owner,
            &InstantiateMsg {
                recipients: vec![],
                schedule: Schedule::Fixed {
                    starts: Timestamp::default(),
                    ends: Timestamp::default().plus_days(1),
                    amount: coins(100, "ruji"),
                },
                owner: None,
            },
            &[],
            "template",
            None,
        )
        .unwrap();
    }
}
