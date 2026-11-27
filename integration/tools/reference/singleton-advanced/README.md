# Advanced singleton bounded reference and implementation

## V4 PoolManager hook and native settlement

The Cancun test deploys the pinned Uniswap V4 PoolManager from verified source. A controlled CREATE2 hook sets the exact beforeSwap, afterSwap and delta-return permission bits. The harness checks both amount modes, both directions and both currency modes against a matched PoolManager control pool. It reconciles caller, receiver, hook and manager balances, compares price and tick after the swap, and checks final lock state. Omitting part of the hook settlement triggers the original `CurrencyNotSettled` revert for each case.

Set `singleton_advanced.hook_model` to `v4-settlement-hook-v1` for this exact hook on a dynamic-fee V4 pool. The Go collector verifies the runtime bytecode and both immutable manager references at the same canonical block as the pool snapshot. Rust prices the fixed 17,000 ppm override and the 7-unit specified and 11-unit unspecified deltas. The pricing model assumes hook data encodes the boolean `false` as a 32-byte ABI word. Other swap-callback runtimes require a separate verified model.

The reference test also deploys `dex-router/evm/V4SingleSwap.sol`. The executor reconciles the caller's ERC-20 debit and the recipient's balance increase. Native ExactOut refunds unused input to the payer after the manager has locked again. The test compares all eight combinations with direct manager calls, checks the tight-limit reverts, and rejects callbacks from other accounts.

The same test records account and storage reads from the original contracts and replays the bound V4 quote on StrictDB using an H+1 Cancun environment. It compares actual payment, output, gas, nonce, native refunds and limit reverts in all eight combinations. Missing Manager slots and a changed hook runtime each reject all eight cases.

For a deployed executor, set `api.v4_execution.<chain-key>` with `executor`, `executor_code_sha256`, and `token_code_sha256` keyed by each supported ERC-20 address. Configure one RPC URL and the chain's genesis hash. `POST /v1/quotes/{id}/build` checks the canonical quote block, executor and manager code, executor manager immutables, and configured token runtimes. The result contains calldata, native transaction value, and the ERC-20 approval required when selling a token. Signing, gas and nonce preparation, and independent receipt verification remain separate.

```sh
python3 integration/tools/ci/singleton-reference-fetch.py
npm ci --ignore-scripts --prefix integration/.runtime/singleton-reference
node integration/tools/ci/v4-manager-settlement-build.cjs /tmp/xone-v4-manager-settlement.json
export XONE_V4_MANAGER_SETTLEMENT_BUILD=/tmp/xone-v4-manager-settlement.json
export XONE_V4_HOOK_QUOTES=/tmp/xone-v4-hook-quotes.json
cargo +1.90.0 test --manifest-path dex-router/Cargo.toml -p xone-simulation \
  --test v4_manager_settlement -- --ignored
cargo +1.90.0 test --manifest-path dex-router/Cargo.toml -p xone-quote-api \
  --test singleton_exact_out v4_settlement_hook_http -- --ignored
(cd chain-parser-go && go test ./internal/chain/evm -run TestV4SettlementHookRuntimeAtCanonicalBlock)
```

## Infinity Manager and Vault settlement

The same build command compiles the original Infinity CL Manager, Bin Manager and Vault from the pinned source. The Cancun test registers both applications in the Vault. It compares 16 swaps with control pools, using dynamic keys, a 17,000 ppm fee override and nonzero deltas in the specified and unspecified currencies. It verifies caller, recipient and hook balances, Vault holdings and per-manager reserves, as well as the final lock, delta and app deficit counters. For each case it omits the specified-currency hook payment and checks the original `CurrencyNotSettled` revert.

Set `XONE_INFINITY_HOOK_QUOTES` before running the simulation test to export results for cross-language and HTTP quote checks.

An Infinity CL pool may set `singleton_advanced.hook_model` to `infinity-settlement-hook-v1` for this exact dynamic-fee hook. The collector checks the permission bitmap, both sets of hook immutables, the Vault runtime and Manager registration at the pool snapshot's canonical block. HTTP ExactIn and ExactOut match all eight CL swaps in the replay. Set the two fixture paths above and run the HTTP test with the `settlement_hook_http` filter. For the collector checks, run the Go EVM tests with the `SettlementHookRuntimeAtCanonicalBlock` filter.

