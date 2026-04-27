use cosmwasm_schema::cw_serde;
use cosmwasm_std::{coins, Addr, BankMsg, Decimal, Response, Timestamp, Uint128};
use rujira_rs::brune::RevenueResponse;

use crate::{config::RUNE, ContractError};

/// Utility struct for managing revenue harvesting and distribution
#[cw_serde]
#[derive(Default)]
pub struct Revenue {
    pub pending: Decimal,

    /// The tokens per smear period that the contract has earned "at"
    /// Used to pro-rate revenue sent to the staking contract
    pub fee_rate: Decimal,

    /// The last time this Revenue was adjusted
    pub timestamp: Timestamp,
}

impl Revenue {
    pub fn allocate(&mut self, ts: &Timestamp, smear_seconds: u32, amount: &Decimal) {
        if amount.is_zero() {
            return;
        }

        self.pending += *amount;
        let smear_units = Decimal::from_ratio(smear_seconds as u128, 1u128);
        self.fee_rate += *amount / smear_units;
        self.timestamp = *ts;
    }

    pub fn distribute(
        &mut self,
        ts: &Timestamp,
        recipient: &Addr,
        response: Response,
        max: Decimal,
    ) -> Result<(Response, Uint128), ContractError> {
        let elapsed = ts.seconds().saturating_sub(self.timestamp.seconds());
        if elapsed == 0 || self.pending.is_zero() || self.fee_rate.is_zero() {
            self.timestamp = *ts;
            return Ok((response, Uint128::zero()));
        }

        let elapsed = Decimal::from_ratio(elapsed as u128, 1u128);
        let due = (self.fee_rate * elapsed).min(self.pending);
        // Clamp to the max liquid available
        let payout = due.min(max).to_uint_floor();

        if payout.is_zero() {
            // Skip zero-amount transfer to avoid bank module rejection.
            // Do not advance timestamp so sub-unit accrual can accumulate.
            return Ok((response, Uint128::zero()));
        }

        self.pending = self
            .pending
            .saturating_sub(Decimal::from_atomics(payout, 0)?);

        let actual = Decimal::from_atomics(payout, 0)?;

        // Proportionally reduce fee_rate as pending is distributed
        if !self.pending.is_zero() && !actual.is_zero() {
            let ratio = actual / self.pending;
            self.fee_rate *= Decimal::one() - ratio;
        }

        if self.pending.is_zero() {
            self.fee_rate = Decimal::zero();
        }
        if payout > Uint128::zero() {
            self.timestamp = *ts;
        }
        Ok((
            response.add_message(BankMsg::Send {
                to_address: recipient.to_string(),
                amount: coins(payout.u128(), RUNE),
            }),
            payout,
        ))
    }
}

