# Rujira Template

Scaffold contract with empty messages.
Useful as a starting point for new modules.
Provides no-op instantiate/execute/query handlers for wiring.

## Execute

No-op; messages are empty and return an empty response.

## Query

No-op; messages are empty and return an empty response.

## Invariants

- **Authorization:** which actors are allowed to execute privileged flows.
- **Accounting:** balances/shares/debt conservation rules before and after each state transition.
- **Safety bounds:** caps, thresholds, and ratio constraints that can never be exceeded.
- **External calls:** callback/execute side effects that must preserve contract safety.

## Notes

- Use this contract as a scaffold to define real message types.
- Update the README after you add concrete execute/query variants.
