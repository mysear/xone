# Bounded Orca adaptive exact-input swap

The independent `orca_adaptive_swap.rs` composes the pinned adaptive fee kernel
with the existing Orca Q64 price/amount step. It implements the complete original
exact-in mathematical swap loop for classic SPL, **uninitialized rewards only**,
and one to three complete, contiguous fixed tick arrays. It does not enable an
online profile or authenticate/decode chain accounts.

## API contract

```rust
pub fn quote_exact_in(
    state: &AdaptiveSwapState,
    params: AdaptiveExactInParams,
) -> xone_domain::Result<AdaptiveSwapResult>;
```

The caller exports the new module from protocol-math's lib.rs. This sidecar does
not edit that export, existing modules, domain/parser/protobuf, previous fee
kernel tests/corpus/reference, or progress files.

`AdaptiveSwapState` includes the pool ID, classic token program ID, tick spacing,
base/protocol fees, current price/tick/liquidity, both global fee growth values,
both protocol-fee owed balances, reward timestamp and all three reward records,
all supplied initialized tick contents plus bitmap/array binding/layout, and the
Oracle's pool binding, trade-enable timestamp, complete constants/variables with
reserved bytes, and 128-byte Oracle reserved area. u128/i128 values use canonical
JSON decimal strings. Fixed-width fields and three-slot arrays are typed.

`AdaptiveExactInParams` has `amount: u64`, `a_to_b: bool`, `sqrt_price_limit:
u128`, `timestamp: u64`, and `max_steps: u32`. A zero price limit is the original
implicit MIN/MAX sentinel. The explicit iteration budget is 1..8192.

`AdaptiveSwapResult` reports actual input consumed, output, remaining input,
LP/protocol fee, the complete next mathematical state, and per-step audit values
(group, fee rate, bounded target, skip, actual next price, prior liquidity,
amounts/fee and initialized-tick crossing). The input is immutable and errors
return no tentative state. Reuse `next_state` for subsequent trades.

**Partial fills are explicit:** reaching either a supplied limit or a protocol
price endpoint returns actual consumed input and `amount_remaining`, matching
original exact-in behavior. A caller requiring complete consumption must check
`amount_remaining == 0`. Zero output can be a valid native mathematical result
(e.g. a one-unit all-fee trade or a zero-liquidity price traversal).

## Progression and numeric fidelity

Before every step the implementation updates volatility, then obtains that
group's fee and bounded target. It preserves the original nested loops so a
single next initialized tick can contain multiple adaptive group steps. It
crosses a tick only at the actual next-tick price, adjusts liquidity in the
original direction with original-width unary-negation wrapping, crosses both
fee-growth outside values after fee accrual, and moves the array/search cursor
using the original shifted tick convention. Skip and non-skip advancement use
the existing fee kernel's separate methods.

The existing Q64 step has a static fee admission cap of 60,000. The composition
therefore calculates `available = floor(remaining * (1_000_000 - fee) /
1_000_000)`, calls that same price/amount step with `available` and zero fee,
then settles the original adaptive fee: `ceil(amount_in * fee /
(1_000_000 - fee))` at the bounded target, or `remaining - amount_in` for a partial
step. This reuses the exact price/delta and overflow behavior for adaptive rates
through 100,000. It does not call the static-fee quote or duplicate its protocol.
Original native cases exercise rates above 60,000 and at the 100,000 cap.

Protocol fees are rounded per step before LP fee growth. Global growth and owed
fees wrap at native widths. Uninitialized rewards neither accrue nor cross;
the original pool post-update still advances `reward_last_updated_timestamp`.
The whole-swap pre/post prices update the last-major timestamp only after the
swap loop. Full Oracle constants, binding, trade gate and reserved bytes remain
present in the returned state.

## Supported-state boundary

