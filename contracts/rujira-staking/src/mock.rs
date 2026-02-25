use cosmwasm_std::{coins, Addr, Binary, Decimal, Uint128};
use cw_multi_test::{ContractWrapper, Executor};

use rujira_rs::staking::{
    AccountMsg, AccountResponse, ExecuteMsg, InstantiateMsg, LiquidMsg, QueryMsg, StatusResponse,
};
use rujira_rs::TokenMetadata;
use rujira_rs_testing::RujiraApp;

use crate::contract::{execute, instantiate, query};

#[derive(Debug, Clone)]
pub struct Staking {
    addr: Addr,
    bond_denom: String,
}

impl Staking {
    pub fn addr(&self) -> &Addr {
        &self.addr
    }

    pub fn create(
        app: &mut RujiraApp,
        owner: &Addr,
        bond_denom: &str,
        revenue_denom: &str,
        fee: Option<(Decimal, String)>,
        swap_contract: Addr,
        converter_message: Binary,
    ) -> Self {
        let code = Box::new(ContractWrapper::new(execute, instantiate, query));
        let code_id = app.store_code(code);
        let addr = app
            .instantiate_contract(
                code_id,
                owner.clone(),
                &InstantiateMsg {
                    bond_denom: bond_denom.to_string(),
                    revenue_denom: revenue_denom.to_string(),
                    revenue_converter: (
                        swap_contract.to_string(),
                        converter_message,
                        Uint128::from(20u128),
                    ),
                    fee,
                    receipt_token_metadata: TokenMetadata {
                        description: "description".to_string(),
                        name: "Liquid Staked".to_string(),
                        display: "sTOKEN".to_string(),
                        symbol: "sTOKEN".to_string(),
                        uri: None,
                        uri_hash: None,
                    },
                },
                &[],
                "staking",
                None,
            )
            .unwrap();

        Self {
            addr,
            bond_denom: bond_denom.to_string(),
        }
    }

    pub fn account_create(
        &self,
        app: &mut RujiraApp,
        sender: &Addr,
        amount: Uint128,
    ) -> anyhow::Result<cw_multi_test::AppResponse> {
        app.execute_contract(
            sender.clone(),
            self.addr.clone(),
            &ExecuteMsg::Account(AccountMsg::Bond {}),
            &coins(amount.u128(), &self.bond_denom),
        )
    }

    pub fn bond_liquid(
        &self,
        app: &mut RujiraApp,
        sender: &Addr,
        amount: Uint128,
    ) -> anyhow::Result<cw_multi_test::AppResponse> {
        app.execute_contract(
            sender.clone(),
            self.addr.clone(),
            &ExecuteMsg::Liquid(LiquidMsg::Bond {}),
            &coins(amount.u128(), &self.bond_denom),
        )
    }

    pub fn account_claim(
        &self,
        app: &mut RujiraApp,
        sender: &Addr,
    ) -> anyhow::Result<cw_multi_test::AppResponse> {
        app.execute_contract(
            sender.clone(),
            self.addr.clone(),
            &ExecuteMsg::Account(AccountMsg::Claim {}),
            &[],
        )
    }

    pub fn query_status(&self, app: &RujiraApp) -> StatusResponse {
        app.wrap()
            .query_wasm_smart(self.addr.clone(), &QueryMsg::Status {})
            .unwrap()
    }

    pub fn query_account(&self, app: &RujiraApp, addr: &Addr) -> AccountResponse {
        app.wrap()
            .query_wasm_smart(
                self.addr.clone(),
                &QueryMsg::Account {
                    addr: addr.to_string(),
                },
            )
            .unwrap()
    }
}
