#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    coin, to_json_binary, BankMsg, Binary, CosmosMsg, Deps, DepsMut, Env, MessageInfo, Response,
};
use cw2::set_contract_version;
use cw_utils::{must_pay, one_coin};
use rujira_rs::bow::{ExecuteMsg, InstantiateMsg, QueryMsg, Strategy, SudoMsg};
use rujira_rs::TokenFactory;

use crate::config::Config;
use crate::error::ContractError;
use crate::events::{event_deposit, event_withdraw};

// version info for migration info
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
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    let config = Config::load(deps.storage)?;
    match msg {
        ExecuteMsg::Swap { min_return, to, .. } => {
            let offer = one_coin(&info)?;
            config
                .strategy
                .validate(deps.as_ref(), env, offer, min_return.clone())?;
            let to_address = to
                .map(|x| deps.api.addr_validate(x.as_str()))
                .transpose()?
                .unwrap_or(info.sender);
            Ok(
                Response::default().add_message(CosmosMsg::Bank(BankMsg::Send {
                    to_address: to_address.to_string(),
                    amount: vec![min_return],
                })),
            )
        }
        ExecuteMsg::Deposit {} => {
            let t = token(&env);
            let supply = t.supply(deps.querier)?;
            let (deposited, minted) =
                config
                    .strategy
                    .calculate_share(deps.as_ref(), env, info.clone(), supply)?;
            Ok(Response::default()
                .add_message(t.mint_msg(minted, info.sender.clone()))
                .add_event(event_deposit(
                    info.sender,
                    deposited,
                    coin(minted.u128(), t.denom()),
                )))
        }
        ExecuteMsg::Withdraw {} => {
            let t = token(&env);
            let supply = t.supply(deps.querier)?;
            let balance = must_pay(&info, t.denom().as_str())?;
            let withdrawn =
                config
                    .strategy
                    .calculate_ownership(deps.as_ref(), env, supply, balance)?;
            Ok(Response::default()
                .add_message(CosmosMsg::Bank(BankMsg::Send {
                    to_address: info.sender.to_string(),
                    amount: withdrawn.clone().into_vec(),
                }))
                .add_event(event_withdraw(
                    info.sender,
                    coin(balance.u128(), t.denom()),
                    withdrawn,
                )))
        }
    }
}

fn token(env: &Env) -> TokenFactory {
    TokenFactory::new(env, "ulp")
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn sudo(deps: DepsMut, _env: Env, msg: SudoMsg) -> Result<Response, ContractError> {
    let mut config = Config::load(deps.storage)?;

    match msg {
        SudoMsg::SetStrategy(strategy) => {
            config.strategy = strategy;
            config.validate()?;
            config.save(deps.storage)?;
            Ok(Response::default())
        }
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> Result<Binary, ContractError> {
    let config = Config::load(deps.storage)?;
    match msg {
        QueryMsg::Quote(req) => Ok(to_json_binary(&config.strategy.quote(deps, env, req)?)?),
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use cosmwasm_std::{Decimal, Uint256};
    use cw_multi_test::{BasicApp, ContractWrapper, Executor};
    use rujira_rs::bow::{Strategies, Xyk};

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
                strategy: Strategies::Xyk(Xyk::new(
                    "ruji".to_string(),
                    "usdc".to_string(),
                    Decimal::from_ratio(1u128, 1000u128),
                    Uint256::zero(),
                )),
            },
            &[],
            "template",
            None,
        )
        .unwrap();
    }
}
