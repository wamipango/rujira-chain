use cosmwasm_schema::write_api;

use rujira_rs::stream;

fn main() {
    write_api! {
        instantiate: stream::InstantiateMsg,
        execute: stream::ExecuteMsg,
        query: stream::QueryMsg,
        sudo: stream::SudoMsg,
    }
}
