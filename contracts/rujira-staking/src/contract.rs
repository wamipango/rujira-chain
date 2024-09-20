#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    coin, coins, to_json_binary, BankMsg, Binary, CosmosMsg, Decimal, Deps, DepsMut, Env,
    MessageInfo, QuerierWrapper, Response, StdError, StdResult, Storage, WasmMsg,
};
use cw2::set_contract_version;
use cw_utils::{must_pay, NativeBalance};
use rujira_rs::staking::{AccountMsg, ExecuteMsg, InstantiateMsg, LiquidMsg, QueryMsg};
use rujira_rs::TokenFactory;
use std::ops::Add;

use crate::config::Config;
use crate::error::ContractError;
use crate::state::{account, init, status, ACCOUNTS, POOL_ACCOUNTS, POOL_LIQUID};

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
    let share_denom = TokenFactory::new(&env, SHARE_DENOM_NONCE);
    distribute(env, deps.querier, deps.storage, &config)?;

    match msg {
        ExecuteMsg::Account(account_msg) => {
            let mut pool = POOL_ACCOUNTS.load(deps.storage)?;
            match account_msg {
                AccountMsg::Bond {} => {
                    let amount = must_pay(&info, config.bond_denom.as_str())?;
                    match ACCOUNTS.load(deps.storage, info.sender.clone()) {
                        Ok(mut account) => {
                            let rewards = pool.claim(&mut account);
                            let account = pool.increase_account(&account, amount);
                            ACCOUNTS.save(deps.storage, info.sender.clone(), &account)?;
                            POOL_ACCOUNTS.save(deps.storage, &pool)?;
                            Ok(
                                Response::default().add_message(CosmosMsg::Bank(BankMsg::Send {
                                    to_address: info.sender.to_string(),
                                    amount: coins(rewards.u128(), config.revenue_denom),
                                })),
                            )
                        }
                        Err(StdError::NotFound { .. }) => {
                            let account = pool.join(amount);
                            ACCOUNTS.save(deps.storage, info.sender, &account)?;
                            POOL_ACCOUNTS.save(deps.storage, &pool)?;
                            Ok(Response::default())
                        }
                        Err(err) => Err(ContractError::Std(err)),
                    }
                }
                AccountMsg::Claim {} => match ACCOUNTS.load(deps.storage, info.sender.clone()) {
                    Ok(mut account) => {
                        let rewards = pool.claim(&mut account);
                        ACCOUNTS.save(deps.storage, info.sender.clone(), &account)?;
                        Ok(
                            Response::default().add_message(CosmosMsg::Bank(BankMsg::Send {
                                to_address: info.sender.to_string(),
                                amount: coins(rewards.u128(), config.revenue_denom),
                            })),
                        )
                    }
                    Err(err) => Err(ContractError::Std(err)),
                },
                AccountMsg::Withdraw { amount } => {
                    let mut account = ACCOUNTS.load(deps.storage, info.sender.clone())?;
                    let rewards = pool.claim(&mut account);
                    let amount = amount.unwrap_or_else(|| account.amount);
                    let account = pool.decrease_account(&account, amount)?;
                    ACCOUNTS.save(deps.storage, info.sender.clone(), &account)?;
                    POOL_ACCOUNTS.save(deps.storage, &pool)?;
                    let mut send = NativeBalance(vec![
                        coin(rewards.u128(), config.revenue_denom),
                        coin(amount.u128(), config.bond_denom),
                    ]);
                    send.normalize();
                    Ok(
                        Response::default().add_message(CosmosMsg::Bank(BankMsg::Send {
                            to_address: info.sender.to_string(),
                            amount: send.into_vec(),
                        })),
                    )
                }
            }
        }
        ExecuteMsg::Liquid(liquid_msg) => match liquid_msg {
            LiquidMsg::Bond {} => {
                // Add Bond token to the Compounding pool, mint and return the Compound Share Token
                let amount = must_pay(&info, config.bond_denom.as_str())?;
                let mut pool = POOL_LIQUID.load(deps.storage)?;
                let shares = pool.join(amount)?;
                POOL_LIQUID.save(deps.storage, &pool)?;
                Ok(Response::default().add_message(share_denom.mint_msg(shares, info.sender)))
            }
            LiquidMsg::Withdraw {} => {
                let shares = must_pay(&info, share_denom.denom().as_str())?;
                let mut pool = POOL_LIQUID.load(deps.storage)?;
                let returned = pool.leave(shares)?;
                POOL_LIQUID.save(deps.storage, &pool)?;
                Ok(Response::default()
                    .add_message(share_denom.burn_msg(shares))
                    .add_message(CosmosMsg::Bank(BankMsg::Send {
                        to_address: info.sender.to_string(),
                        amount: coins(returned.u128(), config.bond_denom),
                    })))
            }
        },
    }
}

/// Calculates the amount to be distributed between (account, liquid) pools;
/// Revenue balance is queried and surplus (ie not allocated to Account stakers) is split pro-rata between ACCOUNT stakers, and LIQUID pool size.
/// Revenue allocated to LIQUID is transformed to a Wasm Execute msg to swap to the bond token
/// The surplus of bond_balance - LIQUID.size() is the return value of the previous swap and can be allocated to the total liquid pool
fn distribute(
    env: Env,
    querier: QuerierWrapper,
    storage: &mut dyn Storage,
    config: &Config,
) -> StdResult<CosmosMsg> {
    let mut account = POOL_ACCOUNTS.load(storage)?;
    let mut liquid = POOL_LIQUID.load(storage)?;

    let bond_balance = querier
        .query_balance(env.contract.address.clone(), config.bond_denom.clone())?
        .amount;

    let revenue_balance = querier
        .query_balance(env.contract.address.clone(), config.revenue_denom.clone())?
        .amount;

    let revenue_surplus = revenue_balance.checked_sub(account.pending)?;

    let account_allocation = Decimal::from_ratio(
        account.total * revenue_surplus,
        account.total.add(liquid.size()),
    )
    .to_uint_floor();

    let liquid_allocation = Decimal::from_ratio(
        liquid.size() * revenue_surplus,
        account.total.add(liquid.size()),
    )
    .to_uint_floor();

    account.distribute(account_allocation);
    POOL_ACCOUNTS.save(storage, &account)?;

    let bond_surplus = bond_balance.checked_sub(liquid.size())?;
    liquid.deposit(bond_surplus);
    POOL_LIQUID.save(storage, &liquid)?;

    Ok(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: config.revenue_converter.0.to_string(),
        msg: config.revenue_converter.1.clone(),
        funds: coins(liquid_allocation.u128(), config.revenue_denom.clone()),
    }))
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
