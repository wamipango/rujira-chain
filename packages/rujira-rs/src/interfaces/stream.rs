use std::ops::Sub;

use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Coin, Decimal, Timestamp};

/// The schedule over which tokens can be streamed
#[cw_serde]
pub enum Schedule {
    /// Continuous, repeating, even distribution over the `period` (in seconds)
    Continuous {
        starts: Timestamp,
        amount: Vec<Coin>,
        period: u64,
    },
}

impl Schedule {
    pub fn allocation(&self, from: &Option<Timestamp>, to: &Timestamp) -> Vec<Coin> {
        match self {
            Schedule::Continuous {
                starts,
                amount,
                period,
            } => {
                let from = from.unwrap_or(*starts);
                let duration = to.seconds().sub(from.seconds());
                let ratio = Decimal::from_ratio(duration, *period);
                amount
                    .iter()
                    .map(|x| Coin {
                        amount: x.amount.mul_floor(ratio),
                        ..x.clone()
                    })
                    .collect()
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
    }

    #[cw_serde]
    #[derive(QueryResponses)]
    pub enum QueryMsg {}
}

#[cw_serde]
pub struct InstantiateMsg {
    pub recipients: Vec<(String, u8)>,
    pub schedule: Schedule,
}

impl From<factory::ExecuteMsg> for InstantiateMsg {
    fn from(value: factory::ExecuteMsg) -> Self {
        Self {
            recipients: value.recipients,
            schedule: value.schedule,
        }
    }
}

#[cw_serde]
pub enum ExecuteMsg {
    /// WIthdraws all pending funds for the calling address
    Claim {},
}

#[cw_serde]
pub enum SudoMsg {}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(Vec<Coin>)]
    /// Queries the pending tokens available for a specific adress
    Pending { address: String },
}
