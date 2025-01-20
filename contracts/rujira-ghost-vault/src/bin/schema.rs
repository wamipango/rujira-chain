use cosmwasm_schema::write_api;

use rujira_rs::ghost_vault;

fn main() {
    write_api! {
        instantiate: ghost_vault::InstantiateMsg,
        execute: ghost_vault::ExecuteMsg,
        query: ghost_vault::QueryMsg,
        sudo: ghost_vault::SudoMsg,
    }
}
