use cosmwasm_std::{coins, Addr, Uint128};
use cw_multi_test::{ContractWrapper, Executor};

use rujira_rs::{
    ghost::{self, vault::Interest},
    TokenMetadata,
};

use crate::RujiraApp;

/// Wrapper struct for Ghost Vault contract with convenience methods
#[derive(Debug, Clone)]
pub struct GhostVault(pub Addr);

impl GhostVault {
    /// Get the contract address
    pub fn addr(&self) -> &Addr {
        &self.0
    }

    /// Execute a deposit into the vault
    pub fn deposit(
        &self,
        app: &mut RujiraApp,
        sender: &Addr,
        amount: u128,
        denom: &str,
    ) -> anyhow::Result<cw_multi_test::AppResponse> {
        app.execute_contract(
            sender.clone(),
            self.0.clone(),
            &ghost::vault::ExecuteMsg::Deposit { callback: None },
            &coins(amount, denom),
        )
    }

    /// Execute a withdrawal from the vault
    pub fn withdraw(
        &self,
        app: &mut RujiraApp,
        sender: &Addr,
        amount: Uint128,
    ) -> anyhow::Result<cw_multi_test::AppResponse> {
        app.execute_contract(
            sender.clone(),
            self.0.clone(),
            &ghost::vault::ExecuteMsg::Withdraw { callback: None },
            &coins(amount.u128(), "receipt-token"), // Withdraw by burning receipt tokens
        )
    }

    /// Set borrower permissions via sudo
    pub fn set_borrower(
        &self,
        app: &mut RujiraApp,
        contract: &str,
        limit: Uint128,
    ) -> anyhow::Result<cw_multi_test::AppResponse> {
        app.wasm_sudo(
            self.0.clone(),
            &ghost::vault::SudoMsg::SetBorrower {
                contract: contract.to_string(),
                limit,
            },
        )
    }

    /// Query vault status
    pub fn query_status(&self, app: &RujiraApp) -> anyhow::Result<ghost::vault::StatusResponse> {
        Ok(app
            .wrap()
            .query_wasm_smart(self.0.clone(), &ghost::vault::QueryMsg::Status {})?)
    }

    /// Query borrower info
    pub fn query_borrower(
        &self,
        app: &RujiraApp,
        addr: &str,
    ) -> anyhow::Result<ghost::vault::BorrowerResponse> {
        Ok(app.wrap().query_wasm_smart(
            self.0.clone(),
            &ghost::vault::QueryMsg::Borrower {
                addr: addr.to_string(),
            },
        )?)
    }
}

/// Setup a ghost vault for a specific denom
pub fn setup_ghost_vault(
    app: &mut RujiraApp,
    owner: &Addr,
    denom: &str,
    receipt_symbol: &str,
) -> GhostVault {
    let vault_code = Box::new(
        ContractWrapper::new(
            rujira_ghost_vault::contract::execute,
            rujira_ghost_vault::contract::instantiate,
            rujira_ghost_vault::contract::query,
        )
        .with_sudo(rujira_ghost_vault::contract::sudo),
    );
    let vault_code_id = app.store_code(vault_code);

    let vault_addr = app
        .instantiate_contract(
            vault_code_id,
            owner.clone(),
            &ghost::vault::InstantiateMsg {
                denom: denom.to_string(),
                interest: Interest::default(),
                receipt: TokenMetadata {
                    description: receipt_symbol.to_string(),
                    display: receipt_symbol.to_string(),
                    name: receipt_symbol.to_string(),
                    symbol: receipt_symbol.to_string(),
                    uri: None,
                    uri_hash: None,
                },
            },
            &[],
            format!("ghost-vault-{}", denom),
            Some(owner.to_string()),
        )
        .unwrap();

    GhostVault(vault_addr)
}

#[cfg(test)]
mod tests {
    use cosmwasm_std::coin;

    use super::*;
    use crate::mock_rujira_app;

    #[test]
    fn test_ghost_vault_setup() {
        let mut app = mock_rujira_app();
        let owner = app.api().addr_make("owner");

        // Init balance for deposit
        app.init_modules(|x, _api, storage| {
            x.bank
                .init_balance(storage, &owner, vec![coin(10000000000, "test-token")])
        })
        .unwrap();

        let vault = setup_ghost_vault(&mut app, &owner, "test-token", "XTEST");

        assert!(!vault.addr().as_str().is_empty());
    }
}
