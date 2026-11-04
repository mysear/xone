# Pinned original Raydium CLMM native oracle

This independent reference executes the **unchanged original program crate**, `raydium-io/raydium-clmm` at `6d7a0f82d2f293c553c69aaf943377367c6c4cc6`, using its `no-entrypoint` feature. It calls actual `instructions::swap::swap_internal` with mutable original `PoolState`, `TickArrayState` and `ObservationState` accounts through `RefCell`/`RefMut`. It imports no XONE business, domain, parser or math crate. The program ID is `CAMMCzo5YL8w4VFF8KVHrK22GGUsp5VTaW7grrKgrWqK`; profile `raydium-clmm-static`, adapter `raydium-clmm/6d7a0f82-spl-static-1`.

## Reproduce

From the XONE root:

```sh
python3 integration/tools/ci/rayclmm-reference-fetch.py
python3 integration/tools/ci/rayclmm-golden.py --offline
python3 integration/tools/ci/rayclmm-golden.py --offline --check
```

The runner explicitly selects Cargo, rustc and rustdoc from Rust **1.90.0**. It builds only this independent native reference, with `--locked`; output is `/tmp/xone-raydium-clmm/target2`. It starts no services, infrastructure or service binary builds. Offline operation requires cached registry dependencies. `--check` tests, regenerates twice, audits integer widths/accounting/coverage and compares both saved golden copies, fixture, TickMath vectors and generation report hashes without rewriting them.

Individual requests use the full shared Pool JSON and decimal-string input:

```sh
/tmp/xone-raydium-clmm/target2/debug/raydium-clmm-reference quote < request.json
/tmp/xone-raydium-clmm/target2/debug/raydium-clmm-reference fixture
/tmp/xone-raydium-clmm/target2/debug/raydium-clmm-reference tick-vectors
```

Successful quotes return `{ "amount_out": "...", "next_pool": <full Pool> }`; errors return `reverted` and a diagnostic reason. Corpus reverts are exactly `{ "reverted": true }`. Every u128 Amount and i128 liquidity net is a decimal string. u64/u32 fields remain JSON integer numbers, including values above `2^53`. Read with Python integers, Rust serde or Go `UseNumber`; ordinary JavaScript Number parsing loses precision.

## Source and dependency integrity

`source-metadata.json` records every file and both retained codeload archives. `rayclmm-reference-fetch.py` compares all **105 upstream files** and **120 parity-common files** against both the manifest and exact archive bytes; it also checks for extra or missing source files.

- Upstream archive SHA-256: `13841cd1cddf0fc906970f70f8a639cb595e484c8ff260176aec0009cc561727`.
- The upstream lock pins `uint` to `raydium-io/parity-common` commit `43a0303b9c2a23a00508816bd6d5a2e9381f143c`.
- Dependency archive SHA-256: `f47bf73b98ccb7c5c70fd5e94d31493e979b709ed241df03500cb48ce08bec8b`.
- `cargo-source.toml` replaces that locked git source with its byte-identical archived directory. The only additions are generated `.cargo-checksum.json` metadata for Cargo directory sources; they are independently verified. Neither original source nor either upstream Cargo manifest/lockfile is modified. No math transcription or alternate uint implementation is used.
- The independent root `Cargo.lock` locks all resolved registry dependencies and preserves the original uint git commit. The upstream workspace lock also contains its client/dev dependencies; the root lock records this runner's actual native dependency graph. Both lock hashes, compiler version, harness hashes, dependencies and commands are recorded in `integration/artifacts/local/rayclmm-golden-generation.json`.

`--fetch` restores the fixed archives and extracted files only after checking hardcoded archive pins. The fixed source is an audit sample, not a claim about latest deployed code.

## Original execution and source corrections

Actual upstream `swap_internal` performs TickMath, bitmap selection, SwapMath, step fee splits, tick crossing, liquidity changes, fee-growth wrapping, protocol/fund counters, pool epoch, crossed-array epochs and observation updates. Original Rust errors and panics become reverts; all failed mutations are discarded.

A deterministic native `solana_sysvar::program_stubs::SyscallStubs` implementation writes the bank `Clock` to the Clock syscall ABI. Quotes are serialized with a mutex and the previous stubs restored on success, error or unwind. The unchanged crate is built without `client` or its own `cfg(test)`, so `util::get_recent_epoch` really executes `Clock::get()`, rather than the upstream wall-clock test helper. `oracle::block_timestamp()` also executes the syscall.

