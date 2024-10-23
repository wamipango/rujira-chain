# Streaming Payments for Rujira

Streaming payment contracts with a "gated withdrawal" mechanism. Instead of progressively sending payments, funds are deposited to a contract address and progressively made available over time. If a contract wishes to progressively consume the streamed funds, the streaming contract should be "cranked" before the subsequent action on the receiving contract.

E.g. if rujira-stream is used to manage the flow of revenue to rujira-staking, then prior to any contract call on the rujira-staking contract, rujira-stream should be called with `Claim {}`

Streaming contracts operate a factory architecture, where the controller contract instantiates a new contract for each streaming payment. This silos funds and allows streaing payments to be created that require "topping up" (eg salaries). See `src/factory/contract.rs` for implementation. This will likely be moved into rujira-rs in a future version as a generic utility.

## Adjustments (WIP)

Streaming payment contracts can be managed via rujira-authz; the generic user-land authorization framework. This can be used to Cancel, Pause, or Adjust a payment

## Permissions (WIP)

rujira-authz can be used to control the use of funds present on streaming payment contracts, via gating generic CosmosMsg variants.

E.g. a streaming contract used for vesting RUNE could permit its recipient to bond the contrat's RUNE balance, allowing a vesting balance to secure the network and earn staking rewards
