use std::ops::Sub;

use cosmwasm_std::{CanonicalAddr, Decimal, MessageInfo, Uint128};
use sha2::{Digest, Sha256};

/// A version of cw_utils::may_pay that doesn't error on > 1 denom sent
pub fn can_pay(info: &MessageInfo, denom: &str) -> Uint128 {
    info.funds
        .iter()
        .find(|x| x.denom == denom)
        .map(|x| x.amount)
        .unwrap_or_default()
}

/// Splits a decimal value into its integer component and Decimal remainder
pub fn split_floor(decimal: Decimal) -> (Uint128, Decimal) {
    let int = decimal.to_uint_floor();
    let dec = decimal.sub(Decimal::from_atomics(int, 0).unwrap());
    (int, dec)
}

/// Cosmos-style module address (tmhash truncated to 20 bytes).
pub fn module_address(module_name: &str) -> CanonicalAddr {
    let mut hasher = Sha256::new();
    hasher.update(module_name.as_bytes());
    let hash = hasher.finalize();
    CanonicalAddr::from(hash[..20].to_vec())
}

#[cfg(test)]
mod tests {
    use super::module_address;
    use cosmwasm_std::testing::MockApi;
    use cosmwasm_std::Api;

    #[test]
    fn module_address_scheduler_thor_prefix() {
        let api = MockApi::default().with_prefix("thor");
        let addr = api.addr_humanize(&module_address("scheduler")).unwrap();

        assert_eq!(addr.as_str(), "thor15q46zcln5qkmyt7azje3qyvlrfxzl8j2v9k6rw");
    }
}
