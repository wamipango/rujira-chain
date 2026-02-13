# Rujira Demo

A pass-through contract, designed to demonstrate the bindings between the App Layer and Base Layer.
It forwards arbitrary CosmosMsg via execute/sudo without additional checks.
It also exposes a simple pool query for inspecting THORChain pool state.

## Invariants

- **Authorization:** which actors are allowed to execute privileged flows.
- **Accounting:** balances/shares/debt conservation rules before and after each state transition.
- **Safety bounds:** caps, thresholds, and ratio constraints that can never be exceeded.
- **External calls:** callback/execute side effects that must preserve contract safety.

