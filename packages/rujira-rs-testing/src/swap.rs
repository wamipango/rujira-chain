use cosmwasm_schema::cw_serde;
use cosmwasm_std::{
    to_json_binary, BankMsg, Binary, Coin, CosmosMsg, Deps, DepsMut, Empty, Env, MessageInfo,
    Response, StdError, StdResult,
};
use cw_multi_test::ContractWrapper;
use rujira_rs::CallbackData;

#[cw_serde]
pub struct MsgSwap {
    pub min_return: Coin,
    pub return_funds: bool,
    /// Bare bank send to the caller, dispatched before the callback and carrying no callback
    /// funds. Models revenue landing in the same tx as the conversion.
    pub send_direct: Option<Coin>,
}

fn instantiate(_deps: DepsMut, _env: Env, _info: MessageInfo, _msg: ()) -> StdResult<Response> {
    Ok(Response::default())
}

fn execute(_deps: DepsMut, _env: Env, info: MessageInfo, msg: MsgSwap) -> StdResult<Response> {
    let mut payout = vec![msg.min_return];
    if msg.return_funds {
        payout.extend(info.funds);
    }
    let mut res = Response::default();
    if let Some(coin) = msg.send_direct {
        res = res.add_message(BankMsg::Send {
            to_address: info.sender.to_string(),
            amount: vec![coin],
        });
    }
    let cb: CosmosMsg = CallbackData(to_json_binary(&Empty {})?)
        .to_message::<Empty>(&info.sender, Empty {}, payout)?
        .into();
    Ok(res.add_message(cb))
}

fn query(_deps: Deps, _env: Env, _msg: ()) -> StdResult<Binary> {
    Ok(Binary::new(vec![0]))
}

pub fn mock_swap_contract() -> Box<ContractWrapper<MsgSwap, (), (), StdError, StdError, StdError>> {
    Box::new(ContractWrapper::new(execute, instantiate, query))
}
