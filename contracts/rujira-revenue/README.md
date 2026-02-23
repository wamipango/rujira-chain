# Rujira Revenue

Revenue routing contract with a scheduled executor. Optionally swaps incoming tokens into target denoms. Distributes balances to weighted target addresses after each run.

One contract instance is deployed per revenue token.

Each instance has a set of Actions that are stepped through on subsequent executions of ExecuteMsg::Run. This is designed to keep execution of the contract in fixed time, and also support more complex routing of token swaps. At the end of each execution, target_denoms balances are read and is distributed to the target_addresses address according to weights.

## Execute

Run a cycle (executor-only):
```json
{ "run": {} }
```

## Invariants

- `ExecuteMsg::Run` is executor-gated; non-executor callers are rejected.
- Each run executes at most one configured action (bounded by per-action `limit`), then distribution executes in reply/fallback path.
- Action execution is denom-consistent (`Action::execute` verifies queried coin denom matches action denom).
- Distribution fans out each target denom by configured address weights, with final-recipient remainder handling to avoid stranded dust.
- Action iteration is stateful (`LAST` cursor) and cycles deterministically over configured actions.
- Target-denom additions are deduplicated, preventing repeated distribution loops for the same denom.

## Notes

- `run` will execute at most one action per call, then distribute balances to targets.
- `target_addresses` are weighted by the u8 weight.
