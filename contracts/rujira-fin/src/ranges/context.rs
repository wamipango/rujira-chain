use std::{collections::BTreeMap, ops::Mul};

use cosmwasm_std::{coin, ensure, Addr, BankMsg, CosmosMsg, Decimal, Response, Storage, Uint128};
use cw_utils::NativeBalance;
use rujira_rs::{
    exchange::{SwapContext, SwapError},
    fin::{Denoms, Side, Tick},
};

use super::events::event_range_fee;
use super::range_offer::RangeHandle;

#[derive(Clone, Debug)]
pub struct RangeContext {
    /// A cache of ranges that have been used to service swaps, so that they can be passed
    /// between iter executions
    pub(crate) ranges: BTreeMap<Uint128, RangeHandle>,
    fee_config: (Addr, Decimal),
    pub(crate) denoms: Denoms,
    pub(crate) tick: Tick,
    fees: NativeBalance,
    // Safety check to ensure we're not comitting multiple times
    comitted: bool,
}

impl RangeContext {
    pub fn new(fee_addr: Addr, fee_range: Decimal, denoms: Denoms, tick: Tick) -> Self {
        Self {
            ranges: BTreeMap::default(),
            fee_config: (fee_addr, fee_range),
            denoms,
            tick,
            fees: NativeBalance::default(),
            comitted: false,
        }
    }

    pub fn fee_range(&self) -> Decimal {
        self.fee_config.1
    }

    pub fn apply_fee(&mut self, side: &Side, amount: Decimal) -> Decimal {
        if self.fee_config.1.is_zero() || amount.is_zero() {
            return Decimal::zero();
        }
        let fee = amount.mul(self.fee_config.1).min(amount);
        if !fee.is_zero() {
            self.fees += coin(fee.to_uint_floor().u128(), self.denoms.ask(side));
        }
        fee
    }

    pub fn to_msgs(&self) -> Vec<CosmosMsg> {
        if self.fees.is_empty() {
            vec![]
        } else {
            vec![CosmosMsg::Bank(BankMsg::Send {
                to_address: self.fee_config.0.to_string(),
                amount: self.fees.clone().into_vec(),
            })]
        }
    }
}

impl SwapContext for RangeContext {
    fn commit(
        &mut self,
        storage: &mut dyn Storage,
        response: Response,
    ) -> Result<Response, SwapError> {
        ensure!(!self.comitted, SwapError::AlreadyComitted {});
        self.comitted = true;

        for r in self.ranges.values() {
            r.borrow().save(storage)?;
        }
        let mut response = response.add_messages(self.to_msgs());
        if !self.fees.is_empty() {
            // NativeBalance is normalised during AddAssign
            response = response.add_event(event_range_fee(&self.denoms, &self.fees));
        }
        Ok(response)
    }

    fn checkpoint(&mut self) {
        for r in self.ranges.values_mut() {
            r.checkpoint();
        }
    }

    fn rollback(&mut self) {
        for r in self.ranges.values_mut() {
            r.rollback();
        }
    }
}
