use std::str::FromStr;

use cosmwasm_std::{coin, coins, to_json_binary, Addr, Binary, Coin, Decimal, Uint128};
use cw_multi_test::{AppResponse, ContractWrapper, Executor, Stargate};
use rujira_ghost_vault::mock::GhostVault;
use rujira_rs::ghost::credit::{
    AccountMsg, AccountResponse, AccountsResponse, ConfigResponse, ExecuteMsg, InstantiateMsg,
    LiquidateMsg, QueryMsg, SudoMsg,
};
use rujira_rs_testing::RujiraApp;
use serde::Serialize;

use crate::contract::{execute, instantiate, query, reply, sudo};

/// Wrapper struct for Ghost Vault contract with convenience methods
#[derive(Debug, Clone)]
pub struct GhostCredit(pub Addr);

impl GhostCredit {
    /// Get the contract address
    pub fn addr(&self) -> &Addr {
        &self.0
    }

    pub fn create_account<T: Stargate>(
        &self,
        app: &mut RujiraApp<T>,
        owner: &Addr,
        label: &str,
        tag: &str,
        salt: Binary,
    ) -> AppResponse {
        app.execute_contract(
            owner.clone(),
            self.0.clone(),
            &ExecuteMsg::Create {
                salt,
                label: label.to_string(),
                tag: tag.to_string(),
            },
            &[],
        )
        .unwrap()
    }

    pub fn account<T: Stargate>(
        &self,
        app: &mut RujiraApp<T>,
        account: &AccountResponse,
        msgs: Vec<AccountMsg>,
    ) -> anyhow::Result<AppResponse> {
        app.execute_contract(
            account.owner.clone(),
            self.0.clone(),
            &ExecuteMsg::Account {
                addr: account.account.to_string(),
                msgs,
            },
            &[],
        )
    }

    pub fn account_borrow<T: Stargate>(
        &self,
        app: &mut RujiraApp<T>,
        account: &AccountResponse,
        amount: u128,
        denom: &str,
    ) -> anyhow::Result<AppResponse> {
        self.account(app, account, vec![AccountMsg::Borrow(coin(amount, denom))])
    }

    pub fn account_execute<M, T: Stargate>(
        &self,
        app: &mut RujiraApp<T>,
        account: &AccountResponse,
        contract: &Addr,
        msg: M,
        funds: Vec<Coin>,
    ) -> anyhow::Result<AppResponse>
    where
        M: Serialize + Sized,
    {
        self.account(
            app,
            account,
            vec![AccountMsg::Execute {
                contract_addr: contract.to_string(),
                msg: to_json_binary(&msg).unwrap(),
                funds,
            }],
        )
    }

    pub fn account_send<T: Stargate>(
        &self,
        app: &mut RujiraApp<T>,
        account: &AccountResponse,
        amount: u128,
        denom: &str,
        recipient: &Addr,
    ) -> anyhow::Result<AppResponse> {
        self.account(
            app,
            account,
            vec![AccountMsg::Send {
                to_address: recipient.to_string(),
                funds: coins(amount, denom),
            }],
        )
    }

    pub fn account_preference_order<T: Stargate>(
        &self,
        app: &mut RujiraApp<T>,
        account: &AccountResponse,
        denom: &str,
        after: Option<&str>,
    ) -> anyhow::Result<AppResponse> {
        self.account(
            app,
            account,
            vec![AccountMsg::SetPreferenceOrder {
                denom: denom.to_string(),
                after: after.map(|x| x.to_string()),
            }],
        )
    }

    pub fn liquidate<T: Stargate>(
        &self,
        app: &mut RujiraApp<T>,
        account: &AccountResponse,
        msgs: Vec<LiquidateMsg>,
    ) -> anyhow::Result<AppResponse> {
        app.execute_contract(
            account.owner.clone(),
            self.0.clone(),
            &ExecuteMsg::Liquidate {
                addr: account.account.to_string(),
                msgs,
            },
            &[],
        )
    }

    pub fn liquidate_execute<M, T: Stargate>(
        &self,
        app: &mut RujiraApp<T>,
        account: &AccountResponse,
        contract: &Addr,
        msg: M,
        funds: Vec<Coin>,
    ) -> anyhow::Result<AppResponse>
    where
        M: Serialize + Sized,
    {
        self.liquidate(
            app,
            account,
            vec![LiquidateMsg::Execute {
                contract_addr: contract.to_string(),
                msg: to_json_binary(&msg).unwrap(),
                funds,
            }],
        )
    }