Infinity Bin uses `infinity_bin.hooks` with `infinity_bin.hook_model` set to `infinity-settlement-hook-v1`. The build uses the Bin Manager and Vault compiler settings pinned by the collector. Eight Bin swaps cross into a neighboring bin and reconcile the full reserve state. The Go collector reads a recorded ERC-20 and native poststate, including the original Manager getter responses. Its Protobuf output can be passed directly to the Rust HTTP tests using the same directory.

Configure `api.infinity_execution.<chain-key>.<manager>` with `vault`, `vault_code_sha256` and a `transaction` object containing `executor`, `executor_code_sha256` and `token_code_sha256`. Register the Manager with its Vault before deploying `dex-router/evm/InfinitySingleSwap.sol` with the Manager, Vault and CL/Bin selector. The build endpoint checks the deployed executor's immutable values and runtime, reads the Manager's Vault and registration, and checks the token runtimes at the quote block. It returns calldata, native transaction value and the approval required for ERC-20 input.

The Cancun replay checks all 16 direct Manager swaps against the executor, including ERC-20 debit and allowance consumption, recipient balance increase and native ExactOut refunds. It verifies 16 tight-limit rollbacks and rejects direct calls to each executor's lock callback. The HTTP tests exercise the corresponding transaction templates and reject executor code changes. The build response reports the independent execution and production readiness flags as false.

The StrictDB test also binds and replays all 16 Infinity CL/Bin cases from an initialized prestate. Each replay checks observed input and output, gas and sender nonce. The same matrix checks native ExactOut refunds, limit reverts, omitted Manager slots and changed hook code.

```sh
export XONE_INFINITY_HOOK_QUOTES=/tmp/xone-infinity-hook-quotes.json
export XONE_INFINITY_BIN_WIRE_DIR=$(mktemp -d)
cargo +1.90.0 test --manifest-path dex-router/Cargo.toml -p xone-simulation \
  --test v4_manager_settlement -- --ignored
(cd chain-parser-go && go test ./internal/chain/evm -run SettlementHookRuntimeAtCanonicalBlock)
cargo +1.90.0 test --manifest-path dex-router/Cargo.toml -p xone-quote-api \
  --test singleton_exact_out settlement_hook_http -- --ignored
```

## Bounded library reference

This slice is explicitly separate from legacy singleton admission. The core protocol names inside `SingletonAdvancedState` remain `uniswap-v4` and `pancake-infinity-cl`; central Pool profiles may use their `-advanced` names. Shared registry/config/wire dispatch is owned by main.

## Independent authority

The generator executes the unchanged original libraries at these pinned revisions:

* Uniswap/v4-core `bea7f6d1c11cd5dc4f0d5151ffa37a1c239feee6`
* pancakeswap/infinity-core `3c5ef1be1f71442d8bc60cc6fd11f9c677b714df`

`source-metadata.json` records the SHA256 of every imported original source, all local harnesses, compiler input, and frozen fixtures. The full upstream archive manifest remains in `../singleton-cl/source-metadata.json`. No source from the services is imported into the generator. Solc 0.8.26, Ganache 7.9.2 and ethers 5.8.0 are reused from `integration/.runtime/singleton-reference`. Shanghai is the executed target; this is library/core reference execution, not a full Cancun manager settlement VM.

Run from the repository root:

```
node integration/tools/ci/singleton-advanced-golden.cjs
node integration/tools/ci/singleton-advanced-check.cjs
```

240 fresh CL cases cover static versus exact dynamic key flags, zero/nonzero live fees, native/ERC20 key currency0, both directions, 100% fees and zero outputs, initialized tick crossing, amount/core post-state, combined fees and per-step protocol allocation. Eight negative capabilities check all swap callback/return-delta flags. Original `PoolIdLibrary` computes keys. Original `Hooks.beforeSwap/afterSwap` and `CLHooks.beforeSwap/afterSwap` execute accepted dispatch; the installed test hook has a real Infinity registration getter and reverts on every callback. Infinity's original `validateHookConfig` also executes. Accepted fixtures have no swap callback permissions; no arbitrary hook behavior is inferred from them.

