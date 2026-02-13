# Rujira Account

A minimal account contract used as an isolated execution container.
It does not expose user-facing execute/query logic and only forwards CosmosMsg via sudo.
Higher-level contracts (for example, rujira-ghost-credit) use it to hold collateral and execute actions safely.

## Execute / Query

There are no user-executable messages.

## Invariants

- **Authorization:** which actors are allowed to execute privileged flows.
- **Accounting:** balances/shares/debt conservation rules before and after each state transition.
- **Safety bounds:** caps, thresholds, and ratio constraints that can never be exceeded.
- **External calls:** callback/execute side effects that must preserve contract safety.

## Notes

- The admin is set at instantiation and is the only entity that can sudo.
- This contract is intended to be instantiated via `instantiate2` for predictable addresses.
