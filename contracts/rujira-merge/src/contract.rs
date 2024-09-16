#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    ensure_eq, to_json_binary, Binary, Deps, DepsMut, Env, Event, MessageInfo, Response,
};
use cw2::set_contract_version;
use cw_utils::must_pay;
use rujira_rs::merge::{ExecuteMsg, InstantiateMsg, QueryMsg};

// use cw2::set_contract_version;

use crate::config::Config;
use crate::error::ContractError;
use crate::state::{account, execute_deposit, execute_withdraw, init, status};

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:ruji";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    let config = Config::from(msg);
    config.validate(&env.block.time)?;
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
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> Result<Binary, ContractError> {
    match msg {
        QueryMsg::Status {} => Ok(to_json_binary(&status(deps.storage)?)?),
        QueryMsg::Account { addr } => Ok(to_json_binary(&account(deps.storage, &addr)?)?),
    }
}

#[cfg(test)]
mod tests {

    use cosmwasm_std::{coins, BlockInfo, Timestamp, Uint128};
    use cw_multi_test::{BasicApp, ContractWrapper, Executor};

    use super::*;

    #[test]
    fn instantiation() {
        let mut app = BasicApp::default();
        let owner = app.api().addr_make("owner");
        app.init_modules(|router, _, storage| {
            router
                .bank
                .init_balance(storage, &owner, coins(100_000_000, "uruji"))
        })
        .unwrap();

        app.set_block(BlockInfo {
            height: 0,
            time: Timestamp::default(),
            chain_id: app.block_info().chain_id,
        });

        let code = Box::new(ContractWrapper::new(execute, instantiate, query));
        let code_id = app.store_code(code);
        app.instantiate_contract(
            code_id,
            owner.clone(),
            &InstantiateMsg {
                merge_denom: "ukuji".to_string(),
                merge_supply: Uint128::from(250_000_000u128),
                ruji_denom: "uruji".to_string(),
                ruji_allocation: Uint128::from(100_000_000u128),
                decay_starts_at: Timestamp::from_seconds(1_000),
                decay_ends_at: Timestamp::from_seconds(1_001_000),
            },
            &coins(100_000_000, "uruji"),
            "merge",
            None,
        )
        .unwrap();

        // Single failure to assert Config verification is executing
        app.instantiate_contract(
            code_id,
            owner,
            &InstantiateMsg {
                merge_denom: "ukuji".to_string(),
                merge_supply: Uint128::from(250_000_000u128),
                ruji_denom: "uruji".to_string(),
                ruji_allocation: Uint128::from(10_000_000u128),
                decay_starts_at: Timestamp::from_seconds(1_000),
                decay_ends_at: Timestamp::from_seconds(1_001_000),
            },
            &coins(100_000_000, "uruji"),
            "merge",
            None,
        )
        .unwrap_err();
    }
}
