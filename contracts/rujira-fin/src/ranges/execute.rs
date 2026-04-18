use cosmwasm_std::{coin, ensure, Addr, BankMsg, Decimal, DepsMut, Env, MessageInfo, Response};
use cw_utils::NativeBalance;
use rujira_rs::{
    fin::RangeMsg,
    utils::{can_pay, split_floor},
};
use std::ops::Div;

use crate::{config::CONFIG, ranges::events::event_deposit_range, ContractError};

use super::{
    events::{
        event_claim_range, event_close_range, event_create_range, event_transfer_range,
        event_withdraw_range,
    },
    Range,
};

pub fn execute_range(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    sender: Addr,
    msg: RangeMsg,
    mid_price: Option<Decimal>,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    match msg {
        RangeMsg::Create {
            config: range_config,
            slippage,
        } => {
            let base = can_pay(&info, config.denoms.base());
            let quote = can_pay(&info, config.denoms.quote());
            ensure!(
                !base.is_zero() || !quote.is_zero(),
                ContractError::Invalid("empty funds".into())
            );
            let base = Decimal::from_atomics(base, 0)?;
            let quote = Decimal::from_atomics(quote, 0)?;
            let mut response = Response::default();
            let range = match mid_price {
                Some(price) => {
                    let ((base, quote), (ref_base, ref_quote)) =
                        Range::balance(&range_config, price, base, quote);
                    let mut refund = NativeBalance(vec![
                        coin(split_floor(ref_base).0.u128(), config.denoms.base()),
                        coin(split_floor(ref_quote).0.u128(), config.denoms.quote()),
                    ]);
                    refund.normalize();
                    if !refund.is_empty() {
                        response = response.add_message(BankMsg::Send {
                            to_address: sender.to_string(),
                            amount: refund.into_vec(),
                        })
                    };

                    Range::new(
                        deps.storage,
                        &config.tick,
                        &config.fee_range,
                        sender,
                        range_config,
                        (base, quote),
                    )?
                }
                None => Range::new(
                    deps.storage,
                    &config.tick,
                    &config.fee_range,
                    sender,
                    range_config,
                    (base, quote),
                )?,
            };
            if let Some((price, slippage)) = slippage {
                let diff = if range.price >= price {
                    range.price - price
                } else {
                    price - range.price
                };

                ensure!(
                    diff.div(price) <= slippage,
                    ContractError::SlippageExceeded(range.price)
                );
            }

            range.validate(&config.tick)?;
            range.save(deps.storage)?;
            Ok(response.add_event(event_create_range(&range)))
        }
        RangeMsg::Claim { idx } => {
            let mut range =
                Range::load(deps.storage, idx).map_err(|_| ContractError::NotFound {})?;
            ensure!(sender == range.owner, ContractError::Unauthorized {});

            let payout = range.collect(&config.denoms);
            let event = event_claim_range(&range, &config.denoms, payout.as_ref());
            match payout {
                Some(payout) => {
                    range.save(deps.storage)?;
                    Ok(Response::default()
                        .add_event(event)
                        .add_message(BankMsg::Send {
                            to_address: sender.to_string(),
                            amount: payout.into_vec(),
                        }))
                }
                _ => Ok(Response::default().add_event(event)),
            }
        }
        RangeMsg::Deposit { idx } => {
            let base = can_pay(&info, config.denoms.base());
            let quote = can_pay(&info, config.denoms.quote());
            let base_dec = Decimal::from_atomics(base, 0)?;
            let quote_dec = Decimal::from_atomics(quote, 0)?;

            ensure!(
                !base.is_zero() || !quote.is_zero(),
                ContractError::Invalid("empty funds".into())
            );

            let mut range =
                Range::load(deps.storage, idx).map_err(|_| ContractError::NotFound {})?;
            ensure!(sender == range.owner, ContractError::Unauthorized {});

            range.deposit(base_dec, quote_dec);
            range.reprice(&config.tick, &config.fee_range);
            range.save(deps.storage)?;
            Ok(Response::default().add_event(event_deposit_range(&range, base, quote)))
        }
        RangeMsg::Withdraw { idx, amount } => {
            let mut range =
                Range::load(deps.storage, idx).map_err(|_| ContractError::NotFound {})?;
            ensure!(sender == range.owner, ContractError::Unauthorized {});
            ensure!(
                amount <= Decimal::one(),
                ContractError::Invalid("amount > 1".into())
            );

            let payout = range.withdraw(&config.denoms, amount);
            range.reprice(&config.tick, &config.fee_range);
            range.save(deps.storage)?;
            let event = event_withdraw_range(&range, amount, &config.denoms, payout.as_ref());
            match payout {
                Some(payout) => {
                    Ok(Response::default()
                        .add_event(event)
                        .add_message(BankMsg::Send {
                            to_address: sender.to_string(),
                            amount: payout.into_vec(),
                        }))
                }
                _ => Ok(Response::default().add_event(event)),
            }
        }
        RangeMsg::Close { idx } => {
            let mut range =
                Range::load(deps.storage, idx).map_err(|_| ContractError::NotFound {})?;
            ensure!(sender == range.owner, ContractError::Unauthorized {});

            let event = event_close_range(&range);
            let payout = range.close(&config.denoms);
            Range::map().remove(deps.storage, idx.u128())?;

            match payout {
                Some(payout) => {
                    Ok(Response::default()
                        .add_event(event)
                        .add_message(BankMsg::Send {
                            to_address: sender.to_string(),
                            amount: payout.into_vec(),
                        }))
                }
                _ => Ok(Response::default().add_event(event)),
            }
        }
        RangeMsg::Transfer { idx, to } => {
            let mut range =
                Range::load(deps.storage, idx).map_err(|_| ContractError::NotFound {})?;

            ensure!(sender == range.owner, ContractError::Unauthorized {});
            let from = range.owner.clone();
            let to = deps.api.addr_validate(&to)?;
            range.owner = to.clone();
            range.save(deps.storage)?;
            Ok(Response::default().add_event(event_transfer_range(&range, &from, &to)))
        }
    }
}
