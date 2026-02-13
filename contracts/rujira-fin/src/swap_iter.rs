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
        oracle: &'a impl Premiumable,
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
                RangeOfferIter::new(storage, side, &self.config.range_delta),
                move |x, y| match side {
                    Side::Base => x.rate().cmp(&y.rate()),
                    Side::Quote => y.rate().cmp(&x.rate()),
                },
            )
    }
}

pub fn create_context(config: &Config) -> RootContext {
    (
        (
            (Default::default(), Default::default()),
            MarketMakerContext::new(
                config.fee_address.clone(),
                config.fee_amm,
                config.denoms.clone(),
            ),
        ),
        RangeContext::new(
            config.fee_address.clone(),
            config.fee_amm,
            config.denoms.clone(),
            config.tick.clone(),
        ),
    )
}
