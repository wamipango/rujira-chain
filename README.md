# APP LAYER

## Getting started

//

## App Layer Process

### Create

Create new WASM contract code

### Compile

<!-- TODO -->

Run `./scripts/lock.sh`, this will:

1. Compile all contracts with scripts/optimizer.sh, forcing x86_64 architecture
1. Append the workspace.version to RELEASES.md, along with checksums for all contracts
1. Commit and create a tag with the current workspace.version

### Audit

// process

### STAGENET

Apply to have it deployed on stagenet. Provide:

1. The checksum
2. The link to the audit and code
3. The deployer address

### Deploy

Deploy and link your app, conduct thorough testing

### Mainnet

Apply for an ADR to launch on Mainnet.
