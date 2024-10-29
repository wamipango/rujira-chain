use cosmwasm_schema::write_api;

use rujira_rs::stream;

fn main() {
    write_api! {
        instantiate: stream::factory::InstantiateMsg,
        execute: stream::factory::ExecuteMsg,
        query: stream::factory::QueryMsg,
    }
}
