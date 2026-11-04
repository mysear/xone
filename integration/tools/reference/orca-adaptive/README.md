# Orca adaptive fee kernel reference

This sidecar implements the adaptive branch of Orca Whirlpools
`cc683ec92acb5e7606caa3d8e514d9383e9c7c11` in the new protocol-math
`orca_adaptive.rs`. It reuses `xone_domain::orca_math` for integer Q64 prices and
inverse ticks. The existing static-fee swap implementation is not an adaptive
quote and is not called here.

## API and integration order

`AdaptiveFeeConstants`, `AdaptiveFeeVariables`, and `AdaptiveFeeInfo` retain the
original u16/u32/i32/u64 widths, including both reserved byte arrays. Serde rejects
unknown fields and out-of-width numbers. These are mathematical state types, not
an Anchor account layout or a wire/protobuf schema.

`FeeRateManager::begin(context, info)` returns a validated manager. The context
contains `a_to_b`, `current_tick_index`, `timestamp`, `static_fee_rate`,
`tick_spacing`, and `trade_enable_timestamp`. It enforces the original constants
predicates, base-fee maximum, timestamp/trade gate, bounded state, and reachable
ticks, including the original shifted minimum tick `-443637`. State bounds are
checked before reference updates; malformed decoded accounts must not be repaired
by a later reset. Reserved bytes are preserved.

For each actual swap:

1. Call `begin` once. This updates the reference but leaves the old volatility
   accumulator intact, just as the original constructor does.
2. Before **every** step call `update_volatility_accumulator()` and read
   `get_total_fee_rate()` (hundredths of a basis point; hard cap 100,000).
3. Pass the next-tick/price-limit target and current liquidity to
   `get_bounded_sqrt_price_target(target, liquidity) -> Result<(u128, bool)>`.
   Feed that bounded target and the current group's fee into the existing Q64
   step computation. Recompute these values after every group.
4. For a non-skip step, call `advance_tick_group()`. For a skip step, call
   `advance_tick_group_after_skip(actual_step_end_price, next_tick_price,
   next_tick_index)`. The cached next-tick price must match the integer tick map.
   The actual end price can be a partial movement or the bounded core edge.
5. At the end of the complete swap, call
   `update_major_swap_timestamp(whole_swap_pre_price, whole_swap_post_price)`.
   It uses the `begin` timestamp and the original Q64 multiplication/threshold
   rounding. Consume `finish()` to obtain the next `AdaptiveFeeInfo`.

All mutating manager methods except `finish` return `Result<()>`.
`get_next_adaptive_fee_info()` copies the current state without consuming it.
`tick_group_index()` and `core_tick_group_range()` expose debugging/verification
facts. `finish` does not invent a last-trade timestamp: original adaptive state
contains last-reference and last-major timestamps. Rewards, pool updates and
account persistence remain the swap owner's responsibility.

The raw variables methods `update_reference` and
`update_volatility_accumulator` mirror the native primitives. Use the validated
manager boundary for decoded external state. The kernel's additional range/error
checks reject malformed inputs that the original internal methods assume cannot
occur. The static `None` branch remains with the existing static implementation.

## Original native oracle and provenance

`Cargo.toml` depends directly on the existing, unchanged upstream `whirlpool`
crate with `no-entrypoint`. `src/main.rs` calls its original
`AdaptiveFeeConstants::validate_constants`, `FeeRateManager` and, through that
manager, `AdaptiveFeeVariables` methods. It never imports the Rust port.
Requests and responses use one JSON object per line; u128 prices/liquidity are
strings. Every operation records the fee, group index, core bounds, and complete
next state. `prices` also calls the original tick math.

The trade-enable check is a harness admission guard matching the OracleAccessor
comparison; it does not execute an Anchor account accessor. The negative decoded
state/width tests run against the standalone API and are not native transaction
observations.

At build time and process startup, `identity.rs` verifies all **1,111** original
files against the existing source inventory, the exact revision/archive pin,
metadata digest, and an empty source-adaptations list. It records critical source
SHA-256 values and an ordered source-closure digest. The harness's Cargo manifest,
Cargo.lock, build script, identity verifier and main source are hashed into the
compiled closure. Source/harness/lock drift fails startup before any output.
The build requires Rust 1.90.0 and records its full identity. Each process hashes
its own executed binary. The corpus includes that identity and all **282** locked
Cargo package entries, with source/checksum/dependency facts where present.

No original source, original native harness, existing crate manifest, module
export, parser, domain/protobuf file or online profile is modified by this sidecar.

## Verification

Use the actual pinned compiler binaries first on PATH; `rustup run` alone can
still select an older `rustc` from this machine's `/usr/local/bin`.

```sh
export PATH="$HOME/.rustup/toolchains/1.90.0-x86_64-apple-darwin/bin:$PATH"
cargo build --locked --offline \
  --manifest-path integration/tools/reference/orca-adaptive/Cargo.toml \
  --target-dir /tmp/xone-orca-adaptive-native
python3 integration/tools/ci/orca-adaptive-golden.py --check
cargo test --locked --offline --manifest-path dex-router/Cargo.toml \
  -p xone-protocol-math --test orca_adaptive \
  --target-dir /tmp/xone-orca-adaptive-router
```

`python3 integration/tools/ci/orca-adaptive-check.py --startup-guard-test` runs
these together and additionally proves a modified sidecar lock is rejected before
output, then restores its exact original bytes. This probe modifies only the NEW
reference's own lock. `--standalone` instead compiles the same assigned test file
and production kernel with the same existing Q64 source plus a minimal test-only
error boundary. It is useful during concurrent edits to the full domain crate;
it is not the oracle or a substitute for the primary crate test.

To deliberately regenerate native evidence after a reviewed harness change, run
`python3 integration/tools/ci/orca-adaptive-golden.py` without `--check`. This writes
only the new integration corpus and byte-identical router copy. Check mode
requires the stored source/dependency/compiler closure, full dependency list,
and every observation to match the currently executed native oracle exactly.
It retains the original generation binary hash and reports the replay binary
hash separately: a native relink can change executable bytes without changing
the verified compiled closure. It does not silently regenerate evidence.

The assigned Rust suite has **7 tests**, including **197 native cases**, **2
sequences / 14 carried-state trades**, and **1,215 state snapshots**. Coverage
includes filter/decay/forced-reset boundaries, recent-major locking, reduction
rounding, invalid timestamps, u64 maximum timestamp, constants and integer-width
limits, signed negative groups, shifted minimum tick, Q64 endpoints, fee caps,
zero liquidity/control, saturated-range skips inward/outward, boundary/partial
skip endings, no-motion rightward skip protection, whole-swap major thresholds,
both directions and same-block trades. The shifted-minimum regression was observed
failing with `ORCA_ADAPTIVE_INVALID_CONTEXT` before the state-envelope fix.

## Explicit remaining boundary

This is the complete adaptive fee/state kernel with native primitive evidence.
It does **not** implement an adaptive swap API, amount-in/output integration,
initialized-Oracle parsing, account provenance, service dispatch, or transaction
execution. The owner must export the module and implement/verify the original
per-group swap loop and persistent account state before enabling an adaptive
profile. Existing initialized-Oracle parser rejection and online profile closure
must remain until then. There is no claim of SBF, Token-2022, token-transfer,
Anchor-instruction or deployed-transaction equivalence.
