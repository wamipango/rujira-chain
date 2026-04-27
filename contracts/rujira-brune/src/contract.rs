use cosmwasm_schema::cw_serde;
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    ensure_eq, from_json, to_json_binary, Binary, Decimal, Deps, DepsMut, Env, MessageInfo, Reply,
    Response,
};
use cw2::set_contract_version;
use cw_utils::must_pay;
use rujira_rs::bow::QuoteResponse;
use rujira_rs::brune::{
    ConfigResponse, ExecuteMsg, InstantiateMsg, NodesResponse, QueryMsg, RevenueResponse,
    StateResponse, SudoMsg,
};
use rujira_rs::TokenFactory;

use crate::config::{Config, RUNE};
use crate::error::ContractError;
use crate::events::{event_node_deregister, event_node_register, query_events};
use crate::state::State;

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
    let token = TokenFactory::new(&env, &msg.token_id).create_msg(msg.token_metadata.clone());
    let config = Config::new(deps.as_ref(), &msg)?;
    config.validate()?;
    config.save(deps.storage)?;
    State::init(deps.storage)?;
    Ok(Response::default().add_message(token))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    let config = Config::load(deps.storage)?;
    let mut state = State::load(deps.storage)?;
    match msg {
        ExecuteMsg::Swap { min_return, .. } => {
            ensure_eq!(
                info.sender,
                config.fin_contract,
                ContractError::Unauthorized {}
            );
            let rune = Decimal::from_ratio(must_pay(&info, RUNE).unwrap_or_default(), 1u128);
            let mut response = Response::default();
            response = state.swap(
                deps.as_ref(),
                &env,
                &info,
                &config,
                &min_return,
                response,
                &state.nodes(deps.as_ref(), &env, &config)?.bonded(),
                &rune,
            )?;
            // Only required to store to the Event Log. Safe to `branch`
            response = state.allocate(deps.branch(), &env, &config, response, Some(rune))?;
            state.save(deps.storage)?;
            Ok(response)
        }
        ExecuteMsg::ProcessNode { addr } => {
            if !config.permissionless {
                return Err(ContractError::Unauthorized {});
            }
            process_node(deps, env, &config, &addr)
        }
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn sudo(deps: DepsMut, env: Env, msg: SudoMsg) -> Result<Response, ContractError> {
    let mut config = Config::load(deps.storage)?;
    match msg {
        SudoMsg::ProcessNode { addr } => process_node(deps, env, &config, &addr),
        SudoMsg::UpdateConfig(update) => {
            config.update(deps.as_ref(), update)?;
            config.validate()?;
            config.save(deps.storage)?;
            Ok(Response::default())
        }
    }
}

fn process_node(
    deps: DepsMut,
    env: Env,
    config: &Config,
    addr: &str,
) -> Result<Response, ContractError> {
    let mut state = State::load(deps.storage)?;
    let addr = deps.api.addr_validate(addr)?;
    let res = state.register(deps.as_ref(), &env, config, &addr)?;
    state.save(deps.storage)?;
    match res {
        Some(node) => {
            Ok(Response::default().add_event(event_node_register(deps.storage, &env, &node.addr)?))
        }
        None => {
            Ok(Response::default().add_event(event_node_deregister(deps.storage, &env, &addr)?))
        }
    }
}

/// Skip re-loading all nodes and state every iteration in Quote
#[cw_serde]
struct StoreCache {
    pub config: Config,
    pub state: State,
    pub balance: Decimal,
    pub bonded: Decimal,
    pub quote: Option<Binary>,
}

pub fn rune_balance(deps: &Deps, env: &Env) -> Result<Decimal, ContractError> {
    Ok(Decimal::from_atomics(
        deps.querier
            .query_balance(&env.contract.address, RUNE)?
            .amount,
        0,
    )?)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> Result<Binary, ContractError> {
    match msg {
        QueryMsg::Quote(req) => {
            let cache = match req.data.clone().map(from_json).transpose()? {
                Some(v) => v,
                None => {
                    let state = State::load(deps.storage)?;
                    let config = Config::load(deps.storage)?;
                    StoreCache {
                        bonded: state.nodes(deps, &env, &config)?.bonded(),
                        quote: None,
                        balance: rune_balance(&deps, &env)?,
                        config,
                        state,
                    }
                }
            };
            let res = cache.state.quote(
                deps,
                &env,
                &cache.config,
                &cache.balance,
                &cache.bonded,
                &cache.quote,
                (req.ask_denom, req.offer_denom),
            )?;
            Ok(to_json_binary(&QuoteResponse {
                data: Some(to_json_binary(&StoreCache {
                    quote: res.data,
                    ..cache
                })?),
                ..res
            })?)
        }
        QueryMsg::Events { start_after, limit } => {
            Ok(to_json_binary(&rujira_rs::brune::EventLogResponse {
                events: query_events(deps.storage, start_after, limit)?,
            })?)
        }
        QueryMsg::Config {} => Ok(to_json_binary(&ConfigResponse::from(Config::load(
            deps.storage,
        )?))?),
        QueryMsg::State {} => {
            let config = Config::load(deps.storage)?;
            let state = State::load(deps.storage)?;

            Ok(to_json_binary(&StateResponse {
                minted: state.minted,
                nodes: NodesResponse::from(state.nodes(deps, &env, &config)?),
                revenue: RevenueResponse::from(state.revenue),
            })?)
        }
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(_deps: DepsMut, _env: Env, _msg: Reply) -> Result<Response, ContractError> {
    // Replies only requested on MsgUnbond error, designed to be ignored
    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, _msg: ()) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    Ok(Response::default())
}

#[cfg(test)]
mod tests {

    use super::*;
    use cosmwasm_std::{coin, coins, Decimal, Event, Timestamp, Uint128};
    use cw_multi_test::{ContractWrapper, Executor};
    use rujira_rs::{
        bow,
        brune::RangeConfig,
        proto::types::{NodeBondProvider, NodeBondProviders, QueryNodeResponse},
        TokenMetadata,
    };
    use rujira_rs_testing::mock_rujira_app;

    fn range() -> RangeConfig {
        RangeConfig {
            high: Decimal::percent(99),
            low: Decimal::percent(80),
            skew: cosmwasm_std::SignedDecimal::zero(),
            step: Decimal::percent(1),
        }
    }

    #[test]
    fn instantiation() {
        let mut app = mock_rujira_app();
        let owner = app.api().addr_make("owner");
        let fin = app.api().addr_make("fin");
        let stake = app.api().addr_make("stake");
        app.init_modules(|router, _, storage| {
            router
                .bank
                .init_balance(storage, &fin, coins(1000000, "rune"))
        })
        .unwrap();

        let code = Box::new(ContractWrapper::new(execute, instantiate, query));
        let code_id = app.store_code(code);
        let contract = app
            .instantiate_contract(
                code_id,
                owner,
                &InstantiateMsg {
                    target_utilization: Decimal::percent(90),
                    min_node_fee: Decimal::percent(5),
                    range: range(),
                    permissionless: false,
                    stake_contract: stake.to_string(),
                    revenue_smear: 100000,
                    token_id: "brune".to_string(),
                    token_metadata: TokenMetadata {
                        description: "Some Description".to_string(),
                        display: "BRUNE".to_string(),
                        name: "BRUNE".to_string(),
                        symbol: "BRUNE".to_string(),
                        uri: None,
                        uri_hash: None,
                    },
                    fin_contract: fin.to_string(),
                    max_bond: Uint128::new(10000),
                    mint_cap: Uint128::new(1_000_000),
                },
                &[],
                "template",
                None,
            )
            .unwrap();

        let config: ConfigResponse = app
            .wrap()
            .query_wasm_smart(contract.clone(), &QueryMsg::Config {})
            .unwrap();

        let state: StateResponse = app
            .wrap()
            .query_wasm_smart(contract.clone(), &QueryMsg::State {})
            .unwrap();

        assert_eq!(
            config,
            ConfigResponse {
                target_utilization: Decimal::percent(90),
                min_node_fee: Decimal::percent(5),
                range: range(),
                permissionless: false,
                fin_contract: fin.to_string(),
                stake_contract: stake.to_string(),
                revenue_smear: 100000,
                token_id: "brune".to_string(),
                max_bond: Uint128::new(10000),
                mint_cap: Uint128::new(1_000_000),
            }
        );
        assert_eq!(
            state,
            StateResponse {
                minted: Decimal::zero(),
                nodes: NodesResponse {
                    bond: Decimal::zero(),
                    weight: Decimal::zero(),
                    capacity: Decimal::zero(),
                    nodes: vec![],
                },
                revenue: RevenueResponse {
                    pending: Decimal::zero(),
                    fee_rate: Decimal::zero(),
                    timestamp: Timestamp::from_seconds(0),
                },
            }
        );

        app.execute_contract(
            fin,
            contract.clone(),
            &ExecuteMsg::Swap {
                min_return: coin(100, "x/brune"),
                to: None,
                callback: None,
            },
            &coins(100, "rune"),
        )
        .unwrap_err();
    }

    #[test]
    fn sudo_whitelists_node() {
        let mut app = mock_rujira_app();
        let owner = app.api().addr_make("owner");
        let fin = app.api().addr_make("fin");
        let stake = app.api().addr_make("stake");
        app.init_modules(|router, _, storage| {
            router
                .bank
                .init_balance(storage, &fin, coins(1000000, "rune"))
        })
        .unwrap();

        let code = Box::new(ContractWrapper::new(execute, instantiate, query).with_sudo(sudo));
        let code_id = app.store_code(code);
        let contract = app
            .instantiate_contract(
                code_id,
                owner,
                &InstantiateMsg {
                    target_utilization: Decimal::percent(90),
                    min_node_fee: Decimal::percent(5),
                    range: range(),
                    permissionless: false,
                    stake_contract: stake.to_string(),
                    revenue_smear: 100000,
                    token_id: "brune".to_string(),
                    token_metadata: TokenMetadata {
                        description: "Some Description".to_string(),
                        display: "BRUNE".to_string(),
                        name: "BRUNE".to_string(),
                        symbol: "BRUNE".to_string(),
                        uri: None,
                        uri_hash: None,
                    },
                    fin_contract: fin.to_string(),
                    max_bond: Uint128::new(10000),
                    mint_cap: Uint128::new(1_000_000),
                },
                &[],
                "template",
                None,
            )
            .unwrap();

        let node = app.api().addr_make("node");
        let node_response = QueryNodeResponse {
            node_address: node.to_string(),
            status: "Active".to_string(),
            bond_providers: Some(NodeBondProviders {
                node_operator_fee: "500".to_string(),
                providers: vec![NodeBondProvider {
                    bond_address: contract.to_string(),
                    bond: "0".to_string(),
                }],
            }),
            ..QueryNodeResponse::default()
        };

        app.init_modules(|router, _, _| {
            router.stargate.with_node(node.clone(), node_response);
        });

        app.wasm_sudo(
            contract.clone(),
            &SudoMsg::ProcessNode {
                addr: node.to_string(),
            },
        )
        .unwrap();

        let state: StateResponse = app
            .wrap()
            .query_wasm_smart(contract.clone(), &QueryMsg::State {})
            .unwrap();

        assert_eq!(state.nodes.nodes.len(), 1);
        assert_eq!(state.nodes.nodes[0].addr, node.to_string());
        assert!(!state.nodes.nodes[0].is_leaving);

        let res = app
            .execute_contract(
                fin.clone(),
                contract.clone(),
                &ExecuteMsg::Swap {
                    min_return: coin(100, "x/brune"),
                    to: None,
                    callback: None,
                },
                &coins(100, "rune"),
            )
            .unwrap();

        let state: StateResponse = app
            .wrap()
            .query_wasm_smart(contract.clone(), &QueryMsg::State {})
            .unwrap();

        res.assert_event(&Event::new("mint").add_attributes(vec![
            ("amount", "100"),
            ("denom", "x/brune"),
            ("recipient", fin.as_str()),
        ]));

        assert_eq!(state.nodes.nodes[0].node.status, "Active");
        assert_eq!(
            state.nodes.nodes[0]
                .node
                .bond_providers
                .clone()
                .unwrap()
                .providers[0]
                .bond,
            "90"
        );
        assert_eq!(
            state.nodes.nodes[0]
                .node
                .bond_providers
                .clone()
                .unwrap()
                .providers[0]
                .bond_address,
            contract.to_string()
        );

        let res: bow::QuoteResponse = app
            .wrap()
            .query_wasm_smart(
                contract.clone(),
                &QueryMsg::Quote(bow::QuoteRequest {
                    min_price: None,
                    offer_denom: RUNE.to_owned(),
                    ask_denom: "x/brune".to_string(),
                    data: None,
                }),
            )
            .unwrap();

        let res = from_json::<StoreCache>(res.data.unwrap()).unwrap();
        assert_eq!(res.bonded, Decimal::percent(9000));
    }
}
