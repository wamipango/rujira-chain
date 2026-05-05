use std::ops::{Add, Div};

use cosmwasm_std::Decimal;
use cosmwasm_std::{QuerierWrapper, Storage};
use itertools::{EitherOrBoth, Itertools};
use rujira_rs::{exchange::SwapItem, fin::Side, Premiumable};

use crate::market_maker::MarketMakerContext;
use crate::market_maker::MarketMakerOffer;
use crate::ranges::RangeOffer;
use crate::ranges::{RangeContext, RangeOfferIter};
use crate::{config::Config, order_pool::Pool};

pub struct SwapIter<'a> {
    querier: QuerierWrapper<'a>,
    config: &'a Config,
}

pub type RootItem =
    EitherOrBoth<EitherOrBoth<EitherOrBoth<Pool>, Vec<MarketMakerOffer>>, RangeOffer>;
pub type RootContext = <RootItem as SwapItem>::Context;

impl<'a> SwapIter<'a> {
    pub fn new(querier: QuerierWrapper<'a>, config: &'a Config) -> Self {
        Self { querier, config }
    }

    pub fn iter(
        &self,
        storage: &'a dyn Storage,
        side: &'a Side,
        oracle: &'a Option<impl Premiumable>,
    ) -> impl Iterator<Item = RootItem> + 'a {
        Pool::iter(storage, side, oracle)
            .merge_join_by(
                self.config.market_makers.iter(
                    self.querier,
                    &self.config.denoms,
                    &self.config.tick,
                    side,
                ),
                move |x, y| match side {
                    Side::Base => x.rate().cmp(&y.rate()),
                    Side::Quote => y.rate().cmp(&x.rate()),
                },
            )
            .merge_join_by(
                RangeOfferIter::new(
                    storage,
                    side,
                    &self.config.range_delta,
                    &self.config.range_min,
                ),
                move |x, y| match side {
                    Side::Base => x.rate().cmp(&y.rate()),
                    Side::Quote => y.rate().cmp(&x.rate()),
                },
            )
            .filter(|x| !x.total().is_zero())
    }

    pub fn mid_price(
        &self,
        storage: &'a dyn Storage,
        oracle: &'a Option<impl Premiumable>,
    ) -> Option<Decimal> {
        Some(
            self.iter(storage, &Side::Base, oracle)
                .next()?
                .rate()
                .add(self.iter(storage, &Side::Quote, oracle).next()?.rate())
                .div(Decimal::one().add(Decimal::one())),
        )
    }
}

pub fn create_context(config: &Config) -> RootContext {
    (
        (
            (Default::default(), Default::default()),
            MarketMakerContext::new(),
        ),
        RangeContext::new(
            config.fee_address.clone(),
            config.fee_range,
            config.denoms.clone(),
            config.tick.clone(),
        ),
    )
}
