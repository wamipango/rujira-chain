use std::str::FromStr;

use super::*;
use cosmwasm_std::{testing::MockStorage, Addr, Decimal, StdResult, Timestamp, Uint128};
use cw_storage_plus::Bound;
use itertools::EitherOrBoth;
use rujira_rs::{
    bid_pool,
    exchange::SwapItem,
    fin::{Price, Side},
};

use crate::{
    order_pool::{PoolKey, PoolType},
    ContractError,
};

#[test]
// Verify that when a Pool is removed from storage, the BidPool is retained and the correct values used for syncing bids
fn pool_bid_pool_replacement() {
    let mut store = MockStorage::new();
    let timestamp = Timestamp::default();
    let owner = Addr::unchecked("owner");
    let offer = Uint128::from(100u128);
    let price = Price::Fixed(Decimal::one());
    let oracle = Some(Decimal::one());
    let mut pool = Pool::load(&store, &price, &Side::Quote, &oracle).unwrap();
    let k = PoolKey::new(Side::Quote, price.clone());
    pool.create_order(&mut store, &timestamp, &owner, offer)
        .unwrap();

    pool.commit(&mut store).unwrap();
    // Check both the pool and bid pool are stored
    POOLS.load(&store, k.clone()).unwrap();
    BID_POOLS.load(&store, k.clone()).unwrap();

    let mut ctx = PoolContext::default();
    pool.swap(&mut ctx, Uint128::from(100u128)).unwrap();

    // Bid Pool should have been emptied, so the container Pool shold be cleared, but the BidPool should remain
    pool.commit(&mut store).unwrap();
    POOLS.load(&store, k.clone()).unwrap_err();
    let bp = BID_POOLS.load(&store, k.clone()).unwrap();
    // Check it's different from the default
    assert_ne!(bp, bid_pool::Pool::default());

    // Now check it's restored to the pool correctly
    let pool = Pool::load(&store, &price, &Side::Quote, &oracle).unwrap();
    assert_eq!(pool.pool, bp);
}

#[test]
fn oracle_pool_load_requires_oracle_price() {
    let store = MockStorage::new();
    let missing_oracle: Option<Decimal> = None;

    let err = Pool::load(&store, &Price::Oracle(0), &Side::Quote, &missing_oracle).unwrap_err();
    assert!(matches!(
        err,
        ContractError::OraclePriceUnavailable {
            side: Side::Quote,
            price: Price::Oracle(0),
        }
    ));

    let pool = Pool::load(
        &store,
        &Price::Fixed(Decimal::one()),
        &Side::Quote,
        &missing_oracle,
    )
    .unwrap();
    assert_eq!(pool.rate(), Decimal::one());
}