Source behavior relevant to product implementation:

- Actual `exact_internal` in `instructions/swap.rs` requires `block_timestamp > pool.open_time`, despite the Pool field comment saying disabled. The wrapper enforces this external admission before calling the core.
- Original `oracle::block_timestamp()` deliberately casts Unix time to u32 modulo `2^32`. This profile explicitly admits only `0..=u32::MAX` bank time; future modulo behavior is not supported.
- Swap status bit **4** disables swaps when set; status `0` permits swaps. The admitted typed scope fixes unsupported fields to their supported zero/default values before native construction.
- Original bitmap methods select the first and next initialized arrays. `None` is passed for the extension account. When required, missing extension/proof arrays cause upstream errors; this does not assert that liquidity outside the proof window is absent.
- Protocol and fund fees each floor their share of the original per-step fee. The remaining fee is LP growth when liquidity is positive; the original zero-liquidity path attributes any remaining fee to protocol.
- Original reward update writes pool `recent_epoch` even when all rewards are uninitialized. Original `TickArrayState::update_tick_state` writes the crossed array epoch. `ObservationState::update` preserves its epoch.
- Observation update runs only if final tick differs, uses the **previous pool tick**, initializes without advancing its ring index, requires at least 15 seconds to advance an initialized ring, saturates time differences and wraps cumulative i64 addition.
- The no-price-limit external exact-input entrypoint requires full input consumption and nonzero input/output. This wrapper enforces those checks and checked classic SPL u64 vault balance effects. A native unit test separately executes a successful partial swap with an explicit price limit, proving the original core's behavior; the supported no-limit profile rejects incomplete swaps.
- `max_steps` is an explicit conservative wrapper admission: all provided initialized ticks plus one final step must fit the budget. It does not instrument or change the original loop.

The original static profile supports input fees (`fee_on=0`), fully default DynamicFeeInfo, legacy seed `[0,0]`, no initialized rewards, no limit orders or unknown padding, classic SPL, and a base 1024-bit bitmap with 3–7 contiguous proof arrays. No original advanced feature is reclassified as supported.

## Shared state, corpus and raw RPC fixture

`integration/test/contracts/rayclmm-quotes.json` and `dex-router/test/contracts/rayclmm-quotes.json` are identical. The corpus contains **480 cases** (320 successful, 160 reverts), balanced directions, plus **12 sequences / 96 successful steps** with complete expected next/final Pool objects. Cases cover zero/nonzero/high fees, full protocol/fund shares, crossing multiple arrays, skipped empty bitmap arrays, zero liquidity, nonzero liquidity deltas, tick boundaries, u64/u128 boundaries, raw vault/counter overflow, growth wrap and observation poststate. Successful full-width probes cover JSON integer precision boundaries and u64 maximum input/output. Reverts include proof-window exhaustion, missing extension, partial/incomplete input, insufficient raw balances, counter overflow and unsupported clock bounds. Some width/overflow probes intentionally model boundary state rather than economically reachable state from the base fixture.

`tick-vectors.json` has **2053 vectors** from original Raydium `get_sqrt_price_at_tick` and `get_tick_at_sqrt_price`, with inverse probes at exact price and one unit below/above. Its exhaustive SHA-256 covers **887273 ticks**, -443636 through 443636 inclusive, encoded as ascending i32 little-endian tick followed by original u128 little-endian price. This is Raydium's own TickMath; for example tick 1 is `18447666387855957090`, independently calculated by the pinned source. No Orca/V3 conversion is used.

`integration/test/solana/rayclmm-base.json` and local `base-fixture.json` contain:

- Full typed base Pool, seven-account PoolConfig, three-to-seven array configuration, program and classic token proof identities.
- Real canonical PDAs derived using original seed constants and Anchor `Pubkey`: ProgramData `HzD2cCXXT3UQNjMMY6kDv9w6gZ9qquSdfoGXrLL3LXx`, config, pool, vaults, observation and arrays.
- `rpc_accounts` keyed by account address, with owner/executable/lamports and both standard `[base64, "base64"]` data and `data_base64`. `rpc_request_accounts` gives the dependency order; `rpc_response` provides a matching finalized-context `getMultipleAccounts` response. A service smoke server may select requested accounts by key.
- Bytes emitted from original packed structs/Anchor serialization: pool **1544**, config **117**, arrays **10240** (60 ticks × **168**), observation **4483**. Discriminators and lengths are in metadata. Clock is 40 bytes; classic SPL mint/vault are 82/165 bytes.
- Loader Program/ProgramData records and clearly labeled **synthetic, not deployed** code bytes, with exact code hashes. The fixture is suitable for independent actual RPC decoder/service verification; its proof identity does not establish deployed bytecode/source equivalence.

