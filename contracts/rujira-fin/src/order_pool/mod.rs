use cw_storage_plus::Map;
use rujira_rs::{bid_pool, DecimalScaled};

mod context;
mod events;
pub mod execute;
mod order;
mod order_manager;
mod pool;
mod pool_key;
pub mod query;

#[cfg(test)]
mod tests;

pub(crate) const SNAPSHOTS: Map<(PoolKey, bid_pool::SumSnapshotKey), DecimalScaled> =
    Map::new("snapshots");
// The POOLS Map is used simply as an indicator that there is a non-zero BidPool at this key
// The BID_POOLS Map is used to store the BidPool itself, and the Key is used to populate the Pool values
pub(crate) const POOLS: Map<PoolKey, ()> = Map::new("pools");
pub(crate) const BID_POOLS: Map<PoolKey, bid_pool::Pool> = Map::new("bid-pools");

pub use context::PoolContext;
pub use events::{
    event_create_order, event_increase_order, event_retract_order, event_withdraw_order,
};
pub use order::Order;
pub use order_manager::{ExecutionResult, OrderManager};
pub use pool::Pool;
pub use pool_key::{PoolKey, PoolType};
