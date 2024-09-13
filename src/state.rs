use std::{
    cmp::min,
    ops::{Add, Mul, Sub},
};

use cosmwasm_std::{Addr, Decimal, StdResult, Storage, Timestamp, Uint128};
use cw_storage_plus::{Item, Map};

use crate::{config::Config, ContractError};

pub static TOTAL_MERGED: Item<Uint128> = Item::new("merged");
pub static POOL_SIZE: Item<Uint128> = Item::new("size");
pub static POOL_SHARES: Item<Uint128> = Item::new("shares");
pub static ACCOUNTS: Map<Addr, Uint128> = Map::new("accounts");

pub fn init(storage: &mut dyn Storage) -> StdResult<()> {
    TOTAL_MERGED.save(storage, &Uint128::zero())?;
    POOL_SIZE.save(storage, &Uint128::zero())?;
    POOL_SHARES.save(storage, &Uint128::zero())?;
    Ok(())
}

/// Deposit an `amount` of merge tokens to an account.
/// Returns the amount of share tokens issued
pub fn execute_deposit(
    storage: &mut dyn Storage,
    config: &Config,
    now: Timestamp,
    account: &Addr,
    amount: Uint128,
) -> StdResult<Uint128> {
    let factor = decay_factor(config, &now);
    // 1. We allocate before we process the deposit in order to increase the share ratio _after_ the
    // suplus has been applied to it. Otherwise a new depositor would instantly earn a share of the
    // surplus accrued since the last `allocate`
    allocate(storage, config, factor)?;

    let pool_size = POOL_SIZE.load(storage).unwrap_or_default();
    let shares_total = POOL_SHARES.load(storage).unwrap_or_default();

    // The value of the merge amount, denominated in RUJI, with the decay factor applied
    let value = merge_ratio(config, &factor).mul(Decimal::from_ratio(amount, Uint128::one()));
    let share_ratio =
        Decimal::checked_from_ratio(pool_size, shares_total).unwrap_or_else(|_| factor);

    // 2. Remove liability from future `allocate`
    TOTAL_MERGED.update(storage, |x| -> StdResult<Uint128> { Ok(x.add(amount)) })?;

    // 3. Issue shares for the deposited value and add the value to the Pool size.
    //    This should be a net-zero operation for shareholders; the pool should be increased by the same as the share inflation
    //    And the new depositor's shares should entitle them to `value` of the total pool size
    let shares_new = value
        .checked_div(share_ratio)
        .unwrap_or_default()
        .to_uint_floor();

    POOL_SHARES.save(storage, &shares_total.add(shares_new))?;
    POOL_SIZE.save(storage, &pool_size.add(value.to_uint_floor()))?;
    ACCOUNTS.update(storage, account.clone(), |x| -> StdResult<Uint128> {
        Ok(x.unwrap_or_default().add(shares_new))
    })?;

    Ok(shares_new)
}

/// Withdraw an `amount` of shares from an account.
/// Returns the amount of RUJI tokens withdrawn
pub fn execute_withdraw(
    storage: &mut dyn Storage,
    config: &Config,
    now: Timestamp,
    account: &Addr,
    amount: Uint128,
) -> Result<Uint128, ContractError> {
    let factor = decay_factor(config, &now);
    allocate(storage, config, factor)?;

    let pool_size = POOL_SIZE.load(storage).unwrap_or_default();
    let shares_total = POOL_SHARES.load(storage).unwrap_or_default();
    // If either are zero, nothing to withdraw
    let share_ratio = Decimal::checked_from_ratio(pool_size, shares_total)?;
    let account_shares = ACCOUNTS.load(storage, account.clone()).unwrap_or_default();
    let checked_amount = min(account_shares, amount);
    let allocation = Decimal::from_ratio(checked_amount, Uint128::one())
        .mul(share_ratio)
        .to_uint_floor();
    ACCOUNTS.save(
        storage,
        account.clone(),
        &account_shares.sub(checked_amount),
    )?;
    POOL_SHARES.save(storage, &shares_total.sub(checked_amount))?;
    POOL_SIZE.save(storage, &pool_size.sub(allocation))?;
    Ok(allocation)
}

