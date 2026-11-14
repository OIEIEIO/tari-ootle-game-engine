<!-- Ootle Asset Game | templates/treasury_controller/README.md -->
# Ootle Treasury Controller — on-chain candidate

This is **not part of the native Axum Cargo package**. It has a separate `Cargo.toml` and `cdylib` target. Build with `./scripts/build-wasm.sh` from the root. This source targets `tari_template_lib 0.31`; verify that it is compatible with the locally checked-out Ootle validator before deploying. The project was **not compiled or executed** by the packaging environment.

## What the template does

`new(house, community, chance, resource)` binds three **existing account component addresses** and one resource (tTARI) to a new on-chain controller instance. `distribute(category, revenue_bucket, amount_in_base_units)` accepts a pre-funded Bucket, checks its amount and resource, divides the funds using integer basis points, then invokes the three destination account `deposit` methods in one transaction. This component holds no funds itself. Any failed transfer should abort the transaction rather than partially settle.

The source account must explicitly authorize and supply that Bucket as an earlier transaction instruction. `distribute` cannot withdraw from House or players unilaterally. Player-to-player payments continue independently via the existing wallet API.

`category` is 0=asset sale, 1=game fee, 2=sponsorship. Initial bps: 5/5 for first two, 15/5 for sponsorship. **Rates are fixed in this first contract.** Dynamic governance/configuration is the next milestone; changing `config/economy.toml` does NOT change an on-chain instance.

## Deployment sequence (operator controlled)

1. `cargo build` and `cargo test` in main project.
2. `./scripts/build-wasm.sh` from main project root. Confirm compilation on the host's toolchain.
3. Publish the produced WASM to **your local Swarm**, using the official Tari CLI (`tari template publish ...`) or the wallet `transactions.publish_template` API. Do **not** assume publishing can happen through the wallet's broken web frontend. Verify appropriate fees with a dry run before broadcast.
4. Create a controller instance on-chain using `new` and the exact 3 treasury component addresses and tTARI resource address in `config/accounts.toml`.
5. Test 1 tTARI with a signed, atomic transaction (withdraw 1 tTARI to workspace; call `distribute(0, bucket, 1000000)`). Query three accounts afterward and compare to fees and split.
6. Only after successful execution, enter the published template and component addresses in `config/deployment.toml` for local reporting.

**Not implemented in the dashboard:** publish, instantiate, live call, signing, blockchain write, contract event index. The dashboard's contract status is only a local file/build indicator, never an on-chain verification claim. Do not connect this build to real-value production wallets.

<!-- End of templates/treasury_controller/README.md -->