## Execution limits

The original native swap/state/math functions and Clock syscalls execute. The full Anchor instruction, SBF program, Solana bank, token CPI/program, user accounts, signatures, locks, rent and WSOL backing do not execute in this runner. Account owner/executable/PDA/ABI admission is the production decoder's responsibility; this oracle emits actual-layout RPC bytes for that integration. The wrapper models classic SPL vault transfers with checked u64 arithmetic. The main worker owns actual three-service verification; this reference does not claim those services have passed.


## Separate token-fee oracle

`generate-token-fee` emits the independent `raydium-clmm-token-fee` corpus, adapter
`raydium-clmm/6d7a0f82-spl-token-fee-1`. The shared state adds
`collect_fee_on: u32`: missing values deserialize as zero and zero is omitted on
serialization. The static profile/adapter admits only mode 0; the new pair admits
only modes 1 and 2. Unknown identities, mixed adapters and other modes are rejected
before account conversion. The conversion calls the original `PoolState::set_fee_on`;
all swaps still execute unchanged `swap_internal` with mutable original accounts
and the deterministic Clock syscall.

The pinned semantic audit is in original `states/pool_fee.rs` (`fee_on`,
`set_fee_on`, `is_fee_on_input`, `is_fee_on_token0`), `libraries/swap_math.rs`
(`compute_swap`), `instructions/swap.rs` (`SwapState::update`, `settle_amounts`,
`swap_internal` and the external classic-SPL transfers), and `states/pool.rs`
(`update_fee`). Mode 1 always collects token0 and mode 2 always collects token1:

| Mode | token0 → token1 | token1 → token0 | Fee ledger |
| --- | --- | --- | --- |
| 1 | input fee | output fee | token0 |
| 2 | output fee | input fee | token1 |

On an exact-input output-fee step the original program uses the complete input
for price calculation, rounds `gross_output * rate / 1_000_000` upward per step,
and returns gross output minus that fee. On a partial step it consumes the full
remaining input, including sub-unit price-rounding dust. The original swap state
updates remaining input and accumulated net output accordingly. Protocol/fund
shares floor per step, and remaining LP fees update the actual fee token's global
growth; crossed ticks use the original outside-growth logic. The other token's
global growth and protocol/fund counters remain unchanged. Original external
transfer amounts are full input and net output, so checked classic-SPL vault
accounting adds full input and subtracts net output. No business math is imported
or reimplemented by this harness. Mode 0's original fee semantics remain intact.

```sh
# Refresh only old evidence after harness edits; every old generated byte must match.
python3 integration/tools/ci/rayclmm-golden.py --offline --report-only
python3 integration/tools/ci/rayclmm-token-fee-golden.py --offline
python3 integration/tools/ci/rayclmm-token-fee-golden.py --offline --check
python3 integration/tools/ci/rayclmm-golden.py --offline --check
```

The new runner uses the same locked offline git-uint source replacement, explicit
Rust 1.90 compiler/rustdoc and `/tmp/xone-raydium-clmm/target2`. It verifies all 225
archived files, source/dependency locks and harness hashes; runs native and Python
audit tests; regenerates twice; checks full-width integer types, actual-fee-token
accounting, balanced mode/direction coverage and complete sequence poststates; and
compares the original 480-case output to its frozen SHA-256
`ef1a0be7c80d6f7f5e815b75a10223a9a7be225d279bc74e057664b5af4f6f31`.

New corpus copies are `integration/test/contracts/rayclmm-token-fee-quotes.json`
and `dex-router/test/contracts/rayclmm-token-fee-quotes.json`. Generation evidence
is `integration/artifacts/local/rayclmm-token-fee-golden-generation.json` and
`.log`. The corpus has **968 cases** (636 successful, 332 reverts), exactly
**242 per mode/direction**, plus **24 sequences / 192 successful steps**. Its
828 successful results include 142 that cross multiple arrays, 18 fee-growth
wraps and 12 observation ring updates. Exact counts and hashes are recorded in
the report. The corpus contains both
`base_pool` (mode 1) and `base_pool_mode2`. They equal the original raw-RPC base
Pool except for protocol, adapter and collect_fee_on, preserving the original
2500 fee rate, liquidity, vaults, Clock and identity proofs. Each mode/direction
has independent successful 1,000,000 and 2,000,000,000 input vectors. The service
smoke worker derives mode 1/2 account bytes by changing packed pool byte **390**
of the original fixture. The original mode0 raw fixture is preserved.

