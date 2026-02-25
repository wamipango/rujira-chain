use cosmwasm_schema::write_api;

use rujira_rs::brune;

fn main() {
    write_api! {
        instantiate: brune::InstantiateMsg,
        execute: brune::ExecuteMsg,
        query: brune::QueryMsg,
        sudo: brune::SudoMsg,
    }
}
