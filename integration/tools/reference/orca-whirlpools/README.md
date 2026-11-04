# Independent native Orca Whirlpools reference

This reference executes the unchanged original `orca-so/whirlpools` program at
`cc683ec92acb5e7606caa3d8e514d9383e9c7c11` as a native Rust library. Its only
local dependencies are the harness and that program crate. It imports no XONE
production math, parser, domain, router, or generated protocol package.

The archive SHA-256 is
`9ddee363a1aec82d4130dd94deb52c78c659dff1e071d0fa7d56a1cbd8e450fb`.
`upstream.tar.gz`, the entire extracted tree, the original license and original
`Cargo.lock` are retained. `source-metadata.json` records hashes and byte lengths
for all 1,111 original files. The verifier checks exact archive/manifest/tree
membership and byte equality for every original file. No original source patch,
transcription, conditional compilation edit, or shim crate is used.

## Reproduce

From the XONE root, with Rust 1.90.0 and Python 3.11 or later:

```sh
python3 integration/tools/ci/orca-reference-fetch.py
python3 integration/tools/ci/orca-golden.py
python3 integration/tools/ci/orca-golden.py --offline --check
```

The script explicitly resolves Cargo, rustc and rustdoc from Rust 1.90.0;
selecting only Cargo is insufficient when Homebrew rustc 1.83 is first on PATH.
Every Cargo invocation uses `--locked`. `--offline` forbids Cargo network access
and requires the locked registry dependencies to have been cached. The initial
online bootstrap, including target-specific dependency metadata, is:

```sh
RUSTC="$HOME/.rustup/toolchains/1.90.0-x86_64-apple-darwin/bin/rustc" \
RUSTDOC="$HOME/.rustup/toolchains/1.90.0-x86_64-apple-darwin/bin/rustdoc" \
"$HOME/.cargo/bin/cargo" +1.90.0 fetch --locked \
  --manifest-path integration/tools/reference/orca-whirlpools/Cargo.toml
```

The toolchain paths in that bootstrap example are macOS Intel specific; the
Python runner uses `rustup which` and is portable across installed targets.
`orca-reference-fetch.py --fetch` can restore verified original archive contents.
The original tree must remain unmodified, including its lockfile. The harness
has a separate locked dependency graph; its manifest matches the original Anchor
and serde JSON pins. New registry versions are not silently resolved on checks.

`orca-golden.py` verifies all source bytes, runs the release tests, builds the
native executable, inventories exact dependencies and checksums, generates the
corpus twice in independent native processes, checks coverage/accounting and
compares the results. `--check` byte-compares every saved output and never
rewrites fixtures. Reports and logs are saved under
`integration/artifacts/local/orca-golden-generation.{json,log}`.

## Original code executed

`default-features=false, features=["no-entrypoint"]` builds the actual program,
including the pinned pinocchio dependencies, without its exported entrypoint.
The wrapper constructs real original account bytes and `AccountInfo` values.
It calls:

- Original `Account<Whirlpool>::try_from` and fixed/dynamic tick-array loaders.
- Original `SparseSwapTickSequenceBuilder::new(...).try_build(...)`.
- Original `manager::swap_manager::swap`, with exact input, no explicit price
  limit, the bank timestamp and `adaptive_fee_info=None`.
- Original `Whirlpool::update_after_swap`, including its owed-fee addition.
- Original immutable tick-array loaders after the swap to export all crossed
  fee-growth-outside state, while preserving all initialized tick proofs.
- Original `sqrt_price_from_tick_index` and `tick_index_from_sqrt_price` for
  independent tick-price expectations.

Release overflow checks are false, matching the original workspace release
profile. This matters because the original `update_after_swap` uses ordinary
`u64 +=` and wraps under the actual release semantics. The test profile also
matches that setting; no wrapped fee is reconstructed by the harness. The swap
manager itself performs explicit u128 fee-growth wrapping and crossed-tick
wrapping in the original helpers.

The small standard-token wrapper requires the whole allocated input to be
consumed, rejects zero output, and uses checked u64 deposit/withdrawal accounting
for the two raw vault balances. Errors, original panics, partial fills, zero
output, raw input overflow and output underflow discard the cloned mutations.
The manager's exact-in API can return partial success at a global price limit;
that is deliberately an adapter rejection. No expected swap amount, price,
liquidity, fee growth, fee owed or crossed-tick value is transcribed in Python.

## Important source constraints

The original maximum fee rate is **60,000**, despite storage in a u16; the
original maximum protocol fee rate is **2,500**, with denominator **10,000**.
Cases with `65535` are admission rejections. Rates 0, 3,000 and 60,000 and
protocol fees 0 and 2,500 have successful examples in both directions.

The original sparse builder selects **at most three arrays per swap**, even when
supplied seven complete accounts. Supplemental arrays are fallbacks after price
movement. B-to-A selects the following array first when current tick plus
spacing reaches the current array's end. Audit metadata records the actual
selected starts, read through the original sequence API. The wrapper never
walks all seven arrays itself. Fixed and dynamic layouts use the original
respective loaders and have identical numeric semantics.

The quote wrapper checks the complete proof window (3..7 contiguous ascending
arrays, valid original starts and PDAs, complete initialized bitmap and ticks),
then delegates directional array requirements entirely to the original builder.
It does not require the floor-divided current-tick array to be present. Following
a left cross at a window's low edge, the legitimate `tick_current_index=start-1`
can lie immediately left of that window, while a B-to-A return selects its first
array under the original shifted rule. Fresh parser snapshot admission remains
a separate bounded-baseline policy; this oracle accepts the resulting shared
poststate according to the original manager's directional semantics.

