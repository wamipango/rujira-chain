use std::cmp::min;
use std::ops::{Add, Sub};

#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    coin, coins, ensure, to_json_binary, Addr, BankMsg, Binary, Coin, Deps, DepsMut, Env,
    MessageInfo, Order, Response, StdResult, Uint128, WasmMsg,
};
use cw2::{ensure_from_older_version, set_contract_version};
use cw_storage_plus::Map;
use cw_utils::one_coin;
use rujira_rs::query::SwapQuoteQuery;
use rujira_rs::thorchain_swap::{
    ConfigResponse, ExecuteMsg, InstantiateMsg, MarketsResponse, QueryMsg, SudoMsg, Vault,
    VaultResponse, VaultsResponse,
};
use rujira_rs::Asset;

use crate::config::Config;
use crate::error::ContractError;
use crate::events::event_swap;
use crate::quote::{build_route, QuoteState};

const CONTRACT_NAME: &str = env!("CARGO_PKG_NAME");
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");
pub static VAULTS: Map<String, Vault> = Map::new("vaults");
pub static MARKETS: Map<Addr, bool> = Map::new("markets");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    let config = Config::new(msg, deps.api)?;
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
    match msg {
        ExecuteMsg::Repay {} => {
            Ok(Response::default().add_messages(execute_repayments(deps.as_ref(), &env, None)?))
        }
        ExecuteMsg::Swap { min_return, to, .. } => {
            let funds = one_coin(&info)?;
            ensure!(
                MARKETS
                    .load(deps.storage, info.sender.clone())
                    .map_err(|_| ContractError::Unauthorized {})?,
                ContractError::Unauthorized {}
            );
            // Validate that the base layer swap will return sufficient funds
            let config = Config::load(deps.storage)?;
            let from_asset = Asset::from_denom(&funds.denom)?;
            let to_asset = Asset::from_denom(&min_return.denom)?;
            let to = to.unwrap_or(info.sender.to_string());
            let vault = VAULTS.load(deps.storage, min_return.denom.clone())?;

            // We just always send min_return back to fin, as all accounting logic there expects this to be the case
            let send_msg = BankMsg::Send {
                to_address: to.clone(),
                amount: vec![min_return.clone()],
            };

            // For very small return amounts, skip quote verification as it fails
            // on zero emit asset.
            // The reserve_fee should cover this edge case as well.
            if min_return.amount < Uint128::new(10) {
                return Ok(Response::default()
                    .add_event(event_swap(
                        &funds,
                        &coin(0u128, min_return.denom.clone()),
                        &min_return,
                        &coin(0u128, min_return.denom.clone()),
                        &coin(0u128, min_return.denom.clone()),
                        &coin(min_return.amount.u128(), min_return.denom.clone()),
                        &format!("%%skipped%%:{} < 100", min_return.amount),
                    ))
                    .add_messages(execute_repayments(deps.as_ref(), &env, Some(funds))?)
                    .add_message(vault.borrow_msg(min_return.amount)?)
                    .add_message(send_msg));
            }
            // CLP verification — same formula used for quoting
            let route = build_route(deps.querier, &funds.denom, &min_return.denom)?;
            let clp_return = route.swap(funds.amount);
            let reserve_fee = clp_return.mul_ceil(config.reserve_fee);
            // Floor, not ceil: the quote prices fees against a large size_q
            // where mul_ceil ≈ smooth rate, but clp_return here can be tiny,
            // where mul_ceil is a lumpy step that overcharges per-unit.
            let amm_fee = clp_return.mul_floor(config.fee.0);
            let required = clp_return.sub(amm_fee);
            ensure!(
                required >= min_return.amount,
                ContractError::InsufficientReturn {
                    quoted: clp_return,
                    asked: min_return.amount,
                    liquidity: Uint128::zero(),
                    outbound: Uint128::zero(),
                    fee: amm_fee
                }
            );

            let quote = SwapQuoteQuery {
                from_asset,
                to_asset,
                amount: funds.amount,
                streaming: Some((1, config.max_stream_length)),
                destination: env.contract.address.to_string(),
                tolerance_bps: None,
                refund_address: Some(env.contract.address.to_string()),
                affiliates: vec![],
            }
            .quote(deps.querier)?;

            let memo = quote.memo.clone();
            let signer = deps.api.addr_canonicalize(env.contract.address.as_str())?;
            let swap_msg = quote.to_msg(signer);
            let fee_msg = if amm_fee.is_zero() {
                vec![]
            } else {
                vec![BankMsg::Send {
                    to_address: config.fee.1.to_string(),
                    amount: coins(amm_fee.u128(), min_return.denom.clone()),
                }]
            };
            let mut response = Response::default().add_event(event_swap(
                &funds,
                &coin(clp_return.u128(), min_return.denom.clone()),
                &min_return,
                &coin(reserve_fee.u128(), min_return.denom.clone()),
                &coin(amm_fee.u128(), min_return.denom.clone()),
                &min_return,
                &memo,
            ));

            let required_amount = min_return.amount.add(amm_fee).saturating_sub(
                deps.querier
                    .query_balance(&env.contract.address, &min_return.denom)?
                    .amount,
            );

            if !required_amount.is_zero() {
                response = response
                    .add_message(vault.borrow_msg(required_amount.max(config.min_borrow_amount))?);
            };

            Ok(response
                .add_message(swap_msg)
                .add_message(send_msg)
                .add_messages(fee_msg)
                .add_message(WasmMsg::Execute {
                    contract_addr: env.contract.address.to_string(),
                    msg: to_json_binary(&ExecuteMsg::Repay {})?,
                    funds: vec![],
                }))
        }
    }
}

