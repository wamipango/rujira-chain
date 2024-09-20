#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    coin, coins, to_json_binary, BankMsg, Binary, CosmosMsg, Deps, DepsMut, Env, Event,
    MessageInfo, Response, StdResult, Uint128,
};
use cw2::set_contract_version;
use cw_utils::{must_pay, NativeBalance};
use rujira_rs::staking::{AccountMsg, ExecuteMsg, InstantiateMsg, LiquidMsg, QueryMsg};
use rujira_rs::TokenFactory;

use crate::config::Config;
use crate::error::ContractError;
use crate::state::{
    account, distribute, execute_account_bond, execute_account_claim, execute_account_withdraw,
    execute_liquid_bond, execute_liquid_unbond, init, status,
};

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:rujira-staking";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");
const SHARE_DENOM_NONCE: &str = "urcpt";

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    let config = Config::from(msg);
    config.validate()?;
    config.save(deps.storage)?;
    init(deps.storage)?;
    let share_denom = TokenFactory::new(&env, SHARE_DENOM_NONCE);
    Ok(Response::default().add_message(share_denom.create_msg()))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    let config = Config::load(deps.storage)?;
    distribute(&env, deps.querier, deps.storage, &config)?;

    match msg {
        ExecuteMsg::Account(account_msg) => execute_account(deps, info, config, account_msg),
        ExecuteMsg::Liquid(liquid_msg) => execute_liquid(deps, &env, info, config, liquid_msg),
    }
}

fn execute_account(
    deps: DepsMut,
    info: MessageInfo,
    config: Config,
    msg: AccountMsg,
) -> Result<Response, ContractError> {
    match msg {
        AccountMsg::Bond {} => {
            let amount = must_pay(&info, config.bond_denom.as_str())?;

            let reward_amount = execute_account_bond(deps.storage, &info.sender, amount)?;
            let mut response = Response::default().add_event(
                Event::new("staking/account/bond")
                    .add_attribute("owner", info.sender.clone())
                    .add_attribute("amount", amount),
            );
            if reward_amount.gt(&Uint128::zero()) {
                response = response.add_message(CosmosMsg::Bank(BankMsg::Send {
                    to_address: info.sender.to_string(),
                    amount: coins(reward_amount.u128(), config.revenue_denom),
                }));
            }
            Ok(response)
        }
        AccountMsg::Claim {} => {
            let reward_amount = execute_account_claim(deps.storage, &info.sender)?;
            let mut response = Response::default().add_event(
                Event::new("staking/account/claim")
                    .add_attribute("owner", info.sender.clone())
                    .add_attribute("amount", reward_amount),
            );
            if reward_amount.gt(&Uint128::zero()) {
                response = response.add_message(CosmosMsg::Bank(BankMsg::Send {
                    to_address: info.sender.to_string(),
                    amount: coins(reward_amount.u128(), config.revenue_denom),
                }));
            }
            Ok(response)
        }
        AccountMsg::Withdraw { amount } => {
            let (rewards, amount) = execute_account_withdraw(deps.storage, &info.sender, amount)?;
            let mut send = NativeBalance(vec![
                coin(rewards.u128(), config.revenue_denom),
                coin(amount.u128(), config.bond_denom),
            ]);
            send.normalize();

            let mut response = Response::default().add_event(
                Event::new("staking/account/withdraw")
                    .add_attribute("owner", info.sender.clone())
                    .add_attribute("amount", amount)
                    .add_attribute("rewards", rewards),
            );
            if !send.is_empty() {
                response = response.add_message(CosmosMsg::Bank(BankMsg::Send {
                    to_address: info.sender.to_string(),
                    amount: send.into_vec(),
                }));
            }
            Ok(response)
        }
    }
}

fn execute_liquid(
    deps: DepsMut,
    env: &Env,
    info: MessageInfo,
    config: Config,
    msg: LiquidMsg,
) -> Result<Response, ContractError> {
    let share_denom = TokenFactory::new(env, SHARE_DENOM_NONCE);

    match msg {
        LiquidMsg::Bond {} => {
            let amount = must_pay(&info, config.bond_denom.as_str())?;
            let shares = execute_liquid_bond(deps.storage, amount)?;
            Ok(Response::default()
                .add_event(
                    Event::new("staking/liquid/bond")
                        .add_attribute("owner", info.sender.clone())
                        .add_attribute("amount", amount)
                        .add_attribute("shares", shares),
                )
                .add_message(share_denom.mint_msg(shares, info.sender)))
        }
        LiquidMsg::Withdraw {} => {
            let shares = must_pay(&info, share_denom.denom().as_str())?;
            let returned = execute_liquid_unbond(deps.storage, shares)?;
            Ok(Response::default()
                .add_event(
                    Event::new("staking/liquid/unbond")
                        .add_attribute("owner", info.sender.clone())
                        .add_attribute("shares", shares)
                        .add_attribute("returned", returned),
                )
                .add_message(share_denom.burn_msg(shares))
                .add_message(CosmosMsg::Bank(BankMsg::Send {
                    to_address: info.sender.to_string(),
                    amount: coins(returned.u128(), config.bond_denom),
                })))
        }
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    let config = Config::load(deps.storage)?;
    match msg {
        QueryMsg::Status {} => to_json_binary(&status(env, deps, &config)?),
        QueryMsg::Account { addr } => to_json_binary(&account(deps.storage, addr)?),
    }
}

#[cfg(test)]
mod tests {}
