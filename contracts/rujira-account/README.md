# Rujira Account

A minimal account contract used as an isolated execution container.
It does not expose user-facing execute/query logic and only forwards CosmosMsg via sudo.
Higher-level contracts (for example, rujira-ghost-credit) use it to hold collateral and execute actions safely.

## Execute / Query

There are no user-executable messages.

## Invariants

- `execute` and `query` always return `Unauthorized`, so users cannot invoke arbitrary account logic directly.
- `sudo` only forwards a supplied `CosmosMsg`; it does not add business logic or mutate internal accounting state.
- The contract stores no mutable balances/debt state; value custody is only whatever tokens sit on the contract address.
- Safety depends on the parent controller contract (for example ghost-credit) that is configured as admin/governance caller.

## Notes

- The admin is set at instantiation and is the only entity that can sudo.
- This contract is intended to be instantiated via `instantiate2` for predictable addresses.
