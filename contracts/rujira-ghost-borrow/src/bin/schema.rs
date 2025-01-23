use cosmwasm_schema::write_api;

use rujira_rs::ghost_borrow;

fn main() {
    write_api! {
        instantiate: ghost_borrow::InstantiateMsg,
        execute: ghost_borrow::ExecuteMsg,
        query: ghost_borrow::QueryMsg,
        sudo: ghost_borrow::SudoMsg,
    }
}
