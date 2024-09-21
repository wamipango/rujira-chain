use anybuf::Bufany;
use anyhow::{Error, Result as AnyResult};
use cosmwasm_std::{
    attr, coins, ensure_eq,
    testing::{MockApi, MockStorage},
    Addr, AnyMsg, Api, BankMsg, Binary, BlockInfo, CosmosMsg, CustomMsg, CustomQuery, Empty, Event,
    GrpcQuery, Querier, Storage, Uint128,
};
use cw_multi_test::{
    App, AppResponse, BankKeeper, BankSudo, BasicAppBuilder, CosmosRouter, FailingModule,
    GovFailingModule, IbcFailingModule, Stargate, SudoMsg, WasmKeeper,
};
use cw_storage_plus::KeyDeserialize;
use serde::de::DeserializeOwned;
use std::str::FromStr;

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
                let full = format!("factory/{sender}/{nonce}");
                storage.set(full.as_bytes(), &sender.as_bytes());

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

                let admin = String::from_vec(storage.get(denom.as_bytes()).unwrap())?;
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

                let admin = String::from_vec(storage.get(denom.as_bytes()).unwrap())?;
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
            "/types.MsgChangeAdmin" => {
                let sender = buf.string(1).unwrap();
                let denom = buf.string(2).unwrap();
                let new_admin = buf.string(3).unwrap();

                let admin = String::from_vec(storage.get(denom.as_bytes()).unwrap())?;
                ensure_eq!(admin, sender, Error::msg("Unauthorized"));
                storage.set(denom.as_bytes(), &new_admin.as_bytes());

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
        anyhow::bail!("Unexpected grpc query: request={:?}", request)
    }
}
