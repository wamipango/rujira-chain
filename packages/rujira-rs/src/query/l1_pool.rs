use anybuf::{Anybuf, Bufany};
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Binary, Decimal, QuerierWrapper, StdResult, Uint128};
use std::{ops::Add, str::FromStr};

use crate::{asset::Asset, L1Asset};

#[cw_serde]
pub enum L1PoolStatus {
    Unknown,
    Available,
    Staged,
    Suspended,
}

impl From<i32> for L1PoolStatus {
    fn from(value: i32) -> Self {
        match value {
            0 => L1PoolStatus::Unknown,
            1 => L1PoolStatus::Available,
            2 => L1PoolStatus::Staged,
            3 => L1PoolStatus::Suspended,
            x => panic!("Invalid L1PoolStatus {}", x),
        }
    }
}

#[cw_serde]
pub struct L1Pool {
    pub balance_rune: Uint128,
    pub balance_asset: Uint128,
    pub asset: Asset,
    pub lp_units: Uint128,
    pub status: L1PoolStatus,
    pub decimals: u8,
    pub synth_units: Uint128,
    pub pending_inbound_rune: Uint128,
    pub pending_inbound_asset: Uint128,
    pub status_since: u64,
}

impl L1Pool {
    pub fn load(q: QuerierWrapper, asset: &L1Asset) -> StdResult<Self> {
        let data = Anybuf::new().append_string(1, asset.to_string()).into_vec();
        let res = q
            .query_grpc("/thorchain.Query/Pool".to_string(), Binary::from(data))?
            .to_vec();
        let m = Bufany::deserialize(&res).unwrap();
        Ok(Self {
            balance_rune: Uint128::from_str(m.string(1).unwrap_or_default().as_str())?,
            balance_asset: Uint128::from_str(m.string(2).unwrap_or_default().as_str())?,
            asset: Asset::L1(asset.clone()),
            lp_units: Uint128::from_str(m.string(4).unwrap_or_default().as_str())?,
            status: m.int32(5).unwrap_or_default().into(),
            decimals: m.int64(6).unwrap_or_default().try_into().unwrap(),
            synth_units: Uint128::from_str(m.string(7).unwrap_or_default().as_str())?,
            pending_inbound_rune: Uint128::from_str(m.string(8).unwrap_or_default().as_str())?,
            pending_inbound_asset: Uint128::from_str(m.string(9).unwrap_or_default().as_str())?,
            status_since: m.int64(10).unwrap_or_default().try_into().unwrap(),
        })
    }

    pub fn units(&self) -> Uint128 {
        self.lp_units.add(self.synth_units)
    }

    pub fn price_tor(&self) -> Decimal {
        Decimal::from_str("69000").unwrap()
    }
}
