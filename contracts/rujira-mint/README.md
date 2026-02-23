# Rujira Mint

One-shot token factory helper for creating a new denom.
On instantiate it creates the token factory denom and mints the initial supply.
There are no execute/query/sudo entrypoints beyond this initial mint used for $RUJI TGE.

## Execute / Query

Not available.

## Invariants

- On instantiate, the contract creates exactly one token-factory denom (`x/<id>`) and mints the declared initial amount to the instantiator.
- There are no post-deployment control paths: `execute`, `sudo`, and `query` all return `Unauthorized`.
- The contract stores no mutable supply/accounting state after instantiation; token supply changes are not possible through this module.
- Safety is one-shot by design: all mint authority usage happens in the instantiate transaction.

## Notes

- The created denom is `x/<id>` via token factory.
- The full `amount` is minted to the instantiator address.
