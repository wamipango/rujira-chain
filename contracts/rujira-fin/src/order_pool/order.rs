use crate::error::ContractError;
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, StdResult, Storage, Timestamp, Uint128};
use cw_storage_plus::{Bound, Map};
use rujira_rs::{
    bid_pool,
    fin::{Price, Side},
};

use super::{Pool, PoolKey};

pub const ORDERS: Map<(Addr, Side, Price), (Timestamp, Uint128, bid_pool::Bid)> =
    Map::new("orders");
const DEFAULT_LIMIT: u8 = 10;

#[cw_serde]
pub struct Order {
    pub owner: Addr,
    pub updated_at: Timestamp,
    /// Original offer amount, as it was at `updated_at` time
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
            .load(storage, (owner.clone(), side.clone(), price.clone()))
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
        side: Option<Side>,
        start_after: Option<(Side, Price)>,
        limit: Option<u8>,
    ) -> StdResult<Vec<(PoolKey, Self)>> {
        let limit = limit.unwrap_or(DEFAULT_LIMIT) as usize;
        match side {
            Some(side) => {
                let cursor = start_after.map(|(_, price)| price);
                Self::by_owner_side(storage, owner, side, cursor, limit)
            }
            None => Self::by_owner_all(storage, owner, start_after, limit),
        }
    }

    pub fn all(
        storage: &dyn Storage,
        side: Option<Side>,
        start_after: Option<(Addr, Side, Price)>,
        limit: Option<u8>,
    ) -> StdResult<Vec<(PoolKey, Self)>> {
        let limit = limit.unwrap_or(DEFAULT_LIMIT) as usize;
        let min = start_after
            .as_ref()
            .map(|(addr, s, p)| Bound::exclusive((addr.clone(), s.clone(), p.clone())));
        ORDERS
            .range(storage, min, None, cosmwasm_std::Order::Ascending)
            .filter(|x| match (&side, x) {
                (Some(s), Ok(((_, ref ks, _), _))) => ks == s,
                _ => true,
            })
            .take(limit)
            .map(|x| {
                x.map(|((owner, side, price), (updated_at, offer, bid))| {
                    (
                        PoolKey::new(side, price),
                        Self {
                            owner,
                            updated_at,
                            offer,
                            bid,
                        },
                    )
                })
            })
            .collect()
    }

    fn by_owner_all(
        storage: &dyn Storage,
        owner: &Addr,
        start_after: Option<(Side, Price)>,
        limit: usize,
    ) -> StdResult<Vec<(PoolKey, Self)>> {
        let min = start_after
            .as_ref()
            .map(|(s, p)| Bound::exclusive((s.clone(), p.clone())));
        ORDERS
            .sub_prefix(owner.clone())
            .range(storage, min, None, cosmwasm_std::Order::Ascending)
            .take(limit)
            .map(|x| {
                x.map(|(k, (updated_at, offer, bid))| {
                    (
                        PoolKey::new(k.0, k.1),
                        Self {
                            owner: owner.clone(),
                            updated_at,
                            offer,
                            bid,
                        },
                    )
                })
            })
            .collect()
    }

    fn by_owner_side(
        storage: &dyn Storage,
        owner: &Addr,
        side: Side,
        start_after: Option<Price>,
        limit: usize,
    ) -> StdResult<Vec<(PoolKey, Self)>> {
        let order = match side {
            Side::Base => cosmwasm_std::Order::Ascending,
            Side::Quote => cosmwasm_std::Order::Descending,
        };

        let bound = start_after.as_ref().map(|p| Bound::exclusive(p.clone()));
        let (min, max) = match side {
            Side::Base => (bound, None),
            Side::Quote => (None, bound),
        };

        ORDERS
            .prefix((owner.clone(), side.clone()))
            .range(storage, min, max, order)
            .take(limit)
            .map(|x| {
                x.map(|(k, (updated_at, offer, bid))| {
                    (
                        PoolKey::new(side.clone(), k),
                        Self {
                            owner: owner.clone(),
                            updated_at,
                            offer,
                            bid,
                        },
                    )
                })
            })
            .collect()
    }

    pub fn amount(&self) -> Uint128 {
        self.bid.amount().try_into().unwrap()
    }

    pub fn save(&self, storage: &mut dyn Storage, pool: &Pool) -> StdResult<()> {
        if self.bid.is_empty() {
            self.remove(storage, pool);
            return Ok(());
        }
        ORDERS.save(
            storage,
            (self.owner.clone(), pool.side.clone(), pool.price.clone()),
            &(self.updated_at, self.offer, self.bid.clone()),
        )?;
        Ok(())
    }

    fn remove(&self, storage: &mut dyn Storage, pool: &Pool) {
        ORDERS.remove(
            storage,
            (self.owner.clone(), pool.side.clone(), pool.price.clone()),
        )
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;
    use cosmwasm_std::{testing::MockStorage, Addr, Decimal, Timestamp, Uint128};
    use rujira_rs::fin::{Price, Side};

    use crate::order_pool::Pool;

    #[test]
    fn query_order() {
        let mut store = MockStorage::new();
        let timestamp = Timestamp::default();
        let owner = Addr::unchecked("owner");
        let offer = Uint128::from(100u128);
        let oracle = Some(Decimal::one());
        let mut pool = Pool::load(&store, &Price::Oracle(0), &Side::Quote, &oracle).unwrap();
        pool.create_order(&mut store, &timestamp, &owner, offer)
            .unwrap();

        pool.commit(&mut store).unwrap();

        let order = Order::load(&store, &owner, &Side::Quote, &Price::Oracle(0)).unwrap();
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
        let oracle = Some(Decimal::one());
        let mut pool1 = Pool::load(&store, &Price::Oracle(0), &Side::Quote, &oracle).unwrap();
        let mut pool2 = Pool::load(&store, &Price::Oracle(1), &Side::Quote, &oracle).unwrap();
        let mut pool3 = Pool::load(&store, &Price::Oracle(2), &Side::Quote, &oracle).unwrap();
        let mut pool4 = Pool::load(
            &store,
            &Price::Fixed(Decimal::from_str("1.0").unwrap()),
            &Side::Quote,
            &oracle,
        )
        .unwrap();
        let mut pool5 = Pool::load(
            &store,
            &Price::Fixed(Decimal::from_str("1.1").unwrap()),
            &Side::Quote,
            &oracle,
        )
        .unwrap();
        let mut pool6 = Pool::load(
            &store,
            &Price::Fixed(Decimal::from_str("1.2").unwrap()),
            &Side::Quote,
            &oracle,
        )
        .unwrap();
        let mut pool7 = Pool::load(
            &store,
            &Price::Fixed(Decimal::from_str("1.2").unwrap()),
            &Side::Base,
            &oracle,
        )
        .unwrap();

        pool1
            .create_order(&mut store, &timestamp, &owner, offer)
            .unwrap();
        pool2
            .create_order(&mut store, &timestamp, &owner, offer)
            .unwrap();
        pool3
            .create_order(&mut store, &timestamp, &owner, offer)
            .unwrap();
        pool4
            .create_order(&mut store, &timestamp, &owner, offer)
            .unwrap();
        pool5
            .create_order(&mut store, &timestamp, &owner, offer)
            .unwrap();
        pool6
            .create_order(&mut store, &timestamp, &owner, offer)
            .unwrap();
        pool7
            .create_order(&mut store, &timestamp, &owner, offer)
            .unwrap();

        pool1
            .create_order(&mut store, &timestamp, &owner2, offer)
            .unwrap();

        pool1.commit(&mut store).unwrap();

        let orders = Order::by_owner(&store, &owner, None, None, None).unwrap();
        assert_eq!(orders.len(), 7);
        assert_eq!(orders[0].1.owner, owner);
        assert_eq!(orders[0].1.offer, offer);

        let orders = Order::by_owner(&store, &owner, Some(Side::Quote), None, None).unwrap();
        assert_eq!(orders.len(), 6);
        assert_eq!(orders[0].1.owner, owner);
        assert_eq!(orders[0].1.offer, offer);
    }

    #[test]
    fn query_orders_by_owner_cursor_pagination() {
        let mut store = MockStorage::new();
        let timestamp = Timestamp::default();
        let owner = Addr::unchecked("owner");
        let offer = Uint128::from(100u128);
        let oracle = Some(Decimal::one());

        let mut pool1 = Pool::load(&store, &Price::Oracle(0), &Side::Quote, &oracle).unwrap();
        let mut pool2 = Pool::load(&store, &Price::Oracle(1), &Side::Quote, &oracle).unwrap();
        let mut pool3 = Pool::load(&store, &Price::Oracle(2), &Side::Quote, &oracle).unwrap();

        pool1
            .create_order(&mut store, &timestamp, &owner, offer)
            .unwrap();
        pool2
            .create_order(&mut store, &timestamp, &owner, offer)
            .unwrap();
        pool3
            .create_order(&mut store, &timestamp, &owner, offer)
            .unwrap();
        pool1.commit(&mut store).unwrap();

        // First page: limit 2
        let page1 = Order::by_owner(&store, &owner, None, None, Some(2)).unwrap();
        assert_eq!(page1.len(), 2);

        // Second page: cursor from last item of page1
        let last = &page1[1];
        let cursor = Some((last.0.side.clone(), last.0.price.clone()));
        let page2 = Order::by_owner(&store, &owner, None, cursor, Some(2)).unwrap();
        assert_eq!(page2.len(), 1);

        // Side-specific cursor pagination (Quote, descending)
        let all_quote = Order::by_owner(&store, &owner, Some(Side::Quote), None, None).unwrap();
        assert_eq!(all_quote.len(), 3);

        let cursor = Some((all_quote[0].0.side.clone(), all_quote[0].0.price.clone()));
        let after_first = Order::by_owner(&store, &owner, Some(Side::Quote), cursor, None).unwrap();
        assert_eq!(after_first.len(), 2);
    }

    #[test]
    fn query_all_orders() {
        let mut store = MockStorage::new();
        let timestamp = Timestamp::default();
        let owner1 = Addr::unchecked("alice");
        let owner2 = Addr::unchecked("bob");
        let offer = Uint128::from(100u128);
        let oracle = Some(Decimal::one());

        let mut pool1 = Pool::load(&store, &Price::Oracle(0), &Side::Quote, &oracle).unwrap();
        let mut pool2 = Pool::load(&store, &Price::Oracle(1), &Side::Base, &oracle).unwrap();

        pool1
            .create_order(&mut store, &timestamp, &owner1, offer)
            .unwrap();
        pool1
            .create_order(&mut store, &timestamp, &owner2, offer)
            .unwrap();
        pool2
            .create_order(&mut store, &timestamp, &owner1, offer)
            .unwrap();
        pool1.commit(&mut store).unwrap();

        // All orders, no filter
        let all = Order::all(&store, None, None, None).unwrap();
        assert_eq!(all.len(), 3);

        // Filter by side
        let quote_only = Order::all(&store, Some(Side::Quote), None, None).unwrap();
        assert_eq!(quote_only.len(), 2);

        let base_only = Order::all(&store, Some(Side::Base), None, None).unwrap();
        assert_eq!(base_only.len(), 1);

        // Cursor pagination across all orders
        let page1 = Order::all(&store, None, None, Some(2)).unwrap();
        assert_eq!(page1.len(), 2);

        let last = &page1[1];
        let cursor = Some((
            last.1.owner.clone(),
            last.0.side.clone(),
            last.0.price.clone(),
        ));
        let page2 = Order::all(&store, None, cursor, Some(2)).unwrap();
        assert_eq!(page2.len(), 1);
    }

    #[test]
    fn query_all_orders_empty() {
        let store = MockStorage::new();
        let all = Order::all(&store, None, None, None).unwrap();
        assert!(all.is_empty());
    }
}
