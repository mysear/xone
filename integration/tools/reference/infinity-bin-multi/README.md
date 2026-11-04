# Infinity initialized-bin kernel reference

`infinity_bin_multi.rs` is a sidecar. It is deliberately absent from `lib.rs`
and the public pool dispatcher. Tests include it with `#[path]`. To consume the
source in another module, provide the existing `crate::infinity_bin` price helper.
It uses production `xone_domain::Amount`, `Result`, and `Error`.

```rust
quote_exact_in(&MultiBinState, &Amount, zero: bool) -> Result<MultiBinSwap>
```

`zero=true` sells token 0 for token 1. `MultiBinState.bins` is a `BTreeMap<u32,
BinReserves>` of **all initialized tree members**, including members whose output
reserve or both reserves are zero. `tree_min_id=0` and `tree_max_id=0xffffff` are
required attestations of complete u24 coverage. They do not establish the
provenance or completeness of a caller-supplied map. A window must not be marked
as complete. Neither an empty active bin nor a depleted bin is removed by swap.
The active ID may be absent from the initialized map. IDs 0 and 0xffffff are
reserved traversal sentinels and rejected as active IDs or initialized members.

The state also carries `active_id`, nonzero u16 `bin_step`, snapshot `lp_fee`
(0..100000), directional `protocol_fee0/1` (0..4000), and `max_steps`. The budget
counts loop visits, including empty active and initialized bins, not price
calculations or fills. Exceeding it returns `INFINITY_BIN_STEP_BUDGET`; this is a
local support rejection even when native execution can succeed. No result is
returned with a false completeness flag or partially consumed input.

Strict predecessor lookup for 0→1 and strict successor lookup for 1→0 reproduce
`TreeMath.findFirstRight/Left` over complete membership. The original tree's
comments say inclusive; its bit operations exclude the current ID. The ordered
map is equivalent to its three bitmap levels. Tests span bit/leaf/level boundaries
and retain depleted membership across reverse swaps.

Each fill uses the existing pinned Q128 price/pow routine, fee ceiling, native
full-depletion gross cap, and protocol allocation `floor(total_fee * protocol /
swap_fee)`. The kernel checks u128 per-bin caps and reserves, packed aggregate
output/protocol widths, u256 liquidity arithmetic, the original per-bin maximum
liquidity, and final positive i128 output/input limits. Input zero is outside this
exact-in API. Native `-2^127` enters the loop but its final `safeInt128` fails;
the kernel rejects that magnitude at entry. Error order for multiple simultaneous
faults is not an EVM revert-selector equivalence claim.

`MultiBinSwap` returns `amount_out`, `consumed_in`, zero `remaining_in`, total
`protocol_amount`, `swap_fee`, `steps`, `bin_fills`, and full `next_state`.
`BinFill` exposes ID, gross input, output, total fee, protocol amount, and Q128
price. Protocol accumulation is returned per quote; the external manager's
persistent protocol balances are not part of `BinPool.State`. The harness tracks
both directional totals and tests accumulation and rollback across sequences.
All failures leave the supplied state unchanged. Nonfillable exact input returns
an error, including exhaustion after intermediate fills.

## Original execution and provenance

The harness imports unmodified Infinity core at revision
`3c5ef1be1f71442d8bc60cc6fd11f9c677b714df`. Imports must be present in the existing
`singleton-advanced/source-metadata.json` and match its SHA-256. There is no
side-effect import of another generator and no JS quote/price implementation.
`seed` uses original `TreeMath.add/remove`; `attempt` executes original
`BinPool.swap` via an external self call. Caught reverts are mined as transactions,
then all reserves, active ID, tree membership and protocol counters are checked
against the pre-call state. Successful transactions check both signed deltas and
full storage post-state. Per-bin fee traces use original `BinHelper` and original
tree lookups independently of the full swap call.

Solidity 0.8.26, Ganache 7.9.2 and ethers 5.8.0 are loaded from the existing
`integration/.runtime/singleton-reference/node_modules`. Runtime sources are read
from its `sources/` directory. Compiler settings are optimizer 200, viaIR,
Shanghai, no CBOR metadata. Normal EIP-170 is enforced and unlimited size is off.
The harness runtime is 8,736 bytes. Missing optional µWS native acceleration can
use Ganache's Node fallback; no Solidity behavior is replaced.

`execution-receipt.json` is reset to `passed:false` before optional dependency
loading, source parsing or compilation. Success is written only after source and
input rechecks and provider cleanup. It records current Node/compiler/dependency,
source, build/runtime and fixture hashes. `source-metadata.json` retains every
imported source pin and compiler input hash. `infinity-bin-multi-check.py` rejects
stale receipts, changed inputs, unequal router/integration corpora, source or
optional dependency changes. These receipts are local reproducibility evidence,
not a service or deployment attestation.

## Commands

Run from the repository root:

```sh
node integration/tools/ci/infinity-bin-multi-golden.cjs
python3 integration/tools/ci/infinity-bin-multi-check.py
python3 integration/tools/ci/infinity-bin-multi-test.py
python3 integration/tools/ci/infinity-bin-multi-receipt-test.py
python3 integration/tools/ci/infinity-bin-multi-check.py --with-tests
```

The 326-case corpus contains 234 exact Rust/native successes, 90 native reverts,
and two additional native successes that the local step budget rejects. It
covers 2/3/8/12 initialized bins, sparse IDs, both directions, empty and absent
active bins, per-bin partial/full depletion, dust, zero/max/snapshot LP fees,
directional protocol fees, sequences, atomic missing-liquidity rollback,
price/pow failure and isolated signed/unsigned arithmetic boundaries. It is
independent of the pre-existing 180-case single-bin corpus. Seven Rust tests
cover the differential and state/budget/API boundaries. Four receipt failure
tests start from an old passing receipt and require failure for a missing
optional dependency, malformed source metadata, changed source pin, and invalid
Solidity. They operate only in disposable temporary directories.

The isolated runner compiles the actual kernel and Rust test files against an
exact extracted production `Amount/Error` source slice in a temporary crate.
It uses installed Rust 1.90 and offline dependencies pinned in
`test-dependencies.lock`, records source/tool/lock
hashes, and does not rewrite workspace sources. This can validate the sidecar
while unrelated workspace domain/proto edits are incomplete. It is **not** a
claim that the full workspace builds. A direct workspace test command is:

```sh
RUSTC=/Users/mac/.rustup/toolchains/1.90.0-x86_64-apple-darwin/bin/rustc \
RUSTDOC=/Users/mac/.rustup/toolchains/1.90.0-x86_64-apple-darwin/bin/rustdoc \
/Users/mac/.rustup/toolchains/1.90.0-x86_64-apple-darwin/bin/cargo test \
  --manifest-path dex-router/Cargo.toml -p xone-protocol-math \
  --test infinity_bin_multi --locked
```

Manager/key/block binding, Go manager/tree sourcing, dynamic fee hook execution,
return deltas, overrides, native currency actual settlement, exact output,
mint/burn/share validity and public pool-profile dispatch remain outside this
kernel's evidence. Dynamic LP cases mean a supplied current fee snapshot only.
