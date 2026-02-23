# Rujira Ghost Vault

Money-market vault for a single denom. Accepts deposits, mints receipt tokens, and tracks utilization/interest rate. Whitelisted contracts such as Ghost Credit can borrow and repay on behalf of users. Interest rate is a function of utilization (borrowing/deposit) and is distributed from borrowers to lenders, minus protocol fee.

## Execute

Deposit (send the vault denom as funds):
```json
{ "deposit": { "callback": null } }
```

Withdraw (send receipt token as funds):
```json
{ "withdraw": { "callback": null } }
```

Borrow (whitelisted market only):
```json
{ "market": { "borrow": { "amount": "1000000", "callback": null, "delegate": null } } }
```

Repay (whitelisted market only, send funds in denom):
```json
{ "market": { "repay": { "delegate": null } } }
```

## Query

Borrower:
```json
{ "borrower": { "addr": "thor1market..." } }
```

Delegate:
```json
{ "delegate": { "borrower": "thor1market...", "addr": "thor1delegate..." } }
```

Borrowers (paged):
```json
{ "borrowers": { "limit": 20, "start_after": null } }
```

## Invariants

- Borrow/repay market actions are restricted to whitelisted borrowers (`Borrower::load`), including delegate accounting per borrower.
- Interest accrual runs before all execute and query paths (`state.distribute_interest`), so debt/deposit pools are always read/written at current accrual.
- Borrowing is capacity-capped (`deposits - debt`) and borrower-limit-capped (share ownership value cannot exceed configured borrower limit).
- Repay is liability-capped per actor/delegate; overpayments are refunded and cannot burn more shares than owed.
- Deposit receipt minting and withdrawal burn paths are symmetric around `SharePool` joins/leaves, preserving receipt-to-pool ownership semantics.
- Config safety checks enforce valid interest curve and `fee < 1`; protocol-fee shares mint only from accrued interest.

## Notes

- Receipt token denom is `ghost-vault/<denom>`.
- Borrow/repay are only allowed for whitelisted contracts.