Token-fee coverage retains the native reference limits above: conservative
`max_steps` admission counts all supplied initialized ticks plus one final step;
this is stricter than metering the native loop. The source, not an instrumented
loop, decides step completion and arithmetic errors. Synthetic counter/width
boundary states are explicit probes and may be unreachable from the base pool.
Dynamic fees, custom seeds, rewards, orders, bitmap extension and Token2022 remain
outside scope. Native execution is not SBF, a bank transaction, or token CPI; this
runner does not validate deployed source identity or start services/infrastructure.

## Separate bitmap oracle (Task 1 / native portion of Task 4)

`raydium-clmm-bitmap` / `raydium-clmm/6d7a0f82-spl-bitmap-1` admits static
`collect_fee_on` modes 0, 1 and 2 and requires `bitmap_extension`. The original
static and token-fee profiles still require its absence. The optional field uses
serde `default` and `skip_serializing_if = "Option::is_none"`; original state JSON,
480-case and 968-case corpora, raw base accounts and TickMath outputs remain
byte-identical. Their evidence reports may be refreshed after harness edits.

The extension JSON is `{address, positive: [112 u64], negative: [112 u64]}`.
Words retain all fourteen groups, including bits beyond the proof window and
logical bits beyond the actual tick domain. Actual source-valid array starts
are checked independently against upstream `TickArrayState`; an initialized
array selected by original search but absent from the supplied proof rejects.
All proof arrays remain contiguous, ordered, and limited to 3..7 arrays.

The pinned `swap_internal` signature is **`Option<&AccountInfo>`**, rather than
`Option<RefMut<TickArrayBitmapExtension>>`. The adapter creates original packed
extension bytes in an `AccountInfo` and passes `Some` for the new profile;
upstream `AccountLoad` supplies the original typed `Ref`/`RefMut`. The old profiles
pass `None`. Post-swap extension bytes are decoded back into the complete shared
Pool, along with original PoolState, ticks, observation ring and checked vaults.
No original source or dependency is patched and no business crate is imported.

The audited original files are `states/tickarray_bitmap_extension.rs`,
`states/pool.rs` (first/next search and bit flips), `states/tick_array.rs`
(valid starts), `libraries/tick_array_bit_map.rs`, and `instructions/swap.rs`.
`TickArrayBitmapExtension::LEN` is **1832**: discriminator 8 + pool key 32 +
positive 14×8×8 + negative 14×8×8. Its original PDA seeds are
`[b"pool_tick_array_bitmap_extension", pool_pubkey]` under the pinned program ID.
The base index domain is [-512, 511]. Positive extension group 0 is [512, 1023]
and negative group 0 is [-1024, -513], with low bits mapping to low signed
indices. Negative integral boundaries use the original offset adjustment:
-1024 is group 0 bit 0, -1025 is group 1 bit 511. The logical extension domains
are [512, 7679] and [-7680, -513]. At spacing 1, actual tick limits
[-443636, 443636] permit array indices **[-7394, 7393]**, including the partial
minimum array starting at -443640. Invalid starts cannot be flipped as valid
source arrays; logical words still retain such bits when supplied.

```sh
python3 integration/tools/ci/rayclmm-bitmap-golden.py --offline
python3 integration/tools/ci/rayclmm-bitmap-golden.py --offline --check

/tmp/xone-raydium-clmm/target2/debug/raydium-clmm-reference generate-bitmap
/tmp/xone-raydium-clmm/target2/debug/raydium-clmm-reference bitmap-fixture
# Alias: fixture-bitmap
/tmp/xone-raydium-clmm/target2/debug/raydium-clmm-reference bitmap-vectors
# Accepts full Pool JSON, or {"pool": <Pool>}; emits packed native account snapshot.
/tmp/xone-raydium-clmm/target2/debug/raydium-clmm-reference fixture-from-pool < pool.json
```

