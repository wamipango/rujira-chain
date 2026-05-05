use cosmwasm_schema::cw_serde;
use cosmwasm_std::{
    Addr, Attribute, Decimal, Decimal256, Fraction, StdResult, Storage, Timestamp, Uint128, Uint256,
};
use itertools::{EitherOrBoth, Itertools};
use rujira_rs::{
    bid_pool::{self, Bid, SumSnapshot},
    exchange::{SwapError, SwapItem},
    fin::{Price, Side},
    DecimalScaled, Premiumable,
};

use crate::ContractError;

use super::{Order, PoolContext, PoolKey, PoolType};
use super::{BID_POOLS, POOLS, SNAPSHOTS};

/// A wrapper around a BidPool to provide a side & price, used for keying orders and
/// storing pools for iterating during execution
#[cw_serde]
pub struct Pool {
    pub price: Price,
    pub side: Side,
    pub(crate) pool: bid_pool::Pool,
    rate: Decimal,
    #[serde(skip)]
    pending_sum_snapshots: Vec<SumSnapshot>,
}

impl Pool {
    pub fn iter<'a>(
        storage: &'a dyn Storage,
        side: &'a Side,
        oracle: &'a Option<impl Premiumable>,
    ) -> impl Iterator<Item = EitherOrBoth<Pool>> + 'a {
        let order = match side {
            Side::Base => cosmwasm_std::Order::Ascending,
            Side::Quote => cosmwasm_std::Order::Descending,
        };

        let populate = |x: StdResult<(Price, ())>| -> Option<Self> {
            match x {
                Ok((price, _)) => Some(Self {
                    price: price.clone(),
                    side: side.clone(),
                    rate: price.to_rate(oracle)?,
                    // The presence of the key indicates a BidPool should be present,
                    // so we should panic if this is incorrect
                    pool: BID_POOLS
                        .load(storage, PoolKey::new(side.clone(), price))
                        .unwrap(),
                    pending_sum_snapshots: vec![],
                }),
                Err(_) => None,
            }
        };

        let fixed = POOLS
            .prefix((side.clone(), PoolType::Fixed))
            .range(storage, None, None, order)
            .filter_map(populate);

        let oracle = POOLS
            .prefix((side.clone(), PoolType::Oracle))
            .range(storage, None, None, order)
            .filter_map(populate);

        fixed.merge_join_by(oracle, move |f, o| match side {
            Side::Base => f.rate().cmp(&o.rate()),
            Side::Quote => o.rate().cmp(&f.rate()),
        })
    }

    pub fn load(
        storage: &dyn Storage,
        price: &Price,
        side: &Side,
        oracle: &Option<impl Premiumable>,
    ) -> Result<Self, ContractError> {
        let key = PoolKey::new(side.clone(), price.clone());
        Ok(Self {
            price: price.clone(),
            rate: price
                .to_rate(oracle)
                .ok_or_else(|| ContractError::OraclePriceUnavailable {
                    side: side.clone(),
                    price: price.clone(),
                })?,
            side: side.clone(),
            pool: BID_POOLS.load(storage, key).unwrap_or_default(),
            pending_sum_snapshots: vec![],
        })
    }

    fn key(&self) -> PoolKey {
        PoolKey::new(self.side.clone(), self.price.clone())
    }

    pub fn create_order(
        &mut self,
        storage: &mut dyn Storage,
        timestamp: &Timestamp,
        owner: &Addr,
        offer: Uint128,
    ) -> Result<Order, ContractError> {
        let order = Order {
            owner: owner.clone(),
            offer,
            updated_at: *timestamp,
            bid: self.pool.new_bid(offer.into()),
        };
        self.commit(storage)?;
        order.save(storage, self)?;
        Ok(order)
    }

    pub fn load_order(&self, storage: &dyn Storage, owner: &Addr) -> Result<Order, ContractError> {
        let mut order = Order::load(storage, owner, &self.side, &self.price)?;
        self.sync_order(storage, &mut order)?;
        Ok(order)
    }

    pub fn increase_order(
        &mut self,
        storage: &mut dyn Storage,
        order: &mut Order,
        timestamp: &Timestamp,
        amount: Uint128,
    ) -> Result<Uint128, ContractError> {
        order.bid.increase(&mut self.pool, amount.into())?;
        order.offer = order.amount();
        order.updated_at = *timestamp;
        order.save(storage, self)?;
        self.commit(storage)?;
        Ok(amount)
    }

    pub fn retract_order(
        &mut self,
        storage: &mut dyn Storage,
        order: &mut Order,
        timestamp: &Timestamp,
        amount: Option<Uint128>,
    ) -> Result<Uint128, ContractError> {
        let amount256 = amount.map(Uint256::from);
        let refund_amount = order.bid.retract(&mut self.pool, amount256)?;
        order.offer = order.amount();
        order.updated_at = *timestamp;
        order.save(storage, self)?;
        self.commit(storage)?;
        Ok(Uint128::try_from(refund_amount)?)
    }

    pub fn claim_order(
        &mut self,
        storage: &mut dyn Storage,
        order: &mut Order,
    ) -> Result<Uint128, ContractError> {
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

    fn sum_snapshot(&self, storage: &dyn Storage, bid: &Bid) -> StdResult<DecimalScaled> {
        let key = (
            PoolKey::new(self.side.clone(), self.price.clone()),
            bid.sum_snapshot_key(),
        );
        SNAPSHOTS.load(storage, key)
    }

    pub fn commit(&mut self, storage: &mut dyn Storage) -> StdResult<()> {
        for s in self.pending_sum_snapshots.clone() {
            SNAPSHOTS.save(storage, (self.key(), s.key()), &s.sum)?;
        }

        BID_POOLS.save(storage, self.key(), &self.pool)?;
        // Clear empty pools so they're not iterated over during a swap
        if self.pool.is_zero() {
            POOLS.remove(storage, self.key());
            return Ok(());
        }

        POOLS.save(storage, self.key(), &())?;
        Ok(())
    }
}

impl SwapItem for Pool {
    type Context = PoolContext;
    fn swap(
        &mut self,
        context: &mut Self::Context,
        offer: Uint128,
    ) -> Result<(Uint128, Uint128, Vec<Attribute>), SwapError> {
        let rate = match self.side {
            Side::Base => self.rate.inv().unwrap(),
            Side::Quote => self.rate,
        };
        let res = self.pool.distribute(
            Decimal256::from_ratio(offer, 1u128),
            &Decimal256::from(rate),
        )?;
        self.pending_sum_snapshots = res.snapshots;
        context.pools.push(self.clone());

        Ok((
            res.consumed_offer.try_into()?,
            res.consumed_bids.try_into()?,
            vec![
                Attribute::new("price", self.price.to_string()),
                Attribute::new("side", self.side.to_string()),
            ],
        ))
    }

    fn rate(&self) -> Decimal {
        self.rate
    }

    fn total(&self) -> Uint128 {
        self.pool.total().try_into().unwrap()
    }
}
