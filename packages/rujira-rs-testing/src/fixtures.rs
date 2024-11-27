use cosmwasm_std::Binary;
use prost::Message;
use rujira_rs::proto;

pub fn mock_pool_btc() -> Binary {
    let pool = proto::types::QueryPoolResponse {
        asset: "BTC.BTC".to_string(),
        short_code: "b".to_string(),
        status: "Available".to_string(),
        decimals: 8,
        pending_inbound_asset: "156524579".to_string(),
        pending_inbound_rune: "0".to_string(),
        balance_asset: "68602648901".to_string(),
        balance_rune: "1172427071332399".to_string(),
        asset_tor_price: "9330837628318".to_string(),
        pool_units: "613518358320559".to_string(),
        lp_units: "347866097255926".to_string(),
        synth_units: "265652261064633".to_string(),
        synth_supply: "59409628248".to_string(),
        savers_depth: "58882558588".to_string(),
        savers_units: "56192173382".to_string(),
        savers_fill_bps: "8660".to_string(),
        savers_capacity_remaining: "9193020653".to_string(),
        synth_mint_paused: false,
        synth_supply_remaining: "22913550433".to_string(),
        loan_collateral: "167294477784".to_string(),
        loan_collateral_remaining: "0".to_string(),
        loan_cr: "0".to_string(),
        derived_depth_bps: "9639".to_string(),
    };

    let mut buf = Vec::new();
    pool.encode(&mut buf).unwrap();
    buf.into()
}
