# Pinned native Raydium CP-Swap oracle

This independent reference executes the **unchanged original program crate**, `raydium-io/raydium-cp-swap` commit `f4b9b26342afe9122d5ff67cd7a4f19ded4ac0a5`, through its `no-entrypoint` feature. It imports no XONE business/domain/math crate. The entire original archive and extracted files are retained, including its license and dependency lock. `source-metadata.json` hashes all 76 original files; the verifier compares every file with both that manifest and the archived bytes. No extraction adaptation or original source patch was necessary.

## Reproduce

From the XONE root, with the official Rust 1.90.0 toolchain installed:

```sh
python3 integration/tools/ci/raydium-reference-fetch.py
python3 integration/tools/ci/raydium-golden.py
python3 integration/tools/ci/raydium-golden.py --offline --check
```

`--offline` requires already cached Cargo crates. The default can download dependencies but always uses the reference `Cargo.lock` through `--locked`. `--check` rebuilds, runs tests, regenerates twice, checks accounting and coverage, and byte-compares the saved artifacts without rewriting them. `raydium-reference-fetch.py --fetch` restores source files only after verifying the fixed archive hash and each manifest entry. These scripts start no services and invoke no repository service builds.

The runner explicitly selects `cargo`, `rustc`, **and** `rustdoc` from Rust 1.90.0. The machine's Homebrew Rust 1.83 does not support the resolved dependencies. Build output stays in `integration/.runtime/raydium-reference/target`.

For an individual exact-input quote:

```sh
integration/.runtime/raydium-reference/target/debug/raydium-cpmm-reference quote < request.json
```

The request is `{ "pool": <PoolJSON>, "input": "100000", "zero_for_one": true }`. `fixture` emits the base fixture; `generate` emits the complete golden corpus. Every amount is serialized directly by Rust: raw balances, counters and rates remain JSON integer numbers, and Pool reserves/input/output are decimal strings. Use Python integers, Rust serde or Go `UseNumber` to read metadata. Ordinary JavaScript `JSON.parse` loses precision above `2^53`.

## Actual functions executed

The native wrapper calls these original functions:

- `PoolState::get_swap_params` (including the original price calculation and vault relationship checks), `is_creator_fee_on_input`, and `adjust_creator_fee_rate`;
- `CurveCalculator::swap_base_input`, which calls original `Fees::trading_fee`, `split_creator_fee`, `protocol_fee`, `fund_fee`, `creator_fee`, and `ConstantProductCurve::swap_base_input_without_fees`;
- `PoolState::update_fees` and `vault_amount_without_fee`, both before and after transfer accounting;
- direct boundary probes of `update_fees`, plus `settle_creator_fee` and its original share-splitting helpers;
- official Solana `Pubkey::find_program_address`, as reexported by pinned Anchor, for authority/config/pool/vault/LP/observation/ProgramData addresses.

The wrapper repeats the swap instruction's positive input/output, full consumption, integer conversion, and curve invariant checks. It applies the classic SPL Token vault balance effects with checked u64 arithmetic (`raw input += input`, `raw output -= output`). It constructs a native Solana `Clock` from the shared bank fields, and mirrors the instruction's exact `pool_state.recent_epoch = clock.epoch` assignment. This does not execute a Clock syscall. Original unwrap panics are caught and represented as rejection; no partially modified state is returned.

The full Anchor instruction, SBF, Solana bank, CPI, classic token program, observation update, signatures, owner/executable checks, account byte decoding, user balances, account locks, rent and WSOL backing are **not executed**. This is a native integer math and shared-State-API poststate oracle, not transaction validation or deployment evidence. The decoder worker independently owns RPC/account validation. Direct u128 and ledger-overflow probes deliberately include states outside reachable SPL u64 transfers and are labeled separately.

## Source semantics and bounds

Original source locations relative to `upstream/programs/cp-swap/src`:

| Fact | Source |
|---|---|
| Fixed production program ID and global authority seed | `lib.rs` |
| Input combined ceil, creator floor split, protocol/fund floors, output creator ceil | `curve/calculator.rs`, `curve/fees.rs` |
| `new_input_vault_amount` excludes *all* current input fees | `curve/calculator.rs::swap_base_input` |
| Creator modes 0/1/2 and both enable states | `states/pool.rs::is_creator_fee_on_input`, `adjust_creator_fee_rate` |
| Subtract accrued protocol + fund + creator even when creator disabled | `states/pool.rs::vault_amount_without_fee` |
| Checked u64 ledger additions (unwrap on overflow) | `states/pool.rs::update_fees` |
| Share only distributes already accrued creator amounts during collection | `states/pool.rs::settle_creator_fee`, `instructions/collect_creator_fee.rs` |
| Native wrapper checks and recent epoch assignment | `instructions/swap_base_input.rs` |
| Pool layout 637 bytes / config layout 236 bytes | `states/pool.rs`, `states/config.rs` |

At the pinned commit, the **public creation entrypoint** `upstream/programs/cp-swap/src/lib.rs:106` asserts `trade + creator < 1_000_000`; lines 107–109 also require each protocol/fund rate and their sum to be `<= 1_000_000` before calling the internal initializer. The internal `instructions/admin/create_config.rs::create_amm_config` assignments are reached through those checks. Updates preserve the combined trade/creator bound at `instructions/admin/update_config.rs:49` and `:60`, the protocol/fund bounds at `:43–44` and `:54–55`, and enforce `creator share <= 1_000_000` at `:68`. These bounds impose no positive minimum.

