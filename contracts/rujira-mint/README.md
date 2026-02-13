# Rujira Mint

One-shot token factory helper for creating a new denom.
On instantiate it creates the token factory denom and mints the initial supply.
There are no execute/query/sudo entrypoints beyond this initial mint used for $RUJI TGE.

## Execute / Query

Not available.

## Invariants

- **Authorization:** which actors are allowed to execute privileged flows.
- **Accounting:** balances/shares/debt conservation rules before and after each state transition.
- **Safety bounds:** caps, thresholds, and ratio constraints that can never be exceeded.
- **External calls:** callback/execute side effects that must preserve contract safety.

## Notes

- The created denom is `x/<id>` via token factory.
- The full `amount` is minted to the instantiator address.
