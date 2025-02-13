use std::ops::Sub;

use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Coin, Decimal, Timestamp};

use crate::coins::Coins;

/// The schedule over which tokens can be streamed
#[cw_serde]
pub enum Schedule {
    /// Continuous, repeating, even distribution over the `period` (in seconds)
    Continuous {
        starts: Timestamp,
        amount: Vec<Coin>,
        period: u64,
    },
    /// Fixed, single distribution
    Fixed {
        starts: Timestamp,
        ends: Timestamp,
        amount: Vec<Coin>,
    },
}

impl Schedule {
    pub fn allocation(&self, from: &Timestamp, to: &Timestamp) -> Vec<Coin> {
        match self {
            Schedule::Continuous {
                starts,
                amount,
                period,
            } => {
                let from = from.max(starts);
                let duration = to.seconds().sub(from.seconds());
                let ratio = Decimal::checked_from_ratio(duration, *period).unwrap_or_default();
                amount.mul_coins(ratio)
            }
            Schedule::Fixed {
                starts,
                ends,
                amount,
            } => {
                let from = from.max(starts);
                let to = to.min(ends);
                let ratio = Decimal::checked_from_ratio(
                    from.seconds().sub(to.seconds()),
                    starts.seconds().sub(ends.seconds()),
                )
                .unwrap_or_default();
                amount.mul_coins(ratio)
            }
        }
    }
}

pub mod factory {
    use super::*;

    #[cw_serde]
    pub struct InstantiateMsg {
        /// The Code ID for the deployed contract code
        pub code_id: u64,
    }

    #[cw_serde]
    pub struct ExecuteMsg {
        pub recipients: Vec<(String, u8)>,
        pub schedule: Schedule,
        pub owner: Option<String>,
    }

    #[cw_serde]
    #[derive(QueryResponses)]
    pub enum QueryMsg {}
}

#[cw_serde]
pub struct InstantiateMsg {
    pub recipients: Vec<(String, u8)>,
    pub schedule: Schedule,
    pub owner: Option<String>,
}

impl From<factory::ExecuteMsg> for InstantiateMsg {
    fn from(value: factory::ExecuteMsg) -> Self {
        Self {
            recipients: value.recipients,
            schedule: value.schedule,
            owner: value.owner,
        }
    }
}

#[cw_serde]
pub enum ExecuteMsg {
    /// Withdraws all pending funds for the calling address
    Claim {},

    /// Update streaming config. All fields must be provided
    Update(InstantiateMsg),
}

#[cw_serde]
pub enum SudoMsg {}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(Vec<Coin>)]
    /// Queries the pending tokens available for a specific adress
    Pending { address: String },

    #[returns(ConfigResponse)]
    Config {},
}

#[cw_serde]
pub struct ConfigResponse {
    pub recipients: Vec<(String, u8)>,
    pub schedule: Schedule,
    pub owner: Option<String>,
}
