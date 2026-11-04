# Advanced singleton bounded reference and implementation

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
