#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    coins, to_json_binary, BankMsg, Binary, Coin, CosmosMsg, Deps, DepsMut, Env, Event,
    MessageInfo, Response, StdResult, Storage, Uint128,
};
use cw2::set_contract_version;
use cw_utils::{must_pay, NativeBalance};
use rujira_rs::staking::{AccountMsg, ExecuteMsg, InstantiateMsg, LiquidMsg, QueryMsg, SudoMsg};
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
    let config = Config::new(deps.api, msg)?;
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

    let res = match msg {
        ExecuteMsg::Account(account_msg) => {
            execute_account(deps.storage, info, &config, account_msg)
        }
        ExecuteMsg::Liquid(liquid_msg) => {
            execute_liquid(deps.storage, &env, info, &config, liquid_msg)
        }
    }?;

    distribute(&env, deps.querier, deps.storage, &config)?;

    Ok(res)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn sudo(deps: DepsMut, _env: Env, msg: SudoMsg) -> Result<Response, ContractError> {
    let mut config = Config::load(deps.storage)?;

    match msg {
        SudoMsg::SetRevenueConverter {
            contract,
            msg,
            limit,
        } => {
            config.revenue_converter = (deps.api.addr_validate(&contract)?, msg, limit);
            config.save(deps.storage)?;
            Ok(Response::default())
        }
    }
}

fn execute_account(
    storage: &mut dyn Storage,
    info: MessageInfo,
    config: &Config,
    msg: AccountMsg,
) -> Result<Response, ContractError> {
    match msg {
        AccountMsg::Bond {} => {
            let amount = must_pay(&info, config.bond_denom.as_str())?;

            let reward_amount = execute_account_bond(storage, &info.sender, amount)?;
            let mut response = Response::default().add_event(
                Event::new("staking/account/bond")
                    .add_attribute("owner", info.sender.clone())
                    .add_attribute("amount", amount),
            );
            if reward_amount.gt(&Uint128::zero()) {
                response = response.add_message(CosmosMsg::Bank(BankMsg::Send {
                    to_address: info.sender.to_string(),
                    amount: coins(reward_amount.u128(), config.revenue_denom.clone()),
                }));
            }
            Ok(response)
        }
        AccountMsg::Claim {} => {
            let reward_amount = execute_account_claim(storage, &info.sender)?;
            let mut response = Response::default().add_event(
                Event::new("staking/account/claim")
                    .add_attribute("owner", info.sender.clone())
                    .add_attribute("amount", reward_amount),
            );
            if reward_amount.gt(&Uint128::zero()) {
                response = response.add_message(CosmosMsg::Bank(BankMsg::Send {
                    to_address: info.sender.to_string(),
                    amount: coins(reward_amount.u128(), config.revenue_denom.clone()),
                }));
            }
            Ok(response)
        }
        AccountMsg::Withdraw { amount } => {
            let (rewards, amount) = execute_account_withdraw(storage, &info.sender, amount)?;
            let mut send = NativeBalance(vec![
                Coin::new(rewards, config.revenue_denom.clone()),
                Coin::new(amount, config.bond_denom.clone()),
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
    storage: &mut dyn Storage,
    env: &Env,
    info: MessageInfo,
    config: &Config,
    msg: LiquidMsg,
) -> Result<Response, ContractError> {
    let share_denom = TokenFactory::new(env, SHARE_DENOM_NONCE);

    match msg {
        LiquidMsg::Bond {} => {
            let amount = must_pay(&info, config.bond_denom.as_str())?;
            let shares = execute_liquid_bond(storage, amount)?;
            Ok(Response::default()
                .add_event(
                    Event::new("staking/liquid/bond")
                        .add_attribute("owner", info.sender.clone())
                        .add_attribute("amount", amount)
                        .add_attribute("shares", shares),
                )
                .add_message(share_denom.mint_msg(shares, info.sender)))
        }
        LiquidMsg::Unbond {} => {
            let shares = must_pay(&info, share_denom.denom().as_str())?;
            let returned = execute_liquid_unbond(storage, shares)?;
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
                    amount: coins(returned.u128(), config.bond_denom.clone()),
                })))
        }
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    let config = Config::load(deps.storage)?;
    match msg {
        QueryMsg::Status {} => to_json_binary(&status(env, deps, &config)?),
        QueryMsg::Account { addr } => {
            to_json_binary(&account(deps.storage, deps.api.addr_validate(&addr)?)?)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cw_multi_test::{ContractWrapper, Executor};
    use rujira_rs_testing::mock_rujira_app;

    #[test]
    fn lifecycle() {
        let mut app = mock_rujira_app();
        let apps = app.api().addr_make("apps");
        let owner = app.api().addr_make("owner");
        let staker_liquid = app.api().addr_make("staker_liquid");
        let staker_account = app.api().addr_make("staker_account");
        app.init_modules(|router, _, storage| {
            router
                .bank
                .init_balance(storage, &apps, coins(500_000_000, "uusdc"))
                .unwrap();
            router
                .bank
                .init_balance(storage, &staker_liquid, coins(500_000_000, "uruji"))
                .unwrap();
            router
                .bank
                .init_balance(storage, &staker_account, coins(500_000_000, "uruji"))
                .unwrap()
        });

        let code = Box::new(ContractWrapper::new(execute, instantiate, query));
        let code_id = app.store_code(code);
        let contract = app
            .instantiate_contract(
                code_id,
                owner.clone(),
                &InstantiateMsg {
                    bond_denom: "uruji".to_string(),
                    revenue_denom: "uusdc".to_string(),
                    revenue_converter: (
                        app.api().addr_make("converter").to_string(),
                        Binary::default(),
                        Uint128::zero(),
                    ),
                },
                &vec![],
                "staking",
                None,
            )
            .unwrap();

        app.execute_contract(
            staker_liquid.clone(),
            contract.clone(),
            &ExecuteMsg::Liquid(LiquidMsg::Bond {}),
            &coins(1_000, "uruji"),
        )
        .unwrap();

        let pool_balance = app.wrap().query_balance(contract.clone(), "uruji").unwrap();
        assert_eq!(pool_balance.amount, Uint128::from(1_000u128));

        let share_balance = app
            .wrap()
            .query_balance(
                staker_liquid.to_string(),
                format!("factory/{contract}/urcpt"),
            )
            .unwrap();
        assert_eq!(share_balance.amount, Uint128::from(1_000u128));
    }
}