The API rejects initialized/noncanonical uninitialized rewards, Token-2022,
dynamic arrays, invalid Oracle/array pool bindings, bad reserved/array widths,
invalid timestamps/fee constants/price direction, inconsistent tick and price,
invalid/duplicate/unsorted initialized ticks, bitmap mismatch, array gaps,
liquidity/arithmetic errors, proof exhaustion, and step-budget exhaustion.
The account decoder must prove that these complete fixed-array snapshots and
Oracle belong to the actual pool. Structural ID/bitmap checks do not establish
chain provenance. Missing proof never becomes an implicit empty array.

Reward growth vectors remain the original three slots; support for initialized
rewards is deliberately rejected. Token balances, transfers, Anchor instruction
execution, SBF execution, wire/parser integration and service dispatch are beyond
this mathematical API. An online adaptive profile still requires those integration
and provenance checks; this delivery does not open it.

## Independent original native evidence

This NEW native harness depends on the unchanged original `whirlpool` crate at
`cc683ec92acb5e7606caa3d8e514d9383e9c7c11`. It builds real FixedTickArray objects,
uses the original SwapTickSequence, calls original `swap_manager::swap` with
adaptive info and exact-input mode, then original `Whirlpool::update_after_swap`
and `Oracle::update_adaptive_fee_variables`. The harness never imports the port.
The supplied ascending arrays are selected in direction before calling the
original sequence; it does not execute an Anchor account loader or sparse account
builder. Trade/proof admission guards are checked separately by Rust tests.

Build and startup verify all 1,111 original source-inventory files, revision and
archive pins, metadata digest, empty adaptations, critical manager/util/math
hashes, and this NEW harness's Cargo.toml/Cargo.lock/build.rs/identity.rs/main.rs
closure. Rust 1.90.0 compiler identity, all 282 locked Cargo entries and the exact
executed native binary SHA-256 are recorded. Native dev overflow checks are disabled
to reproduce original program release arithmetic. A changed source or lock causes
startup refusal before output. The previous fee-only harness/closure is untouched.

The corpus has 101 successful original swaps, two native proof-exhaustion errors,
and two carried-state sequences with 12 trades. It covers both directions,
negative/boundary groups, per-group adaptive changes, caps, skip ranges,
initialized ticks/liquidity, fee/owed wrapping, filter/decay/age windows, zero L,
partial price-limit fills, same-block reversal, shifted starts and -443637.

## Reproduction and guards

From repository root:

```sh
export PATH="$HOME/.rustup/toolchains/1.90.0-x86_64-apple-darwin/bin:$PATH"
cargo build --locked --offline \
  --manifest-path integration/tools/reference/orca-adaptive-swap/Cargo.toml \
  --target-dir /tmp/xone-orca-adaptive-swap-native
python3 integration/tools/ci/orca-adaptive-swap-golden.py --check
python3 -OO integration/tools/ci/orca-adaptive-swap-golden.py --check
cargo test --locked --offline --manifest-path dex-router/Cargo.toml \
  -p xone-protocol-math --test orca_adaptive_swap \
  --target-dir /tmp/xone-orca-adaptive-router
cargo clippy --locked --offline --manifest-path dex-router/Cargo.toml \
  -p xone-protocol-math --test orca_adaptive_swap \
  --target-dir /tmp/xone-orca-adaptive-router -- -D warnings
python3 integration/tools/ci/orca-adaptive-swap-guards.py --startup-guard-test
```

The Rust suite has three tests and reads **only** the router's own
`test/contracts/orca-adaptive-swap.json`. The new Python generator checks the
integration/router corpus copies are identical. Deliberate regeneration uses the
same generator without `--check`; it writes only these NEW corpus files.

All Python guards use explicit `require` and remain active with `-OO`, including
native process wait/exit verification. The guard script verifies both modes accept
the real evidence and reject a corrupted observation, dependency closure and
source closure (six counterexamples). Temporary test fixtures live outside the
repository. Its optional startup probe modifies only this NEW harness lock,
checks refusal before output, and restores exact bytes and timestamps.
Replay records the original generation binary hash and the current replay hash
separately, allowing a native relink only when source/dependency/compiler closure
and every original observation match exactly. Check mode never regenerates.
