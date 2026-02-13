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

- **Authorization:** which actors are allowed to execute privileged flows.
- **Accounting:** balances/shares/debt conservation rules before and after each state transition.
- **Safety bounds:** caps, thresholds, and ratio constraints that can never be exceeded.
- **External calls:** callback/execute side effects that must preserve contract safety.

## Notes

- `run` will execute at most one action per call, then distribute balances to targets.
- `target_addresses` are weighted by the u8 weight.