Two native regression tests first failed with `missing current array`, recorded
in `integration/artifacts/local/orca-golden-generation-boundary-red.log`, then
passed after removing only that false precondition. The exact fixed/dynamic
regression starts at tick 1 with spacing 64 and arrays `[0,5632,11264]`:
`A->B 50146692` yields `49998750`, tick `-1` and price
`18446744073709551616`; the following `B->A 10000` yields `9969`, selecting
`[0,5632,11264]`. Adding an unused `-5632` proof leaves the complete numeric
poststate unchanged. An A-to-B attempt from the same boundary still fails in the
original builder, and incomplete, unordered or inconsistent bitmap proofs
continue to fail window admission.

The corpus appends eight boundary cases and two eight-step boundary-return
sequences, one per layout, including the exact crossing and return followed by
mixed directions. `boundary-return-baseline.json` stores canonical hashes of all
872 previous cases and eight previous sequences; regeneration verifies every
previous entry is unchanged. The base pool, base fixture and tickmath vectors
are unchanged.

Vaults are **arbitrary signer accounts**, as shown by `InitializePool` and
`initialize_vault_token_account`; they are not PDAs. The fixture uses distinct,
deterministically generated on-curve keys for vaults. The SPL authority stored
in each vault is the Whirlpool PDA. Pool mints are sorted by raw 32-byte keys.
Pool, oracle, tick-array, fee-tier and ProgramData seeds follow the original
program and official SDK. A native test includes the official SDK tick-array
PDA vector. Tick-array starts use signed decimal ASCII, not binary i32 seeds.

## Outputs and exact JSON

- `integration/test/contracts/orca-quotes.json` and the matching
  `dex-router/test/contracts/orca-quotes.json` contain `base_pool`, `reference`,
  `cases` and `sequences`.
- A success is `expected: {amount_out: "...", next_pool: <complete Pool>}`.
  A failure is exactly `expected: {reverted: true}`. Revert detail is separate.
- `reserve0` and `reserve1` remain string `"0"`; `fee_numerator` equals the
  nested `fee_rate`; adapter is `orca-whirlpools/cc683ec9-spl-static-1`, profile
  is `orca-whirlpools-static`, and chain is `solana:localnet`.
- CoreAmount u128 values, input/output and signed tick liquidity are decimal
  strings. u64 account metadata and raw balances remain exact JSON integers,
  emitted by Rust/Python. Do not parse those integers through JavaScript Number.
- Every array includes its complete 88-bit initialized bitmap and all initialized
  tick proofs with liquidity gross/net and both fee-growth-outside fields.
- `integration/test/solana/orca-base.json` contains the full pool/config, ordered
  accounts `[pool,vault0,vault1,config,mint0,mint1]`, config and fee-tier authorities,
  lengths, PDA metadata, code UTF8/hex strings and hashes. `rpc_accounts` supplies
  exact account byte strings for a process fixture, including original-serialized
  pool/config/tier, fixed arrays, classic SPL mints/vaults, Clock, upgradeable
  program/ProgramData, immutable token proof and system-owned empty oracle.
- `orca-tickmath.json` remains in this reference directory for the main worker to
  copy as needed. It has 1,045 original-function vectors, inverse probes one unit
  above/below, endpoints and a SHA-256 over all 887,273 tick/price pairs encoded
  `tick:i32 little endian || price:u128 little endian`, ascending tick order.
  Original positive-Q96/negative-Q64 coefficients are retained in the pinned
  `upstream/programs/whirlpool/src/math/tick_math.rs`; no V3 shift is involved.

The 880 single cases include 346 positive-output complete swaps and 534
rejections. Ten sequences each have eight steps, including same direction,
alternating direction and mixed allocation order. There are successful crossings,
empty arrays, low/zero/max liquidity, successful zero-liquidity gaps and recovery, signed i128 crossing boundaries, dust, u64/u128 bounds, missing directional
coverage, full exact-in partial-fill rejection, raw vault overflow/underflow,
exact u64-max raw balance, fee-growth wrap, owed-fee wrap and crossed outside-fee
wrap. Every successful case/sequence step exports the complete next pool.

## Scope limits

This is native execution of the original manager and account loaders, not an SBF
transaction or a validator/bank execution. It does not execute CPI, the Token
program, signatures, initialization constraints, RPC snapshot finality, WSOL
backing checks, or the account parser. Token2022, adaptive fees and initialized
rewards are excluded. All reward mint/vault/emission/growth fields and tick reward
outside fields are zero; only reward 0's known authority extension is populated.
Reward 1 control flags and its reserved extension, and reward 2 extension, are zero.
The oracle account is an uninitialized system-owned empty PDA; only that declared
oracle may be represented by RPC null.

**The code proofs are synthetic fixture identities.** Their byte strings and
hashes are not hashes of deployed mainnet bytecode and are not attestations that
the production deployment corresponds to this source. The ProgramData PDA is
real; its fixture deployment slot, authority and code bytes are synthetic.

The corpus uses the plan's full `max_steps=8192` budget. The original manager has
no adapter-budget parameter, and the reference deliberately does not claim to
validate smaller custom budgets. Full-range-only high spacing can leave fewer
than three valid arrays within global tick bounds; the plan's 3..7-array admission
requirement remains a separate profile limitation.
