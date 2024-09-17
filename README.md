# APP LAYER

## Contribution Process

### 1. Create a new contract

For a quick-start, use [cw-template](https://github.com/CosmWasm/cw-template)

```bash
cd contracts
cargo generate --git https://github.com/CosmWasm/cw-template.git --name PROJECT_NAME -d minimal=true
cd PROJECT_NAME
rm -rf .git
rm .gitignore
git add .
```

<!--
Now, integrate it into the CI and build system

1. Edit `.circleci/config.yml`, copy an existing contracts job and replace the name.
Then add your new job to the jobs list on top. (eg. copy `contracts_cw1_whitelist`
to `contracts_cw721_base` and then replace the 3 instances of `cw1-whitelist` in
that job description with `cw721-base`. And don't forget to add `contracts_cw721_base`
to `workflows.test.jobs`)

1. Add to the `ALL_CONTRACTS` variable in `scripts/publish.sh`

1. Set the `version` variable in `Cargo.toml` to the same version as `packages/cw20`.
For example, "0.5.0" rather than the default "0.1.0"

1. Edit the root `Cargo.toml` file and add a `profile.release.package.CONTRACT_NAME`
section, just like `profile.release.package.cw1-subkeys`, but with your
package name. -->

1. Run `cargo build && cargo test && cargo coverage` in the new contract dir

1. Commit all changes and push the branch. Open a PR and ensure the CI runs this.

### 2. Compile & Commit

1. Compile all contracts with scripts/optimize.sh, forcing x86_64 architecture
1. Bump the workspace.version and create a new entry in RELEASES.md
1. Include checksums for all contract artifacts, even if unchanged from the previous release
1. Commit and create a tag with the current workspace.version

### Audit

// process

### STAGENET

Apply to have it deployed on stagenet. Provide:

1. The checksum(s) of the modified contract(s)
1. The link to the audit and code, with tag
1. The deployer address

### Deploy

Deploy and link your app, conduct thorough testing

### Mainnet

Apply for an ADR to launch on Mainnet.