180 separate original `BinPool.swap` cases cover one active bin, positive/negative price exponents, bin steps 1/25, directional protocol rates, LP fees 0/3000/100000, fee rounding, reserve post-state, zero-output and exhaustion reverts. `PriceHelper`/`Uint128x128Math.pow` are ported independently; Bin fee allocation uses `totalFee * protocolFee / swapFee`, unlike CL. Inputs spilling into another bin fail with `INFINITY_BIN_NEXT_STATE_REQUIRED`; no tree completeness claim is made.

## APIs and integration requirements

Rust domain: `singleton::SingletonAdvancedState::{key_id,validate,to_proto,from_proto}` in both service copies. Fields: base `protocol`, `currency0`, `currency1`, immutable `key_fee`, nonzero `snapshot_block_hash`, and `snapshot: SingletonCLState`. Dynamic key fee must be exactly `0x800000`; live `snapshot.lp_fee` ranges 0..1000000. Static key/live fees must match. No legacy validation is relaxed.

Rust math: `singleton::quote_advanced_exact_in` returns amount, consumed/remaining input, combined swap fee, per-step protocol amount, and a typed `next_state`. It preserves immutable PoolKey and snapshot identity while updating core state. The current primitive requires full input consumption within captured bitmap state. Zero output is represented faithfully for 100% fee/tiny input; central route admission must decide whether to admit such an edge.

Go: `Adapter.ReadSingletonAdvanced(ctx, SingletonAdvancedReadConfig)` reuses original manager code identity, direct runtime/proxy rejection, slot0 width, canonical block-hash RPC anchoring, bitmap completeness and tick budgets. Infinity compares its full returned PoolKey and reads `getHooksRegistrationBitmap()` at the same anchor. All key/hook/capability checks happen before collection. Returned state receives live LP/protocol fees and core; a dynamic request may supply LP/protocol placeholders of zero. Static requests supply their key fee as LP fee.

Collection preconditions: State base protocol/currencies/key fee/block hash, snapshot manager/code digest/pool ID/hooks/parameters and `core.tick_spacing`; `TickWordRadius <=16`, `MaxInitializedTicks 1..4096`, `MaxSteps 1..8192`. The core price/liquidity/bitmap/ticks in the request are not trusted as collected state. Central config must create the correct immutable key and snapshot identity. Advanced protobuf is a separate message because legacy `SingletonCLState` fields 10..19 are reserved. The new message wraps the existing snapshot shape but invokes advanced capability validation rather than legacy `validate_wire`.

`singleton::infinity_bin` exports `InfinityBinState`, `price_from_id`, `quote_exact_in`. It is a core math primitive and has no Pool/profile/manager/wire adapter admission. Its caller must separately bind manager/key/hook/block and authenticate bin reserves. Do not expose it centrally as a collected protocol until those dependencies exist.

## Explicit boundaries

Native currency support proves key hashing, ordering, snapshot collection and quotes denominated in zero-address currency0. It does not execute native transfers, unlock/settle/take, Vault accounting or transaction construction. Hook addresses and manager runtime hashes in fixture snapshots identify the actual deployed reference harness, not deployed production managers.

Swap callbacks, custom accounting/return deltas, per-swap fee overrides, sender-dependent behavior and arbitrary hook programs require independent execution of the actual callback in a capable VM. These are rejected even if a caller supplies no-op metadata. The dynamic snapshot capability is safe only when swap callback permissions are absent; an external fee update creates a new anchored snapshot. Hook code registration/config must be attested at that anchor (Go performs the Infinity getter check; externally supplied snapshots must carry equivalent authority).

Infinity Bin tree traversal, multi-bin collection, native settlement, manager execution and arbitrary hook VM support remain hard dependencies. Main owns versioned central admission, protobuf tags, config and dispatch. This slice edits none of those shared files.
