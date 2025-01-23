# Ghost (lending) registry

Ghost operated on a hub-spoke architecture to achieve the following:

1. Centralised risk management. The registry has an overview of all contracts, stores their individual config settings, and therefore can dynamically set borrow limits that apply to all contracts at the same time
1. Trustless debt transfer. Debt drawn from from ghost-vault is tokenised and can be moved with a regular send, as long as the destination has also been approved as a borrower on the vault. The registry contract guarantees that all borrow contracts deployed by itself will have the same function and be registered with the correct lending vaults
1. Isolation of funds. Each lending contract supports a single debt token and single collateral token. Distribution of concentration of collateral reduces the risk in the protocol architecture
