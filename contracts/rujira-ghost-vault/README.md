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

- **Authorization:** which actors are allowed to execute privileged flows.
- **Accounting:** balances/shares/debt conservation rules before and after each state transition.
- **Safety bounds:** caps, thresholds, and ratio constraints that can never be exceeded.
- **External calls:** callback/execute side effects that must preserve contract safety.

## Notes

- Receipt token denom is `ghost-vault/<denom>`.
- Borrow/repay are only allowed for whitelisted contracts.
