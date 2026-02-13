use cosmwasm_std::{Addr, Decimal, Event, Uint128};
use cw_utils::NativeBalance;
use rujira_rs::fin::Denoms;

use super::Range;

fn payout_amounts(payout: Option<&NativeBalance>, denoms: &Denoms) -> (Uint128, Uint128) {
    let mut base = Uint128::zero();
    let mut quote = Uint128::zero();
    if let Some(payout) = payout {
        for coin in payout.clone().into_vec() {
            if coin.denom == denoms.base() {
                base = coin.amount;
            } else if coin.denom == denoms.quote() {
                quote = coin.amount;
            }
        }
    }
    (base, quote)
}

pub fn event_create_range(range: &Range) -> Event {
    Event::new(format!("{}/range.create", env!("CARGO_PKG_NAME")))
        .add_attribute("owner", range.owner.clone())
        .add_attribute("idx", range.idx)
        .add_attribute("high", range.high.to_string())
        .add_attribute("low", range.low.to_string())
        .add_attribute("skew", range.skew.to_string())
        .add_attribute("spread", range.spread.to_string())
        .add_attribute("fee", range.fee.to_string())
        .add_attribute("base", range.base.to_uint_floor())
        .add_attribute("quote", range.quote.to_uint_floor())
}

pub fn event_claim_range(range: &Range, denoms: &Denoms, payout: Option<&NativeBalance>) -> Event {
    let (base, quote) = payout_amounts(payout, denoms);
    Event::new(format!("{}/range.claim", env!("CARGO_PKG_NAME")))
        .add_attribute("owner", range.owner.clone())
        .add_attribute("idx", range.idx)
        .add_attribute("base", base)
        .add_attribute("quote", quote)
}

pub fn event_deposit_range(range: &Range, base: Uint128, quote: Uint128) -> Event {
    Event::new(format!("{}/range.deposit", env!("CARGO_PKG_NAME")))
        .add_attribute("owner", range.owner.clone())
        .add_attribute("idx", range.idx)
        .add_attribute("base", base)
        .add_attribute("quote", quote)
}

pub fn event_withdraw_range(
    range: &Range,
    amount: Decimal,
    denoms: &Denoms,
    payout: Option<&NativeBalance>,
) -> Event {
    let (base, quote) = payout_amounts(payout, denoms);
    Event::new(format!("{}/range.withdraw", env!("CARGO_PKG_NAME")))
        .add_attribute("owner", range.owner.clone())
        .add_attribute("idx", range.idx)
        .add_attribute("amount", amount.to_string())
        .add_attribute("base", base)
        .add_attribute("quote", quote)
}

pub fn event_close_range(range: &Range) -> Event {
    Event::new(format!("{}/range.close", env!("CARGO_PKG_NAME")))
        .add_attribute("owner", range.owner.clone())
        .add_attribute("idx", range.idx)
        .add_attribute("base", range.base.to_uint_floor())
        .add_attribute("quote", range.quote.to_uint_floor())
        .add_attribute("fee_base", range.fees[0].to_uint_floor())
        .add_attribute("fee_quote", range.fees[1].to_uint_floor())
}

pub fn event_transfer_range(range: &Range, from: &Addr, to: &Addr) -> Event {
    Event::new(format!("{}/range.transfer", env!("CARGO_PKG_NAME")))
        .add_attribute("from", from.clone())
        .add_attribute("to", to.clone())
        .add_attribute("idx", range.idx)
}

pub fn event_range_fee(denoms: &Denoms, fees: &NativeBalance) -> Event {
    let (base, quote) = payout_amounts(Some(fees), denoms);
    Event::new(format!("{}/range.fee", env!("CARGO_PKG_NAME")))
        .add_attribute("base", base)
        .add_attribute("quote", quote)
}
