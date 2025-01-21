#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    coins, to_json_binary, BankMsg, Binary, Deps, DepsMut, Env, MessageInfo, Response,
};
use cw2::set_contract_version;
use cw_utils::{must_pay, PaymentError};
use rujira_rs::ghost_vault::{ExecuteMsg, InstantiateMsg, QueryMsg, SudoMsg};
use rujira_rs::TokenFactory;

use crate::borrowers::Borrower;
use crate::config::Config;
use crate::error::ContractError;
use crate::events::{event_borrow, event_deposit, event_repay, event_withdraw};
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
    let config = Config::from(msg.clone());
    config.validate()?;
    config.save(deps.storage)?;
    State::init(deps.storage, &env)?;
    let rcpt = TokenFactory::new(&env, format!("ghost-vault-{}-rcpt", config.denom).as_str());
    let debt = TokenFactory::new(&env, format!("ghost-vault-{}-debt", config.denom).as_str());

    Ok(Response::default()
        .add_message(rcpt.create_msg(msg.receipt))
        .add_message(debt.create_msg(msg.debt)))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    let config = Config::load(deps.storage)?;
    let mut state = State::load(deps.storage)?;
    let rcpt = TokenFactory::new(&env, format!("ghost-vault-{}-rcpt", config.denom).as_str());
    let debt = TokenFactory::new(&env, format!("ghost-vault-{}-debt", config.denom).as_str());
    state.distribute_interest(&env, &config)?;
    let response = match msg {
        ExecuteMsg::Deposit {} => {
            let amount = must_pay(&info, config.denom.as_str())?;
            let mint = state.deposit(amount)?;
            Response::default()
                .add_message(rcpt.mint_msg(mint, info.sender.clone()))
                .add_event(event_deposit(info.sender, amount, mint))
        }
        ExecuteMsg::Withdraw {} => {
            let amount = must_pay(&info, rcpt.denom().as_str())?;
            let withdrawn = state.withdraw(amount)?;
            Response::default()
                .add_message(rcpt.burn_msg(amount))
                .add_message(BankMsg::Send {
                    to_address: info.sender.to_string(),
                    amount: coins(withdrawn.u128(), config.denom),
                })
                .add_event(event_withdraw(info.sender, withdrawn, amount))
        }
        ExecuteMsg::Borrow { amount } => {
            let mut borrower = Borrower::load(deps.storage, info.sender.clone())?;
            borrower.borrow(amount)?;
            borrower.save(deps.storage)?;
            let mint = state.borrow(amount)?;
            Response::default()
                .add_message(debt.mint_msg(mint, info.sender.clone()))
                .add_message(BankMsg::Send {
                    to_address: info.sender.to_string(),
                    amount: coins(amount.u128(), config.denom),
                })
                .add_event(event_borrow(info.sender, amount, mint))
        }
        ExecuteMsg::Repay {} => {
            let mut borrower = Borrower::load(deps.storage, info.sender.clone())?;

            let amount = info
                .funds
                .iter()
                .find(|x| x.denom == config.denom)
                .ok_or(PaymentError::MissingDenom(config.denom))?;
            let debt_amount = info
                .funds
                .iter()
                .find(|x| x.denom == debt.denom())
                .ok_or(PaymentError::MissingDenom(debt.denom()))?;

            borrower.repay(amount.amount);
            borrower.save(deps.storage)?;

            state.repay(amount.amount, debt_amount.amount)?;
            Response::default()
                .add_message(debt.burn_msg(debt_amount.amount))
                .add_event(event_repay(info.sender, amount.amount, debt_amount.amount))
        }
    };
    state.save(deps.storage)?;
    Ok(response)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn sudo(deps: DepsMut, _env: Env, msg: SudoMsg) -> Result<Response, ContractError> {
    let mut config = Config::load(deps.storage)?;

    match msg {
        SudoMsg::AddBorrower { addr, borrow_limit } => {
            Borrower::set(
                deps.storage,
                deps.api.addr_validate(addr.as_str())?,
                borrow_limit,
            )?;
            Ok(Response::default())
        }
        SudoMsg::UpdateBorrower { addr, borrow_limit } => {
            let addr = deps.api.addr_validate(addr.as_str())?;
            // Check borrower already exists
            Borrower::load(deps.storage, addr.clone())?;
            Borrower::set(deps.storage, addr, borrow_limit)?;
            Ok(Response::default())
        }
        SudoMsg::UpdateInterest { interest } => {
            config.interest = interest;
            config.save(deps.storage)?;
            Ok(Response::default())
        }
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(_deps: Deps, _env: Env, _msg: QueryMsg) -> Result<Binary, ContractError> {
    Ok(to_json_binary(&())?)
}

#[cfg(test)]
mod tests {

    use super::*;
    use cosmwasm_std::{coin, Decimal, Event, Uint128};
    use cw_multi_test::{ContractWrapper, Executor};
    use rujira_rs::{ghost_vault::Interest, TokenMetadata};
    use rujira_rs_testing::mock_rujira_app;

    #[test]
    fn lifecycle() {
        let mut app = mock_rujira_app();
        let owner = app.api().addr_make("owner");
        let borrower = app.api().addr_make("borrower");

        app.init_modules(|router, _, storage| {
            router
                .bank
                .init_balance(storage, &owner, coins(1_000_000, "btc"))
                .unwrap();
            router
                .bank
                .init_balance(storage, &borrower, coins(1_000_000, "btc"))
                .unwrap();
        });

        let code = Box::new(ContractWrapper::new(execute, instantiate, query).with_sudo(sudo));
        let code_id = app.store_code(code);
        let contract = app
            .instantiate_contract(
                code_id,
                owner.clone(),
                &InstantiateMsg {
                    denom: "btc".to_string(),
                    receipt: TokenMetadata {
                        description: "".to_string(),
                        exponent: 6,
                        display: "".to_string(),
                        name: "".to_string(),
                        symbol: "".to_string(),
                        uri: None,
                        uri_hash: None,
                    },
                    debt: TokenMetadata {
                        description: "".to_string(),
                        exponent: 6,
                        display: "".to_string(),
                        name: "".to_string(),
                        symbol: "".to_string(),
                        uri: None,
                        uri_hash: None,
                    },
                    interest: Interest {
                        target_utilization: Decimal::from_ratio(8u128, 10u128),
                        base_rate: Decimal::from_ratio(3u128, 10000u128),
                        slope1: Decimal::from_ratio(8u128, 10u128),
                        slope2: Decimal::from_ratio(8u128, 10u128),
                    },
                },
                &[],
                "template",
                None,
            )
            .unwrap();

        // First deposit
        let res = app
            .execute_contract(
                owner.clone(),
                contract.clone(),
                &ExecuteMsg::Deposit {},
                &coins(1_000u128, "btc"),
            )
            .unwrap();

        res.assert_event(
            &Event::new("wasm-rujira-ghost-vault/deposit").add_attributes(vec![
                ("amount", "1000"),
                ("owner", owner.as_str()),
                ("minted", "1000"),
            ]),
        );

        res.assert_event(&Event::new("mint").add_attributes(vec![
            ("amount", "1000"),
            ("denom", "x/ghost-vault-btc-rcpt"),
            ("recipient", owner.as_str()),
        ]));

        // Withdraw some

        let res = app
            .execute_contract(
                owner.clone(),
                contract.clone(),
                &ExecuteMsg::Withdraw {},
                &coins(200u128, "x/ghost-vault-btc-rcpt"),
            )
            .unwrap();

        res.assert_event(
            &Event::new("wasm-rujira-ghost-vault/withdraw").add_attributes(vec![
                ("amount", "200"),
                ("owner", owner.as_str()),
                ("shares", "200"),
            ]),
        );

        res.assert_event(
            &Event::new("burn")
                .add_attributes(vec![("amount", "200"), ("denom", "x/ghost-vault-btc-rcpt")]),
        );

        // Whitelist a borrower address
        app.wasm_sudo(
            contract.clone(),
            &SudoMsg::AddBorrower {
                addr: borrower.to_string(),
                borrow_limit: Uint128::from(500u128),
            },
        )
        .unwrap();

        // Check we can't borrow more than the limit
        app.execute_contract(
            borrower.clone(),
            contract.clone(),
            &ExecuteMsg::Borrow {
                amount: Uint128::from(501u128),
            },
            &vec![],
        )
        .unwrap_err();

        // Borrow the whole lot, check debt tokens minted
        let res = app
            .execute_contract(
                borrower.clone(),
                contract.clone(),
                &ExecuteMsg::Borrow {
                    amount: Uint128::from(500u128),
                },
                &vec![],
            )
            .unwrap();

        res.assert_event(
            &Event::new("wasm-rujira-ghost-vault/borrow").add_attributes(vec![
                ("borrower", borrower.as_str()),
                ("amount", "500"),
                ("minted", "500"),
            ]),
        );

        res.assert_event(&Event::new("mint").add_attributes(vec![
            ("amount", "500"),
            ("denom", "x/ghost-vault-btc-debt"),
            ("recipient", borrower.as_str()),
        ]));

        res.assert_event(
            &Event::new("transfer")
                .add_attributes(vec![("amount", "500btc"), ("recipient", borrower.as_str())]),
        );

        // Test repay debt without token
        app.execute_contract(
            borrower.clone(),
            contract.clone(),
            &ExecuteMsg::Repay {},
            &coins(100, "x/ghost-vault-btc-debt"),
        )
        .unwrap_err();

        // Now repay with the required asset
        let res = app
            .execute_contract(
                borrower.clone(),
                contract.clone(),
                &ExecuteMsg::Repay {},
                &vec![coin(100, "btc"), coin(100, "x/ghost-vault-btc-debt")],
            )
            .unwrap();

        res.assert_event(
            &Event::new("wasm-rujira-ghost-vault/repay").add_attributes(vec![
                ("amount", "100"),
                ("borrower", borrower.as_str()),
                ("shares", "100"),
            ]),
        );

        res.assert_event(
            &Event::new("burn")
                .add_attributes(vec![("amount", "100"), ("denom", "x/ghost-vault-btc-debt")]),
        );

        app.update_block(|x| x.time = x.time.plus_days(30));

        // Make another deposit
        let res = app
            .execute_contract(
                owner.clone(),
                contract.clone(),
                &ExecuteMsg::Deposit {},
                &coins(1_000u128, "btc"),
            )
            .unwrap();

        // Ensure that < 1000 tokens are minted to accommodate the increase in interest payments
        res.assert_event(
            &Event::new("wasm-rujira-ghost-vault/deposit").add_attributes(vec![
                ("amount", "1000"),
                ("owner", owner.as_str()),
                ("minted", "984"),
            ]),
        );

        res.assert_event(&Event::new("mint").add_attributes(vec![
            ("amount", "984"),
            ("denom", "x/ghost-vault-btc-rcpt"),
            ("recipient", owner.as_str()),
        ]));

        // finally check that a 1:1 repay doesn't work, and that more btc is required

        // Now repay with the required asset
        app.execute_contract(
            borrower.clone(),
            contract.clone(),
            &ExecuteMsg::Repay {},
            &vec![coin(100, "btc"), coin(100, "x/ghost-vault-btc-debt")],
        )
        .unwrap_err();
        // debt rate is 1.0325

        let res = app
            .execute_contract(
                borrower.clone(),
                contract.clone(),
                &ExecuteMsg::Repay {},
                &vec![coin(103, "btc"), coin(100, "x/ghost-vault-btc-debt")],
            )
            .unwrap();
        res.assert_event(
            &Event::new("wasm-rujira-ghost-vault/repay").add_attributes(vec![
                ("amount", "103"),
                ("borrower", borrower.as_str()),
                ("shares", "100"),
            ]),
        );

        res.assert_event(
            &Event::new("burn")
                .add_attributes(vec![("amount", "100"), ("denom", "x/ghost-vault-btc-debt")]),
        );
    }
}
