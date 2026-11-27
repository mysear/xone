# Composable Stable V6 actual-supply reference

`supply.cjs` deploys the published factory, Vault and fee contracts from the Balancer deployments archive at `b55dc2b8a46bce8f27aed5a3b03f719f662f860a`. It checks the build-info SHA-256 hashes and verifies the factory's stored pool creation bytecode. The fixture records fixed-block Vault and pool getters from the local chain. A mock provider supplies controlled rate changes.

The reference executes consecutive BPT joins and exits in both swap modes while amplification is changing. It records actual transfers, registered balances, both protocol fee mints and the updated join/exit baseline. It also checks that a failed limit check rolls back the balances, supply and fee recipient state.

Regular swaps before and after rate expiry record the cache entries and effective scaling factors. Failed swaps verify that automatic cache refreshes roll back with the Vault transaction.

Use the build-info JSON files from these directories and the dependencies from `integration/.runtime/stable-reference`.

| File | Directory in the archive |
| --- | --- |
| `ComposableStablePoolFactory.json` | `v2/deprecated/20240223-composable-stable-pool-v6/build-info` |
| `Vault.json` | `v2/tasks/20210418-vault/build-info` |
| `Authorizer.json` | `v2/tasks/20210418-authorizer/build-info` |
| `ProtocolFeePercentagesProvider.json` | `v2/tasks/20220725-protocol-fee-percentages-provider/build-info` |

```sh
node integration/tools/reference/balancer-composable-v6/supply.cjs \
  /path/to/build-info integration/.runtime/stable-reference \
  dex-router/crates/protocol-math/tests/fixtures/composable-v6-supply.json
cargo test --manifest-path dex-router/Cargo.toml -p xone-protocol-math --test composable_supply
```

Pass a fourth argument to record the fixed-block source for the no-provider pool. Compress that file with `gzip -n` to `chain-parser-go/test/contracts/composable-v6-wire-source.json.gz`. The Go test `TestComposableV6OriginalVaultSnapshotWire` replays its RPC responses and writes the Go snapshots when `XONE_COMPOSABLE_V6_WIRE_OUTPUT` points to an absolute fixture directory. The `composable_v6_wire` Quote API test installs these snapshots in a replica, checks four consecutive swap post-states and requests both ExactOut directions over HTTP.