/// Calculates the maximum liability of the contract based on the remaining merge tokens,
/// and therefore the surplus supply of allocation, which can be dsitributed to current
/// Share holders (mergers), and increases TOTAL_ALLOCATED accordingly
fn allocate(storage: &mut dyn Storage, config: &Config, factor: Decimal) -> StdResult<Uint128> {
    let merged = TOTAL_MERGED.load(storage).unwrap_or_default();
    let allocated = POOL_SIZE.load(storage).unwrap_or_default();
    let remaining = config.merge_supply.checked_sub(merged)?;
    let liability = merge_ratio(config, &factor)
        .mul(Decimal::from_ratio(remaining, Uint128::one()))
        .to_uint_ceil();

    let surplus = config.ruji_allocation.sub(allocated).sub(liability);
    POOL_SIZE.update(storage, |x| Ok(x.add(surplus)))
}

/// The amount of decay remaining in a linear model
fn decay_factor(config: &Config, now: &Timestamp) -> Decimal {
    if now.le(&config.decay_starts_at) {
        return Decimal::one();
    }
    if now.gt(&config.decay_ends_at) {
        return Decimal::zero();
    }
    let remaning = config.decay_ends_at.seconds().sub(now.seconds());
    let duration = config
        .decay_ends_at
        .seconds()
        .sub(config.decay_starts_at.seconds());

    Decimal::from_ratio(remaning, duration)
}

fn merge_ratio(config: &Config, factor: &Decimal) -> Decimal {
    Decimal::from_ratio(config.ruji_allocation, config.merge_supply).mul(factor)
}

#[cfg(test)]
mod tests {
    use cosmwasm_std::testing::mock_dependencies;

    use super::*;

    #[test]
    fn test_deposit_pre_decay() {
        let config = Config {
            merge_denom: "ukuji".to_string(),
            merge_supply: Uint128::from(200_000_000u128),
            ruji_denom: "uruji".to_string(),
            ruji_allocation: Uint128::from(100_000_000u128),
            decay_starts_at: Timestamp::from_seconds(1_000),
            decay_ends_at: Timestamp::from_seconds(1_000_000),
        };
        let now = Timestamp::from_seconds(0);
        let mut deps = mock_dependencies();
        let account = Addr::unchecked("account");

        let storage = deps.as_mut().storage;
        init(storage).unwrap();

        let shares = execute_deposit(
            storage,
            &config,
            now,
            &account,
            Uint128::from(5_000_000u128),
        )
        .unwrap();

        assert_eq!(shares, Uint128::from(2_500_000u128));
        assert_eq!(
            TOTAL_MERGED.load(storage).unwrap(),
            Uint128::from(5_000_000u128)
        );
        assert_eq!(
            POOL_SIZE.load(storage).unwrap(),
            Uint128::from(2_500_000u128)
        );
        assert_eq!(
            POOL_SHARES.load(storage).unwrap(),
            Uint128::from(2_500_000u128)
        );
        assert_eq!(
            ACCOUNTS.load(storage, account).unwrap(),
            Uint128::from(2_500_000u128)
        );

        let account = Addr::unchecked("account2");

        let shares = execute_deposit(
            storage,
            &config,
            now,
            &account,
            Uint128::from(10_000_000u128),
        )
        .unwrap();

        assert_eq!(shares, Uint128::from(5_000_000u128));
        assert_eq!(
            TOTAL_MERGED.load(storage).unwrap(),
            Uint128::from(15_000_000u128)
        );
        assert_eq!(
            POOL_SIZE.load(storage).unwrap(),
            Uint128::from(7_500_000u128)
        );
        assert_eq!(
            POOL_SHARES.load(storage).unwrap(),
            Uint128::from(7_500_000u128)
        );
        assert_eq!(
            ACCOUNTS.load(storage, account.clone()).unwrap(),
            Uint128::from(5_000_000u128)
        );

        let withdrawal = execute_withdraw(
            storage,
            &config,
            now,
            &account,
            Uint128::from(2_000_000u128),
        )
        .unwrap();

        assert_eq!(withdrawal, Uint128::from(2_000_000u128));
        assert_eq!(
            TOTAL_MERGED.load(storage).unwrap(),
            Uint128::from(15_000_000u128)
        );
        assert_eq!(
            POOL_SIZE.load(storage).unwrap(),
            Uint128::from(5_500_000u128)
        );
        assert_eq!(
            POOL_SHARES.load(storage).unwrap(),
            Uint128::from(5_500_000u128)
        );
        assert_eq!(
            ACCOUNTS.load(storage, account).unwrap(),
            Uint128::from(3_000_000u128)
        );
    }
}
