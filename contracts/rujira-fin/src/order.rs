use std::cmp::min;

use crate::{error::ContractError, pool::Pool};
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, StdError, StdResult, Storage, Timestamp, Uint128};
use cw_storage_plus::{Key, KeyDeserialize, Map, Prefixer, PrimaryKey};
use rujira_rs::{
    bid_pool,
    fin::{Price, Side},
};

pub const ORDERS: Map<(Addr, OrderKey), Order> = Map::new("orders");
const MAX_LIMIT: u8 = 31;
const DEFAULT_LIMIT: u8 = 10;

#[derive(Clone)]
pub struct OrderKey(Side, Price);

impl OrderKey {
    pub fn new(side: Side, price: Price) -> Self {
        Self(side, price)
    }
}

impl PrimaryKey<'_> for OrderKey {
    type Prefix = ();
    type SubPrefix = ();
    type Suffix = ();
    type SuperSuffix = ();

    fn key(&self) -> Vec<cw_storage_plus::Key> {
        match self.0 {
            Side::Bid => [0u8].key(),
            Side::Ask => [1u8].key(),
        }
    }
}
impl<'a> Prefixer<'a> for OrderKey {
    fn prefix(&self) -> Vec<Key> {
        self.key()
    }
}
impl KeyDeserialize for OrderKey {
    type Output = Self;
    const KEY_ELEMS: u16 = 1;

    fn from_vec(value: Vec<u8>) -> cosmwasm_std::StdResult<Self::Output> {
        match value.first() {
            Some(0u8) => Ok(Self(Side::Ask, todo!())),
            Some(1u8) => Ok(Self(Side::Bid, todo!())),
            _ => Err(StdError::generic_err("invalid Side key")),
        }
    }
}

#[cw_serde]
pub struct Order {
    pub owner: Addr,
    pub updated_at: Timestamp,
    pub offer: Uint128,
    pub bid: bid_pool::Bid,
}

impl Order {
    pub fn load(
        storage: &dyn Storage,
        owner: &Addr,
        side: &Side,
        price: &Price,
    ) -> Result<Self, ContractError> {
        ORDERS
            .load(
                storage,
                (owner.clone(), OrderKey::new(side.clone(), price.clone())),
            )
            .map_err(|_| ContractError::NotFound {})
    }

    pub fn by_owner(
        storage: &dyn Storage,
        owner: &Addr,
        offset: Option<u8>,
        limit: Option<u8>,
    ) -> StdResult<Vec<Self>> {
        let limit: u8 = min(limit.unwrap_or(DEFAULT_LIMIT), MAX_LIMIT);
        ORDERS
            .prefix(owner.clone())
            .range(storage, None, None, cosmwasm_std::Order::Ascending)
            .skip(offset.unwrap_or(0) as usize)
            .take(limit as usize)
            .map(|x| x.map(|(_, o)| o))
            .collect()
    }

    pub fn save(&self, storage: &mut dyn Storage, pool: &Pool) -> StdResult<()> {
        if self.bid.is_empty() {
            self.remove(storage, pool);
            return Ok(());
        }
        ORDERS.save(
            storage,
            (
                self.owner.clone(),
                OrderKey::new(pool.side.clone(), pool.price.clone()),
            ),
            self,
        )?;
        Ok(())
    }

    fn remove(&self, storage: &mut dyn Storage, pool: &Pool) {
        ORDERS.remove(
            storage,
            (
                self.owner.clone(),
                OrderKey::new(pool.side.clone(), pool.price.clone()),
            ),
        )
    }
}
