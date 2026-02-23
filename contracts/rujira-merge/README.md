# RUJI Merge

Merge contract for $RUJI, the fee-switch token of The Rujira Alliance. Merge tokens are exchanged for $RUJI, with a linear decay, and $RUJI re-allocation mechansism.

A user can either `Deposit` merge tokens, or `Withdraw` $RUJI tokens from the contract.

## Deposit

When depositing, Merge tokens are exchanged for Shares in the contract.

The deposit value is calculated as `deposit * ratio * decay`, this value is transferred into the `POOL_SIZE` (ie $RUJI allocated) bucket, and a quantity of shares are issued, such that `POOL_SIZE / POOL_SHARES == (POOL_SIZE + value) / (POOL_SHARES + new_shares)`. `TOTAL_MERGED` is also increased by `deposit` amount.

## Withdraw

A user can withdraw $RUJI from the contract, requesting an amount of their pool shares to redeem.

$RUJI withdrawn amount is `share_amount * POOL_SIZE / POOL_SHARES`, `POOL_SIZE`, `POOL_SHARES` and the Account share balance are reduced accordingly by the withdrawn amount and the redeemed shares.

## Allocate

`Allocate` is executed prior to any user action (`Deposit` or `Withdraw`).

This takes the value `TOTAL_MERGED` (the total amount of merge tokens deposited), and uses `Config.merge_supply` to calculate the un-merged supply of merge tokens. `unmerged * ratio * (1 - decay)` therefore returns the maximum liability of the contract in $RUJI terms (i.e. if every remaining merge token was merged in the next block). `Config.ruji_allocation - POOL_SIZE - liability` therefore is the accounted surplus of $RUJI tokens. Finally, `POOL_SIZE += surplus`, increasing the Pool Size distributes the surplus $RUJI tokens proportionally across all existing mergers.

## Execute

Deposit merge tokens (send `merge_denom` as funds):
```json
{ "deposit": {} }
```

Withdraw RUJI by share amount (no funds):
```json
{ "withdraw": { "share_amount": "1000" } }
```

## Query

Account:
```json
{ "account": { "addr": "thor1..." } }
```

## Invariants

- Instantiate requires exact funding of `ruji_allocation` in `ruji_denom`; under/overfunded deployments fail.
- Deposits accept only `merge_denom` (`must_pay`), and withdrawals are nonpayable requests against owned shares.
- `allocate` is executed before each deposit/withdraw, so surplus RUJI distribution is applied to existing share holders before new share issuance.
- Share math is pool-proportional (`SharePool` join/leave), and account state updates atomically with pool updates.
- Withdrawals are clamped to owned shares (`min(requested, account_shares)`), preventing negative-share states.
- Config validation enforces non-empty denoms, non-zero supplies/allocations, and valid decay window ordering.

## Notes

- Deposits after `decay_starts_at` receive a linearly decaying share.
- Withdrawals return `ruji_denom` proportional to shares.