`bitmap-vectors` emits **86 independently source-derived one-bit vectors** plus
four invalid-start rejection vectors. Every nonzero word is produced by original
`TickArrayBitmapExtension::flip_tick_array_bit`; `expected_initialized` is read
using original `check_tick_array_is_initialized`, without a harness placement
formula. All fourteen positive and negative groups and both actual array limits
are covered, including -513, -1024, -1025, 512, 1023 and 1024. The file
`integration/test/contracts/rayclmm-bitmap-indices.json` is copied byte-for-byte
to all three services' `test/contracts/` directories for independent Go/Rust
mapping tests. SHA-256:
`24bbd337f791f217e6b3476d2284acaebeb53de77f26baaf99556ba9dadaea29`.

The quote corpus is `integration/test/contracts/rayclmm-bitmap-quotes.json`, with
an identical `dex-router/test/contracts/` copy. It contains **364 cases**
(131 successes / 233 reverts), modes 0/1/2, 60 cases per direction in modes 0/2
and 62 per direction in mode 1 (including next-bank vectors). It has **12 full
Pool sequences / 24 successful steps**, covering same and mixed directions,
both starting directions, in every fee mode. Its 155 successful results include
63 base/extension crossings and 76 bitmap-group crossings; these counts include
sequence steps. Cases include positive/negative base boundaries, ±1024 groups,
±7168 high groups, empty segments, selected missing proof arrays, tick limits,
fee ceil, counters, growth wraps, retained outside-domain bits and numeric widths.
The false-direction `max_steps=1` probes use 30b, which reaches at least three
original arrays under a relaxed reference bound. They exercise actual multi-step
traversal rather than a preflight-only rejection; business runtime budget semantics
remain independent. `reference.business_math_imported` is explicitly false.
Twenty-four independent positive/negative base vectors cover 1m and 2b inputs,
every mode and direction. SHA-256:
`350d4d447af4e07acb22786480cd2aa69ddcb85073a0ecfd80427079f20a851a`.

The raw base is `integration/test/solana/rayclmm-bitmap-base.json`, with identical
`chain-parser-rust/test/fixtures/solana/` and reference
`bitmap-base-fixture.json` copies. The top-level schema preserves `pool`,
`pool_config`, `rpc_accounts`, `rpc_request_accounts`, `rpc_response`, and
`metadata`. The positive base preserves original identity, fee rates, liquidity,
vault balances, Clock and code proofs; it relocates price/current tick to 30720
(array 512) and the proof window to starts 30480..30840 (indices 508..514).
The original seven configured business accounts are preserved; the actual
extension account is appended to the dependency request, for 19 total accounts.
Both directions at 1m/2b succeed. SHA-256:
`9ffc9ca1874243819b296dbd968e9407ecd1fd685c807606eca555628d5c164e`.

Metadata exposes complete nested snapshots for process acceptance:

- `alternate_negative_base`: current tick -30720, window indices -514..-508.
- `alternate_fixture`: actual full native next Pool after positive-base input
  1m, zero-for-one; `alternate_fixture_source` identifies the swap. Its raw
  pool/ticks/observations/vaults are serialized from that full post-Pool.
- `next_bank_fixture`: same identity and configured account window, mode 0→1,
  with highest proof array 30840 made empty. Its original extension bit is
  cleared, original array initialized count becomes zero, and packed pool byte
  390 reflects mode 1. `next_bank_vectors` supplies both directions / 1m and 2b
  with full native expected Pools, also present in the quote corpus.
- `next_bank_scope_probe`: input `30000000000`, `zero_for_one: false`, full
  `before_expected` output `1377715832`, `after_expected: {"reverted": true}`,
  and full `mode_only_expected`. The fee-only control still succeeds; clearing
  array 30840's extension bit causes original `LiquidityInsufficient`.
  `next_bank_scope_probe_candidates` records native 20b/25b/30b/35b checks;
  20b succeeds in both banks, and 25b/30b/35b succeed before and reject after.
  Matching before/cleared cases for every fee mode/direction are in the corpus.
- `mode_bank_change_vectors`: positive-base modes 0/1 with both directions and
  both process input sizes. `next_bank_change` records the actual raw changes.

Native and audit evidence is `integration/artifacts/local/rayclmm-bitmap-*`.
The runner verifies all **225 original files** against retained archives, locks
Rust **1.90.0**, uses the original archived git `uint`, builds **only** this
independent crate under `/tmp/xone-raydium-clmm/target2`, regenerates every new
output twice, checks original generator hashes and audits full post-Pool
accounting. `--check` compares saved outputs and report hashes without rewriting.

