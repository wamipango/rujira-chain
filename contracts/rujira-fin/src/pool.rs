use cosmwasm_schema::cw_serde;
use cosmwasm_std::{
    Addr, Decimal, Fraction, StdError, StdResult, Storage, Timestamp, Uint128, Uint256,
};
use cw_storage_plus::{IntKey, Key, KeyDeserialize, Map, Prefixer, PrimaryKey};
use rujira_rs::{
    bid_pool,
    fin::{Price, Side},
    Frac,
};

use crate::{order::Order, ContractError};
const SNAPSHOTS: Map<(PoolKey, bid_pool::SumSnapshotKey), Frac> = Map::new("snapshots");
const POOLS: Map<PoolKey, Pool> = Map::new("pools");
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
        POOLS.load(storage, key.clone()).unwrap_or_else(|_| Self {
            price: price.clone(),
            side: side.clone(),
            pool: BID_POOLS.load(storage, key).unwrap_or_default(),
        })
    }

    pub fn save(&self, storage: &mut dyn Storage) -> StdResult<()> {
        let key = PoolKey::new(self.side.clone(), self.price.clone());
        BID_POOLS.save(storage, key.clone(), &self.pool)?;
        // Clear empty pools so they're not iterated over during a swap
        if self.pool.is_zero() {
            POOLS.remove(storage, key);
            return Ok(());
        }

        POOLS.save(storage, key, self)
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

// Provided as a type to prefix on price type, without duplicating in the key
#[derive(Clone, Debug, PartialEq)]
enum PoolType {
    Fixed,
    Oracle,
}

impl PrimaryKey<'_> for PoolType {
    type Prefix = ();
    type SubPrefix = ();
    type Suffix = ();
    type SuperSuffix = ();

    fn key(&self) -> std::vec::Vec<Key<'_>> {
        match self {
            PoolType::Fixed => vec![Key::Val8([0])],
            PoolType::Oracle => vec![Key::Val8([1])],
        }
    }
}

impl<'a> Prefixer<'a> for PoolType {
    fn prefix(&self) -> Vec<Key> {
        self.key()
    }
}

impl KeyDeserialize for PoolType {
    type Output = Self;
    const KEY_ELEMS: u16 = 1;

    fn from_vec(value: Vec<u8>) -> cosmwasm_std::StdResult<Self::Output> {
        match value.first() {
            Some(0u8) => Ok(Self::Fixed),
            Some(1u8) => Ok(Self::Oracle),
            _ => Err(StdError::generic_err("invalid PoolType key")),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct PoolKey {
    side: Side,
    price: Price,
}

impl PoolKey {
    pub fn new(side: Side, price: Price) -> Self {
        Self { side, price }
    }
}

impl<'a> PrimaryKey<'a> for PoolKey {
    type Prefix = (Side, PoolType);
    type SubPrefix = Side;
    type Suffix = Price;
    type SuperSuffix = (PoolType, Price);

    fn key(&self) -> std::vec::Vec<Key<'_>> {
        let mut key = self.side.key();

        match self.price {
            Price::Fixed(fixed) => {
                key.extend(PoolType::Fixed.key());
                key.extend(vec![Key::Val128(fixed.numerator().to_be_bytes())])
            }
            Price::Oracle(deviation) => {
                key.extend(PoolType::Oracle.key());
                key.extend(vec![Key::Val8(deviation.to_cw_bytes())])
            }
        };

        key
    }
}

impl<'a> Prefixer<'a> for PoolKey {
    fn prefix(&self) -> Vec<Key> {
        self.key()
    }
}

impl KeyDeserialize for PoolKey {
    type Output = Self;
    const KEY_ELEMS: u16 = 3;

    fn from_vec(value: Vec<u8>) -> cosmwasm_std::StdResult<Self::Output> {
        let side = <Side>::from_vec(value[..1].to_vec())?;
        let price = <Price>::from_vec(value[2..].to_vec())?;
        Ok(Self { side, price })
    }
}

#[cfg(test)]

mod tests {
    use super::*;
    use cosmwasm_std::{testing::MockStorage, Decimal};
    use rujira_rs::fin::{Price, Side};
    use std::str::FromStr;

    #[test]
    fn pool_key() {
        let key = PoolKey::new(Side::Ask, Price::Fixed(Decimal::from_str("1.0").unwrap()));
        let key_bytes = key.key();

        assert_eq!(key_bytes.len(), 3);
        assert_eq!(key_bytes[0].as_ref(), &[0]);
        assert_eq!(key_bytes[1].as_ref(), &[0]);
        assert_eq!(
            key_bytes[2].as_ref(),
            &[0, 0, 0, 0, 0, 0, 0, 0, 13, 224, 182, 179, 167, 100, 0, 0]
        );

        let key = PoolKey::new(Side::Bid, Price::Oracle(-15));
        let key_bytes = key.key();

        assert_eq!(key_bytes[0].as_ref(), &[1]);
        assert_eq!(key_bytes[1].as_ref(), &[1]);
        assert_eq!(key_bytes[2].as_ref(), &[113]);

        let key = PoolKey::new(Side::Bid, Price::Oracle(21));
        let key_bytes = key.key();

        assert_eq!(key_bytes[0].as_ref(), &[1]);
        assert_eq!(key_bytes[1].as_ref(), &[1]);
        assert_eq!(key_bytes[2].as_ref(), &[149]);

        let key = PoolKey::new(Side::Bid, Price::Oracle(0));
        let key_bytes = key.key();

        assert_eq!(key_bytes[0].as_ref(), &[1]);
        assert_eq!(key_bytes[1].as_ref(), &[1]);
        assert_eq!(key_bytes[2].as_ref(), &[128]);

        assert_eq!(
            <PoolKey>::from_slice(&[1, 1, 128]).unwrap(),
            PoolKey::new(Side::Bid, Price::Oracle(0))
        );
        assert_eq!(
            <PoolKey>::from_slice(&[0, 1, 128]).unwrap(),
            PoolKey::new(Side::Ask, Price::Oracle(0))
        );

        assert_eq!(
            <PoolKey>::from_slice(&[
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 13, 224, 182, 179, 167, 100, 0, 0
            ])
            .unwrap(),
            PoolKey::new(Side::Ask, Price::Fixed(Decimal::one()))
        );

        assert_eq!(
            <PoolKey>::from_slice(&[
                1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 13, 224, 182, 179, 167, 100, 0, 0
            ])
            .unwrap(),
            PoolKey::new(Side::Bid, Price::Fixed(Decimal::one()))
        );
    }

    #[test]
    fn pool_key_iter() {
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

        let iter = POOLS.range(&store, None, None, cosmwasm_std::Order::Ascending);

        let ordered: Vec<StdResult<(PoolKey, Pool)>> = iter.collect();
        assert_eq!(ordered.len(), 6);
        dbg!(ordered);

        // assert_eq!(key_bytes.len(), 2);
    }
}
