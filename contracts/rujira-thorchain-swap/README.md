# Rujira Thorchain Swap

Rujira THORChain Swap (aka Virtualization Strategy) is an Autmoated Market Making strategy that provides atomic swap execution, settled over base layer pools, by borrowing returned funds from rujira-ghost-vault.

Conforms to BOW (RUJI Pools) market maker interface for integration with FINS's orderbook (RUJI Trade).
Only whitelisted markets can call swap to validate the base-layer quote before execution. Supports reserve fees and streaming swaps with configurable limits.

## Execute

Swap (market-only; send one coin as funds):
```json
{
  "swap": {
    "min_return": { "denom": "usdc", "amount": "100000" },
    "to": "thor1...",
    "callback": null
  }
}
```

Repay any outstanding vault debt (anyone can call):
```json
{ "repay": {} }
```

## Query

Quote (BOW-style):
```json
{ "quote": { "min_price": "0.25", "offer_denom": "ruji", "ask_denom": "usdc", "data": null } }
```

## Invariants

- **Authorization:** which actors are allowed to execute privileged flows.
- **Accounting:** balances/shares/debt conservation rules before and after each state transition.
- **Safety bounds:** caps, thresholds, and ratio constraints that can never be exceeded.
- **External calls:** callback/execute side effects that must preserve contract safety.

## Notes

- `swap` validates the base-layer quote before executing.
- Borrowed funds are sourced from the configured vault for the output denom.