fn execute_repayments(deps: Deps, env: &Env, funds: Option<Coin>) -> StdResult<Vec<WasmMsg>> {
    // Execute repayments first
    // Deprecated but the only other options are a) explicit list to repay or iterate over vaults every time, both of which are sub optimal
    #[allow(deprecated)]
    let balances = deps.querier.query_all_balances(&env.contract.address)?;
    let mut repay_msgs: Vec<WasmMsg> = vec![];
    for balance in balances {
        let amount = match funds.clone() {
            Some(coin) => {
                if balance.denom == coin.denom {
                    balance.amount - coin.amount
                } else {
                    balance.amount
                }
            }
            None => balance.amount,
        };

        match VAULTS.load(deps.storage, balance.denom.clone()) {
            Ok(vault) => {
                let debt = vault.debt(deps.querier, &env.contract.address)?;
                let repay = min(debt, amount);
                // Ghost Vault has truncation issues when calculating repaid shares
                // TODO: Add a min_repay to ghost vault to protect against small values
                if repay.gt(&Uint128::from(1000u128)) {
                    repay_msgs.push(vault.repay_msg(coin(repay.u128(), balance.denom))?)
                }
            }
            // Prevent execution from halting if random tokens get sent to the contract
            Err(_) => continue,
        }
    }
    Ok(repay_msgs)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn sudo(deps: DepsMut, _env: Env, msg: SudoMsg) -> Result<Response, ContractError> {
    let mut config = Config::load(deps.storage)?;
    match msg {
        SudoMsg::SetConfig(update) => {
            config.update(&update, deps.api)?;
            config.validate()?;
            config.save(deps.storage)?;
            Ok(Response::default())
        }
        SudoMsg::SetMarket { addr, enabled } => {
            let k = deps.api.addr_validate(&addr)?;
            MARKETS.save(deps.storage, k, &enabled)?;
            Ok(Response::default())
        }
        SudoMsg::SetVault { denom, vault } => {
            match vault {
                Some(vault) => VAULTS.save(deps.storage, denom, &vault)?,
                None => VAULTS.remove(deps.storage, denom),
            }
            Ok(Response::default())
        }
        SudoMsg::Execute(cosmos_msg) => Ok(Response::default().add_message(cosmos_msg)),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> Result<Binary, ContractError> {
    let config = Config::load(deps.storage)?;
    match msg {
        QueryMsg::Config {} => Ok(to_json_binary(&ConfigResponse::from(config))?),
        QueryMsg::Quote(req) => {
            let vault = VAULTS.load(deps.storage, req.ask_denom.clone())?;
            let mut state = match req.data {
                Some(data) => QuoteState::decode(&data)?,
                None => {
                    let borrow_limit = vault.available(deps.querier, &env.contract.address)?;

                    QuoteState::load(
                        deps.querier,
                        &req.offer_denom,
                        &req.ask_denom,
                        &config,
                        borrow_limit,
                    )?
                }
            };
            // Clamp the borrow limit to the smaller of vault available and the pool percentage cap

            Ok(to_json_binary(&state.quote()?)?)
        }
        QueryMsg::Markets {} => Ok(to_json_binary(&MarketsResponse {
            markets: MARKETS
                .range(deps.storage, None, None, Order::Ascending)
                .fold(vec![], |mut agg, x| match x {
                    Ok((addr, true)) => {
                        agg.push(addr);
                        agg
                    }
                    _ => agg,
                }),
        })?),
        QueryMsg::Vaults {} => Ok(to_json_binary(&VaultsResponse {
            vaults: VAULTS
                .range(deps.storage, None, None, Order::Ascending)
                .map(|x| x.map(|(denom, vault)| VaultResponse { denom, vault }))
                .collect::<StdResult<Vec<VaultResponse>>>()?,
        })?),
    }
}
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, _msg: ()) -> Result<Response, ContractError> {
    ensure_from_older_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    let config = Config::load(deps.storage)?;
    config.validate()?;
    config.save(deps.storage)?;

    Ok(Response::default())
}

#[cfg(test)]
mod tests {

    use std::str::FromStr;

    use crate::quote::build_route;
    use crate::testing::setup;

    use super::*;
    use cosmwasm_schema::cw_serde;
    use cosmwasm_std::testing::{mock_dependencies, mock_env};
    use cosmwasm_std::{Decimal, Uint128};
    use cw2::{get_contract_version, set_contract_version};
    use cw_multi_test::{BasicApp, ContractWrapper, Executor};
    use cw_storage_plus::Item;
    use rujira_rs::fin::market_maker::{QuoteRequest, QuoteResponse};
    use rujira_rs::thorchain_swap::{ConfigUpdate, DEFAULT_SPREAD_BPS};
    use rujira_rs_testing::mock_rujira_app;

    #[cw_serde]
    struct LegacyConfig {
        max_stream_length: u32,
        stream_step_ratio: Decimal,
        max_borrow_ratio: Decimal,
        min_borrow_amount: Uint128,
        reserve_fee: Decimal,
        fee: (Decimal, Addr),
    }

    #[test]
    fn instantiation() {
        let mut app = BasicApp::default();
        let owner = app.api().addr_make("owner");

        let code = Box::new(ContractWrapper::new(execute, instantiate, query));
        let code_id = app.store_code(code);
        app.instantiate_contract(
            code_id,
            owner.clone(),
            &InstantiateMsg {
                max_stream_length: 1u32,
                max_borrow_ratio: Decimal::one(),
                reserve_fee: Decimal::from_ratio(10u128, 500u128),
                stream_step_ratio: Decimal::one(),
                spread_bps: rujira_rs::thorchain_swap::DEFAULT_SPREAD_BPS,
                min_borrow_amount: Uint128::zero(),
                fee: (Decimal::permille(15), owner.to_string()),
            },
            &[],
            "template",
            None,
        )
        .unwrap();
    }

    #[test]
    fn migration_adds_default_spread_bps() {
        let mut deps = mock_dependencies();
        let legacy_config = LegacyConfig {
            max_stream_length: 1,
            stream_step_ratio: Decimal::one(),
            max_borrow_ratio: Decimal::one(),
            min_borrow_amount: Uint128::zero(),
            reserve_fee: Decimal::zero(),
            fee: (Decimal::zero(), deps.api.addr_make("fee")),
        };
        Item::new("config")
            .save(deps.as_mut().storage, &legacy_config)
            .unwrap();
        set_contract_version(deps.as_mut().storage, CONTRACT_NAME, "1.1.2").unwrap();

        migrate(deps.as_mut(), mock_env(), ()).unwrap();

        let config = Config::load(deps.as_ref().storage).unwrap();
        assert_eq!(config.spread_bps, DEFAULT_SPREAD_BPS);
        let stored = deps.as_ref().storage.get(b"config").unwrap();
        assert!(String::from_utf8(stored)
            .unwrap()
            .contains("\"spread_bps\":10"));
        assert_eq!(
            get_contract_version(deps.as_ref().storage).unwrap().version,
            CONTRACT_VERSION
        );
    }

    #[test]
    fn quote() {
        let mut app = mock_rujira_app();
        let owner = app.api().addr_make("owner");
        let contract = setup(&mut app, &owner);
        // RUNE to B
        let quote_1: QuoteResponse = app
            .wrap()
            .query_wasm_smart(
                contract.clone(),
                &QueryMsg::Quote(QuoteRequest {
                    min_price: None,
                    offer_denom: "rune".to_string(),
                    ask_denom: "btc-btc".to_string(),
                    data: None,
                }),
            )
            .unwrap();

        assert_eq!(
            quote_1.price,
            Decimal::from_str("0.000057508882768629").unwrap()
        );
        assert_eq!(quote_1.size, Uint128::from(67424971u128));

        let quote_2: QuoteResponse = app
            .wrap()
            .query_wasm_smart(
                contract.clone(),
                &QueryMsg::Quote(QuoteRequest {
                    min_price: None,
                    offer_denom: "rune".to_string(),
                    ask_denom: "btc-btc".to_string(),
                    data: quote_1.data,
                }),
            )
            .unwrap();

        assert_eq!(
            quote_2.price,
            Decimal::from_str("0.000057279420308594").unwrap()
        );

        assert_eq!(quote_2.size, Uint128::from(67155943u128));

        assert!(quote_2.price < quote_1.price);

        // A to RUNE
        let quote_1: QuoteResponse = app
            .wrap()
            .query_wasm_smart(
                contract.clone(),
                &QueryMsg::Quote(QuoteRequest {
                    min_price: None,
                    offer_denom: "btc-btc".to_string(),
                    ask_denom: "rune".to_string(),
                    data: None,
                }),
            )
            .unwrap();

        assert_eq!(
            quote_1.price,
            Decimal::from_str("16796.86167491045539826").unwrap()
        );
        // The vault is only funded with 1000000000, so the offer should be clamped,
        // less reserve + amm fee from setup (0.02% + 1.5% = 1.52%)
        assert_eq!(quote_1.size, Uint128::from(984800000u128));
        // assert_eq!(quote.size, Uint128::from(136658118u128));

        // Out of borrowable funds, no more quotes
        app.wrap()
            .query_wasm_smart::<QuoteResponse>(
                contract.clone(),
                &QueryMsg::Quote(QuoteRequest {
                    min_price: None,
                    offer_denom: "btc-btc".to_string(),
                    ask_denom: "rune".to_string(),
                    data: quote_1.data,
                }),
            )
            .unwrap_err();

        // A to B
        let quote_1: QuoteResponse = app
            .wrap()
            .query_wasm_smart(
                contract.clone(),
                &QueryMsg::Quote(QuoteRequest {
                    min_price: None,
                    ask_denom: "btc-btc".to_string(),
                    offer_denom: "eth-usdc-0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48".to_string(),
                    data: None,
                }),
            )
            .unwrap();

        assert_eq!(
            quote_1.price,
            Decimal::from_str("0.000011708234911896").unwrap()
        );
        assert_eq!(quote_1.size, Uint128::from(12514468u128));

        let quote_2: QuoteResponse = app
            .wrap()
            .query_wasm_smart(
                contract.clone(),
                &QueryMsg::Quote(QuoteRequest {
                    min_price: None,
                    ask_denom: "btc-btc".to_string(),
                    offer_denom: "eth-usdc-0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48".to_string(),
                    data: quote_1.data,
                }),
            )
            .unwrap();

        assert_eq!(
            quote_2.price,
            Decimal::from_str("0.000011652897467382").unwrap()
        );

        assert_eq!(quote_2.size, Uint128::from(12455320u128));

        assert!(quote_2.price < quote_1.price);
    }

    #[test]
    fn spread_bps_configures_quote_width() {
        let mut app = mock_rujira_app();
        let owner = app.api().addr_make("owner");
        let contract = setup(&mut app, &owner);
        let request = || {
            QueryMsg::Quote(QuoteRequest {
                min_price: None,
                offer_denom: "rune".to_string(),
                ask_denom: "btc-btc".to_string(),
                data: None,
            })
        };

        let default_quote: QuoteResponse = app
            .wrap()
            .query_wasm_smart(contract.clone(), &request())
            .unwrap();

        app.wasm_sudo(
            contract.clone(),
            &SudoMsg::SetConfig(ConfigUpdate {
                max_stream_length: None,
                stream_step_ratio: None,
                spread_bps: Some(20),
                max_borrow_ratio: None,
                reserve_fee: None,
                min_borrow_amount: None,
                fee: None,
            }),
        )
        .unwrap();

        let wider_quote: QuoteResponse = app.wrap().query_wasm_smart(contract, &request()).unwrap();

        assert!(wider_quote.size > default_quote.size);
        assert!(wider_quote.price < default_quote.price);
    }

    /// Regression: a small offer whose CLP return lands in the range
    /// where mul_ceil(clp, amm_rate) rounds up to a per-unit fee well
    /// above amm_rate, producing "InsufficientReturn asked N quoted N fee 1".
    #[test]
    fn small_swap_rounding_error() {
        let mut app = mock_rujira_app();
        let owner = app.api().addr_make("owner");
        let contract = setup(&mut app, &owner);
        app.wasm_sudo(
            contract.clone(),
            &SudoMsg::SetMarket {
                addr: owner.to_string(),
                enabled: true,
            },
        )
        .unwrap();

        let route = build_route(app.wrap(), &"rune".to_string(), &"btc-btc".to_string()).unwrap();
        let offer_amount = 500_000u128;
        let clp_return = route.swap(Uint128::new(offer_amount));

        app.execute_contract(
            owner.clone(),
            contract.clone(),
            &ExecuteMsg::Swap {
                min_return: coin(clp_return.u128(), "btc-btc"),
                to: None,
                callback: None,
            },
            &coins(offer_amount, "rune"),
        )
        .unwrap();
    }
}
