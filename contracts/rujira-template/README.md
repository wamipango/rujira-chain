# Rujira Template

Scaffold contract with empty messages.
Useful as a starting point for new modules.
Provides no-op instantiate/execute/query handlers for wiring.

## Execute

No-op; messages are empty and return an empty response.

## Query

No-op; messages are empty and return an empty response.

## Invariants

- Instantiate stores validated config and version metadata; runtime message handlers are intentionally no-op.
- `execute` and `sudo` return empty responses and do not move funds or mutate business state.
- `query` always returns an empty payload, so there is no state-derived read surface yet.
- This contract is a scaffold; real invariants only exist after message types and state transitions are implemented.

## Notes

- Use this contract as a scaffold to define real message types.
- Update the README after you add concrete execute/query variants.
