use cosmwasm_schema::write_api;

use rujira_rs::ghost_registry;

fn main() {
    write_api! {
        instantiate: ghost_registry::InstantiateMsg,
        execute: ghost_registry::ExecuteMsg,
        query: ghost_registry::QueryMsg,
        sudo: ghost_registry::SudoMsg,
    }
}
