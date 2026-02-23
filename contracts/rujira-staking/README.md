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

- All user staking paths (`Account` and `Liquid`) call `distribute` first, so rewards and pending swap accounting are synchronized before mutation.
- `Settle` is self-call-only, preventing external spoofing of swap-return accounting into `PENDING_SWAP`.
- Account staking always claims accrued rewards before bond/withdraw mutations, keeping per-account pending revenue consistent.
- Liquid staking is share-based: bond mints receipt shares from `SharePool`, unbond burns shares and returns proportional bond principal.
- Revenue allocation splits surplus across account and liquid pools pro-rata by bonded size; swap conversion is capped by `revenue_converter` limit.
- Optional fee handling is bounded (`0 < fee <= 1`) and charged from distributable revenue before user allocations.

## Notes

- Liquid staking receipt denom is `staking-<bond_denom>`.
- `fee` (if set) is taken from revenue conversions.
