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