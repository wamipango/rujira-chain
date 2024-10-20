use cosmwasm_schema::write_api;

use rujira_rs::authz;

fn main() {
    write_api! {
        instantiate: authz::InstantiateMsg,
        execute: authz::ExecuteMsg,
        query: authz::QueryMsg,
        sudo: authz::SudoMsg,
    }
}
