use std::{collections::BTreeMap, ops::Sub};

use cosmwasm_std::{
    coin, ensure, to_json_binary, Addr, BankMsg, Coin, CosmosMsg, Decimal, Fraction, Response,
    Storage, Uint128, WasmMsg,
};
use cw_utils::NativeBalance;
use rujira_rs::{
    bow,
    exchange::{SwapContext, SwapError},
    fin::{Denoms, Side},
};

use super::event_market_maker_fee;
/// Due to type Context = T::Context for impl SwapItem<Vec<T: SwapItem>>,
/// we share a MarketMakerContext between all market maker instances.
/// We need to collect raw requests and then compile the net requests in
/// `commit`, otherwise the Arb flow will attempt to sub overflow during
/// its temporary context updates
#[derive(Clone, Debug)]
pub struct MarketMakerContext {
    entries: Vec<(Addr, Coin, Coin)>,
    fee_config: (Addr, Decimal),
    denoms: Denoms,
    // Safety check to ensure we're not comitting multiple times
    comitted: bool,
}

impl MarketMakerContext {
    pub fn new(fee_addr: Addr, fee_amm: Decimal, denoms: Denoms) -> Self {
        Self {
            entries: vec![],
            fee_config: (fee_addr, fee_amm),
            denoms,
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

    pub fn compile(&self) -> BTreeMap<Addr, (Coin, Coin)> {
        let mut map = BTreeMap::new();
        for (addr, offer_coin, ask_coin) in self.entries.clone() {
            map.entry(addr.clone())
                .and_modify(|(o, a): &mut (Coin, Coin)| {
                    // Same direction, increase
                    if o.denom == offer_coin.denom {
                        o.amount += offer_coin.amount;
                        a.amount += ask_coin.amount;
                        // Opposite direction but smaller than current, shrink
                    } else if o.amount > offer_coin.amount && a.amount >= ask_coin.amount {
                        o.amount -= offer_coin.amount;
                        a.amount -= ask_coin.amount;
                        // Larger than current, invert
                    } else {
                        o.amount = offer_coin.amount - o.amount;
                        o.denom = offer_coin.denom.clone();
                        a.amount = ask_coin.amount - a.amount;
                        a.denom = ask_coin.denom.clone();
                    }
                })
                .or_insert((offer_coin, ask_coin));
        }
        map
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

        let (fee_addr, fee) = self.fee_config.clone();
        let mut fees = NativeBalance::default();
        let mut messages: Vec<CosmosMsg> = vec![];

        for (addr, (offer, ask)) in self.compile() {
            let offer_fee = coin(
                offer
                    .amount
                    .multiply_ratio(fee.numerator(), fee.denominator())
                    .u128(),
                offer.denom.clone(),
            );
            let offer = coin(offer.amount.sub(offer_fee.amount).u128(), offer.denom);
            fees += offer_fee;

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

        fees.normalize();
        let mut response = response.add_messages(messages);
        if !fees.is_empty() {
            response = response
                .add_message(BankMsg::Send {
                    to_address: fee_addr.to_string(),
                    amount: fees.clone().into_vec(),
                })
                .add_event(event_market_maker_fee(&self.denoms, &fees));
        }
        Ok(response)
    }
}
