# BRUNE

BRUNE is a restaking token allowing permissionless access to RUNE's underlying revenue model.

The rujira-brune contract acts as a market maker on the RUNE/BRUNE rujira-fin pair, where it asks for 1 RUNE per BRUNE, and bids at a dynamic spread based on the contract's liquid RUNE reserves.

Each churn, the contract will calculate the increase in RUNE denominated value through bonding, and add it to a queue that is streamed to the BRUNE staking contract, where yield can be earned by BRUNE holders.

## Entry, Exit

To acquire BRUNE, the contract will place a single ask in the orderbook at a price of 1.0, up to the capacity of its whitelisted nodes, where max-effective is the ceiling.

To sell BRUNE, the contract will place a range of bids in the orderbook, along an XYK curve where the "balance" of BRUNE is taken as the target liquid amount, ie when liquidity == target liquidity, bid price == 1.0.

## Bond Allocation

The contract will target a config.target_utilization ratio of the deposited RUNE as yield-earning bond. The total balance is distributed evenly across whitelisted nodes, proportional to (1 - min(node.comission, config.min_node_commission)) in order to optimize yield from each node, whilst not favouring 0% comission nodes

## Revenue Distribution

After a churn, when the yeild is allocated to the contract's bonded balances,

## Factory
