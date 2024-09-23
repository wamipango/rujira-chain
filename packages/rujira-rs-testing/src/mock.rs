use anybuf::Bufany;
use anyhow::{Error, Result as AnyResult};
use cosmwasm_std::{
    attr, coins, ensure_eq,
    testing::{MockApi, MockStorage},
    Addr, AnyMsg, Api, BankMsg, Binary, BlockInfo, CosmosMsg, CustomMsg, CustomQuery,
    DenomMetadata, DenomUnit, Empty, Event, GrpcQuery, Querier, Storage, Uint128,
};
use cw_multi_test::{
    App, AppResponse, BankKeeper, BankSudo, BasicAppBuilder, CosmosRouter, FailingModule,
    GovFailingModule, IbcFailingModule, Stargate, SudoMsg, WasmKeeper,
};
use cw_storage_plus::Map;
use serde::de::DeserializeOwned;
use std::str::FromStr;

use crate::fixtures::mock_pool_btc;

pub type RujiraApp = App<
    BankKeeper,
    MockApi,
    MockStorage,
    // Custom
    FailingModule<Empty, Empty, Empty>,
    WasmKeeper<Empty, Empty>,
    // SDK Staking
    FailingModule<Empty, Empty, Empty>,
    // SDK Distribution
    FailingModule<Empty, Empty, Empty>,
    IbcFailingModule,
    GovFailingModule,
    RujiraStargate,
>;

static DENOM_ADMIN: Map<String, String> = Map::new("denom_admin");
static DENOM_METADATA: Map<String, DenomMetadata> = Map::new("denom_meta");

pub fn mock_rujira_app() -> RujiraApp {
    BasicAppBuilder::new()
        .with_stargate(RujiraStargate::default())
        .build(|_, _, _| {})
}

#[derive(Default)]
pub struct RujiraStargate {}

impl Stargate for RujiraStargate {
    fn execute_stargate<ExecC, QueryC>(
        &self,
        _api: &dyn Api,
        _storage: &mut dyn Storage,
        _router: &dyn CosmosRouter<ExecC = ExecC, QueryC = QueryC>,
        _block: &BlockInfo,
        sender: Addr,
        type_url: String,
        value: Binary,
    ) -> AnyResult<AppResponse>
    where
        ExecC: CustomMsg + DeserializeOwned + 'static,
        QueryC: CustomQuery + DeserializeOwned + 'static,
    {
        anyhow::bail!(
            "Unexpected stargate execute: type_url={}, value={} from {}",
            type_url,
            value,
            sender,
        )
    }

    fn query_stargate(
        &self,
        _api: &dyn Api,
        _storage: &dyn Storage,
        _querier: &dyn Querier,
        _block: &BlockInfo,
        path: String,
        data: Binary,
    ) -> AnyResult<Binary> {
        anyhow::bail!("Unexpected stargate query: path={}, data={}", path, data)
    }

