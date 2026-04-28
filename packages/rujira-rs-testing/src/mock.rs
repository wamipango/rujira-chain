use anyhow::Result as AnyResult;
use cosmwasm_std::{
    testing::{MockApi, MockStorage},
    Addr, AnyMsg, Api, Binary, BlockInfo, CustomMsg, CustomQuery, Decimal, DenomMetadata, Empty,
    GrpcQuery, Querier, Storage,
};
use cw_multi_test::{
    App, AppResponse, BankKeeper, BasicAppBuilder, CosmosRouter, FailingModule, GovFailingModule,
    IbcFailingModule, Stargate, WasmKeeper,
};
use cw_storage_plus::Map;
use rujira_rs::proto::types::QueryNodeResponse;
use serde::de::DeserializeOwned;
use std::{cell::RefCell, collections::BTreeMap};

mod denom;
mod thorchain;
mod wasm;

pub type RujiraApp<T = RujiraStargate> = App<
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
    T,
>;

pub fn mock_rujira_app() -> RujiraApp<RujiraStargate> {
    BasicAppBuilder::new()
        .with_stargate(RujiraStargate::default())
        .build(|_, _, _| {})
}

pub struct RujiraStargate {
    prices: BTreeMap<String, Decimal>,
    nodes: RefCell<BTreeMap<Addr, QueryNodeResponse>>,
    denom_admin: Map<String, String>,
    denom_metadata: Map<String, DenomMetadata>,
}

impl Default for RujiraStargate {
    fn default() -> Self {
        Self {
            prices: BTreeMap::new(),
            nodes: RefCell::new(BTreeMap::new()),
            denom_admin: Map::new("denom_admin"),
            denom_metadata: Map::new("denom_meta"),
        }
    }
}

impl RujiraStargate {
    pub fn with_price(&mut self, symbol: &str, price: Decimal) {
        self.prices.insert(symbol.to_string(), price);
    }

    pub fn with_prices(&mut self, prices: Vec<(&str, Decimal)>) {
        for (symbol, price) in prices {
            self.with_price(symbol, price);
        }
    }

    pub fn with_node(&mut self, addr: Addr, response: QueryNodeResponse) {
        self.nodes.borrow_mut().insert(addr, response);
    }
}

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
        if let Some(response) =
            thorchain::handle_execute_any(api, storage, router, block, &self.nodes, &msg)?
        {
            return Ok(response);
        }
        if let Some(response) = denom::handle_execute_any(
            api,
            storage,
            router,
            block,
            &self.denom_admin,
            &self.denom_metadata,
            &msg,
        )? {
            return Ok(response);
        }
        if let Some(response) = wasm::handle_execute_any(api, storage, router, block, &msg)? {
            return Ok(response);
        }
        anyhow::bail!("Unexpected any execute: msg={:?} from {}", msg, sender)
    }

    fn query_grpc(
        &self,
        _api: &dyn Api,
        _storage: &dyn Storage,
        _querier: &dyn Querier,
        _block: &BlockInfo,
        request: GrpcQuery,
    ) -> AnyResult<Binary> {
        if let Some(response) =
            thorchain::handle_query_grpc(&self.nodes, &self.prices, request.clone())?
        {
            return Ok(response);
        }
        anyhow::bail!("Unexpected grpc query: request={:?}", request)
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use cosmwasm_std::{Decimal, Uint128};
    use rujira_rs::{
        query::{Pool, PoolStatus},
        Asset, Layer1Asset,
    };

    use super::*;

    #[test]
    fn query_pool() {
        let app = mock_rujira_app();
        let asset = Layer1Asset::new("BTC", "BTC");
        let res = Pool::load(app.wrap(), &asset).unwrap();
        assert_eq!(res.asset, Asset::Layer1(Layer1Asset::new("BTC", "BTC")),);
        assert_eq!(res.short_code, "b".to_string());
        assert_eq!(res.status, PoolStatus::Available);
        assert_eq!(res.decimals, 8);
        assert_eq!(res.pending_inbound_asset, Uint128::from(156524579u128));
        assert_eq!(res.pending_inbound_rune, Uint128::from(0u128));
        assert_eq!(res.balance_asset, Uint128::from(68602648901u128));
        assert_eq!(res.balance_rune, Uint128::from(1172427071332399u128));
        assert_eq!(res.asset_tor_price, Decimal::from_str("100100").unwrap(),);
        assert_eq!(res.pool_units, Uint128::from(613518358320559u128));
        assert_eq!(res.lp_units, Uint128::from(347866097255926u128));
        assert_eq!(res.synth_units, Uint128::from(265652261064633u128));
        assert_eq!(res.synth_supply, Uint128::from(59409628248u128));
        assert_eq!(res.savers_depth, Uint128::from(58882558588u128));
        assert_eq!(res.savers_units, Uint128::from(56192173382u128));
        assert_eq!(res.savers_fill_bps, 8660);
        assert_eq!(res.savers_capacity_remaining, Uint128::from(9193020653u128));
        assert!(!res.synth_mint_paused);
        assert_eq!(res.synth_supply_remaining, Uint128::from(22913550433u128));
        assert_eq!(res.derived_depth_bps, 9639);
    }
}
