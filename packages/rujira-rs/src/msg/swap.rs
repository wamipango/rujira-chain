use cosmwasm_std::{Addr, CosmosMsg, Uint256};

use crate::asset::Asset;
use crate::coin::Coin;
use crate::memoed::{Memo, Memoed};

use super::deposit::MsgDeposit;

#[derive(Clone)]
pub struct MsgSwap {
    from: Coin,
    to: Asset,
    destination: Destination,
    slip: Option<Slip>,
    affiliate: Option<Affiliate>,
    dex: Option<Dex>,
}

impl MsgSwap {
    pub fn new<T: Into<Asset>>(
        from: Coin,
        to: T,
        destination: Destination,
        slip: Option<Slip>,
        affiliate: Option<Affiliate>,
        dex: Option<Dex>,
    ) -> Self {
        Self {
            from,
            to: to.into(),
            destination,
            slip,
            affiliate,
            dex,
        }
    }
}

impl Memoed for MsgSwap {
    fn to_memo(&self) -> String {
        Memo::default()
            .push(&"=")
            .push(&self.to)
            .push(&self.destination)
            .push(&self.slip)
            .push(&self.affiliate)
            .push(&self.dex)
            .to_string()
    }
}

impl From<MsgSwap> for CosmosMsg {
    fn from(value: MsgSwap) -> Self {
        MsgDeposit::new(vec![value.from.clone()], value.to_memo()).into()
    }
}

#[derive(Clone)]
pub enum Slip {
    Limit(Uint256),
    Stream {
        limit: Uint256,
        interval: u8,
        quantity: Uint256,
    },
}

#[derive(Clone)]
pub enum Destination {
    Direct(Addr),
    Refundable {
        destination_addr: Addr,
        refund_addr: Addr,
    },
}

#[derive(Clone)]
pub struct Affiliate {
    pub addr: Addr,
    pub basis_points: i8,
}

impl Affiliate {
    pub fn new(addr: Addr, basis_points: i8) -> Self {
        Self { addr, basis_points }
    }
}

#[derive(Clone)]
pub struct Dex {
    aggregator: String,
    target_address: Addr,
    limit: Option<Uint256>,
}

impl Memoed for Destination {
    fn to_memo(&self) -> String {
        match self {
            Destination::Direct(addr) => addr.to_string(),
            Destination::Refundable {
                destination_addr,
                refund_addr,
            } => format!("{}/{}", destination_addr, refund_addr),
        }
    }
}

impl Memoed for Slip {
    fn to_memo(&self) -> String {
        match self {
            Slip::Limit(lim) => lim.to_string(),
            Slip::Stream {
                limit,
                interval,
                quantity,
            } => format!("{}/{}/{}", limit, interval, quantity),
        }
    }
}

impl Memoed for Affiliate {
    fn to_memo(&self) -> String {
        format!("{}:{}", self.addr, self.basis_points)
    }
}

impl Memoed for Dex {
    fn to_memo(&self) -> String {
        format!(
            "{}:{}:{}",
            self.aggregator,
            self.target_address,
            match self.limit {
                Some(x) => x.to_string(),
                None => "".to_string(),
            }
        )
    }
}