The earlier exclusions continue to apply separately to each profile. This new
oracle adds the original extension path only: it executes no SBF transaction,
CPI, real SPL Token transfer, Anchor entrypoint validation, signature, RPC owner
admission, service, infrastructure or deployment. Synthetic loader proofs make
no deployed bytecode/source identity claim. Dynamic fees, custom seeds, rewards,
orders and Token2022 remain excluded. `max_steps` admission conservatively counts
all supplied initialized ticks plus a final partial step (at most 8192); it is
not a native instruction compute meter. Main owns domain/proto and process
acceptance; the math and parser workers own their production changes.

## Customizable-v2 native seed reference (Task1 / Task4)

Profile `raydium-clmm-custom-seed`, adapter
`raydium-clmm/6d7a0f82-spl-custom-seed-1`, admits `seed_index: u32` only in
`1..=65535`, with static `collect_fee_on` 0/1/2 and a mandatory complete bitmap
extension (112 positive plus 112 negative u64 words). Serde defaults the field
to zero and omits zero on serialization. Static, token-fee and bitmap profiles
still require seed zero; their 480/968/364-case corpora, raw fixtures, bitmap
indices and TickMath vectors are protected by hardcoded SHA-256 pins.

The fixed original `create_customizable_pool_v2` has a u16 argument, requires it
to be positive, and derives `[pool, config, mint0, mint1, seed.to_le_bytes()]`.
The native adapter calls original `PoolState::set_seed_index`, checks original
`PoolState::seeds()` and `PoolState::key()`, and calls the same unchanged
`swap_internal`. It rebuilds vaults, observation, tick arrays and extension from
each new pool identity. Vault authority is that pool key. Config-index seeds
remain u16 **big-endian** and tick-array starts remain i32 **big-endian**.
Legacy seed zero passes an empty pool seed. The packed account stores the seed
at absolute bytes `[391,393)`; bytes `[393,397)` remain zero padding, while
byte 390 is `fee_on`. No original source files, manifests or lockfiles change.

Generate and verify from the XONE root, in this order after harness changes:

```sh
python3 integration/tools/ci/rayclmm-golden.py --offline --report-only
python3 integration/tools/ci/rayclmm-token-fee-golden.py --offline --report-only
python3 integration/tools/ci/rayclmm-bitmap-golden.py --offline --report-only
python3 integration/tools/ci/rayclmm-seed-golden.py --offline
python3 integration/tools/ci/rayclmm-seed-golden.py --offline --check
```

The seed generator builds only this reference with explicit Rust 1.90.0,
`--locked --offline`, into `/tmp/xone-raydium-clmm/target2`. It verifies all 225
original files against the pinned archives, performs native Rust tests and
Python audit mutation tests, regenerates twice, replays error and budget
controls, and checks the ordered static → token-fee → bitmap report chain.
It writes `integration/artifacts/local/rayclmm-seed-native-source.json` and
`rayclmm-seed-reference-build.log`. A checked invocation rewrites no artifacts.
The native commands are `generate-seed`, `seed-fixture`, `seed-identities` and
`quote`; `fixture-from-pool` also accepts a valid custom-seed Pool.

`rayclmm-seed-quotes.json` and its router copy retain the established full-Pool
case/sequence schema. `base_pool` is seed1, positive array index512/current
tick30720, preserving the original bitmap fixture's price, liquidity, fees,
vault amounts and Clock. `seed_pools` is a **list** of complete positive base
Pools for seeds1/255/256/65535; these share config and mints but have distinct
physical pool/vault/observation/array/extension identities. `cases` contains
only seed1..65535/profile/mode0..2 states (including intentional proof/overflow
failures). Dedicated `error_probes` covers seed0/65536, old-profile borrowing,
new-profile legacy PDA, BE pool addresses, bad bumps and mixed dependencies.
`independence_cases` carries two pools, steps selected by `pool_id`, and both
complete `expected_final_pools`. All sequence and successful quote expectations
include full post-Pool state, including fees, raw vault deltas, observations,
epochs, arrays and extension. Each seed has 1m/2b quotes in both directions,
positive/negative bases and all three fee modes. Budget failure inputs are
30b one-for-zero and 5b zero-for-one: each succeeds through multiple native
arrays with the budget relaxed. The admission preflight remains conservative
(count all initialized proof ticks plus one); it is not an upstream runtime
compute meter.

