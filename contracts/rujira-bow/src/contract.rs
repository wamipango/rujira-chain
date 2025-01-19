#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    coin, to_json_binary, BankMsg, Binary, CosmosMsg, Deps, DepsMut, Env, MessageInfo, Response,
};
use cw2::set_contract_version;
use cw_utils::one_coin;
use rujira_rs::bow::{ExecuteMsg, InstantiateMsg, QueryMsg, Strategy, SudoMsg};

use crate::config::Config;
use crate::error::ContractError;

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
        ExecuteMsg::Swap {
            min_return,
            to,
            callback,
        } => {
            let offer = one_coin(&info)?;
            let ask = coin(min_return.u128(), "TODO");
            config
                .strategy
                .validate(deps.as_ref(), env, offer, ask.clone())?;
            let to_address = to
                .map(|x| deps.api.addr_validate(x.as_str()))
                .transpose()?
                .unwrap_or(info.sender);
            Ok(
                Response::default().add_message(CosmosMsg::Bank(BankMsg::Send {
                    to_address: to_address.to_string(),
                    amount: vec![ask],
                })),
            )
        }
        ExecuteMsg::Deposit {} => todo!(),
        ExecuteMsg::Withdraw {} => todo!(),
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
        QueryMsg::Quote(req) => Ok(to_json_binary(&config.strategy.quote(deps, env, req)?)?),
    }
}

#[cfg(test)]
mod tests {

    use super::*;
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
                strategy: Strategies::Xyk(Xyk::new("ruji", "usdc")),
            },
            &[],
            "template",
            None,
        )
        .unwrap();
    }
}