#[test]
fn pools_map() {
    let mut store = MockStorage::new();
    let timestamp = Timestamp::default();
    let owner = Addr::unchecked("owner");
    let offer = Uint128::from(100u128);
    let oracle = Some(Decimal::one());
    Pool::load(&store, &Price::Oracle(0), &Side::Quote, &oracle)
        .unwrap()
        .create_order(&mut store, &timestamp, &owner, offer)
        .unwrap();

    Pool::load(&store, &Price::Oracle(-1), &Side::Quote, &oracle)
        .unwrap()
        .create_order(&mut store, &timestamp, &owner, offer)
        .unwrap();

    Pool::load(&store, &Price::Oracle(1), &Side::Quote, &oracle)
        .unwrap()
        .create_order(&mut store, &timestamp, &owner, offer)
        .unwrap();

    Pool::load(&store, &Price::Oracle(2), &Side::Quote, &oracle)
        .unwrap()
        .create_order(&mut store, &timestamp, &owner, offer)
        .unwrap();

    Pool::load(&store, &Price::Oracle(2), &Side::Base, &oracle)
        .unwrap()
        .create_order(&mut store, &timestamp, &owner, offer)
        .unwrap();

    Pool::load(&store, &Price::Fixed(Decimal::one()), &Side::Base, &oracle)
        .unwrap()
        .create_order(&mut store, &timestamp, &owner, offer)
        .unwrap();

    Pool::load(
        &store,
        &Price::Fixed(Decimal::from_ratio(10u128, 9u128)),
        &Side::Quote,
        &oracle,
    )
    .unwrap()
    .create_order(&mut store, &timestamp, &owner, offer)
    .unwrap();

    let pool = Pool::load(&store, &Price::Oracle(0), &Side::Quote, &oracle).unwrap();
    assert_eq!(pool.price, Price::Oracle(0));
    assert_eq!(pool.side, Side::Quote);

    let iter = POOLS.range(&store, None, None, cosmwasm_std::Order::Ascending);

    let ordered = iter.collect::<StdResult<Vec<(PoolKey, ())>>>().unwrap();
    assert_eq!(ordered.len(), 7);

    // Result should be ordered by Side, then Price Type, then Price Value
    let (key, _) = ordered[0].clone();
    assert_eq!(key.side, Side::Base);
    assert_eq!(key.price, Price::Fixed(Decimal::one()));

    let (key, _) = ordered[1].clone();
    assert_eq!(key.side, Side::Base);
    assert_eq!(key.price, Price::Oracle(2));

    let (key, _) = ordered[2].clone();
    assert_eq!(key.side, Side::Quote);
    assert_eq!(
        key.price,
        Price::Fixed(Decimal::from_str("1.111111111111111111").unwrap())
    );

    let (key, _) = ordered[3].clone();
    assert_eq!(key.side, Side::Quote);
    assert_eq!(key.price, Price::Oracle(-1));

    let (key, _) = ordered[4].clone();
    assert_eq!(key.side, Side::Quote);
    assert_eq!(key.price, Price::Oracle(0));

    let (key, _) = ordered[5].clone();
    assert_eq!(key.side, Side::Quote);
    assert_eq!(key.price, Price::Oracle(1));

    let (key, _) = ordered[6].clone();
    assert_eq!(key.side, Side::Quote);
    assert_eq!(key.price, Price::Oracle(2));

    let iter =
        POOLS
            .sub_prefix(Side::Base)
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

    let iter = POOLS.prefix((Side::Quote, PoolType::Oracle)).range(
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

    let iter = POOLS.prefix((Side::Quote, PoolType::Oracle)).range(
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

#[test]
fn side_iter_omits_oracle_pools_without_price() {
    let mut store = MockStorage::new();
    let timestamp = Timestamp::default();
    let owner = Addr::unchecked("owner");
    let offer = Uint128::from(100u128);
    let oracle = Some(Decimal::one());

    Pool::load(&store, &Price::Oracle(0), &Side::Quote, &oracle)
        .unwrap()
        .create_order(&mut store, &timestamp, &owner, offer)
        .unwrap();

    Pool::load(
        &store,
        &Price::Fixed(Decimal::from_ratio(11u128, 10u128)),
        &Side::Quote,
        &oracle,
    )
    .unwrap()
    .create_order(&mut store, &timestamp, &owner, offer)
    .unwrap();

    let missing_oracle: Option<Decimal> = None;
    let ordered: Vec<EitherOrBoth<Pool>> =
        Pool::iter(&store, &Side::Quote, &missing_oracle).collect();

    assert_eq!(ordered.len(), 1);
    assert_eq!(ordered[0].clone().right(), None);
    assert_eq!(
        ordered[0].clone().left().unwrap().rate(),
        Decimal::from_ratio(11u128, 10u128)
    );
}

#[test]
fn side_iter() {
    let mut store = MockStorage::new();
    let timestamp = Timestamp::default();
    let owner = Addr::unchecked("owner");
    let offer = Uint128::from(100u128);
    let oracle = Some(Decimal::one());
    Pool::load(&store, &Price::Oracle(0), &Side::Quote, &oracle)
        .unwrap()
        .create_order(&mut store, &timestamp, &owner, offer)
        .unwrap();

    Pool::load(&store, &Price::Oracle(-1), &Side::Quote, &oracle)
        .unwrap()
        .create_order(&mut store, &timestamp, &owner, offer)
        .unwrap();

    Pool::load(&store, &Price::Oracle(1), &Side::Quote, &oracle)
        .unwrap()
        .create_order(&mut store, &timestamp, &owner, offer)
        .unwrap();

    Pool::load(&store, &Price::Oracle(2), &Side::Quote, &oracle)
        .unwrap()
        .create_order(&mut store, &timestamp, &owner, offer)
        .unwrap();

    Pool::load(&store, &Price::Oracle(2), &Side::Base, &oracle)
        .unwrap()
        .create_order(&mut store, &timestamp, &owner, offer)
        .unwrap();

    Pool::load(&store, &Price::Fixed(Decimal::one()), &Side::Base, &oracle)
        .unwrap()
        .create_order(&mut store, &timestamp, &owner, offer)
        .unwrap();

    Pool::load(&store, &Price::Fixed(Decimal::one()), &Side::Quote, &oracle)
        .unwrap()
        .create_order(&mut store, &timestamp, &owner, offer)
        .unwrap();

    Pool::load(&store, &Price::Oracle(-1), &Side::Base, &oracle)
        .unwrap()
        .create_order(&mut store, &timestamp, &owner, offer)
        .unwrap();

    Pool::load(&store, &Price::Oracle(1), &Side::Base, &oracle)
        .unwrap()
        .create_order(&mut store, &timestamp, &owner, offer)
        .unwrap();

    Pool::load(&store, &Price::Oracle(2), &Side::Base, &oracle)
        .unwrap()
        .create_order(&mut store, &timestamp, &owner, offer)
        .unwrap();

    Pool::load(
        &store,
        &Price::Fixed(Decimal::from_ratio(10u128, 9u128)),
        &Side::Quote,
        &oracle,
    )
    .unwrap()
    .create_order(&mut store, &timestamp, &owner, offer)
    .unwrap();

    Pool::load(
        &store,
        &Price::Fixed(Decimal::from_ratio(15u128, 9u128)),
        &Side::Quote,
        &oracle,
    )
    .unwrap()
    .create_order(&mut store, &timestamp, &owner, offer)
    .unwrap();

    Pool::load(
        &store,
        &Price::Fixed(Decimal::from_ratio(21u128, 9u128)),
        &Side::Quote,
        &oracle,
    )
    .unwrap()
    .create_order(&mut store, &timestamp, &owner, offer)
    .unwrap();

    let iter = Pool::iter(&store, &Side::Quote, &oracle);
    let ordered: Vec<EitherOrBoth<Pool>> = iter.collect();
    assert_eq!(ordered.len(), 7);

    let x = ordered[0].clone();
    assert_eq!(
        x.clone().left().unwrap().rate(),
        Decimal::from_ratio(21u128, 9u128)
    );
    assert_eq!(x.right(), None);

    let x = ordered[1].clone();
    assert_eq!(
        x.clone().left().unwrap().rate(),
        Decimal::from_ratio(15u128, 9u128)
    );
    assert_eq!(x.right(), None);

    let x = ordered[2].clone();
    assert_eq!(
        x.clone().left().unwrap().rate(),
        Decimal::from_ratio(10u128, 9u128)
    );
    assert_eq!(x.right(), None);

    let x = ordered[3].clone();
    assert_eq!(x.clone().left(), None);
    assert_eq!(
        x.right().unwrap().rate(),
        Decimal::from_ratio(10002u128, 10000u128)
    );

    let x = ordered[4].clone();
    assert_eq!(x.clone().left(), None);
    assert_eq!(
        x.right().unwrap().rate(),
        Decimal::from_ratio(10001u128, 10000u128)
    );

    let x = ordered[5].clone();
    assert_eq!(x.clone().left().unwrap().rate(), Decimal::one());
    assert_eq!(
        x.clone().right().unwrap().rate(),
        x.clone().left().unwrap().rate(),
    );

    let x = ordered[6].clone();
    assert_eq!(x.clone().left(), None);
    assert_eq!(
        x.right().unwrap().rate(),
        Decimal::from_ratio(9999u128, 10000u128)
    );

    let iter = Pool::iter(&store, &Side::Base, &oracle);
    let ordered: Vec<EitherOrBoth<Pool>> = iter.collect();

    assert_eq!(ordered.len(), 4);

    let x = ordered[0].clone();
    assert_eq!(x.clone().left(), None);
    assert_eq!(
        x.right().unwrap().rate(),
        Decimal::from_ratio(9999u128, 10000u128)
    );

    let x = ordered[1].clone();
    assert_eq!(x.clone().left().unwrap().rate(), Decimal::one());
    assert_eq!(x.right(), None);

    let x = ordered[2].clone();
    assert_eq!(x.clone().left(), None);
    assert_eq!(
        x.right().unwrap().rate(),
        Decimal::from_ratio(10001u128, 10000u128)
    );

    let x = ordered[3].clone();
    assert_eq!(x.clone().left(), None);
    assert_eq!(
        x.right().unwrap().rate(),
        Decimal::from_ratio(10002u128, 10000u128)
    );
}
