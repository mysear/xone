# Real Agave Bank exporter validation

This harness compiles the pinned Agave v4.3.0 source and includes the test inside
its real Geyser accounts notifier module. It creates real AccountsDb-backed
Banks with a genuinely staked genesis and real BankForks program-cache graph,
registers real ticks to the block boundary, uses the actual
AccountsUpdateNotifierImpl, and exercises the patched
Bank freeze and drop hooks. It does not call observe_account or frozen_bank
from the test to manufacture notifications.

A transparent trace tap forwards each native callback unchanged into the actual
AccountsUpdateNotifierImpl and retains its raw arguments in
`bank-evidence/native-notifications.json`.

The single test checks native write-version zero without renumbering, parent scope state, two writes to one child account,
write-version order, inherited account state, parent/child native identity,
idempotent freeze, divergent-slot fork isolation, and 16 genuinely notified Banks
abandoned before freeze under a max_banks budget of 8. JSON manifests and the
result record remain in the chosen output directory.

The runner rechecks the acquired archive hash, original source pins, generated
patched file bytes, compiler identity (exact 1.97.1), and preservation of every
original external lock dependency identity. It refuses changed upstream inputs
and counts a test as passed only when exactly one test passed and its real Bank
result artifact exists. It limits time and jobs and terminates its own process
group at the deadline. No other worker's Rust source or workspace manifest is
edited.

From the repository root, choose a fresh output directory per run:

```sh
R03_ROOT="$PWD/integration/artifacts/local/r03-agave-validation"
R03_CARGO="$R03_ROOT/native-validation/toolchain-1.97.1/bin/cargo"
CARGO_HOME="$R03_ROOT/native-validation/cargo-home" python3 \
  chain-parser-rust/crates/bank-source/agave-4.3.0/real-validator/run.py \
  --agave-root "$R03_ROOT/agave-4.3.0" --cargo "$R03_CARGO" \
  --libclang-dir /Library/Developer/CommandLineTools/usr/lib \
  --stage check --timeout 600 --jobs 2 \
  --output "$R03_ROOT/native-validation/check-real"
```

Repeat with `--stage build` and `--stage test`, using fresh output directories.
The runner sets both PATH and RUSTC/RUSTDOC to the selected toolchain, avoiding
an older rustc elsewhere on PATH. On macOS, `--libclang-dir` sets LIBCLANG_PATH
and DYLD_LIBRARY_PATH only for the spawned command, using the existing CLT
library required by bindgen. It records that library's SHA-256. Source changes inside the artifact tree are
limited to the pinned exporter patch and a test-only module include. The test
needs no RPC endpoint, validator network, paid service, plugin shared library,
or mainnet synchronization.

A dependency acquisition failure is not a Bank test result. The run's
`result.json`, `cargo.log`, and `bank-evidence/real-bank-result.json` (only when
the test actually reaches completion) distinguish these outcomes. Direct Bank
stores have no transaction signature; transaction execution, snapshot restore,
root/finality propagation, sustained throughput, and mainnet behavior are
outside this minimal test's claims.

`native_zero_version_repro.rs` is a diagnostic replay of one callback captured
by the actual Bank test. It invokes the public kernel observer with the captured
metadata and bytes unchanged, allowing its exact error to be observed when the
global frozen guard only reports AUTHORITY_UNAVAILABLE. It is not an alternative
to the real-Bank execution criterion.
