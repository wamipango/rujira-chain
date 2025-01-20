# Streaming Payments for Rujira

Streaming payment contracts with a "gated withdrawal" mechanism. Instead of progressively sending payments, funds are deposited to a contract address and progressively made available over time. If a contract wishes to progressively consume the streamed funds, the streaming contract should be "cranked" before the subsequent action on the receiving contract.

E.g. if rujira-stream is used to manage the flow of revenue to rujira-staking, then prior to any contract call on the rujira-staking contract, rujira-stream should be called with `Claim {}`

Streaming contracts operate a factory architecture, where the controller contract instantiates a new contract for each streaming payment. This silos funds and allows streaing payments to be created that require "topping up" (eg salaries). See `src/factory/contract.rs` for implementation. This will likely be moved into rujira-rs in a future version as a generic utility.
