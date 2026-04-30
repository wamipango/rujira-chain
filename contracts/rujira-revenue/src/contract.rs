use std::ops::Mul;

#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    coins, to_json_binary, Addr, BankMsg, Binary, CosmosMsg, Decimal, Deps, DepsMut, Env,
    MessageInfo, QuerierWrapper, Reply, Response, StdResult, Storage, SubMsg, Timestamp, Uint128,
    WasmMsg,
};
use cw_storage_plus::Item;
use rujira_rs::msg::deposit::MsgDeposit;
use rujira_rs::proto::scheduler::MsgScheduleExecuteContract;
use rujira_rs::utils::module_address;
use rujira_rs::Asset;
use rujira_rs::Coin;

use crate::error::ContractError;
use crate::events::event_run;
use crate::state::{Action, Config};
use rujira_rs::revenue::{
    ActionResponse, ActionsResponse, ConfigResponse, ExecuteMsg, InstantiateMsg, QueryMsg,
    StatusResponse, SudoMsg,
};

const CONTRACT_NAME: &str = env!("CARGO_PKG_NAME");
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");
const LAST_EXECUTED: Item<Timestamp> = Item::new("last_executed");
const SCHEDULE: Item<Option<u64>> = Item::new("schedule");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    cw2::set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    let config = Config::new(deps.api, msg)?;
    config.save(deps.storage)?;
    LAST_EXECUTED.save(deps.storage, &env.block.time)?;
    SCHEDULE.save(deps.storage, &None)?;
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
        ExecuteMsg::Run {} => {
            if info.sender != config.executor {
                return Err(ContractError::Unauthorized {});
            }
            execute_run(deps, env)
        }
        ExecuteMsg::Distribute {} => {
            if info.sender != env.contract.address {
                return Err(ContractError::Unauthorized {});
            }

            let mut sends: Vec<CosmosMsg> = vec![];
            for (denom, limit) in config.target_denoms().clone() {
                distribute_denom(deps.as_ref(), &env, &config, &mut sends, denom, limit)?;
            }
            LAST_EXECUTED.save(deps.storage, &env.block.time)?;

            match SCHEDULE.load(deps.storage)? {
                Some(after) => Ok(Response::default().add_messages(sends).add_message(
                    MsgScheduleExecuteContract {
                        sender: env.contract.address.to_string(),
                        after,
                        msg: to_json_binary(&ExecuteMsg::Run {})?.to_vec(),
                    },
                )),
                None => Ok(Response::default().add_messages(sends)),
            }
        }
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(_deps: DepsMut, _env: Env, _reply: Reply) -> Result<Response, ContractError> {
    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn sudo(deps: DepsMut, env: Env, msg: SudoMsg) -> Result<Response, ContractError> {
    let mut config = Config::load(deps.storage)?;
    match msg {
        SudoMsg::Run {} => execute_run(deps, env),
        SudoMsg::SetOwner(owner) => {
            config.owner = deps.api.addr_validate(&owner)?;
            config.save(deps.storage)?;
            Ok(Response::default())
        }
        SudoMsg::SetAction {
            denom,
            contract,
            min,
            max,
            msg,
        } => {
            Action::set(
                deps.storage,
                denom,
                deps.api.addr_validate(&contract)?,
                min,
                max,
                msg,
            )?;
            Ok(Response::default())
        }
        SudoMsg::UnsetAction(denom) => {
            Action::unset(deps.storage, denom);
            Ok(Response::default())
        }
        SudoMsg::SetExecutor(executor) => {
            config.executor = deps.api.addr_validate(&executor)?;
            config.save(deps.storage)?;
            Ok(Response::default())
        }
        SudoMsg::SetTargetDenom((denom, limit)) => {
            config.set_target_denom(denom, limit);
            config.save(deps.storage)?;
            Ok(Response::default())
        }
        SudoMsg::SetTargetAddress((address, weight)) => {
            config.set_target_address(deps.api.addr_validate(&address)?, weight);
            config.save(deps.storage)?;
            Ok(Response::default())
        }
        SudoMsg::SetSchedule(v) => {
            SCHEDULE.save(deps.storage, &Some(v))?;
            Ok(Response::default())
        }
        SudoMsg::UnsetSchedule() => {
            SCHEDULE.save(deps.storage, &None)?;
            Ok(Response::default())
        }
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, _msg: ()) -> Result<Response, ContractError> {
    cw2::set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    Ok(Response::default())
}

fn execute_run(deps: DepsMut, env: Env) -> Result<Response, ContractError> {
    let action_msg = get_action_msg(deps.storage, deps.querier, &env.contract.address)?;
    let distribute_msg = WasmMsg::Execute {
        contract_addr: env.contract.address.to_string(),
        msg: to_json_binary(&ExecuteMsg::Distribute {})?,
        funds: vec![],
    };
    match action_msg {
        Some((action, msg)) => Ok(Response::default()
            .add_submessage(SubMsg::reply_on_error(msg, 0))
            .add_event(event_run(action.denom))
            .add_message(distribute_msg)),
        // If there's no compatible action, skip straight to distribution.
        None => Ok(Response::default().add_message(distribute_msg)),
    }
}

fn get_action_msg(
    storage: &mut dyn Storage,
    querier: QuerierWrapper,
    contract: &Addr,
) -> StdResult<Option<(Action, WasmMsg)>> {
    // Fetch the next action in the iterator
    if let Some(action) = Action::next(storage)? {
        let balance = querier.query_balance(contract, action.denom.to_string())?;
        return match action.execute(balance)? {
            None => Ok(None),
            Some(msg) => Ok(Some((action, msg))),
        };
    }
    Ok(None)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => {
            let mut res = ConfigResponse::from(Config::load(deps.storage)?);
            res.schedule = SCHEDULE.load(deps.storage)?;
            res.last_executed = LAST_EXECUTED.load(deps.storage)?;
            to_json_binary(&res)
        }
        QueryMsg::Actions {} => to_json_binary(&ActionsResponse {
            actions: Action::all(deps.storage)?
                .iter()
                .map(|x| ActionResponse::from(x.clone()))
                .collect(),
        }),
        QueryMsg::Status {} => to_json_binary(&StatusResponse {
            last: Action::last(deps.storage)?,
        }),
    }
}

fn distribute_denom(
    deps: Deps,
    env: &Env,
    config: &Config,
    sends: &mut Vec<CosmosMsg>,
    denom: String,
    limit: Uint128,
) -> Result<(), ContractError> {
    let seconds = env
        .block
        .time
        .seconds()
        .saturating_sub(LAST_EXECUTED.load(deps.storage)?.seconds());

    let balance = deps
        .querier
        .query_balance(env.contract.address.clone(), denom.to_string())?;

    let distributing = Uint128::from(seconds).mul(limit).min(balance.amount);

    let total_weight = config
        .target_addresses
        .iter()
        .fold(Uint128::zero(), |a, e| Uint128::from(e.1) + a);
    if total_weight.is_zero() {
        return Ok(());
    }
    if !distributing.is_zero() {
        let mut remaining = distributing;
        let mut targets = config.target_addresses.iter().peekable();

        while let Some((addr, weight)) = targets.next() {
            let amount = if targets.peek().is_none() {
                remaining
            } else {
                let ratio = Decimal::from_ratio(*weight, total_weight);
                distributing.mul_floor(ratio)
            };

            if amount.is_zero() {
                continue;
            }
            remaining -= amount;

            sends.push(
                if deps.api.addr_canonicalize(addr.as_str())? == module_address("reserve") {
                    MsgDeposit::new(
                        vec![Coin::new(Asset::from_denom(&denom)?, amount.into())],
                        "RESERVE".to_owned(),
                        deps.api.addr_canonicalize(env.contract.address.as_str())?,
                    )
                    .into()
                } else {
                    BankMsg::Send {
                        to_address: addr.to_string(),
                        amount: coins(amount.u128(), denom.clone()),
                    }
                    .into()
                },
            )
        }
    };
    Ok(())
}

#[cfg(test)]
mod tests {

    use super::*;
    use cosmwasm_std::{
        coin, from_json,
        testing::{message_info, mock_dependencies, mock_env},
        BlockInfo, Empty, Timestamp, Uint128,
    };
    use cw_multi_test::{BasicApp, ContractWrapper, Executor};
    use rujira_rs_testing::{mock_failing_contract, mock_noop_contract};

    #[test]
    fn instantiation() {
        let mut deps = mock_dependencies();
        let app = BasicApp::default();
        let owner = app.api().addr_make("owner");
        let fees = app.api().addr_make("fees");
        let executor = app.api().addr_make("executor");
        let info = message_info(&owner, &[]);
        let msg = InstantiateMsg {
            owner: owner.to_string(),
            target_denoms: vec![
                ("uruji".to_string(), Uint128::from(10u128)),
                ("another".to_string(), Uint128::from(20u128)),
            ],
            target_addresses: vec![(fees.to_string(), 1)],
            executor: executor.to_string(),
        };
        instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();
        let config: ConfigResponse =
            from_json(query(deps.as_ref(), mock_env(), QueryMsg::Config {}).unwrap()).unwrap();
        assert_eq!(config.owner, owner.to_string());
        assert_eq!(
            config.target_denoms,
            vec![
                ("uruji".to_string(), Uint128::from(10u128)),
                ("another".to_string(), Uint128::from(20u128)),
            ],
        );
        let status: StatusResponse =
            from_json(query(deps.as_ref(), mock_env(), QueryMsg::Status {}).unwrap()).unwrap();
        assert_eq!(status.last, None);
        let actions: ActionsResponse =
            from_json(query(deps.as_ref(), mock_env(), QueryMsg::Actions {}).unwrap()).unwrap();
        assert_eq!(actions.actions, vec![]);
    }
    #[test]
    fn authorization() {
        let mut deps = mock_dependencies();
        let app = BasicApp::default();
        let owner = app.api().addr_make("owner");
        let fees = app.api().addr_make("fees");
        let executor = app.api().addr_make("executor");

        let info = message_info(&owner, &[]);
        let msg = InstantiateMsg {
            owner: owner.to_string(),
            target_denoms: vec![
                ("uruji".to_string(), Uint128::from(10u128)),
                ("another".to_string(), Uint128::from(20u128)),
            ],
            target_addresses: vec![(fees.to_string(), 1)],
            executor: executor.to_string(),
        };
        instantiate(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();

        sudo(
            deps.as_mut(),
            mock_env(),
            SudoMsg::SetOwner(app.api().addr_make("owner-new").to_string()),
        )
        .unwrap();

        let contract = app.api().addr_make("fin");
        let action = Action {
            denom: "uatom".to_string(),
            contract,
            min: Uint128::zero(),
            max: Uint128::MAX,
            msg: Binary::new(vec![0]),
        };

        let a = action.clone();

        sudo(
            deps.as_mut(),
            mock_env(),
            SudoMsg::SetAction {
                denom: a.denom,
                contract: a.contract.to_string(),
                min: Uint128::zero(),
                max: a.max,
                msg: a.msg,
            },
        )
        .unwrap();

        let actions: ActionsResponse =
            from_json(query(deps.as_ref(), mock_env(), QueryMsg::Actions {}).unwrap()).unwrap();
        assert_eq!(
            actions.actions,
            vec![ActionResponse {
                denom: action.denom.clone(),
                contract: action.contract.to_string(),
                min: Uint128::zero(),
                max: action.max,
                msg: action.msg
            }]
        );

        sudo(
            deps.as_mut(),
            mock_env(),
            SudoMsg::UnsetAction(action.denom),
        )
        .unwrap();

        let actions: ActionsResponse =
            from_json(query(deps.as_ref(), mock_env(), QueryMsg::Actions {}).unwrap()).unwrap();
        assert_eq!(actions.actions, vec![]);

        execute(
            deps.as_mut(),
            mock_env(),
            message_info(&executor, &[]),
            ExecuteMsg::Run {},
        )
        .unwrap();

        sudo(
            deps.as_mut(),
            mock_env(),
            SudoMsg::SetExecutor(app.api().addr_make("owner-new").to_string()),
        )
        .unwrap();

        execute(
            deps.as_mut(),
            mock_env(),
            message_info(&app.api().addr_make("owner-new"), &[]),
            ExecuteMsg::Run {},
        )
        .unwrap();
    }

    #[test]
    fn cranking() {
        let mut app = BasicApp::default();
        let owner = app.api().addr_make("owner");
        let fees = app.api().addr_make("fees");

        let funds = vec![
            // coin(1000u128, "token-a"),
            coin(1000u128, "token-b"),
            coin(1000u128, "token-c"),
            coin(1000u128, "token-d"),
            coin(1000u128, "token-e"),
        ];

        app.init_modules(|router, _, storage| {
            router.bank.init_balance(storage, &owner, funds.clone())
        })
        .unwrap();
        let code = Box::new(
            ContractWrapper::new(execute, instantiate, query)
                .with_reply(reply)
                .with_sudo(sudo),
        );
        let code_id = app.store_code(code);
        let noop_code_id = app.store_code(mock_noop_contract());
        let contract = app
            .instantiate_contract(
                code_id,
                owner.clone(),
                &InstantiateMsg {
                    owner: owner.to_string(),
                    target_denoms: vec![
                        ("uruji".to_string(), Uint128::MAX),
                        ("another".to_string(), Uint128::MAX),
                    ],
                    target_addresses: vec![(fees.to_string(), 1)],
                    executor: owner.to_string(),
                },
                &[],
                "revenue",
                None,
            )
            .unwrap();
        let noop = app
            .instantiate_contract(
                noop_code_id,
                owner.clone(),
                &Empty::default(),
                &[],
                "noop",
                None,
            )
            .unwrap();

        app.send_tokens(owner.clone(), contract.clone(), &funds)
            .unwrap();

        app.execute_contract(owner.clone(), contract.clone(), &ExecuteMsg::Run {}, &[])
            .unwrap();

        // Make sure that execution ends when there are no actions
        let status: StatusResponse = app
            .wrap()
            .query_wasm_smart(contract.clone(), &QueryMsg::Status {})
            .unwrap();
        assert_eq!(status.last, None);

        // Set some actions
        set_action(
            &mut app,
            &contract,
            "token-a",
            &noop,
            Uint128::zero(),
            Uint128::MAX,
        );
        set_action(
            &mut app,
            &contract,
            "token-b",
            &noop,
            Uint128::zero(),
            Uint128::MAX,
        );
        set_action(
            &mut app,
            &contract,
            "token-c",
            &noop,
            Uint128::zero(),
            Uint128::from(100u128),
        );
        set_action(
            &mut app,
            &contract,
            "token-d",
            &noop,
            Uint128::zero(),
            Uint128::MAX,
        );
        set_action(
            &mut app,
            &contract,
            "token-e",
            &noop,
            Uint128::zero(),
            Uint128::MAX,
        );

        let res = app
            .execute_contract(owner.clone(), contract.clone(), &ExecuteMsg::Run {}, &[])
            .unwrap();
        assert_no_run_event(&res);
        let status: StatusResponse = app
            .wrap()
            .query_wasm_smart(contract.clone(), &QueryMsg::Status {})
            .unwrap();
        assert_eq!(status.last, Some("token-a".to_string()));

        // Iterator should start at the beginning again and execute token-a
        let res = app
            .execute_contract(owner.clone(), contract.clone(), &ExecuteMsg::Run {}, &[])
            .unwrap();
        let status: StatusResponse = app
            .wrap()
            .query_wasm_smart(contract.clone(), &QueryMsg::Status {})
            .unwrap();
        assert_eq!(status.last, Some("token-b".to_string()));
        assert_run_event(&res, "token-b");

        // Run for c, d, e and then loop back to a
        let res = app
            .execute_contract(owner.clone(), contract.clone(), &ExecuteMsg::Run {}, &[])
            .unwrap();
        assert_run_event(&res, "token-c");

        let res = app
            .execute_contract(owner.clone(), contract.clone(), &ExecuteMsg::Run {}, &[])
            .unwrap();
        assert_run_event(&res, "token-d");

        let res = app
            .execute_contract(owner.clone(), contract.clone(), &ExecuteMsg::Run {}, &[])
            .unwrap();
        assert_run_event(&res, "token-e");

        let res = app
            .execute_contract(owner.clone(), contract.clone(), &ExecuteMsg::Run {}, &[])
            .unwrap();

        assert_no_run_event(&res);
        let status: StatusResponse = app
            .wrap()
            .query_wasm_smart(contract.clone(), &QueryMsg::Status {})
            .unwrap();
        assert_eq!(status.last, Some("token-a".to_string()));
    }

    fn assert_run_event(res: &cw_multi_test::AppResponse, denom: &str) {
        let event = res
            .events
            .iter()
            .find(|event| event.ty == "wasm-rujira-revenue/run")
            .unwrap();
        assert!(event
            .attributes
            .iter()
            .any(|attribute| attribute.key == "denom" && attribute.value == denom));
    }

    fn assert_no_run_event(res: &cw_multi_test::AppResponse) {
        assert!(!res
            .events
            .iter()
            .any(|event| event.ty == "wasm-rujira-revenue/run"));
    }

    fn set_action(
        app: &mut BasicApp,
        contract: &Addr,
        denom: &str,
        target: &Addr,
        min: Uint128,
        max: Uint128,
    ) {
        app.wasm_sudo(
            contract.clone(),
            &SudoMsg::SetAction {
                denom: denom.to_string(),
                contract: target.to_string(),
                min,
                max,
                msg: to_json_binary(&Empty::default()).unwrap(),
            },
        )
        .unwrap();
    }

    fn set_block_time(app: &mut BasicApp, seconds: u64) {
        app.set_block(BlockInfo {
            height: seconds,
            time: Timestamp::from_seconds(seconds),
            chain_id: app.block_info().chain_id,
        });
    }

    #[test]
    fn distribution_continues_when_action_errors() {
        let mut app = BasicApp::default();
        let fees = app.api().addr_make("fees");
        let owner = app.api().addr_make("owner");

        let funds = vec![coin(1_000u128, "uruji")];

        app.init_modules(|router, _, storage| {
            router.bank.init_balance(storage, &owner, funds.clone())
        })
        .unwrap();

        set_block_time(&mut app, 0);

        let code = Box::new(
            ContractWrapper::new(execute, instantiate, query)
                .with_reply(reply)
                .with_sudo(sudo),
        );
        let code_id = app.store_code(code);
        let failing_code_id = app.store_code(mock_failing_contract());
        let contract = app
            .instantiate_contract(
                code_id,
                owner.clone(),
                &InstantiateMsg {
                    owner: owner.to_string(),
                    target_denoms: vec![("uruji".to_string(), Uint128::from(1_000u128))],
                    target_addresses: vec![(fees.to_string(), 1)],
                    executor: owner.to_string(),
                },
                &[],
                "revenue",
                None,
            )
            .unwrap();
        let failing = app
            .instantiate_contract(
                failing_code_id,
                owner.clone(),
                &Empty::default(),
                &[],
                "failing",
                None,
            )
            .unwrap();

        set_action(
            &mut app,
            &contract,
            "uruji",
            &failing,
            Uint128::zero(),
            Uint128::MAX,
        );

        app.send_tokens(owner.clone(), contract.clone(), &funds)
            .unwrap();

        set_block_time(&mut app, 2);

        let res = app
            .execute_contract(owner, contract.clone(), &ExecuteMsg::Run {}, &[])
            .unwrap();

        assert_run_event(&res, "uruji");
        assert_eq!(
            app.wrap().query_balance(fees, "uruji").unwrap().amount,
            Uint128::from(1_000u128)
        );
        assert_eq!(
            app.wrap().query_balance(contract, "uruji").unwrap().amount,
            Uint128::zero()
        );
    }

    #[test]
    fn distribution_clamps_to_elapsed_limit() {
        let mut app = BasicApp::default();
        let fees = app.api().addr_make("fees");
        let owner = app.api().addr_make("owner");
        let another = app.api().addr_make("another");

        let funds = vec![coin(1_000u128, "uruji")];

        app.init_modules(|router, _, storage| {
            router.bank.init_balance(storage, &owner, funds.clone())
        })
        .unwrap();

        set_block_time(&mut app, 0);

        let code = Box::new(
            ContractWrapper::new(execute, instantiate, query)
                .with_reply(reply)
                .with_sudo(sudo),
        );
        let code_id = app.store_code(code);
        let contract = app
            .instantiate_contract(
                code_id,
                owner.clone(),
                &InstantiateMsg {
                    owner: owner.to_string(),
                    target_denoms: vec![("uruji".to_string(), Uint128::from(100u128))],
                    target_addresses: vec![(fees.to_string(), 1), (another.to_string(), 3)],
                    executor: owner.to_string(),
                },
                &[],
                "revenue",
                None,
            )
            .unwrap();

        app.send_tokens(owner.clone(), contract.clone(), &funds)
            .unwrap();

        set_block_time(&mut app, 2);

        app.execute_contract(owner.clone(), contract.clone(), &ExecuteMsg::Run {}, &[])
            .unwrap();

        assert_eq!(
            app.wrap()
                .query_balance(fees.clone(), "uruji")
                .unwrap()
                .amount,
            Uint128::from(50u128)
        );

        assert_eq!(
            app.wrap()
                .query_balance(another.clone(), "uruji")
                .unwrap()
                .amount,
            Uint128::from(150u128)
        );

        assert_eq!(
            app.wrap().query_balance(contract, "uruji").unwrap().amount,
            Uint128::from(800u128)
        );
    }

    #[test]
    fn distribution_without_clamp_when_under_limit() {
        let mut app = BasicApp::default();
        let fees = app.api().addr_make("fees");
        let owner = app.api().addr_make("owner");
        let another = app.api().addr_make("another");

        let funds = vec![
            coin(1000u128, "uruji"),
            coin(2000u128, "another"),
            coin(2000u128, "ignored"),
        ];

        app.init_modules(|router, _, storage| {
            router.bank.init_balance(storage, &owner, funds.clone())
        })
        .unwrap();

        set_block_time(&mut app, 0);

        let code = Box::new(
            ContractWrapper::new(execute, instantiate, query)
                .with_reply(reply)
                .with_sudo(sudo),
        );
        let code_id = app.store_code(code);
        let contract = app
            .instantiate_contract(
                code_id,
                owner.clone(),
                &InstantiateMsg {
                    owner: owner.to_string(),
                    target_denoms: vec![
                        ("uruji".to_string(), Uint128::from(1_000u128)),
                        ("another".to_string(), Uint128::from(1_000u128)),
                    ],
                    target_addresses: vec![
                        (fees.to_string(), 1),
                        (another.to_string(), 3),
                        (app.api().addr_make("nope").to_string(), 0),
                    ],
                    executor: owner.to_string(),
                },
                &[],
                "revenue",
                None,
            )
            .unwrap();

        app.send_tokens(owner.clone(), contract.clone(), &funds)
            .unwrap();

        set_block_time(&mut app, 2);

        app.execute_contract(owner.clone(), contract.clone(), &ExecuteMsg::Run {}, &[])
            .unwrap();

        assert_eq!(
            app.wrap()
                .query_balance(fees.clone(), "uruji")
                .unwrap()
                .amount,
            Uint128::from(250u128)
        );

        assert_eq!(
            app.wrap()
                .query_balance(another.clone(), "uruji")
                .unwrap()
                .amount,
            Uint128::from(750u128)
        );

        assert_eq!(
            app.wrap()
                .query_balance(another.clone(), "ignored")
                .unwrap()
                .amount,
            Uint128::zero()
        );

        assert_eq!(
            app.wrap()
                .query_balance(fees.clone(), "another")
                .unwrap()
                .amount,
            Uint128::from(500u128)
        );

        assert_eq!(
            app.wrap()
                .query_balance(another.clone(), "another")
                .unwrap()
                .amount,
            Uint128::from(1500u128)
        );

        assert_eq!(
            app.wrap()
                .query_balance(contract.clone(), "uruji")
                .unwrap()
                .amount,
            Uint128::zero()
        );

        app.wasm_sudo(
            contract.clone(),
            &SudoMsg::SetTargetDenom(("ignored".to_string(), Uint128::from(1_000u128))),
        )
        .unwrap();
        set_block_time(&mut app, 100);

        app.execute_contract(owner.clone(), contract.clone(), &ExecuteMsg::Run {}, &[])
            .unwrap();

        assert_eq!(
            app.wrap().query_balance(fees, "ignored").unwrap().amount,
            Uint128::from(500u128)
        );

        assert_eq!(
            app.wrap().query_balance(another, "ignored").unwrap().amount,
            Uint128::from(1500u128)
        );

        assert_eq!(
            app.wrap()
                .query_balance(contract, "ignored")
                .unwrap()
                .amount,
            Uint128::zero()
        );
    }
}
