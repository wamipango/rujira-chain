# Rujira Staking

Generic staking contract for revenue distribution with inbuilt liquid staking.
Depositors have two options:
1. Staking in an account for yield paid in revenue_denom, typically $USDC, periodically manually claiming.
2. Minting a liquid token, representing a share of a pool, which claims revenue_denom and swaps to bond_denom, increasing the size of the pool over time

## Execute

Account bond (send `bond_denom` as funds):
```json
{ "account": { "bond": {} } }
```

Account claim:
```json
{ "account": { "claim": {} } }
```

Account withdraw (optional amount):
```json
{ "account": { "withdraw": { "amount": "1000000" } } }
```

Liquid bond (send `bond_denom` as funds):
```json
{ "liquid": { "bond": {} } }
```

Liquid unbond (send receipt token as funds):
```json
{ "liquid": { "unbond": {} } }
```

## Query

Account:
```json
{ "account": { "addr": "thor1..." } }
```

## Invariants

- **Authorization:** which actors are allowed to execute privileged flows.
- **Accounting:** balances/shares/debt conservation rules before and after each state transition.
- **Safety bounds:** caps, thresholds, and ratio constraints that can never be exceeded.
- **External calls:** callback/execute side effects that must preserve contract safety.

## Notes

- Liquid staking receipt denom is `staking-<bond_denom>`.
- `fee` (if set) is taken from revenue conversions.
