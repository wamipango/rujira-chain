# Rujira BOW

Automated market maker (AMM) for RUJI Trade orderbook DEX (FIN) using XYK constant product logic to distribute liquidity in the orderbook.
Mints an LP receipt token via token factory and tracks pool state on-chain.
Supports swap, deposit, and withdraw flows with optional callbacks.

## Execute

Swap (send exactly one coin as the offer; `min_return` is the ask coin):
```json
{
  "swap": {
    "min_return": { "denom": "usdc", "amount": "100000" },
    "to": "thor1...",
    "callback": null
  }
}
```

Deposit (send pool assets as funds; optional min LP shares):
```json
{
  "deposit": {
    "min_return": "1000",
    "callback": null
  }
}
```

Withdraw (send LP shares as funds; LP denom is `bow-xyk-<x>-<y>`):
```json
{
  "withdraw": {
    "callback": null
  }
}
```

## Query

Strategy and state:
```json
{ "strategy": {} }
```

Quote for market-making:
```json
{
  "quote": {
    "min_price": "0.25",
    "offer_denom": "usdc",
    "ask_denom": "ruji",
    "data": null
  }
}
```

## Invariants

- **Authorization:** which actors are allowed to execute privileged flows.
- **Accounting:** balances/shares/debt conservation rules before and after each state transition.
- **Safety bounds:** caps, thresholds, and ratio constraints that can never be exceeded.
- **External calls:** callback/execute side effects that must preserve contract safety.

## Notes

- `swap` requires exactly one input coin as funds.
- `deposit` accepts multiple coins and mints LP shares via the token factory.
- `withdraw` requires LP shares as funds and returns underlying assets.
