use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, StdResult, Storage, Timestamp, Uint128};
use cw_storage_plus::Map;
use rujira_rs::{
    bid_pool,
    fin::{Price, Side},
    Frac,
};

use crate::{order::Order, pool_key::PoolKey, ContractError};
const SNAPSHOTS: Map<(PoolKey, bid_pool::SumSnapshotKey), Frac> = Map::new("snapshots");
// The POOLS Map is used simply as an indicator that there is a non-zero BidPool at this key
// The BID_POOLS Map is used to store the BidPool itself, and the Key is used to populate the Pool values
const POOLS: Map<PoolKey, ()> = Map::new("pools");
const BID_POOLS: Map<PoolKey, bid_pool::Pool> = Map::new("bid-pools");

/// A wrapper around a BidPool to provide a side & price, used for keying orders and
/// storing pools for iterating during execution
#[cw_serde]
pub struct Pool {
    pub price: Price,
    pub side: Side,
    pub pool: bid_pool::Pool,
}

impl Pool {
    pub fn load(storage: &dyn Storage, price: &Price, side: &Side) -> Self {
        let key = PoolKey::new(side.clone(), price.clone());
        Self {
            price: price.clone(),
            side: side.clone(),
            pool: BID_POOLS.load(storage, key).unwrap_or_default(),
        }
    }

    pub fn save(&self, storage: &mut dyn Storage) -> StdResult<()> {
        let key = PoolKey::new(self.side.clone(), self.price.clone());
        BID_POOLS.save(storage, key.clone(), &self.pool)?;
        // Clear empty pools so they're not iterated over during a swap
        if self.pool.is_zero() {
            POOLS.remove(storage, key);
            return Ok(());
        }

        POOLS.save(storage, key, &())
    }

    pub fn create_order(
        &mut self,
        storage: &mut dyn Storage,
        timestamp: Timestamp,
        owner: &Addr,
        offer: Uint128,
    ) -> StdResult<Order> {
        let order = Order {
            owner: owner.clone(),
            offer: offer.clone(),
            updated_at: timestamp,
            bid: self.pool.new_bid(offer.into()),
        };
        self.save(storage)?;
        order.save(storage, &self)?;
        Ok(order)
    }

    pub fn load_order(&self, storage: &dyn Storage, owner: &Addr) -> Result<Order, ContractError> {
        let mut order = Order::load(storage, owner, &self.side, &self.price)?;
        self.sync_order(storage, &mut order)?;
        Ok(order)
    }

    pub fn retract_order(
        &mut self,
        storage: &mut dyn Storage,
        owner: &Addr,
    ) -> Result<Uint128, ContractError> {
        let mut order = self.load_order(storage, owner)?;
        let refund_amount = order.bid.retract(&mut self.pool, None)?;
        order.save(storage, self)?;
        self.save(storage)?;
        Ok(Uint128::try_from(refund_amount)?)
    }

    pub fn claim_order(
        &mut self,
        storage: &mut dyn Storage,
        owner: &Addr,
    ) -> Result<Uint128, ContractError> {
        let mut order = self.load_order(storage, owner)?;
        let claimed = order.bid.claim_filled();
        order.save(storage, self)?;
        Ok(Uint128::try_from(claimed)?)
    }

    pub fn sync_order(
        &self,
        storage: &dyn Storage,
        order: &mut Order,
    ) -> Result<(), ContractError> {
        let sum_snapshot = self.sum_snapshot(storage, &order.bid).ok();
        Ok(self.pool.sync_bid(&mut order.bid, sum_snapshot)?)
    }

