use crate::{fin, proto::types::QueryNodeResponse, CallbackData, TokenMetadata};
use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Coin, Decimal, Event, SignedDecimal, Timestamp, Uint128};
use std::str::FromStr;

#[cw_serde]
pub struct RangeConfig {
    /// Bid-side single-sided SCL liquidity config. Highest bid price offered by the BRUNE
    /// market maker. Must be < 1.
    pub high: Decimal,
    /// Lowest bid price offered by the BRUNE market maker
    pub low: Decimal,
    /// Linear skew of quote distribution
    pub skew: SignedDecimal,
    /// Distance between successive bid quote levels
    pub step: Decimal,
}

#[cw_serde]
pub struct InstantiateMsg {
    /// The FIN orderbook contract address that trades the B-TOKEN
    pub fin_contract: String,
    /// Target amount of RUNE bonded as a percentage of BRUNE minted
    pub target_utilization: Decimal,
    /// The Node Operator fee below which all Bond weighting is euqal
    pub min_node_fee: Decimal,
    /// Single-sided SCL range used for BRUNE bid liquidity
    pub range: RangeConfig,
    /// Allow any caller to process nodes via execute
    pub permissionless: bool,
    /// The contract address that receives revenue
    pub stake_contract: String,
    /// The duration over which revenue is distributed, to prevent spikes following churn
    pub revenue_smear: u32,
    /// Max bond per whitelisted node. Should eventually be replaced with a query of the hard bond cap
    pub max_bond: Uint128,
    /// Max amount of BRUNE that can be minted
    pub mint_cap: Uint128,
    /// The B-TOKEN denom ID
    pub token_id: String,
    /// Token Metadata for the B-TOKEN
    pub token_metadata: TokenMetadata,
}

#[cw_serde]
pub enum ExecuteMsg {
    Swap {
        min_return: Coin,
        to: Option<String>,
        callback: Option<CallbackData>,
    },
    /// Processes a new Node Address as a bond target (permissionless only)
    ProcessNode { addr: String },
}

#[allow(clippy::large_enum_variant)]
#[cw_serde]
pub enum SudoMsg {
    /// Processes a new Node Address as a bond target
    /// The contract address must already be registered as a bond provider on the Node
    /// otherwise, Node is removed from the candidate list
    ProcessNode {
        addr: String,
    },
    UpdateConfig(ConfigUpdate),
}

#[cw_serde]
pub struct ConfigUpdate {
    pub fin_contract: Option<String>,
    pub target_utilization: Option<Decimal>,
    pub min_node_fee: Option<Decimal>,
    pub range: Option<RangeConfig>,
    pub permissionless: Option<bool>,
    pub stake_contract: Option<String>,
    pub revenue_smear: Option<u32>,
    pub max_bond: Option<Uint128>,
    pub mint_cap: Option<Uint128>,
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(ConfigResponse)]
    Config {},
    #[returns(StateResponse)]
    State {},
    #[returns(EventLogResponse)]
    Events {
        start_after: Option<u128>,
        limit: Option<u32>,
    },
    #[returns(fin::market_maker::QuoteResponse)]
    Quote(fin::market_maker::QuoteRequest),
}

#[cw_serde]
pub struct ConfigResponse {
    pub target_utilization: Decimal,
    pub min_node_fee: Decimal,
    pub range: RangeConfig,
    pub permissionless: bool,
    pub fin_contract: String,
    pub stake_contract: String,
    pub revenue_smear: u32,
    pub token_id: String,
    pub max_bond: Uint128,
    pub mint_cap: Uint128,
}

#[cw_serde]
pub struct StateResponse {
    /// Total BRUNE minted.
    pub minted: Decimal,
    /// Whitelisted nodes
    pub nodes: NodesResponse,
    pub revenue: RevenueResponse,
}

#[cw_serde]
pub struct NodesResponse {
    pub bond: Decimal,
    pub weight: Decimal,
    pub capacity: Decimal,
    pub nodes: Vec<NodeResponse>,
}

#[cw_serde]
pub struct EventLogResponse {
    pub events: Vec<LoggedEvent>,
}

#[cw_serde]
pub struct LoggedEvent {
    pub id: u128,
    pub time: Timestamp,
    pub height: u64,
    pub event: Event,
}

#[cw_serde]
pub enum NodeStatus {
    Unknown,
    Whitelisted,
    Standby,
    Ready,
    Active,
    Disabled,
}

impl FromStr for NodeStatus {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Ok(match value {
            "Whitelisted" => Self::Whitelisted,
            "Standby" => Self::Standby,
            "Ready" => Self::Ready,
            "Active" => Self::Active,
            "Disabled" => Self::Disabled,
            _ => Self::Unknown,
        })
    }
}

#[cw_serde]
pub struct NodeResponse {
    pub addr: String,
    /// The fee charged by the node operator
    pub fee: Decimal,
    /// The amount of bond on this Node that we own
    pub bond: Decimal,
    /// The allocation weight
    pub weight: Decimal,
    /// The remaining bond capacity
    pub capacity: Decimal,
    /// Whether it's asking or forced to leave this churn
    pub is_leaving: bool,
    /// Current node status
    pub status: NodeStatus,
    /// Raw Node response from THORChain
    pub node: QueryNodeResponse,
}

#[cw_serde]
pub struct RevenueResponse {
    /// The amount of revenue pending distribution
    pub pending: Decimal,
    /// The tokens per smear period that the contract has earned "at"
    /// Used to pro-rate revenue sent to the staking contract
    pub fee_rate: Decimal,
    /// The last time this Revenue was adjusted
    pub timestamp: Timestamp,
}
