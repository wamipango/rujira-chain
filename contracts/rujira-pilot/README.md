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

- Swap intake is constrained to the configured ask denom (`must_pay`), and outputs are only bid-denom return plus any unspent ask-denom remainder.
- Order placement/updates enforce `premium <= max_premium`; out-of-range premium levels are rejected.
- Order execution nets required sends against available funds/withdrawals, so orders cannot increase beyond user-provided collateral.
- Maker and taker fees are separated from user payouts and routed to `fee_address`.
- Query and execution derive prices from the same premium/oracle model, keeping pool pricing and simulation semantics aligned.

## Notes

- `max_premium` is enforced at order placement time.
- `swap` uses the oracle price plus the pool premium.