    pub fn liquidate_execute_repay<M, T: Stargate>(
        &self,
        app: &mut RujiraApp<T>,
        account: &AccountResponse,
        contract: &Addr,
        msg: M,
        funds: Vec<Coin>,
        repay: &str,
    ) -> anyhow::Result<AppResponse>
    where
        M: Serialize + Sized,
    {
        self.liquidate(
            app,
            account,
            vec![
                LiquidateMsg::Execute {
                    contract_addr: contract.to_string(),
                    msg: to_json_binary(&msg).unwrap(),
                    funds,
                },
                LiquidateMsg::Repay(repay.to_string()),
            ],
        )
    }

    pub fn set_collateral<T: Stargate>(
        &self,
        app: &mut RujiraApp<T>,
        denom: &str,
        ratio: &str,
    ) -> AppResponse {
        app.wasm_sudo(
            self.0.clone(),
            &SudoMsg::SetCollateral {
                denom: denom.to_string(),
                collateralization_ratio: Decimal::from_str(ratio).unwrap(),
            },
        )
        .unwrap()
    }

    pub fn set_vault<T: Stargate>(
        &self,
        app: &mut RujiraApp<T>,
        vault: &GhostVault,
    ) -> AppResponse {
        app.wasm_sudo(
            self.0.clone(),
            &SudoMsg::SetVault {
                address: vault.addr().to_string(),
            },
        )
        .unwrap()
    }

    pub fn query_account<T: Stargate>(&self, app: &RujiraApp<T>, addr: &Addr) -> AccountResponse {
        app.wrap()
            .query_wasm_smart(self.0.clone(), &QueryMsg::Account(addr.to_string()))
            .unwrap()
    }

    pub fn query_accounts<T: Stargate>(
        &self,
        app: &RujiraApp<T>,
        owner: &Addr,
        tag: Option<String>,
    ) -> AccountsResponse {
        app.wrap()
            .query_wasm_smart(
                self.0.clone(),
                &QueryMsg::Accounts {
                    owner: owner.to_string(),
                    tag,
                },
            )
            .unwrap()
    }

    pub fn query_all_accounts<T: Stargate>(
        &self,
        app: &RujiraApp<T>,
        cursor: Option<Addr>,
    ) -> AccountsResponse {
        app.wrap()
            .query_wasm_smart(
                self.0.clone(),
                &QueryMsg::AllAccounts {
                    cursor: cursor.map(|x| x.to_string()),
                    limit: None,
                },
            )
            .unwrap()
    }

    pub fn query_config<T: Stargate>(&self, app: &RujiraApp<T>) -> ConfigResponse {
        app.wrap()
            .query_wasm_smart(self.0.clone(), &QueryMsg::Config {})
            .unwrap()
    }

    pub fn query_next<T: Stargate>(&self, app: &RujiraApp<T>, owner: &Addr, salt: Binary) -> Addr {
        app.wrap()
            .query_wasm_smart(
                self.0.clone(),
                &QueryMsg::Predict {
                    salt,
                    owner: owner.to_string(),
                },
            )
            .unwrap()
    }

    pub fn create<T: Stargate>(app: &mut RujiraApp<T>, owner: &Addr, fees: &Addr) -> Self {
        let account_code = Box::new(
            ContractWrapper::new(
                rujira_account::contract::execute,
                rujira_account::contract::instantiate,
                rujira_account::contract::query,
            )
            .with_sudo(rujira_account::contract::sudo),
        );
        let account_code_id = app.store_code(account_code);

        let code = Box::new(
            ContractWrapper::new(execute, instantiate, query)
                .with_sudo(sudo)
                .with_reply(reply),
        );
        let code_id = app.store_code(code);
        let addr = app
            .instantiate_contract(
                code_id,
                owner.clone(),
                &InstantiateMsg {
                    code_id: account_code_id,
                    fee_liquidation: Decimal::from_str("0.01").unwrap(),
                    fee_liquidator: Decimal::from_str("0.005").unwrap(),
                    liquidation_max_slip: Decimal::from_str("0.3").unwrap(),
                    liquidation_threshold: Decimal::one(),
                    adjustment_threshold: Decimal::from_str("0.95").unwrap(),
                    fee_address: fees.clone(),
                    full_liquidation_threshold: Uint128::zero(),
                },
                &[],
                "template",
                None,
            )
            .unwrap();
        Self(addr)
    }
}
