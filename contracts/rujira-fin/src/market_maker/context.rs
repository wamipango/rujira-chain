use std::collections::BTreeMap;

use cosmwasm_std::{
    coin, ensure, to_json_binary, Addr, Coin, CosmosMsg, Response, Storage, Uint128, WasmMsg,
};
use rujira_rs::{
    bow,
    exchange::{SwapContext, SwapError},
    fin::{Denoms, Side},
};

/// Due to type Context = T::Context for impl SwapItem<Vec<T: SwapItem>>,
/// we share a MarketMakerContext between all market maker instances.
/// We need to collect raw requests and then compile the net requests in
/// `commit`, otherwise the Arb flow will attempt to sub overflow during
/// its temporary context updates
#[derive(Clone, Debug, Default)]
pub struct MarketMakerContext {
    entries: Vec<(Addr, Coin, Coin)>,
    // Safety check to ensure we're not comitting multiple times
    comitted: bool,
}

impl MarketMakerContext {
    pub fn new() -> Self {
        Self {
            entries: vec![],
            comitted: false,
        }
    }
}

impl MarketMakerContext {
    pub fn insert(
        &mut self,
        addr: &Addr,
        denoms: &Denoms,
        side: &Side,
        offer: Uint128,
        ask: Uint128,
    ) {
        let offer_coin = coin(offer.u128(), denoms.ask(side));
        let ask_coin = coin(ask.u128(), denoms.bid(side));
        self.entries.push((addr.clone(), offer_coin, ask_coin));
    }

    /// Reduce the `entries` into individual Swap events that should be executed
    /// When a
    pub fn compile(&self) -> Vec<(Addr, (Coin, Coin))> {
        let mut vec = vec![];
        // Aggregator. (contract, offer_denom, ask_denom)
        let mut agg: BTreeMap<(Addr, String, String), (Uint128, Uint128)> = BTreeMap::new();

        for (addr, offer_coin, ask_coin) in self.entries.clone() {
            let default = (offer_coin.amount, ask_coin.amount);
            agg.entry((addr, offer_coin.denom, ask_coin.denom))
                .and_modify(|(o, a)| {
                    *o += offer_coin.amount;
                    *a += ask_coin.amount;
                })
                .or_insert(default);
        }
        for ((addr, offer_denom, ask_denom), (offer_amount, ask_amount)) in agg {
            if offer_amount.is_zero() || ask_amount.is_zero() {
                continue;
            }
            vec.push((
                addr,
                (
                    coin(offer_amount.u128(), offer_denom),
                    coin(ask_amount.u128(), ask_denom),
                ),
            ));
        }

        vec
    }
}

impl SwapContext for MarketMakerContext {
    fn commit(
        &mut self,
        _storage: &mut dyn Storage,
        response: Response,
    ) -> Result<Response, SwapError> {
        ensure!(!self.comitted, SwapError::AlreadyComitted {});
        self.comitted = true;

        let mut messages: Vec<CosmosMsg> = vec![];

        // Each market maker handles its own fees internally.
        // We send the full offer and request the full ask as min_return.
        for (addr, (offer, ask)) in self.compile() {
            messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: addr.to_string(),
                msg: to_json_binary(&bow::ExecuteMsg::Swap {
                    min_return: ask.clone(),
                    to: None,
                    callback: None,
                })?,
                funds: vec![offer.clone()],
            }));
        }

        Ok(response.add_messages(messages))
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn compile_never_panics_two_entries(
            o1 in 0u128..=1_000_000_000u128,
            a1 in 0u128..=1_000_000_000u128,
            o2 in 0u128..=1_000_000_000u128,
            a2 in 0u128..=1_000_000_000u128,
        ) {
            let denoms = Denoms::new("btc", "usdc");
            let bow = Addr::unchecked("bow");
            let mut ctx = MarketMakerContext::new();

            ctx.insert(&bow, &denoms, &Side::Quote, Uint128::new(o1), Uint128::new(a1));
            ctx.insert(&bow, &denoms, &Side::Base, Uint128::new(o2), Uint128::new(a2));

            let compiled = ctx.compile();
            let (_,(offer, ask)) = compiled[0].clone();

            // Denoms must be valid
            prop_assert!(offer.denom == "btc" || offer.denom == "usdc");
            prop_assert!(ask.denom == "btc" || ask.denom == "usdc");
            prop_assert_ne!(&offer.denom, &ask.denom);
        }

        #[test]
        fn compile_never_panics_many_entries(
            entries in prop::collection::vec((0u128..=100_000_000u128, 0u128..=100_000_000u128, prop::bool::ANY), 2..10),
        ) {
            let denoms = Denoms::new("btc", "usdc");
            let bow = Addr::unchecked("bow");
            let mut ctx = MarketMakerContext::new();

            for (o, a, is_quote) in &entries {
                let side = if *is_quote { Side::Quote } else { Side::Base };
                ctx.insert(&bow, &denoms, &side, Uint128::new(*o), Uint128::new(*a));
            }

            let compiled = ctx.compile();
            if let Some((_,(offer, ask))) = compiled.first() {
                prop_assert!(offer.denom == "btc" || offer.denom == "usdc");
                prop_assert!(ask.denom == "btc" || ask.denom == "usdc");
            }
        }
    }

    #[test]
    fn test_compile_overflow_small_amounts() {
        let denoms = Denoms::new("btc", "usdc");
        let bow = Addr::unchecked("bow");
        let mut ctx = MarketMakerContext::new();

        // MM at price 1.5 (Quote): arb sells 3 btc, gets floor(3*1.5)=4 usdc
        ctx.insert(
            &bow,
            &denoms,
            &Side::Quote,
            Uint128::new(30),
            Uint128::new(45),
        );
        // MM at price 2.0 (Base): complete fill, pool_value=floor(5/2)=2, ask=5
        ctx.insert(
            &bow,
            &denoms,
            &Side::Base,
            Uint128::new(25),
            Uint128::new(50),
        );

        let res = ctx.compile();
        dbg!(&res);
    }
}
