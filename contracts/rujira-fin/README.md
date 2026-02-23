# RUJI Trade - FIN

100% on-chain orderbook for spot trading.
Supports market swaps, limit orders, and oracle-priced orders with deterministic execution.
Designed to operate cranklessly with O(1) complexity per operation.

## Execute

Swap (send exactly one input coin as funds):
```json
{ "swap": { "min_return": "100000", "to": "thor1...", "callback": null } }
```

Exact return swap:
```json
{ "swap": { "exact_return": "250000", "to": null, "callback": null } }
```

Limit swap (best effort at or better than price):
```json
{ "swap": { "price": "0.25", "to": "thor1...", "callback": null } }
```

Manage orders (order targets are tuples: [side, price, optional_offer_amount]):
```json
{
  "order": [
    [
      ["base", {"fixed": "0.25"}, "1000000"],
      ["quote", {"oracle": 50}, "500000"]
    ],
    null
  ]
}
```

Notes for `order`:
- `side` is `"base"` or `"quote"`.
- `price` is either `{ "fixed": "<decimal>" }` or `{ "oracle": <bps> }`.
- `optional_offer_amount` is the target amount to maintain at that price (omit or null to withdraw).

## Query

Simulate a swap:
```json
{ "simulate": { "denom": "ruji", "amount": "1000000" } }
```

Single order:
```json
{ "order": ["thor1...", "base", {"fixed": "0.25"}] }
```

Paged orders:
```json
{ "orders": { "owner": "thor1...", "side": "base", "offset": 0, "limit": 20 } }
```

Orderbook:
```json
{ "book": { "limit": 20, "offset": 0 } }
```

## Callbacks

If `callback` is provided, the contract sends the swap result to the callback address with the provided payload; otherwise it sends directly to `to` (or the sender).

## Invariants

- User-facing `Swap`/`Order`/`Range` paths are wrapped into self-calls via `Arb`; `DoSwap`/`DoOrder`/`DoRange` reject non-contract callers.
- Net taker/maker/arb fees are normalized and sent to `fee_address`, so fee accrual is explicit and not left in transient balances.
- Config updates (`instantiate`, `sudo`, `migrate`) are re-validated: denoms valid, oracle pairs resolvable (if set), `fee_maker`/`fee_taker` < 1, non-zero `range_delta`, valid tick.
- Order execution enforces fund sufficiency by netting required sends against available/withdrawn balances; insufficient collateralized orders fail atomically.
- Scheduled arbitrage only re-schedules when invoked by the scheduler module address, preventing arbitrary callers from forcing recurring cranks.