impl From<&Revenue> for RevenueResponse {
    fn from(value: &Revenue) -> Self {
        Self {
            pending: value.pending,
            fee_rate: value.fee_rate,
            timestamp: value.timestamp,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;
    use cosmwasm_std::{CosmosMsg, SubMsg, Uint128};

    #[test]
    fn allocate_increases_pending_and_fee_rate_time() {
        let mut revenue = Revenue::default();
        let ts = Timestamp::from_seconds(1000);
        revenue.allocate(&ts, 100, &Decimal::from_ratio(50u128, 1u128));
        assert_eq!(revenue.pending, Decimal::from_ratio(50u128, 1u128));
        assert_eq!(revenue.fee_rate, Decimal::from_ratio(1u128, 2u128));
        assert_eq!(revenue.timestamp, ts);
    }

    #[test]
    fn allocate_accumulates_fee_rate() {
        let mut revenue = Revenue::default();
        let ts = Timestamp::from_seconds(1000);
        revenue.allocate(&ts, 100, &Decimal::from_ratio(100u128, 1u128));
        revenue.allocate(&ts, 100, &Decimal::from_ratio(50u128, 1u128));
        assert_eq!(revenue.pending, Decimal::from_ratio(150u128, 1u128));
        assert_eq!(revenue.fee_rate, Decimal::from_ratio(3u128, 2u128));
    }

    #[test]
    fn allocate_ignores_zero_amount_or_zero_smear() {
        let mut revenue = Revenue::default();
        let ts = Timestamp::from_seconds(1000);
        revenue.allocate(&ts, 100, &Decimal::zero());
        assert_eq!(revenue.pending, Decimal::zero());
        assert_eq!(revenue.fee_rate, Decimal::zero());
    }

    #[test]
    fn distribute_pays_full_when_elapsed_matches_pending() {
        let mut revenue = Revenue {
            pending: Decimal::from_ratio(50u128, 1u128),
            fee_rate: Decimal::from_ratio(1u128, 2u128),
            timestamp: Timestamp::from_seconds(1000),
        };
        let (msg, payout) = revenue
            .distribute(
                &Timestamp::from_seconds(1100),
                &Addr::unchecked("stake"),
                Response::default(),
                Decimal::MAX,
            )
            .unwrap();
        assert_eq!(payout, Uint128::from(50u128));
        assert_eq!(
            msg.messages[0],
            SubMsg::new(CosmosMsg::Bank(BankMsg::Send {
                to_address: "stake".to_string(),
                amount: coins(50, RUNE),
            }))
        );
        assert!(revenue.pending.is_zero());
        assert!(revenue.fee_rate.is_zero());
        assert_eq!(revenue.timestamp, Timestamp::from_seconds(1100));
    }

    #[test]
    fn distribute_caps_to_pending() {
        let mut revenue = Revenue {
            pending: Decimal::from_ratio(10u128, 1u128),
            fee_rate: Decimal::one(),
            timestamp: Timestamp::from_seconds(1000),
        };
        let (msg, payout) = revenue
            .distribute(
                &Timestamp::from_seconds(1020),
                &Addr::unchecked("stake"),
                Response::default(),
                Decimal::MAX,
            )
            .unwrap();
        assert_eq!(payout, Uint128::from(10u128));
        assert_eq!(
            msg.messages[0],
            SubMsg::new(CosmosMsg::Bank(BankMsg::Send {
                to_address: "stake".to_string(),
                amount: coins(10, RUNE),
            }))
        );
        assert!(revenue.pending.is_zero());
        assert!(revenue.fee_rate.is_zero());
    }

    #[test]
    fn distribute_partial_keeps_fee_rate() {
        let mut revenue = Revenue {
            pending: Decimal::from_ratio(100u128, 1u128),
            fee_rate: Decimal::one(),
            timestamp: Timestamp::from_seconds(1000),
        };
        let (msg, payout) = revenue
            .distribute(
                &Timestamp::from_seconds(1030),
                &Addr::unchecked("stake"),
                Response::default(),
                Decimal::MAX,
            )
            .unwrap();
        assert_eq!(payout, Uint128::from(30u128));
        assert_eq!(
            msg.messages[0],
            SubMsg::new(CosmosMsg::Bank(BankMsg::Send {
                to_address: "stake".to_string(),
                amount: coins(30, RUNE),
            }))
        );
        assert_eq!(revenue.pending, Decimal::from_ratio(70u128, 1u128));
        assert_eq!(
            revenue.fee_rate,
            Decimal::from_str("0.571428571428571429").unwrap()
        );
    }

    #[test]
    fn distribute_zero_elapsed_sends_zero() {
        let mut revenue = Revenue {
            pending: Decimal::from_ratio(10u128, 1u128),
            fee_rate: Decimal::one(),
            timestamp: Timestamp::from_seconds(1000),
        };
        let (msg, payout) = revenue
            .distribute(
                &Timestamp::from_seconds(1000),
                &Addr::unchecked("stake"),
                Response::default(),
                Decimal::MAX,
            )
            .unwrap();
        assert_eq!(payout, Uint128::zero());
        assert!(msg.messages.is_empty());
        assert_eq!(revenue.pending, Decimal::from_ratio(10u128, 1u128));
        assert_eq!(revenue.fee_rate, Decimal::one());
    }
}
