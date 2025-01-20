use cw_storage_plus::Item;
use rujira_rs::SharePool;

// Apply interest to pooled token deposits
pub static DEPOSIT_POOL: Item<SharePool> = Item::new("deposits");
// Apply interest to debt that is issued to trusted contracts when borrows are created
pub static DEBT_POOL: Item<SharePool> = Item::new("debt");
