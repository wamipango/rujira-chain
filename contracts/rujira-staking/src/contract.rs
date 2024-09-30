#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    coins, to_json_binary, BankMsg, Binary, Coin, CosmosMsg, Deps, DepsMut, Env, Event,
    MessageInfo, Response, StdResult, Storage, Uint128, WasmMsg,
};
use cw2::set_contract_version;
use cw_utils::{may_pay, must_pay, NativeBalance};
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
    let config = Config::new(deps.api, msg.clone())?;
    config.validate()?;
    config.save(deps.storage)?;
    init(deps.storage)?;
    let share_denom = TokenFactory::new(&env, SHARE_DENOM_NONCE);
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
    use cosmwasm_std::coin;
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
            .query_balance(staker_account_1.to_string(), format!("uusdc"))
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
                &vec![],
            )
            .unwrap();
        res.assert_event(
            &Event::new("transfer")
                .add_attribute("amount", "25uusdc")
                .add_attribute("recipient", staker_account_2),
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
                &coins(500, format!("factory/{contract}/urcpt")),
            )
            .unwrap();

        res.assert_event(
            &Event::new("burn")
                .add_attribute("amount", "500")
                .add_attribute("denom", format!("factory/{contract}/urcpt")),
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
    }
}
