#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    coin, coins, to_json_binary, BankMsg, Binary, CosmosMsg, Deps, DepsMut, Empty, Env,
    MessageInfo, Response,
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
    env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    let config = Config::from(msg.clone());
    config.validate()?;
    config.save(deps.storage)?;
    Ok(Response::default().add_message(
        TokenFactory::new(&env, config.strategy.denom().as_str()).create_msg(msg.metadata),
    ))
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
            config.strategy.validate(
                deps.as_ref(),
                env,
                info.clone(),
                offer,
                min_return.clone(),
            )?;
            let to_address = to
                .map(|x| deps.api.addr_validate(x.as_str()))
                .transpose()?
                .unwrap_or(info.sender);

            match callback {
                None => Ok(
                    Response::default().add_message(CosmosMsg::Bank(BankMsg::Send {
                        to_address: to_address.to_string(),
                        amount: vec![min_return],
                    })),
                ),
                Some(cb) => Ok(Response::default().add_message(cb.to_message(
                    &to_address,
                    Empty {},
                    vec![min_return],
                )?)),
            }
        }
        ExecuteMsg::Deposit { callback } => {
            let t = TokenFactory::new(&env, config.strategy.denom().as_str());
            let supply = t.supply(deps.querier)?;
            let (deposited, minted) = config.strategy.calculate_share(
                deps.as_ref(),
                env.clone(),
                info.clone(),
                supply,
            )?;

            match callback {
                None => Ok(Response::default()
                    .add_message(t.mint_msg(minted, info.sender.clone()))
                    .add_event(event_deposit(
                        info.sender,
                        deposited,
                        coin(minted.u128(), t.denom()),
                    ))),
                Some(cb) => Ok(Response::default()
                    .add_message(t.mint_msg(minted, env.contract.address))
                    .add_message(cb.to_message(
                        &info.sender,
                        Empty {},
                        coins(minted.u128(), &t.denom()),
                    )?)
                    .add_event(event_deposit(
                        info.sender,
                        deposited,
                        coin(minted.u128(), t.denom()),
                    ))),
            }
        }
        ExecuteMsg::Withdraw { callback } => {
            let t = TokenFactory::new(&env, config.strategy.denom().as_str());
            let supply = t.supply(deps.querier)?;
            let balance = must_pay(&info, t.denom().as_str())?;
            let withdrawn = config.strategy.calculate_ownership(
                deps.as_ref(),
                env,
                info.clone(),
                supply,
                balance,
            )?;

            match callback {
                None => Ok(Response::default()
                    .add_message(CosmosMsg::Bank(BankMsg::Send {
                        to_address: info.sender.to_string(),
                        amount: withdrawn.clone().into_vec(),
                    }))
                    .add_message(t.burn_msg(balance))
                    .add_event(event_withdraw(
                        info.sender,
                        coin(balance.u128(), t.denom()),
                        withdrawn.into_vec().try_into()?,
                    ))),
                Some(cb) => Ok(Response::default()
                    .add_message(cb.to_message(
                        &info.sender,
                        Empty {},
                        withdrawn.clone().into_vec(),
                    )?)
                    .add_message(t.burn_msg(balance))
                    .add_event(event_withdraw(
                        info.sender,
                        coin(balance.u128(), t.denom()),
                        withdrawn.into_vec().try_into()?,
                    ))),
            }
        }
    }
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
        QueryMsg::Quote(req) => {
            let q = config.strategy.quote(deps, env, req)?;
            Ok(to_json_binary(&q)?)
        }
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use cosmwasm_std::{Decimal, Event, Uint256};
    use cw_multi_test::{ContractWrapper, Executor};
    use rujira_rs::{
        bow::{Strategies, Xyk},
        TokenMetadata,
    };
    use rujira_rs_testing::mock_rujira_app;

    #[test]
    fn instantiation() {
        let mut app = mock_rujira_app();
        let owner = app.api().addr_make("owner");

        let code = Box::new(ContractWrapper::new(execute, instantiate, query));
        let code_id = app.store_code(code);
        app.instantiate_contract(
            code_id,
            owner,
            &InstantiateMsg {
                metadata: TokenMetadata {
                    description: "RUJI-USDC XYK Liquidity Pool Token".to_string(),
                    display: "LP RUJI-USDC".to_string(),
                    name: "LP RUJI-USDC".to_string(),
                    symbol: "LP/RUJI-USDC".to_string(),
                    uri: None,
                    uri_hash: None,
                },
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

    #[test]
    fn deposit_withdraw() {
        let mut app = mock_rujira_app();
        let addr = app.api().addr_make("depositor");
        app.init_modules(|router, _api, storage| {
            router
                .bank
                .init_balance(storage, &addr, vec![coin(1000, "ruji"), coin(1000, "usdc")])
        })
        .unwrap();
        let owner = app.api().addr_make("owner");

        let code = Box::new(ContractWrapper::new(execute, instantiate, query));
        let code_id = app.store_code(code);
        let contract = app
            .instantiate_contract(
                code_id,
                owner,
                &InstantiateMsg {
                    metadata: TokenMetadata {
                        description: "RUJI-USDC XYK Liquidity Pool Token".to_string(),
                        display: "LP RUJI-USDC".to_string(),
                        name: "LP RUJI-USDC".to_string(),
                        symbol: "LP/RUJI-USDC".to_string(),
                        uri: None,
                        uri_hash: None,
                    },
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

        let res = app
            .execute_contract(
                addr.clone(),
                contract.clone(),
                &ExecuteMsg::Deposit { callback: None },
                &vec![coin(1000, "ruji"), coin(1000, "usdc")],
            )
            .unwrap();

        res.assert_event(&Event::new("wasm-rujira-bow/deposit").add_attributes(vec![
            ("deposit", "1000ruji,1000usdc"),
            ("minted", "1000x/bow-xyk-ruji-usdc"),
        ]));

        res.assert_event(&Event::new("mint").add_attributes(vec![
            ("amount", "1000"),
            ("denom", "x/bow-xyk-ruji-usdc"),
            ("recipient", addr.as_str()),
        ]));

        let res = app
            .execute_contract(
                addr.clone(),
                contract.clone(),
                &ExecuteMsg::Withdraw { callback: None },
                &vec![coin(200, "x/bow-xyk-ruji-usdc")],
            )
            .unwrap();

        res.assert_event(&Event::new("wasm-rujira-bow/withdraw").add_attributes(vec![
            ("share", "200x/bow-xyk-ruji-usdc"),
            ("withdrawn", "200ruji,200usdc"),
        ]));

        res.assert_event(&Event::new("transfer").add_attributes(vec![
            ("amount", "200ruji,200usdc"),
            ("recipient", addr.as_str()),
            ("sender", contract.as_str()),
        ]));

        res.assert_event(
            &Event::new("burn")
                .add_attributes(vec![("amount", "200"), ("denom", "x/bow-xyk-ruji-usdc")]),
        );
    }
}