    fn execute_any<ExecC, QueryC>(
        &self,
        api: &dyn Api,
        storage: &mut dyn Storage,
        router: &dyn CosmosRouter<ExecC = ExecC, QueryC = QueryC>,
        block: &BlockInfo,
        sender: Addr,
        msg: AnyMsg,
    ) -> AnyResult<AppResponse>
    where
        ExecC: CustomMsg + DeserializeOwned + 'static,
        QueryC: CustomQuery + DeserializeOwned + 'static,
    {
        let type_url = msg.type_url.clone();
        let serialized = msg.value.to_vec();
        let buf = Bufany::deserialize(&serialized)?;
        match type_url.as_str() {
            "/types.MsgCreateDenom" => {
                let sender = buf.string(1).unwrap();
                let nonce = buf.string(2).unwrap();
                let m = buf.message(3).unwrap();
                let metadata = decode_metadata(m);
                let full = format!("factory/{sender}/{nonce}");
                DENOM_ADMIN.save(storage, full.clone(), &sender)?;
                DENOM_METADATA.save(storage, full, &metadata)?;

                Ok(AppResponse {
                    events: vec![],
                    data: None,
                })
            }
            "/types.MsgMint" => {
                let sender = buf.string(1).unwrap();
                let coin: Bufany<'_> = buf.message(2).unwrap();
                let recipient = buf.string(3).unwrap();

                let denom = coin.string(1).unwrap();
                let amount = Uint128::from_str(&coin.string(2).unwrap())?;

                let admin = DENOM_ADMIN.load(storage, denom.clone())?;
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
                Ok(AppResponse {
                    events: vec![Event::new("mint").add_attributes(vec![
                        attr("amount", amount),
                        attr("denom", denom.to_string()),
                        attr("recipient", recipient),
                    ])],
                    data: None,
                })
            }
            "/types.MsgBurn" => {
                let sender = buf.string(1).unwrap();
                let coin: Bufany<'_> = buf.message(2).unwrap();

                let denom = coin.string(1).unwrap();
                let amount = Uint128::from_str(&coin.string(2).unwrap())?;

                let admin = DENOM_ADMIN.load(storage, denom.clone())?;
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
                Ok(AppResponse {
                    events: vec![Event::new("burn").add_attributes(vec![
                        attr("amount", amount),
                        attr("denom", denom.to_string()),
                    ])],
                    data: None,
                })
            }
            "/types.MsgSetAdmin" => {
                let sender = buf.string(1).unwrap();
                let denom = buf.string(2).unwrap();
                let new_admin = buf.string(3).unwrap();

                let admin = DENOM_ADMIN.load(storage, denom.clone())?;
                ensure_eq!(admin, sender, Error::msg("Unauthorized"));
                DENOM_ADMIN.save(storage, denom, &new_admin)?;

                Ok(AppResponse {
                    events: vec![],
                    data: None,
                })
            }
            "/types.MsgSetMetadata" => {
                let sender = buf.string(1).unwrap();
                let denom = buf.string(2).unwrap();
                let m = buf.message(3).unwrap();
                let metadata = decode_metadata(m);

                let admin = DENOM_ADMIN.load(storage, denom.clone())?;
                ensure_eq!(admin, sender, Error::msg("Unauthorized"));
                DENOM_METADATA.save(storage, denom, &metadata)?;

                Ok(AppResponse {
                    events: vec![],
                    data: None,
                })
            }

            _ => {
                anyhow::bail!("Unexpected any execute: msg={:?} from {}", msg, sender)
            }
        }
    }

    fn query_grpc(
        &self,
        _api: &dyn Api,
        _storage: &dyn Storage,
        _querier: &dyn Querier,
        _block: &BlockInfo,
        request: GrpcQuery,
    ) -> AnyResult<Binary> {
        match request.path.as_str() {
            "/thorchain.Query/Pool" => Ok(mock_pool_btc()),
            _ => {
                anyhow::bail!("Unexpected grpc query: request={:?}", request)
            }
        }
    }
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

#[cfg(test)]
mod tests {
    use rujira_rs::{Asset, Chain, L1Asset, L1Pool, L1PoolStatus};

    use super::*;

    #[test]
    fn query_pool() {
        let app = mock_rujira_app();
        let asset = L1Asset::new(Chain::Btc, "BTC");
        let res = L1Pool::load(app.wrap(), &asset).unwrap();
        assert_eq!(res.balance_rune, Uint128::from(1000u128));
        assert_eq!(res.balance_asset, Uint128::from(2000u128));
        assert_eq!(res.asset, Asset::L1(asset));
        assert_eq!(res.lp_units, Uint128::from(3000u128));
        assert_eq!(res.status, L1PoolStatus::Available);
        assert_eq!(res.decimals, 6);
        assert_eq!(res.synth_units, Uint128::zero());
        assert_eq!(res.pending_inbound_rune, Uint128::zero());
        assert_eq!(res.pending_inbound_asset, Uint128::zero());
        assert_eq!(res.status_since, 1_000_000);
    }
}