Exact pinned source identity: revision `f4b9b26342afe9122d5ff67cd7a4f19ded4ac0a5`; public entrypoint file SHA256 `d4151c3959d5b066d459e02f8ffcb45c842944231d68aaff659909c232639f7b`; update-config file SHA256 `4733711176053272ce83d22ec1f00cdaca05471abe38b740d32595e65998b8fb`. Both files remain byte-for-byte unchanged and are covered by `source-metadata.json` and archive verification.

The oracle wrapper intentionally accepts the broader metadata bounds `trade < 1_000_000`, `creator/share <= 1_000_000`, and `protocol + fund <= 1_000_000` to probe the original helper arithmetic. **Cases with configured trade + creator >= 1_000_000 are arithmetic probes outside the public creation/update bounds, not evidence of reachable configurations or deployed admitted states**, even when creator fees are disabled or a helper returns success. In the byte-preserved golden corpus this class comprises schedules 7, 8 and 9 and the `invalid-combined-rate-equals-denominator-branch-dependent-*` cases; classification follows the configured sum, not the effective disabled-creator rate. The broader `core_cases` and direct helper probes likewise establish only helper outcomes within their documented input domains. They do not exercise configuration admission. No swap math or expected outcome changes are implied by this classification. The original input branch with effective trade+creator equal to zero returns `None` because creator splitting divides by zero; zero-rate output branches can succeed.

Actual next reserves must come from the transferred raw balances less **new total ledgers**. LP fees remain in the vault and net reserve. For the base pool, input `100000`, direction 0→1 gives output `169489`, trade/creator/protocol/fund `250/50/30/10`, curve temporary input `1000000099700`, and actual next net input `1000000099910`. The retained `210` is the LP fee. With reserves `1000/1000`, input `10`, trade rate `2500` and creator disabled, output is `8` because the original trade fee ceils to `1`.

## Identity and shared base metadata

Production program: `CPMMoo8L3F4NbTegBCKVNunggL7H1ZpdTHKxQB5qKP1C`.

Global authority: `GpMZbSM2GgvTKHJirzeGfMFoaZ8UR2X7F4v8vHTvxFbL`, bump `253`, seeds `[b"vault_and_lp_mint_auth_seed"]`. This authority is global, not a pool-seeded PDA.

`base-fixture.json` is copied byte-for-byte to `integration/test/solana/raydium-base.json`. It exports:

- `pool`: the complete shared PoolJSON and exact plan State API;
- `pool_config`: config-compatible ID, profile, tokens, fee fields, program, ordered six accounts `[pool,vault0,vault1,config,mint0,mint1]` and expected proof identity;
- `metadata`: config index and bump, LP/observation keys, creator, loader/Clock/token IDs, layout lengths, and synthetic program/token proof bytes and hashes.

Mint keys are SHA256 of the documented strings `xone:raydium:mint:A` and `xone:raydium:mint:B`, sorted by their 32 raw bytes before PDA derivation. Config index is zero; its seed index uses **big endian u16**. Pool/config/vault/LP/observation and ProgramData addresses use the original seeds and official SDK. All account roles are distinct canonical base58 32-byte keys. The base creator key is SHA256 of `xone:raydium:creator`.

**Both code proofs are synthetic.** The exact non-executable byte strings are included in the fixture and their SHA256 hashes are labeled `synthetic_proof_identity`. The ProgramData address is an official PDA, but the fixture's deployed slot and code bytes are test identities. They do not assert the production deployment has these bytes or this source revision.

## Outputs and coverage

`integration/test/contracts/raydium-quotes.json` and `dex-router/test/contracts/raydium-quotes.json` are identical. The top-level keys are `schema_version`, `reference`, `base_pool`, `base_pools`, `cases`, `sequences`, `core_cases`, `helper_cases`.

A normal case is `{name,pool,input,zero_for_one,expected,raw_reference}`. Rejection is exactly `expected: {reverted:true}`. Success contains string `amount_out`, `reserve0`, `reserve1` and the entire new `raydium_cpmm` object. `raw_reference` preserves fees, curve temporary balances, input amount and both curve/net invariants. Metadata is carried forward unchanged except for raw balances, affected fee ledgers and `recent_epoch`.

Sequences have an initial `pool`, eight `steps` in the same case shape, and `expected_final_pool`. Every step embeds the actual previous step's full pool. Modes 0/1/2, enabled/disabled, same-0→1, same-1→0 and alternating directions produce 18 sequences and 144 chained swaps. Single cases cover both directions, all modes/enables, zero/nonzero rates and accrued ledgers, rounding thresholds, dust, exact `2^53`/u64 limits, transfer overflow, invalid metadata, and full-state rejection. `core_cases` separately record original `Some`/`None`/panic outcomes around u128 fee multiplication, ceil addition, curve multiplication/division, new-balance addition and rate addition. `helper_cases` cover all three u64 ledger additions and collection sharing.

Generation writes `integration/artifacts/local/raydium-golden-generation.json` and `.log`. The report includes compiler identity, original and harness lock hashes, every resolved dependency version and registry checksum, source/archive/harness/output hashes, exact commands, coverage counts, determinism and scope limits.

## Dependency compatibility

The unchanged upstream crate pins `anchor-lang =1.0.2` and `anchor-spl =1.0.2`. Its upstream lock selects Anchor procedural macros and `anchor-syn =1.1.2`. Fresh unconstrained resolution selected 1.2.1 macros, whose generated pausable-token initialization called APIs absent from Anchor SPL 1.0.2. Preserve this reference lock; do not run an unreviewed `cargo update`. The harness lock was seeded from the original lock, pruned to the native dependency closure and includes exact additional harness serde/JSON/SHA versions. The full original lock remains unmodified in `upstream/Cargo.lock`.