    fn sum_snapshot(
        &self,
        storage: &dyn Storage,
        bid: &bid_pool::Bid,
    ) -> StdResult<bid_pool::SumSnapshot> {
        let key = (
            PoolKey::new(self.side.clone(), self.price.clone()),
            bid.sum_snapshot_key(),
        );
        let sum = SNAPSHOTS.load(storage, key)?;
        Ok(bid_pool::SumSnapshot {
            epoch: todo!(),
            scale: todo!(),
            sum,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::pool_key::PoolType;

    use super::*;
    use cosmwasm_std::{testing::MockStorage, Decimal, Decimal256, Uint256};
    use cw_storage_plus::Bound;
    use rujira_rs::fin::{Price, Side};
    use std::str::FromStr;

    #[test]
    // Verify that when a Pool is removed from storage, the BidPool is retained and the correct values used for syncing bids
    fn pool_bid_pool_replacement() {
        let mut store = MockStorage::new();
        let timestamp = Timestamp::default();
        let owner = Addr::unchecked("owner");
        let offer = Uint128::from(100u128);
        let mut pool = Pool::load(&store, &Price::Oracle(0), &Side::Bid);
        let k = PoolKey::new(Side::Bid, Price::Oracle(0));
        pool.create_order(&mut store, timestamp, &owner, offer)
            .unwrap();

        pool.save(&mut store).unwrap();
        // Check both the pool and bid pool are stored
        POOLS.load(&store, k.clone()).unwrap();
        BID_POOLS.load(&store, k.clone()).unwrap();

        pool.pool
            .distribute(Uint256::from(100u128), &Decimal256::one())
            .unwrap();

        // Bid Pool should have been emptied, so the container Pool shold be cleared, but the BidPool should remain
        pool.save(&mut store).unwrap();
        POOLS.load(&store, k.clone()).unwrap_err();
        let bp = BID_POOLS.load(&store, k.clone()).unwrap();
        // Check it's different from the default
        assert_ne!(bp, bid_pool::Pool::default());

        // Now check it's restored to the pool correctly
        let pool = Pool::load(&store, &Price::Oracle(0), &Side::Bid);
        assert_eq!(pool.pool, bp);
    }

    #[test]
    fn pools_map() {
        let mut store = MockStorage::new();
        let timestamp = Timestamp::default();
        let owner = Addr::unchecked("owner");
        let offer = Uint128::from(100u128);
        Pool::load(&store, &Price::Oracle(0), &Side::Bid)
            .create_order(&mut store, timestamp, &owner, offer)
            .unwrap();

        Pool::load(&store, &Price::Oracle(-1), &Side::Bid)
            .create_order(&mut store, timestamp, &owner, offer)
            .unwrap();

        Pool::load(&store, &Price::Oracle(1), &Side::Bid)
            .create_order(&mut store, timestamp, &owner, offer)
            .unwrap();

        Pool::load(&store, &Price::Oracle(2), &Side::Bid)
            .create_order(&mut store, timestamp, &owner, offer)
            .unwrap();

        Pool::load(&store, &Price::Oracle(2), &Side::Ask)
            .create_order(&mut store, timestamp, &owner, offer)
            .unwrap();

        Pool::load(&store, &Price::Fixed(Decimal::one()), &Side::Ask)
            .create_order(&mut store, timestamp, &owner, offer)
            .unwrap();

        Pool::load(
            &store,
            &Price::Fixed(Decimal::from_ratio(10u128, 9u128)),
            &Side::Bid,
        )
        .create_order(&mut store, timestamp, &owner, offer)
        .unwrap();

        let pool = Pool::load(&store, &Price::Oracle(0), &Side::Bid);
        assert_eq!(pool.price, Price::Oracle(0));
        assert_eq!(pool.side, Side::Bid);

        let iter = POOLS.range(&store, None, None, cosmwasm_std::Order::Ascending);

        let ordered = iter.collect::<StdResult<Vec<(PoolKey, ())>>>().unwrap();
        assert_eq!(ordered.len(), 7);

        // Result should be ordered by Side, then Price Type, then Price Value
        let (key, _) = ordered[0].clone();
        assert_eq!(key.side, Side::Ask);
        assert_eq!(key.price, Price::Fixed(Decimal::one()));

        let (key, _) = ordered[1].clone();
        assert_eq!(key.side, Side::Ask);
        assert_eq!(key.price, Price::Oracle(2));

        let (key, _) = ordered[2].clone();
        assert_eq!(key.side, Side::Bid);
        assert_eq!(
            key.price,
            Price::Fixed(Decimal::from_str("1.111111111111111111").unwrap())
        );

        let (key, _) = ordered[3].clone();
        assert_eq!(key.side, Side::Bid);
        assert_eq!(key.price, Price::Oracle(-1));

        let (key, _) = ordered[4].clone();
        assert_eq!(key.side, Side::Bid);
        assert_eq!(key.price, Price::Oracle(0));

        let (key, _) = ordered[5].clone();
        assert_eq!(key.side, Side::Bid);
        assert_eq!(key.price, Price::Oracle(1));

        let (key, _) = ordered[6].clone();
        assert_eq!(key.side, Side::Bid);
        assert_eq!(key.price, Price::Oracle(2));

        let iter =
            POOLS
                .sub_prefix(Side::Ask)
                .range(&store, None, None, cosmwasm_std::Order::Ascending);

        let ordered = iter
            .collect::<StdResult<Vec<((PoolType, Price), ())>>>()
            .unwrap();

        assert_eq!(ordered.len(), 2);

        let (key, _) = ordered[0].clone();
        assert_eq!(key.0, PoolType::Fixed);
        assert_eq!(key.1, Price::Fixed(Decimal::one()));

        let (key, _) = ordered[1].clone();
        assert_eq!(key.0, PoolType::Oracle);
        assert_eq!(key.1, Price::Oracle(2));

        let iter = POOLS.prefix((Side::Bid, PoolType::Oracle)).range(
            &store,
            None,
            None,
            cosmwasm_std::Order::Ascending,
        );

        let ordered = iter.collect::<StdResult<Vec<(Price, ())>>>().unwrap();

        assert_eq!(ordered.len(), 4);

        let (price, _) = ordered[0].clone();
        assert_eq!(price, Price::Oracle(-1));

        let (price, _) = ordered[1].clone();
        assert_eq!(price, Price::Oracle(0));

        let (price, _) = ordered[2].clone();
        assert_eq!(price, Price::Oracle(1));

        let (price, _) = ordered[3].clone();
        assert_eq!(price, Price::Oracle(2));

        let iter = POOLS.prefix((Side::Bid, PoolType::Oracle)).range(
            &store,
            Some(Bound::exclusive(Price::Oracle(0))),
            None,
            cosmwasm_std::Order::Ascending,
        );

        let ordered = iter.collect::<StdResult<Vec<(Price, ())>>>().unwrap();

        assert_eq!(ordered.len(), 2);

        let (price, _) = ordered[0].clone();
        assert_eq!(price, Price::Oracle(1));

        let (price, _) = ordered[1].clone();
        assert_eq!(price, Price::Oracle(2));

        // assert_eq!(key_bytes.len(), 2);
    }
}
