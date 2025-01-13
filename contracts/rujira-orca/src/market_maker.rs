use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Attribute, Binary, Decimal, QuerierWrapper, StdResult, Storage, Uint128};
use rujira_rs::{
    orca::{Denoms, MarketMakerQuery, MarketMakerResponse},
    SwapCommit, Swappable,
};

use crate::ContractError;
pub struct MarketMaker<'a> {
    q: QuerierWrapper<'a>,
    denoms: Denoms,
    contract: Option<Addr>,
    last_price: Option<Decimal>,
    bid: Uint128,
    ask: Uint128,
    data: Option<Binary>,
}

impl<'a> MarketMaker<'a> {
    pub fn new(q: QuerierWrapper<'a>, denoms: Denoms, contract: Option<Addr>) -> Self {
        Self {
            q,
            denoms,
            contract,
            last_price: None,
            bid: Uint128::zero(),
            ask: Uint128::zero(),
            data: None,
        }
    }
}

#[cw_serde]
pub struct MarketMakerOffer {
    pub premium: u16,
    pub total: Uint128,
    commitment: (Uint128, Uint128),
}

impl Swappable<ContractError> for MarketMakerOffer {
    fn rate(&self) -> Decimal {
        todo!()
    }

    fn attributes(&self) -> Vec<Attribute> {
        vec![]
    }

    fn total(&self) -> Uint128 {
        self.total
    }

    fn swap(&mut self, _offer: Uint128) -> Result<(Uint128, Uint128), ContractError> {
        todo!()
    }

    fn commit(&self, _storage: &mut dyn Storage) -> StdResult<SwapCommit> {
        Ok(SwapCommit {
            market_maker: self.commitment,
        })
    }
}

impl<'a> MarketMaker<'a> {
    pub fn totals(&self) -> (Uint128, Uint128) {
        (self.bid, self.ask)
    }

    fn query_next(&mut self, contract: Addr) -> Option<MarketMakerOffer> {
        let price = self.last_price;
        let res: Option<MarketMakerResponse> = self
            .q
            .query_wasm_smart(
                contract.to_string(),
                &MarketMakerQuery {
                    ask_denom: self.denoms.ask().to_string(),
                    ask_total: self.ask,
                    bid_denom: self.denoms.bid().to_string(),
                    bid_total: self.bid,
                    last_price: price,
                    data: self.data.clone(),
                },
            )
            .ok();

        match res {
            Some(_res) => {
                todo!()
            }
            None => None,
        }
    }
}

impl Iterator for MarketMaker<'_> {
    type Item = MarketMakerOffer;

    fn next(&mut self) -> Option<Self::Item> {
        self.contract.clone().map(|x| self.query_next(x)).flatten()
    }
}

#[cfg(test)]

mod test {
    use super::*;
    use cosmwasm_std::{testing::MockQuerier, SystemError, SystemResult, WasmQuery};

    #[test]
    fn test_iterator() {
        let mut q = MockQuerier::default();
        q.update_wasm(|x| match x {
            WasmQuery::Smart { .. } => {
                todo!()
            }
            _ => SystemResult::Err(SystemError::Unknown {}),
        });
        let mut _iter = MarketMaker::new(
            QuerierWrapper::new(&q),
            Denoms::new("btc", "usdc"),
            Some(Addr::unchecked("bow")),
        );

        // assert_eq!(items, todo!());
    }
}
