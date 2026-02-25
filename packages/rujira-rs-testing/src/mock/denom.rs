use anybuf::Bufany;
use anyhow::{Error, Result as AnyResult};
use cosmwasm_std::{
    attr, coins, ensure_eq, Api, BankMsg, BlockInfo, CosmosMsg, CustomMsg, CustomQuery,
    DenomMetadata, DenomUnit, Event, Storage, Uint128,
};
use cw_multi_test::{AppResponse, BankSudo, CosmosRouter, SudoMsg};
use cw_storage_plus::Map;
use serde::de::DeserializeOwned;
use std::str::FromStr;

pub fn handle_execute_any<ExecC, QueryC>(
    api: &dyn Api,
    storage: &mut dyn Storage,
    router: &dyn CosmosRouter<ExecC = ExecC, QueryC = QueryC>,
    block: &BlockInfo,
    denom_admin: &Map<String, String>,
    denom_metadata: &Map<String, DenomMetadata>,
    msg: &cosmwasm_std::AnyMsg,
) -> AnyResult<Option<AppResponse>>
where
    ExecC: CustomMsg + DeserializeOwned + 'static,
    QueryC: CustomQuery + DeserializeOwned + 'static,
{
    let buf = Bufany::deserialize(msg.value.as_slice())?;
    let response = match msg.type_url.as_str() {
        "/thorchain.denom.v1.MsgCreateDenom" => {
            let sender = buf.string(1).unwrap();
            let id = buf.string(2).unwrap();
            let m = buf.message(3).unwrap();
            let metadata = decode_metadata(m);
            let full = format!("x/{id}");
            denom_admin.save(storage, full.clone(), &sender)?;
            denom_metadata.save(storage, full, &metadata)?;

            AppResponse {
                events: vec![],
                data: None,
            }
        }
        "/thorchain.denom.v1.MsgMintTokens" => {
            let sender = buf.string(1).unwrap();
            let coin: Bufany<'_> = buf.message(2).unwrap();
            let recipient = buf.string(3).unwrap();

            let denom = coin.string(1).unwrap();
            let amount = Uint128::from_str(&coin.string(2).unwrap())?;

            let admin = denom_admin.load(storage, denom.clone())?;
            ensure_eq!(admin, sender, Error::msg("Unauthorized"));

            router.sudo(
                api,
                storage,
                block,
                SudoMsg::Bank(BankSudo::Mint {
                    to_address: recipient.clone(),
                    amount: coins(amount.u128(), denom.clone()),
                }),
            )?;
            AppResponse {
                events: vec![Event::new("mint").add_attributes(vec![
                    attr("amount", amount),
                    attr("denom", denom.to_string()),
                    attr("recipient", recipient),
                ])],
                data: None,
            }
        }
        "/thorchain.denom.v1.MsgBurnTokens" => {
            let sender = buf.string(1).unwrap();
            let coin: Bufany<'_> = buf.message(2).unwrap();

            let denom = coin.string(1).unwrap();
            let amount = Uint128::from_str(&coin.string(2).unwrap())?;

            let admin = denom_admin.load(storage, denom.clone())?;
            ensure_eq!(admin, sender, Error::msg("Unauthorized"));

            router.execute(
                api,
                storage,
                block,
                api.addr_validate(&sender)?,
                CosmosMsg::Bank(BankMsg::Burn {
                    amount: coins(amount.u128(), denom.clone()),
                }),
            )?;
            AppResponse {
                events: vec![Event::new("burn").add_attributes(vec![
                    attr("amount", amount),
                    attr("denom", denom.to_string()),
                ])],
                data: None,
            }
        }
        "/thorchain.denom.v1.MsgSetDenomAdmin" => {
            let sender = buf.string(1).unwrap();
            let denom = buf.string(2).unwrap();
            let new_admin = buf.string(3).unwrap();

            let admin = denom_admin.load(storage, denom.clone())?;
            ensure_eq!(admin, sender, Error::msg("Unauthorized"));
            denom_admin.save(storage, denom, &new_admin)?;

            AppResponse {
                events: vec![],
                data: None,
            }
        }
        "/thorchain.denom.v1.MsgSetDenomMetadata" => {
            let sender = buf.string(1).unwrap();
            let denom = buf.string(2).unwrap();
            let m = buf.message(3).unwrap();
            let metadata = decode_metadata(m);

            let admin = denom_admin.load(storage, denom.clone())?;
            ensure_eq!(admin, sender, Error::msg("Unauthorized"));
            denom_metadata.save(storage, denom, &metadata)?;

            AppResponse {
                events: vec![],
                data: None,
            }
        }
        _ => {
            return Ok(None);
        }
    };

    Ok(Some(response))
}

fn decode_metadata(m: Bufany) -> DenomMetadata {
    let denom_units = m
        .repeated_bytes(2)
        .unwrap_or_default()
        .iter()
        .map(|x| {
            let m = Bufany::deserialize(x).unwrap();
            DenomUnit {
                denom: m.string(1).unwrap_or_default(),
                exponent: m.uint32(2).unwrap_or_default(),
                aliases: m.repeated_string(3).unwrap_or_default(),
            }
        })
        .collect();

    DenomMetadata {
        description: m.string(1).unwrap_or_default(),
        denom_units,
        base: m.string(3).unwrap_or_default(),
        display: m.string(4).unwrap_or_default(),
        name: m.string(5).unwrap_or_default(),
        symbol: m.string(6).unwrap_or_default(),
        uri: m.string(7).unwrap_or_default(),
        uri_hash: m.string(8).unwrap_or_default(),
    }
}
