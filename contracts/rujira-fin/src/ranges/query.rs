use cosmwasm_std::{Addr, Deps, Uint128};
use rujira_rs::fin::{RangeResponse, RangesResponse};

use crate::ContractError;

use super::Range;

const DEFAULT_LIMIT: u8 = 30;

pub fn query_range(deps: Deps, idx: Uint128) -> Result<RangeResponse, ContractError> {
    let range = Range::load(deps.storage, idx).map_err(|_| ContractError::NotFound {})?;
    Ok(range_response(range))
}

pub fn query_ranges(
    deps: Deps,
    cursor: Option<Uint128>,
    limit: Option<u8>,
    owner: Option<Addr>,
) -> Result<RangesResponse, ContractError> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT) as usize;
    let ranges = Range::list(deps.storage, cursor, limit, owner)?;
    Ok(RangesResponse {
        ranges: ranges.into_iter().map(range_response).collect(),
    })
}

fn range_response(range: Range) -> RangeResponse {
    RangeResponse {
        idx: range.idx,
        owner: range.owner.to_string(),
        high: range.high,
        low: range.low,
        skew: range.skew,
        spread: range.spread,
        fee: range.fee,
        base: range.base,
        quote: range.quote,
        price: range.price,
        ask: range.ask,
        bid: range.bid,
        fees: range.fees,
    }
}
