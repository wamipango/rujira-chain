use anyhow::Result as AnyResult;
use bytes::{Buf, BufMut};
use cosmwasm_std::{
    Addr, AnyMsg, Api, Binary, BlockInfo, CustomMsg, CustomQuery, GrpcQuery, Querier, Storage,
};
use cw_multi_test::{AppResponse, CosmosRouter, Stargate};
use serde::de::DeserializeOwned;
use tonic::client::Grpc;
use tonic::codec::{Codec, DecodeBuf, Decoder, EncodeBuf, Encoder};
use tonic::codegen::http::uri::PathAndQuery;
use tonic::metadata::MetadataValue;
use tonic::transport::Endpoint;
use tonic::{Request, Status};

/// An adapter that routes grpc queries to a live thornode instance,
/// and swallows all executions without errors
pub struct ThorchainStargate {
    endpoint: String,
}

impl ThorchainStargate {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

impl Stargate for ThorchainStargate {
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
        _api: &dyn Api,
        _storage: &mut dyn Storage,
        _router: &dyn CosmosRouter<ExecC = ExecC, QueryC = QueryC>,
        _block: &BlockInfo,
        _sender: Addr,
        _msg: AnyMsg,
    ) -> AnyResult<AppResponse>
    where
        ExecC: CustomMsg + DeserializeOwned + 'static,
        QueryC: CustomQuery + DeserializeOwned + 'static,
    {
        Ok(AppResponse::default())
    }

    fn query_grpc(
        &self,
        _api: &dyn Api,
        _storage: &dyn Storage,
        _querier: &dyn Querier,
        _block: &BlockInfo,
        request: GrpcQuery,
    ) -> AnyResult<Binary> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;

        rt.block_on(async move {
            let channel = Endpoint::from_shared(self.endpoint.clone())?
                .connect()
                .await?;
            let mut grpc = Grpc::new(channel);
            grpc.ready().await?;
            let path = PathAndQuery::from_maybe_shared(request.path)?;
            let mut req = Request::new(request.data.to_vec());
            if let Ok(token) = std::env::var("GRPC_TOKEN") {
                req.metadata_mut().insert(
                    "authorization",
                    MetadataValue::try_from(format!("Bearer {token}").as_str())?,
                );
            }
            let res = grpc.unary(req, path, PassthroughCodec).await?;
            Ok(Binary::from(res.into_inner()))
        })
    }
}

#[derive(Clone, Copy, Default)]
struct PassthroughCodec;

impl Codec for PassthroughCodec {
    type Encode = Vec<u8>;
    type Decode = Vec<u8>;
    type Encoder = Self;
    type Decoder = Self;

    fn encoder(&mut self) -> Self::Encoder {
        *self
    }

    fn decoder(&mut self) -> Self::Decoder {
        *self
    }
}

impl Encoder for PassthroughCodec {
    type Item = Vec<u8>;
    type Error = Status;

    fn encode(&mut self, item: Self::Item, dst: &mut EncodeBuf<'_>) -> Result<(), Self::Error> {
        dst.put_slice(&item);
        Ok(())
    }
}

impl Decoder for PassthroughCodec {
    type Item = Vec<u8>;
    type Error = Status;

    fn decode(&mut self, src: &mut DecodeBuf<'_>) -> Result<Option<Self::Item>, Self::Error> {
        if src.remaining() == 0 {
            return Ok(None);
        }
        Ok(Some(src.copy_to_bytes(src.remaining()).to_vec()))
    }
}

#[cfg(test)]
mod tests {
    use cosmwasm_std::{
        testing::{mock_dependencies, mock_env},
        Binary, GrpcQuery,
    };
    use cw_multi_test::Stargate;
    use prost::Message;
    use rujira_rs::proto::types::{QueryQuoteSwapRequest, QueryQuoteSwapResponse};

    use super::ThorchainStargate;

    #[test]
    #[ignore = "requires live gRPC network access"]
    fn query_live_swap_quote() {
        let stargate = ThorchainStargate::new("https://grpc.rujira.network:443".to_string());
        let deps = mock_dependencies();
        let env = mock_env();

        let req = QueryQuoteSwapRequest {
            from_asset: "BTC.BTC".to_string(),
            to_asset: "THOR.RUNE".to_string(),
            amount: "10000".to_string(),
            streaming_interval: "".to_string(),
            streaming_quantity: "".to_string(),
            destination: "thor1a3xfqx4yt4yhymhm22m36huuh3eklf49gztyvj".to_string(),
            tolerance_bps: "".to_string(),
            refund_address: "".to_string(),
            affiliate: vec![],
            affiliate_bps: vec![],
            height: "".to_string(),
        };

        let mut data = Vec::new();
        req.encode(&mut data).unwrap();

        let res = stargate
            .query_grpc(
                &deps.api,
                &deps.storage,
                &deps.querier,
                &env.block,
                GrpcQuery {
                    path: "/types.Query/QuoteSwap".to_string(),
                    data: Binary::from(data),
                },
            )
            .unwrap();

        let decoded = QueryQuoteSwapResponse::decode(res.as_slice()).unwrap();
        assert!(!decoded.expected_amount_out.is_empty());
        assert!(!decoded.memo.is_empty());
    }
}
