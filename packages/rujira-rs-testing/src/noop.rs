use cosmwasm_std::{
    to_json_binary, Binary, Deps, DepsMut, Empty, Env, MessageInfo, Response, StdError, StdResult,
};
use cw_multi_test::ContractWrapper;

fn instantiate(_deps: DepsMut, _env: Env, _info: MessageInfo, _msg: Empty) -> StdResult<Response> {
    Ok(Response::default())
}

fn execute(_deps: DepsMut, _env: Env, _info: MessageInfo, _msg: Empty) -> StdResult<Response> {
    Ok(Response::default())
}

fn query(_deps: Deps, _env: Env, _msg: Empty) -> StdResult<Binary> {
    to_json_binary(&Empty::default())
}

pub fn mock_noop_contract(
) -> Box<ContractWrapper<Empty, Empty, Empty, StdError, StdError, StdError>> {
    Box::new(ContractWrapper::new(execute, instantiate, query))
}

fn execute_failing(
    _deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    _msg: Empty,
) -> StdResult<Response> {
    Err(StdError::generic_err("mock failure"))
}

pub fn mock_failing_contract(
) -> Box<ContractWrapper<Empty, Empty, Empty, StdError, StdError, StdError>> {
    Box::new(ContractWrapper::new(execute_failing, instantiate, query))
}
