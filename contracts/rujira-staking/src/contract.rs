#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    coins, to_json_binary, BankMsg, Binary, Coin, CosmosMsg, Deps, DepsMut, Env, MessageInfo,
    Response, StdResult, Storage, Uint128, WasmMsg,
};
use cw2::set_contract_version;
use cw_utils::{may_pay, must_pay, nonpayable, NativeBalance};
use rujira_rs::staking::{
    AccountMsg, ConfigResponse, ExecuteMsg, InstantiateMsg, LiquidMsg, QueryMsg, SudoMsg,
};
use rujira_rs::TokenFactory;

use crate::config::Config;
use crate::error::ContractError;
use crate::events::{
    event_account_bond, event_account_claim, event_account_withdraw, event_liquid_bond,
    event_liquid_unbond,
};
use crate::state::{
    account, distribute, execute_account_bond, execute_account_claim, execute_account_withdraw,
    execute_liquid_bond, execute_liquid_unbond, init, status,
};

const CONTRACT_NAME: &str = env!("CARGO_PKG_NAME");
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    let config = Config::new(deps.api, msg.clone())?;
    config.validate()?;
    config.save(deps.storage)?;
    init(deps.storage)?;
    let share_denom = TokenFactory::new(&env, format!("staking-{}", config.bond_denom).as_str());
    Ok(Response::default().add_message(share_denom.create_msg(msg.receipt_token_metadata)))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    let config = Config::load(deps.storage)?;
    let bond_amount_sent = may_pay(&info, config.bond_denom.as_str()).unwrap_or_default();
    let swap_amount = distribute(&env, deps.querier, deps.storage, &config, &bond_amount_sent)?;
    let mut res = match msg {
        ExecuteMsg::Account(account_msg) => {
            execute_account(deps.storage, info, &config, account_msg)
        }
        ExecuteMsg::Liquid(liquid_msg) => {
            execute_liquid(deps.storage, &env, info, &config, liquid_msg)
        }
    }?;
    if swap_amount.gt(&Uint128::zero()) {
        res = res.add_message(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: config.revenue_converter.0.to_string(),
            msg: config.revenue_converter.1.clone(),
            funds: coins(swap_amount.u128(), config.revenue_denom.clone()),
        }));
    }
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
            let mut response =
                Response::default().add_event(event_account_bond(info.sender.clone(), amount));
            if reward_amount.gt(&Uint128::zero()) {
                response = response.add_message(CosmosMsg::Bank(BankMsg::Send {
                    to_address: info.sender.to_string(),
                    amount: coins(reward_amount.u128(), config.revenue_denom.clone()),
                }));
            }
            Ok(response)
        }
        AccountMsg::Claim {} => {
            nonpayable(&info)?;
            let reward_amount = execute_account_claim(storage, &info.sender)?;
            let mut response = Response::default()
                .add_event(event_account_claim(info.sender.clone(), reward_amount));
            if reward_amount.gt(&Uint128::zero()) {
                response = response.add_message(CosmosMsg::Bank(BankMsg::Send {
                    to_address: info.sender.to_string(),
                    amount: coins(reward_amount.u128(), config.revenue_denom.clone()),
                }));
            }
            Ok(response)
        }
        AccountMsg::Withdraw { amount } => {
            nonpayable(&info)?;
            let (rewards, amount) = execute_account_withdraw(storage, &info.sender, amount)?;
            let mut send = NativeBalance(vec![
                Coin::new(rewards, config.revenue_denom.clone()),
                Coin::new(amount, config.bond_denom.clone()),
            ]);
            send.normalize();

            let mut response = Response::default().add_event(event_account_withdraw(
                info.sender.clone(),
                amount,
                rewards,
            ));
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
    let share_denom = TokenFactory::new(env, format!("staking-{}", config.bond_denom).as_str());

    match msg {
        LiquidMsg::Bond {} => {
            let amount = must_pay(&info, config.bond_denom.as_str())?;
            let shares = execute_liquid_bond(storage, amount)?;
            Ok(Response::default()
                .add_event(event_liquid_bond(info.sender.clone(), amount, shares))
                .add_message(share_denom.mint_msg(shares, info.sender)))
        }
        LiquidMsg::Unbond {} => {
            let shares = must_pay(&info, share_denom.denom().as_str())?;
            let returned = execute_liquid_unbond(storage, shares)?;
            Ok(Response::default()
                .add_event(event_liquid_unbond(info.sender.clone(), shares, returned))
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
        QueryMsg::Config {} => to_json_binary(&ConfigResponse::from(config)),
        QueryMsg::Status {} => to_json_binary(&status(env, deps, &config)?),
        QueryMsg::Account { addr } => {
            to_json_binary(&account(deps.storage, deps.api.addr_validate(&addr)?)?)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::{coin, Event};
    use cw_multi_test::{ContractWrapper, Executor};
    use rujira_rs::{
        staking::{AccountResponse, StatusResponse},
        TokenMetadata,
    };
    use rujira_rs_testing::{mock_rujira_app, mock_swap_contract, MsgSwap};

    #[test]
    fn lifecycle() {
        let mut app = mock_rujira_app();
        let apps = app.api().addr_make("apps");
        let owner = app.api().addr_make("owner");
        let staker_liquid = app.api().addr_make("staker_liquid");
        let staker_account_1 = app.api().addr_make("staker_account_1");
        let staker_account_2 = app.api().addr_make("staker_account_2");
        app.init_modules(|router, _, storage| {
            router
                .bank
                .init_balance(storage, &owner, coins(500_000_000, "uruji"))
                .unwrap();

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
                .init_balance(storage, &staker_account_1, coins(500_000_000, "uruji"))
                .unwrap();
            router
                .bank
                .init_balance(storage, &staker_account_2, coins(500_000_000, "uruji"))
                .unwrap()
        });

        let swap_code_id = app.store_code(mock_swap_contract());
        let swap_contract = app
            .instantiate_contract(
                swap_code_id,
                owner.clone(),
                &(),
                &coins(10_000_000u128, "uruji"),
                "swapper",
                None,
            )
            .unwrap();

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
                        swap_contract.to_string(),
                        to_json_binary(&MsgSwap {
                            min_return: coin(10, "uruji"),
                        })
                        .unwrap(),
                        Uint128::from(20u128),
                    ),
                    receipt_token_metadata: TokenMetadata {
                        description: "description".to_string(),
                        name: "Liquid Staked RUJI".to_string(),
                        display: "sRUJI".to_string(),
                        symbol: "sRUJI".to_string(),
                        uri: None,
                        uri_hash: None,
                        exponent: 6,
                    },
                },
                &[],
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
            .query_balance(staker_liquid.to_string(), format!("x/staking-uruji"))
            .unwrap();
        assert_eq!(share_balance.amount, Uint128::from(1_000u128));

        app.execute_contract(
            staker_account_1.clone(),
            contract.clone(),
            &ExecuteMsg::Account(AccountMsg::Bond {}),
            &coins(500, "uruji"),
        )
        .unwrap();

        app.send_tokens(apps.clone(), contract.clone(), &coins(100, "uusdc"))
            .unwrap();

        app.execute_contract(
            staker_account_2.clone(),
            contract.clone(),
            &ExecuteMsg::Account(AccountMsg::Bond {}),
            &coins(500, "uruji"),
        )
        .unwrap();

        // We've staked _after_ the revenue has been collected. 33% share (given 1000/500 split when fee was collected)
        let account: AccountResponse = app
            .wrap()
            .query_wasm_smart(
                contract.clone(),
                &QueryMsg::Account {
                    addr: staker_account_1.to_string(),
                },
            )
            .unwrap();

        assert_eq!(account.bonded, Uint128::from(500u128));
        assert_eq!(account.pending_revenue, Uint128::from(33u128));

        // We've staked _after_ the revenue has been collected. No ownership
        let account: AccountResponse = app
            .wrap()
            .query_wasm_smart(
                contract.clone(),
                &QueryMsg::Account {
                    addr: staker_account_2.to_string(),
                },
            )
            .unwrap();

        assert_eq!(account.bonded, Uint128::from(500u128));
        assert_eq!(account.pending_revenue, Uint128::zero());

        app.send_tokens(apps.clone(), contract.clone(), &coins(100, "uusdc"))
            .unwrap();

        // Another bond to trigger a distribution
        app.execute_contract(
            staker_account_1.clone(),
            contract.clone(),
            &ExecuteMsg::Account(AccountMsg::Bond {}),
            &coins(500, "uruji"),
        )
        .unwrap();

        // 1000/500 split of 100 + 1500/500 split of 100 = 33 + 25 = 58
        let account: AccountResponse = app
            .wrap()
            .query_wasm_smart(
                contract.clone(),
                &QueryMsg::Account {
                    addr: staker_account_1.to_string(),
                },
            )
            .unwrap();

        let revenue_balance = app
            .wrap()
            .query_balance(staker_account_1.to_string(), "uusdc")
            .unwrap();
        assert_eq!(revenue_balance.amount, Uint128::from(58u128));
        assert_eq!(account.bonded, Uint128::from(1000u128));
        assert_eq!(account.pending_revenue, Uint128::zero());

        // 1500/500 split of 100 = 25
        let account: AccountResponse = app
            .wrap()
            .query_wasm_smart(
                contract.clone(),
                &QueryMsg::Account {
                    addr: staker_account_2.to_string(),
                },
            )
            .unwrap();

        assert_eq!(account.bonded, Uint128::from(500u128));
        assert_eq!(account.pending_revenue, Uint128::from(25u128));

        let status: StatusResponse = app
            .wrap()
            .query_wasm_smart(contract.clone(), &QueryMsg::Status {})
            .unwrap();
        assert_eq!(status.account_bond, Uint128::from(1500u128));
        assert_eq!(status.account_revenue, Uint128::from(25u128));
        assert_eq!(status.liquid_bond_shares, Uint128::from(1000u128));
        assert_eq!(status.liquid_bond_size, Uint128::from(1010u128));

        let res = app
            .execute_contract(
                staker_account_2.clone(),
                contract.clone(),
                &ExecuteMsg::Account(AccountMsg::Claim {}),
                &[],
            )
            .unwrap();
        res.assert_event(
            &Event::new("transfer")
                .add_attribute("amount", "25uusdc")
                .add_attribute("recipient", staker_account_2.clone()),
        );

        let status: StatusResponse = app
            .wrap()
            .query_wasm_smart(contract.clone(), &QueryMsg::Status {})
            .unwrap();
        assert_eq!(status.account_bond, Uint128::from(1500u128));
        assert_eq!(status.account_revenue, Uint128::zero());
        assert_eq!(status.liquid_bond_shares, Uint128::from(1000u128));
        assert_eq!(status.liquid_bond_size, Uint128::from(1020u128));

        let res = app
            .execute_contract(
                staker_liquid.clone(),
                contract.clone(),
                &ExecuteMsg::Liquid(LiquidMsg::Unbond {}),
                &coins(500, format!("x/staking-uruji")),
            )
            .unwrap();

        res.assert_event(
            &Event::new("burn")
                .add_attribute("amount", "500")
                .add_attribute("denom", format!("x/staging-uruji")),
        );
        // total distribution of 30 across 1000 shares, so withdrawal should be 515 uruji
        res.assert_event(
            &Event::new("transfer")
                .add_attribute("amount", "515uruji")
                .add_attribute("recipient", staker_liquid),
        );

        let status: StatusResponse = app
            .wrap()
            .query_wasm_smart(contract.clone(), &QueryMsg::Status {})
            .unwrap();
        assert_eq!(status.account_bond, Uint128::from(1500u128));
        assert_eq!(status.account_revenue, Uint128::zero());
        assert_eq!(status.liquid_bond_shares, Uint128::from(500u128));
        assert_eq!(status.liquid_bond_size, Uint128::from(515u128));

        // Test partial withdrawal and that it claims rewards
        // 100 split 75:25 account:liquid.
        // 75 split 2:1 beteen staker 1 and 2
        app.send_tokens(apps.clone(), contract.clone(), &coins(100, "uusdc"))
            .unwrap();

        let res = app
            .execute_contract(
                staker_account_1.clone(),
                contract.clone(),
                &ExecuteMsg::Account(AccountMsg::Withdraw {
                    amount: Some(Uint128::from(50u128)),
                }),
                &[],
            )
            .unwrap();

        res.assert_event(
            &Event::new("transfer")
                .add_attribute("amount", "50uruji,50uusdc")
                .add_attribute("recipient", staker_account_1.clone()),
        );

        let status: StatusResponse = app
            .wrap()
            .query_wasm_smart(contract.clone(), &QueryMsg::Status {})
            .unwrap();
        assert_eq!(status.account_bond, Uint128::from(1450u128));
        assert_eq!(status.account_revenue, Uint128::from(25u128));
        assert_eq!(status.liquid_bond_shares, Uint128::from(500u128));
        assert_eq!(status.liquid_bond_size, Uint128::from(525u128));

        let account: AccountResponse = app
            .wrap()
            .query_wasm_smart(
                contract.clone(),
                &QueryMsg::Account {
                    addr: staker_account_1.to_string(),
                },
            )
            .unwrap();

        assert_eq!(account.bonded, Uint128::from(950u128));
        assert_eq!(account.pending_revenue, Uint128::zero());

        let account: AccountResponse = app
            .wrap()
            .query_wasm_smart(
                contract.clone(),
                &QueryMsg::Account {
                    addr: staker_account_2.to_string(),
                },
            )
            .unwrap();

        assert_eq!(account.bonded, Uint128::from(500u128));
        assert_eq!(account.pending_revenue, Uint128::from(25u128));
    }
}
