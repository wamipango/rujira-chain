# Streaming Payments for Rujira

Streaming payment contracts with a "gated withdrawal" mechanism. Instead of progressively sending payments, funds are deposited to a contract address and progressively made available over time.

Streaming contracts operate a factory architecture, where the controller contract instantiates a new contract for each streaming payment. This silos funds and allows streaing payments to be created that require "topping up" (eg salaries)

## Adjustments

Streaming payment contracts can be managed via rujira-authz; the generic user-land authorization framework. This can be used to Cancel, Pause, or Adjust a payment
