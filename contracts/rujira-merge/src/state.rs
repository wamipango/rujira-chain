use std::{
    cmp::min,
    ops::{Add, Mul, Sub},
};

use cosmwasm_std::{Addr, Decimal, StdResult, Storage, Timestamp, Uint128};
use cw_storage_plus::{Item, Map};
use rujira_rs::{
    merge::{AccountResponse, StatusResponse},
    SharePool,
};

use crate::{config::Config, ContractError};

pub static TOTAL_MERGED: Item<Uint128> = Item::new("merged");
pub static POOL: Item<SharePool> = Item::new("pool");
pub static ACCOUNTS: Map<Addr, (Uint128, Uint128)> = Map::new("accounts");

pub fn init(storage: &mut dyn Storage) -> StdResult<()> {
    TOTAL_MERGED.save(storage, &Uint128::zero())?;
    POOL.save(storage, &SharePool::default())?;
    Ok(())
}

pub fn status(storage: &dyn Storage) -> StdResult<StatusResponse> {
    let pool = POOL.load(storage)?;
    Ok(StatusResponse {
        merged: TOTAL_MERGED.load(storage)?,
        shares: pool.shares(),
        size: pool.size(),
    })
}

pub fn account(storage: &dyn Storage, addr: &Addr) -> Result<AccountResponse, ContractError> {
    let pool = POOL.load(storage)?;
    let (shares, merged) = ACCOUNTS.load(storage, addr.clone()).unwrap_or_default();
    let size = pool.ownership(shares);
    Ok(AccountResponse {
        addr: addr.to_string(),
        shares,
        merged,
        size,
    })
}

/// Deposit an `amount` of merge tokens to an account.
/// Returns the amount of share tokens issued
pub fn execute_deposit(
    storage: &mut dyn Storage,
    config: &Config,
    now: Timestamp,
    account: &Addr,
    amount: Uint128,
) -> Result<Uint128, ContractError> {
    let factor = decay_factor(config, &now);
    // 1. We allocate before we process the deposit in order to increase the share ratio _after_ the
    // suplus has been applied to it. Otherwise a new depositor would instantly earn a share of the
    // surplus accrued since the last `allocate`
    let mut pool = allocate(storage, config, factor)?;

    // The value of the merge amount, denominated in RUJI, with the decay factor applied
    let value = merge_ratio(config, &factor).mul(Decimal::from_ratio(amount, Uint128::one()));
    let shares_new = pool.join(value.to_uint_floor())?;
    // 2. Remove liability from future `allocate`
    TOTAL_MERGED.update(storage, |x| -> StdResult<Uint128> { Ok(x.add(amount)) })?;

    POOL.save(storage, &pool)?;
    ACCOUNTS.update(
        storage,
        account.clone(),
        |x| -> StdResult<(Uint128, Uint128)> {
            match x {
                Some((shares, merged)) => Ok((shares.add(shares_new), merged.add(amount))),
                None => Ok((shares_new, amount)),
            }
        },
    )?;

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
    let mut pool = allocate(storage, config, factor)?;
    let (account_shares, merged) = ACCOUNTS.load(storage, account.clone()).unwrap_or_default();
    let checked_amount = min(account_shares, amount);
    let allocation = pool.leave(checked_amount)?;
    ACCOUNTS.save(
        storage,
        account.clone(),
        &(account_shares.sub(checked_amount), merged),
    )?;
    POOL.save(storage, &pool)?;
    Ok(allocation)
}

