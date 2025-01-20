#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    coin, coins, ensure, to_json_binary, BankMsg, Binary, Deps, DepsMut, Env, MessageInfo, Response,
};
use cw2::set_contract_version;
use cw_utils::must_pay;
use rujira_rs::ghost_vault::{ExecuteMsg, InstantiateMsg, QueryMsg, SudoMsg};
use rujira_rs::{SharePool, TokenFactory};

use crate::config::Config;
use crate::error::ContractError;
use crate::state::{update_interest, DEBT_POOL, DEPOSIT_POOL, MARKETS};

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

    DEPOSIT_POOL.save(deps.storage, &SharePool::default())?;
    DEBT_POOL.save(deps.storage, &SharePool::default())?;

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
    update_interest(deps.storage, &config)?;
    let rcpt = TokenFactory::new(&env, format!("ghost-vault-{}-rcpt", config.denom).as_str());
    let debt = TokenFactory::new(&env, format!("ghost-vault-{}-debt", config.denom).as_str());
    match msg {
        ExecuteMsg::Deposit {} => {
            let amount = must_pay(&info, config.denom.as_str())?;
            let mut pool = DEPOSIT_POOL.load(deps.storage)?;
            let mint = pool.join(amount)?;
            DEPOSIT_POOL.save(deps.storage, &pool)?;
            Ok(Response::default().add_message(rcpt.mint_msg(mint, info.sender)))
        }
        ExecuteMsg::Withdraw {} => {
            let amount = must_pay(&info, rcpt.denom().as_str())?;
            let mut pool = DEPOSIT_POOL.load(deps.storage)?;
            let withdrawn = pool.leave(amount)?;
            DEPOSIT_POOL.save(deps.storage, &pool)?;
            Ok(Response::default()
                .add_message(rcpt.burn_msg(amount))
                .add_message(BankMsg::Send {
                    to_address: info.sender.to_string(),
                    amount: coins(withdrawn.u128(), config.denom),
                }))
        }
        ExecuteMsg::Borrow { amount } => {
            MARKETS.load(deps.storage, info.sender.clone())?;

            let mut pool = DEBT_POOL.load(deps.storage)?;
            let mint = pool.join(amount)?;
            DEPOSIT_POOL.save(deps.storage, &pool)?;
            Ok(Response::default()
                // Mint to contract and send both together for clarity
                .add_message(debt.mint_msg(mint, env.contract.address.clone()))
                .add_message(BankMsg::Send {
                    to_address: info.sender.to_string(),
                    amount: vec![
                        coin(amount.u128(), config.denom),
                        coin(mint.u128(), rcpt.denom()),
                    ],
                }))
        }
        ExecuteMsg::Repay {} => {
            MARKETS.load(deps.storage, info.sender.clone())?;

            // TODO: Fetch the debt token + deposit token
            let amount = must_pay(&info, config.denom.as_str())?;
            let debt_amount = must_pay(&info, debt.denom().as_str())?;
            let mut pool = DEBT_POOL.load(deps.storage)?;
            let withdrawn = pool.leave(debt_amount)?;
            // Ensure that the amount being repaid is at least the value of the debt being burned
            ensure!(
                amount.ge(&withdrawn),
                ContractError::InsufficientRepay {
                    debt: debt_amount,
                    value: withdrawn,
                    repaid: amount
                }
            );
            DEPOSIT_POOL.save(deps.storage, &pool)?;
            Ok(Response::default().add_message(debt.burn_msg(amount)))
        }
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn sudo(_deps: DepsMut, _env: Env, _msg: SudoMsg) -> Result<Response, ContractError> {
    Ok(Response::default())
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
            &InstantiateMsg {
                denom: "btc".to_string(),
            },
            &[],
            "template",
            None,
        )
        .unwrap();
    }
}
