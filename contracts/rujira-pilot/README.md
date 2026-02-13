# Rujira Pilot

Launchpad for the Rujira network. Utilizes a Dutch auction mechanism for public token launches. Users place bids at a discount (in %) relative to a reference target price set by the issuer. Bids get filled from the lowest discount to the highest discount until the amount of token offered for sale is fully exhausted.

## Execute

Swap (send ask denom as funds):
```json
{ "swap": { "min_return": "100000", "to": "thor1...", "callback": null } }
```

Order management (premium, offer_amount):
```json
{ "order": [ [10, "1000000"], [25, "500000"] ] }
```

## Query

Simulate:
```json
{ "simulate": { "denom": "ruji", "amount": "1000000" } }
```

Single order:
```json
{ "order": ["thor1...", 10] }
```

Orders:
```json
{ "orders": { "owner": "thor1...", "offset": 0, "limit": 20 } }
```

Pools:
```json
{ "pools": { "offset": 0, "limit": 20 } }
```

## Invariants

- **Authorization:** which actors are allowed to execute privileged flows.
- **Accounting:** balances/shares/debt conservation rules before and after each state transition.
- **Safety bounds:** caps, thresholds, and ratio constraints that can never be exceeded.
- **External calls:** callback/execute side effects that must preserve contract safety.

## Notes

- `max_premium` is enforced at order placement time.
- `swap` uses the oracle price plus the pool premium.