`rayclmm-seed-identities.json` is copied into all three services' local
`test/contracts` directories. It has top-level `program_id`, `amm_config`,
`token0`, `token1`, `config_index` and a `vectors` list. Each vector includes
`seed_index`, `seed_le`, `seed_be`, `pool_id`, `pool_bump`, the same config/mints,
`wrong_be_pool_id`, `wrong_be_pool_bump`, and `be_differs`. Flat `vault0`,
`vault1`, `observation`, `bitmap_extension` address fields accompany
`dependencies` entries `{address,bump}`; `dependencies.tick_arrays` adds
`start_tick_index`. Seeds0/1/2/255/256/257/32768/65534/65535 cover legacy,
byte order, high bits and symmetric controls. BE(1) equals LE(256), proving an
actual different pool rather than a spelling difference. Seed65535 (`ff ff`)
and257 (`01 01`) are symmetric controls and cannot distinguish endianness.

The raw fixture `integration/test/solana/rayclmm-seed-base.json`, parser local
copy, and `seed-base-fixture.json` retain top-level `pool`, `pool_config`,
`rpc_accounts`, `rpc_request_accounts` and `rpc_response`. The seven configured
business accounts precede program/ProgramData/classic-SPL/Clock, seven arrays,
and the mandatory extension in the 19-account atomic response. Metadata keys:

- `base_vectors`, `next_bank_vectors`: native 1m/2b, both directions, full Pool.
- `next_bank_fixture`: same **seed1**, same configured identity/account window;
  fee0→1 and clear the last array's extension bit plus initialized ticks.
  Four dedicated `seed1-next-bank-exact-fixture-*` corpus cases reproduce
  exactly this Pool and all four `next_bank_vectors` inputs/directions, with
  identical full poststates for business differential verification.
- `next_bank_scope_probe`: 30b one-for-zero outputs `1377715832` before the
  change, rejects after it, and still succeeds under `mode_only_expected`.
- `alternate_fixture`: actual full native Pool after baseline1m zero-for-one;
  `alternate_negative_base` provides the negative boundary proof.
- `seed_fixtures`: **list** of separate seed255/256/65535 physical-pool raw
  fixtures, each with its own actual `pool_config`, account map and response.
  They never represent mutating a seed in the same pool account.

The original native functions execute with RefMut accounts, full extension
AccountInfo and deterministic Clock syscalls. This oracle does not execute
Anchor account validation, SBF transactions, CPI, real SPL transfers, signatures,
user balances or WSOL backing. Synthetic loader/code proofs are decoder test
evidence, not deployed-bytecode identity. Dynamic fees, orders, rewards and
Token2022 remain excluded. Service/process acceptance belongs to the main
integration task; this generator starts no services or infrastructure.

## Dynamic profile and held native Clock snapshot

`raydium-clmm-dynamic`, adapter `raydium-clmm/6d7a0f82-spl-dynamic-1`,
adds active original dynamic fees to classic SPL fee modes0/1/2, seed0..65535,
and a mandatory bitmap extension. `State.dynamic_fee` is optional with serde
`default` and `skip_serializing_if = "Option::is_none"`; all four older profiles
must omit it. The nine fields are exactly `filter_period`, `decay_period`,
`reduction_factor`, `dynamic_fee_control`, `max_volatility_accumulator`,
`tick_spacing_index_reference`, `volatility_reference`, `volatility_accumulator`,
and `last_update_timestamp`. Parameter, accumulator, signed reference-index,
seed, bank-time, dependency and extension bounds are checked before execution.

The adapter constructs original `DynamicFeeInfo` inside original `PoolState`,
then invokes original `swap_internal` with RefMut accounts, extension AccountInfo
and the Clock syscall. Original `SwapState::new`, saturating time reference,
spacing prices/indices, dynamic fee ceiling, fee cap, volatility updates, tick
crossings and settlement execute unchanged. All nine dynamic storage fields are
extracted alongside the established full Pool poststate. No business math is
imported; none of the 225 archived upstream/dependency source files is changed.

```sh
python3 integration/tools/ci/rayclmm-golden.py --offline --report-only
python3 integration/tools/ci/rayclmm-token-fee-golden.py --offline --report-only
python3 integration/tools/ci/rayclmm-bitmap-golden.py --offline --report-only
python3 integration/tools/ci/rayclmm-seed-golden.py --offline --report-only
python3 integration/tools/ci/rayclmm-dynamic-golden.py --offline
python3 integration/tools/ci/rayclmm-dynamic-golden.py --offline --check
```