/// Calculates the maximum liability of the contract based on the remaining merge tokens,
/// and therefore the surplus supply of allocation, which can be dsitributed to current
/// Share holders (mergers), and increases TOTAL_ALLOCATED accordingly
fn allocate(storage: &dyn Storage, config: &Config, factor: Decimal) -> StdResult<SharePool> {
    let mut pool = POOL.load(storage)?;
    let merged = TOTAL_MERGED.load(storage).unwrap_or_default();
    let remaining = config.merge_supply.checked_sub(merged)?;
    let liability = merge_ratio(config, &factor)
        .mul(Decimal::from_ratio(remaining, Uint128::one()))
        .to_uint_ceil();

    let surplus = config.ruji_allocation.sub(pool.size()).sub(liability);
    pool.deposit(surplus);
    Ok(pool)
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
            POOL.load(storage).unwrap().size(),
            Uint128::from(2_500_000u128)
        );
        assert_eq!(
            POOL.load(storage).unwrap().shares(),
            Uint128::from(2_500_000u128)
        );
        assert_eq!(
            ACCOUNTS.load(storage, account).unwrap(),
            (Uint128::from(2_500_000u128), Uint128::from(5_000_000u128))
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
            POOL.load(storage).unwrap().size(),
            Uint128::from(7_500_000u128)
        );
        assert_eq!(
            POOL.load(storage).unwrap().shares(),
            Uint128::from(7_500_000u128)
        );
        assert_eq!(
            ACCOUNTS.load(storage, account.clone()).unwrap(),
            (Uint128::from(5_000_000u128), Uint128::from(10_000_000u128))
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
            POOL.load(storage).unwrap().size(),
            Uint128::from(5_500_000u128)
        );
        assert_eq!(
            POOL.load(storage).unwrap().shares(),
            Uint128::from(5_500_000u128)
        );
        assert_eq!(
            ACCOUNTS.load(storage, account).unwrap(),
            (Uint128::from(3_000_000u128), Uint128::from(10_000_000u128))
        );
    }

    #[test]
    fn test_decaying_deposits() {
        let config = Config {
            merge_denom: "ukuji".to_string(),
            merge_supply: Uint128::from(200_000_000u128),
            ruji_denom: "uruji".to_string(),
            ruji_allocation: Uint128::from(100_000_000u128),
            decay_starts_at: Timestamp::from_seconds(1_000),
            decay_ends_at: Timestamp::from_seconds(1_001_000),
        };
        let mut deps = mock_dependencies();
        let storage = deps.as_mut().storage;

        init(storage).unwrap();

        let shares = execute_deposit(
            storage,
            &config,
            Timestamp::from_seconds(0),
            &Addr::unchecked("account"),
            Uint128::from(5_000_000u128),
        )
        .unwrap();
        assert_eq!(shares, Uint128::from(2_500_000u128));
        assert_eq!(
            TOTAL_MERGED.load(storage).unwrap(),
            Uint128::from(5_000_000u128)
        );
        assert_eq!(
            POOL.load(storage).unwrap().size(),
            Uint128::from(2_500_000u128)
        );
        assert_eq!(
            POOL.load(storage).unwrap().shares(),
            Uint128::from(2_500_000u128)
        );
        assert_eq!(
            ACCOUNTS.load(storage, Addr::unchecked("account")).unwrap(),
            (Uint128::from(2_500_000u128), Uint128::from(5_000_000u128))
        );

        let shares = execute_deposit(
            storage,
            &config,
            Timestamp::from_seconds(101_000),
            &Addr::unchecked("account2"),
            Uint128::from(2_500_000u128),
        )
        .unwrap();
        // // 10% of the way through at a 1:2 initial ratio.
        assert_eq!(shares, Uint128::from(229_591u128));
        assert_eq!(
            TOTAL_MERGED.load(storage).unwrap(),
            Uint128::from(7_500_000u128)
        );
        // Initial allocation of 2.5m
        // 10% of remaining 97.5m = 9.75m
        // Deposit ownership of 2.5m * 0.5 * 0.9 = 1.125m
        assert_eq!(
            POOL.load(storage).unwrap().size(),
            Uint128::from(13_375_000u128)
        );
        // Now we issue shares so that 229_591 * (13_375_000 / 2_729_591) = 1.125m
        // and 2_500_000 * (13_375_000 / 2_729_591) = original ownership + 10% bonus (9.75m) = 12.25
        assert_eq!(
            POOL.load(storage).unwrap().shares(),
            Uint128::from(2_729_591u128)
        );
        assert_eq!(
            ACCOUNTS.load(storage, Addr::unchecked("account2")).unwrap(),
            (Uint128::from(229_591u128), Uint128::from(2_500_000u128))
        );

        // Deposit extra
        let shares = execute_deposit(
            storage,
            &config,
            Timestamp::from_seconds(101_000),
            &Addr::unchecked("account2"),
            Uint128::from(2_500_000u128),
        )
        .unwrap();
        // 10% of the way through at a 1:2 initial ratio.
        assert_eq!(shares, Uint128::from(229_591u128));
        assert_eq!(
            TOTAL_MERGED.load(storage).unwrap(),
            Uint128::from(10_000_000u128)
        );
        // Initial allocation of 2.5m
        // 10% of remaining 97.5m = 9.75m
        // Deposit ownership of 5m * 0.5 * 0.9 = 2.5m
        assert_eq!(
            POOL.load(storage).unwrap().size(),
            Uint128::from(14_500_000u128)
        );
        assert_eq!(
            POOL.load(storage).unwrap().shares(),
            Uint128::from(2_959_182u128)
        );
        assert_eq!(
            ACCOUNTS.load(storage, Addr::unchecked("account2")).unwrap(),
            (Uint128::from(459_182u128), Uint128::from(5_000_000u128))
        );

        // Attempt to execute a merge after the decay window ends.
        execute_deposit(
            storage,
            &config,
            Timestamp::from_seconds(1_001_001),
            &Addr::unchecked("account3"),
            Uint128::from(2_500_000u128),
        )
        .unwrap_err();
        assert_eq!(
            TOTAL_MERGED.load(storage).unwrap(),
            Uint128::from(10_000_000u128)
        );
        // Size remains the same - execute_deposit errors before the pool is saved
        assert_eq!(
            POOL.load(storage).unwrap().size(),
            Uint128::from(14_500_000u128)
        );
        assert_eq!(
            POOL.load(storage).unwrap().shares(),
            Uint128::from(2_959_182u128)
        );
        ACCOUNTS
            .load(storage, Addr::unchecked("account3"))
            .unwrap_err();
    }
}
