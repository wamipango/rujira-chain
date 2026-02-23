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

- `swap` is market-gated: caller must be explicitly enabled in `MARKETS`, otherwise execution is unauthorized.
- Before borrowing, the contract validates base-layer quote return (`expected + outbound`) against `min_return`; insufficient quotes revert.
- Output denom borrow source is explicit per-denom vault mapping (`VAULTS`), and reserve fee is deducted before borrow payout.
- Callback settlement is vault-authenticated: only the configured vault for the received denom can trigger `ExecuteMsg::Callback`.
- Repayment runs before new swap borrow (or via `Repay`) and caps each repay to outstanding vault debt for that denom.
- Unknown/random token balances are ignored by repayment scanning, preventing accidental hard failures from unsolicited deposits.

## Notes

- `swap` validates the base-layer quote before executing.
- Borrowed funds are sourced from the configured vault for the output denom.
