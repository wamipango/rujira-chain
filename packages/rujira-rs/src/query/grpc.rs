use crate::proto::types::{
    QueryNetworkRequest, QueryNetworkResponse, QueryPoolRequest, QueryPoolResponse,
};
use cosmwasm_std::{Binary, QuerierWrapper, StdError};
use prost::{DecodeError, EncodeError, Message};
use thiserror::Error;

pub trait QueryablePair {
    type Request: Message + Default;
    type Response: Message + Sized + Default;

    fn grpc_path() -> &'static str;
}

pub trait Queryable: Sized {
    type Pair: QueryablePair;

    fn get(
        querier: QuerierWrapper,
        req: <Self::Pair as QueryablePair>::Request,
    ) -> Result<Self, QueryError>;
}

impl<T> Queryable for T
where
    T: QueryablePair<Response = Self> + Message + Default,
{
    type Pair = T;

    fn get(
        querier: QuerierWrapper,
        req: <Self::Pair as QueryablePair>::Request,
    ) -> Result<Self, QueryError> {
        let mut buf = Vec::new();
        req.encode(&mut buf)?;
        let res = querier
            .query_grpc(Self::grpc_path().to_string(), Binary::from(buf))?
            .to_vec();
        Ok(Self::decode(&*res)?)
    }
}

#[derive(Error, Debug)]
pub enum QueryError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("{0}")]
    Encode(#[from] EncodeError),

    #[error("{0}")]
    Decode(#[from] DecodeError),
}

impl QueryablePair for QueryPoolResponse {
    type Request = QueryPoolRequest;
    type Response = QueryPoolResponse;

    fn grpc_path() -> &'static str {
        "/types.Query/Pool"
    }
}

impl QueryablePair for QueryNetworkResponse {
    type Request = QueryNetworkRequest;
    type Response = QueryNetworkResponse;

    fn grpc_path() -> &'static str {
        "/types.Query/Network"
    }
}
