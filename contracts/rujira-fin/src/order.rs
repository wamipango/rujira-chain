use std::cmp::min;

use crate::{error::ContractError, pool::Pool, pool_key::PoolKey};
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, StdResult, Storage, Timestamp, Uint128};
use cw_storage_plus::Map;
use rujira_rs::{
    bid_pool,
    fin::{Price, Side},
};

pub const ORDERS: Map<(Addr, PoolKey), (Timestamp, Uint128, bid_pool::Bid)> = Map::new("orders");
const MAX_LIMIT: u8 = 31;
const DEFAULT_LIMIT: u8 = 10;

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
        let (updated_at, offer, bid) = ORDERS
            .load(
                storage,
                (owner.clone(), PoolKey::new(side.clone(), price.clone())),
            )
            .map_err(|_| ContractError::NotFound {})?;
        Ok(Self {
            owner: owner.clone(),
            updated_at,
            offer,
            bid,
        })
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
            .map(|x| {
                x.map(|(_, (updated_at, offer, bid))| Self {
                    owner: owner.clone(),
                    updated_at,
                    offer,
                    bid,
                })
            })
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
                PoolKey::new(pool.side.clone(), pool.price.clone()),
            ),
            &(self.updated_at, self.offer, self.bid.clone()),
        )?;
        Ok(())
    }

    fn remove(&self, storage: &mut dyn Storage, pool: &Pool) {
        ORDERS.remove(
            storage,
            (
                self.owner.clone(),
                PoolKey::new(pool.side.clone(), pool.price.clone()),
            ),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::{testing::MockStorage, Addr, Timestamp, Uint128};
    use rujira_rs::fin::{Price, Side};

    use crate::{pool::Pool, pool_key::PoolKey};

    #[test]
    fn query_order() {
        let mut store = MockStorage::new();
        let timestamp = Timestamp::default();
        let owner = Addr::unchecked("owner");
        let offer = Uint128::from(100u128);
        let mut pool = Pool::load(&store, &Price::Oracle(0), &Side::Bid);
        let k = PoolKey::new(Side::Bid, Price::Oracle(0));
        pool.create_order(&mut store, timestamp, &owner, offer)
            .unwrap();

        pool.save(&mut store).unwrap();

        let order = Order::load(&store, &owner, &Side::Bid, &Price::Oracle(0)).unwrap();
        assert_eq!(order.owner, owner);
        assert_eq!(order.offer, offer);
    }

    #[test]
    fn query_orders_by_owner() {
        let mut store = MockStorage::new();
        let timestamp = Timestamp::default();
        let owner = Addr::unchecked("owner");
        let owner2 = Addr::unchecked("owner2");
        let offer = Uint128::from(100u128);
        let mut pool1 = Pool::load(&store, &Price::Oracle(0), &Side::Bid);
        let mut pool2 = Pool::load(&store, &Price::Oracle(1), &Side::Bid);
        let mut pool3 = Pool::load(&store, &Price::Oracle(2), &Side::Bid);
        pool1
            .create_order(&mut store, timestamp, &owner, offer)
            .unwrap();
        pool2
            .create_order(&mut store, timestamp, &owner, offer)
            .unwrap();
        pool3
            .create_order(&mut store, timestamp, &owner, offer)
            .unwrap();
        pool1
            .create_order(&mut store, timestamp, &owner2, offer)
            .unwrap();

        pool1.save(&mut store).unwrap();

        let orders = Order::by_owner(&store, &owner, None, None).unwrap();
        assert_eq!(orders.len(), 3);
        assert_eq!(orders[0].owner, owner);
        assert_eq!(orders[0].offer, offer);
    }
}
