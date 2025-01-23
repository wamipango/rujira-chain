#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    instantiate2_address, to_json_binary, Binary, Decimal, Deps, DepsMut, Env, MessageInfo,
    Response, Uint128, WasmMsg,
};
use cw2::set_contract_version;
use rujira_rs::ghost_registry::{
    ConfigResponse, ContractConfigResponse, ContractStatusResponse, ExecuteMsg, InstantiateMsg,
    QueryMsg, SudoMsg,
};
use rujira_rs::{ghost_borrow, ghost_vault};

use crate::config::Config;
use crate::error::ContractError;
use crate::state::ContractConfig;

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
    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    _deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    _msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn sudo(deps: DepsMut, env: Env, msg: SudoMsg) -> Result<Response, ContractError> {
    let mut config = Config::load(deps.storage)?;
    let salt = to_json_binary(&msg)?;
    match msg {
        SudoMsg::Create {
            vault_addr,
            vault_limit,
            orca_addr,
            collateral_denom,
            debt_denom,
        } => {
            let label = format!("rujira-ghost-borrow/{collateral_denom}-{debt_denom}");
            let contract = instantiate2_address(
                &config.checksum,
                &deps.api.addr_canonicalize(env.contract.address.as_str())?,
                &salt,
            )?;
            let contract = deps.api.addr_humanize(&contract)?;
            let init_msg = WasmMsg::Instantiate2 {
                admin: Some(env.contract.address.to_string()),
                code_id: config.code_id,
                label,
                msg: to_json_binary(&ghost_borrow::InstantiateMsg {
                    registry: env.contract.address.to_string(),
                })?,
                funds: vec![],
                salt,
            };
            let vault_msg = WasmMsg::Execute {
                contract_addr: deps.api.addr_validate(&vault_addr)?.to_string(),
                msg: to_json_binary(&ghost_vault::ExecuteMsg::Sudo(
                    ghost_vault::SudoMsg::AddBorrower {
                        addr: contract.to_string(),
                        borrow_limit: vault_limit,
                    },
                ))?,
                funds: vec![],
            };

            ContractConfig {
                contract,
                collateral_denom,
                debt_denom,
                vault_addr,
                orca_addr,
            }
            .save(deps.storage)?;
            Ok(Response::default()
                .add_message(init_msg)
                .add_message(vault_msg))
        }
        SudoMsg::UpdateConfig { code } => match code {
            Some((code_id, checksum)) => {
                config.code_id = code_id;
                config.checksum = checksum;
                config.save(deps.storage)?;
                Ok(Response::default())
            }
            None => Ok(Response::default()),
        },
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> Result<Binary, ContractError> {
    match msg {
        QueryMsg::Config {} => Ok(to_json_binary(&ConfigResponse::from(Config::load(
            deps.storage,
        )?))?),
        QueryMsg::Status {} => todo!(),
        QueryMsg::ContractConfig(addr) => Ok(to_json_binary(&ContractConfigResponse::from(
            ContractConfig::load(deps.storage, deps.api.addr_validate(&addr)?)?,
        ))?),
        QueryMsg::ContractStatus(addr) => {
            let config = ContractConfig::load(deps.storage, deps.api.addr_validate(&addr)?)?;
            Ok(to_json_binary(&ContractStatusResponse {
                contract: config.contract.to_string(),
                collateral_denom: config.collateral_denom,
                debt_denom: config.debt_denom,
                collateral_amount: Uint128::zero(),
                debt_amount: Uint128::zero(),
                borrow_rate: Decimal::zero(),
                borrow_limit: Uint128::zero(),
            })?)
        }
    }
}
