use cosmwasm_std::{Addr, BankMsg, CosmosMsg, DepsMut, Empty, Response};
use cw_utils::NativeBalance;
use rujira_rs::fin::OrderTarget;
use rujira_rs::{CallbackData, Premiumable};

use crate::swap_iter::SwapIter;
use crate::{config::Config, ContractError};

use super::OrderManager;

pub fn execute_order(
    deps: DepsMut,
    manager: &mut OrderManager,
    recipient: Addr,
    orders: Vec<OrderTarget>,
    callback: Option<CallbackData>,
    config: &Config,
    oracle: &Option<impl Premiumable>,
) -> Result<Response, ContractError> {
    let swap_iter = SwapIter::new(deps.querier, config);

    let mut messages: Vec<CosmosMsg> = vec![];
    let mut fees = NativeBalance::default();

    let res = manager.execute_orders(deps.storage, &swap_iter, orders, oracle)?;
    fees += res.fees;

    if !res.withdraw.is_empty() {
        match callback {
            None => messages.push(CosmosMsg::Bank(BankMsg::Send {
                to_address: recipient.to_string(),
                amount: res.withdraw.into_vec(),
            })),
            Some(cb) => messages.push(
                cb.to_message(&recipient, Empty {}, res.withdraw.into_vec())?
                    .into(),
            ),
        }
    }

    fees.normalize();

    if !fees.is_empty() {
        messages.push(CosmosMsg::Bank(BankMsg::Send {
            to_address: config.fee_address.to_string(),
            amount: fees.into_vec(),
        }))
    }

    Ok(res.response.add_messages(messages))
}
