# Rujira Demo

A pass-through contract, designed to demonstrate the bindings between the App Layer and Base Layer.
It forwards arbitrary CosmosMsg via execute/sudo without additional checks.
It also exposes a simple pool query for inspecting THORChain pool state.

## Invariants

- `execute` and `sudo` are intentional pass-throughs that forward arbitrary `CosmosMsg` without extra authorization checks.
- The contract keeps no business state beyond contract version metadata, so it has no internal accounting invariants to preserve.
- `query` exposes only pool lookup (`QueryMsg::Pool`) and does not mutate state.
- This module is a wiring/demo surface and should not be treated as a policy-enforcing vault/account contract.
