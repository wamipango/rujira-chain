use cosmwasm_std::{coin, Decimal, Event, Storage, Uint128};
use cw_utils::NativeBalance;
use itertools::EitherOrBoth;
use rujira_rs::fin::{Denoms, Side};

use crate::{pool::Pool, ContractError};

pub struct Swapper {
    denoms: Denoms,
    events: Vec<Event>,
    min_return: Option<Uint128>,
    offer: Uint128,
    oracle: Decimal,
    returned: Uint128,
    side: Side,
    pending_pools: Vec<Pool>,
}

impl Swapper {
    pub fn new(
        side: Side,
        denoms: Denoms,
        offer: Uint128,
        min_return: Option<Uint128>,
        oracle: Decimal,
    ) -> Self {
        Self {
            denoms,
            events: vec![],
            min_return,
            offer,
            oracle,
            returned: Uint128::zero(),
            side,
            pending_pools: vec![],
        }
    }

    pub fn swap(&mut self, storage: &mut dyn Storage) -> Result<SwapResult, ContractError> {
        let iter = Pool::side(storage, &self.side, &self.oracle);
        for v in iter {
            if self.offer.is_zero() {
                break;
            }
            match v {
                EitherOrBoth::Both(mut a, mut b) => {
                    let offer_a =
                        Decimal::from_ratio(a.total() * self.offer, b.total() + a.total())
                            .to_uint_floor();
                    let offer_b = self.offer - offer_a;

                    let (consumed_offer, consumed_bids) = a.distribute(offer_a)?;
                    self.pending_pools.push(a);
                    self.offer -= consumed_offer;
                    self.returned += consumed_bids;

                    let (consumed_offer, consumed_bids) = b.distribute(offer_b)?;
                    self.pending_pools.push(b);
                    self.offer -= consumed_offer;
                    self.returned += consumed_bids;
                }
                EitherOrBoth::Left(mut x) => {
                    let (consumed_offer, consumed_bids) = x.distribute(self.offer)?;
                    self.pending_pools.push(x);
                    self.offer -= consumed_offer;
                    self.returned += consumed_bids;
                }
                EitherOrBoth::Right(mut x) => {
                    let (consumed_offer, consumed_bids) = x.distribute(self.offer)?;
                    self.pending_pools.push(x);
                    self.offer -= consumed_offer;
                    self.returned += consumed_bids;
                }
            }
        }

        if let Some(min_return) = self.min_return {
            if self.returned < min_return {
                return Err(ContractError::InsufficientReturn {
                    expected: min_return,
                    returned: self.returned,
                });
            }
        }

        for pool in self.pending_pools.iter_mut() {
            pool.save(storage)?;
        }

        Ok(SwapResult {
            events: self.events.clone(),
            funds: NativeBalance(vec![
                coin(self.returned.u128(), self.denoms.ask(&self.side)),
                coin(self.offer.u128(), self.denoms.bid(&self.side)),
            ]),
        })
    }
}

pub struct SwapResult {
    pub events: Vec<Event>,
    pub funds: NativeBalance,
}