Use Rust1.90.0, `--locked --offline`, and only
`/tmp/xone-raydium-clmm/target2`. Native commands `generate-dynamic` and
`dynamic-fixture` accept `--timestamp <unix>` for an explicit new native Clock
snapshot. The default held snapshot is `1793245486` (October29,2026 UTC).
The Python runner accepts the same option; it does not read the wall clock.
The fixture records this scalar as `metadata.native_time_anchor` and the
corpus records it as `reference.native_time_anchor`.

The dedicated corpus has348 mathematical cases (287 successful,61 reverted),
12 same/mixed direction sequences with24 full poststates, six separately
labeled admission probes and eight native time invariance probes. It covers
filter/decay before/equal/after, future timestamp saturation, positive/negative
spacing floor, fee ceiling/cap, accumulator cap/skip, input/output fees, bitmap
group crossing, zero liquidity, counter/vault/width overflow, observation wrap,
selected missing proof and partial exhaustion. The runner regenerates both
outputs twice, replays every case and each probe/control through the JSON CLI,
and runs native tests plus Python mutation audits. Old480/968/364/464 corpora,
identity/tick/index vectors, raw fixtures, copies and lockfiles are hash pinned;
old reports refresh in static → token fee → bitmap → seed → dynamic order.

Dynamic `max_steps` uses a separate conservative admission domain: full proof
window spacing intervals plus initialized tick boundaries and final steps.
Static initialized-tick-count+1 is insufficient for dynamic intervals. All
mathematical cases have the full8192 budget and pass the spacing admission;
low budgets1/22/420 appear only in `admission_probes` with successful relaxed
native controls. These are not runtime metering or parity-budget failures.

The new raw fixture and its parser/reference copies keep top-level `pool`,
`pool_config`, `rpc_accounts`, `rpc_request_accounts` and `rpc_response`.
Nineteen atomic accounts contain the same seed1 pool and window in both banks.
Actual original packed pool bytes1096..1130 contain the nine fields in format
`<HHHIIiIIQ`; bytes1130..1176 are the dynamic struct's zero padding and
1176..1544 remain zero. The seed bytes391..393 remain u16 LE. Fixture metadata
contains `base_vectors`, `next_bank_fixture`, `next_bank_vectors`,
`next_bank_change`, `time_policy` and `clock_strategy`. Eight exact fixture
cases in the corpus match both banks'1m/2b inputs and both directions, including
identical full native poststates. The next bank changes actual control,
maximum accumulator and volatility input fields, preserving identity and mode.
Both2b amounts change;1m outputs remain unchanged because their first interval
has zero dynamic volatility after reset. This is native evidence, not a claim
that every input amount must change.

Both raw banks deliberately use `last_update_timestamp=0`. At the held snapshot
and at any later admitted time beyond decay, original `SwapState::new` resets
reference index to the current floor spacing index and volatility reference to
zero. The actual dynamic control and maximum remain active; volatility then
accumulates as the swap crosses intervals. Eight probes execute base/next banks
at T and T+3600, verifying equal amounts and full Pool equality except bank
unix time, dynamic last-update timestamp and written observation timestamps.
Observation tick cumulatives are also checked. All fixed-time corpus poststates
retain the exact Clock used by native execution.

Process policy: use actual current Clock plus matching decoded bank timestamp,
keep freshness `configMaxAge=180s`, and assert
`current.saturating_sub(last_update_timestamp) >= decay_period` in both banks
before comparing the held fixture's exact amounts. A main-process fixture
Clock rewrite does not establish full poststate equality. No future timestamp,
weakened freshness, wall-clock substitution in the native generator, or
historical-as-current source claim is needed. Time-regime corpus cases remain
fixed-time full-Pool differential evidence and are separate from process banks.

`rayclmm-dynamic-native-source.json` records ordered prior report hashes, all
harness/output/source/lock fingerprints, counts, controls and explicit boolean
false SBF/token/Anchor execution flags. Native core and Clock execute; Anchor
instruction/account validation, SBF bank execution, CPI, real SPL transfers,
signatures, user balances and WSOL backing do not. This generator starts no
services, infrastructure or main-workspace Cargo builds.
