# APP LAYER

Rujira is the THORChain App Layer, built for CosmWasm smart contracts that compose with THORChain’s secured assets and app primitives.
Contracts run inside a permissioned, sandboxed environment that preserves base-layer safety and allows governance to pause apps if needed.
Deployments are curated: projects are expected to be open source and go through review/audit and whitelisting before mainnet.

## How to Build on the App Layer

- Use CosmWasm (Rust) and standard message types (Instantiate/Execute/Query).
- Keep your repository public and align early with the Rujira team to avoid duplication.
- Design around Secured Assets and existing primitives so your app composes cleanly.
- Plan for a staged rollout: local testing, stagenet validation, internal/external review, then mainnet approval.

## Development Process (High Level)

1. Align with the Rujira team early to avoid duplicates and fit the roadmap.
2. Build and test locally and on test environments (including stagenet).
3. Undergo review and external audit, then request mainnet whitelisting.

## App Layer Concepts

- **Secured Assets**: L1 assets are deposited to THORChain and minted 1:1 as App Layer bank tokens. These are the primary assets used by apps.
- **Sandboxed execution**: Contracts are constrained to reduce base-layer risk and are limited to bank-style operations (e.g., MsgSend/MsgDeposit).
- **Operational controls**: THORChain Node Operators can pause the app layer or specific contracts if issues are discovered.
- **Whitelisting**: Mainnet deployments require a whitelisted deployer address and approved contract bytecode.
  - Typical controls include global halts or checksum/contract-specific halts via Mimir parameters.

## Development Process (Detailed)

1. **Before you start**
   - Confirm your project fits the ecosystem and doesn’t duplicate existing plans.
   - Contact the Rujira team before starting to align on scope.
   - Review this repo’s `README.md` and `RELEASES.md` for precedent and expectations.
   - Ensure your repository is public and ready for review.

2. **Write the contract**
   - Use CosmWasm message structure (Instantiate/Execute/Query).
   - Use `#[cw_serde]` for serialization and keep message payloads compact (Rujira may use a more compact encoding for ExecuteMsg).
   - Organize code into modules with clear state and message types.

3. **Test locally**
   - Unit tests for core logic (`cargo test`).
   - Integration tests with `cw-multi-test` (`cargo test --features multitest`).

4. **Test deployment**
   - Use a permissionless test environment to validate end-to-end flows:
     - Bloctopus (forkable THORChain testnets)
     - Localnet/Mocknet (local THORChain environment with Midgard/THORNode and mocked L1s)
   - Deploy to stagenet for a real environment before mainnet review (stagenet is permissionless but uses real value).
   - Use Developer Endpoints to connect to the right network and a stagenet explorer to verify transactions.

5. **Review & audit**
   - Internal review by the Rujira team.
   - External audit by an independent auditor.
   - Follow the repository templates for README and Cargo.toml where applicable.

6. **Mainnet readiness**
   - Prepare a THORChain multisig deployer address.
   - Submit artifacts and request whitelisting for mainnet deployment by THORChain node operators.

## References

- Development process: https://docs.rujira.network/developers/development-process
- App layer overview: https://docs.rujira.network

## Contribution Process

### 1. Create a new contract

For a quick-start, use [cw-template](https://github.com/CosmWasm/cw-template)

```bash
git checkout -b new_contract
./scripts/create.sh NAME
git add .
git commit -m "New Contract"
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
1. Bump the relevant contract versions and create a new RC entry in RELEASES.md. Observe semver.
   - Major bump if contract interfaces are changed in a non backward-compatible way (eg fields are added without a `#[serde(default)]` decorator)
   - Minor if interfaces or functionality is changed and is backward compatible
   - Patch for bugfixes
1. Include entries for all contract artifacts, even if unchanged from the previous release
1. Commit and create a tag with the current workspace.version

### Audit

1. Make code changes
1. Re-compile and commit as per 2, bumping the RC tag
1. Add links to the audit files in RELEASES.md

### STAGENET

Apply to have it deployed on stagenet. Provide:

1. The version tag
1. The deployer address

### Deploy

1. Deploy and link your app, conduct thorough testing
1. Any modifications to contracts must be re-audited as above, increasing the RC version

### Mainnet

1. Create a release in RELEASES.md without the RC tag
1. Apply for an ADR to launch on Mainnet

## Invariants

- **Authorization:** which actors are allowed to execute privileged flows.
- **Accounting:** balances/shares/debt conservation rules before and after each state transition.
- **Safety bounds:** caps, thresholds, and ratio constraints that can never be exceeded.
- **External calls:** callback/execute side effects that must preserve contract safety.

