# Original Curve NG full exchange state reference

Pinned original: `curvefi/stableswap-ng@3332cd44656ec64b3c048885e1dd71955254b262`,
`contracts/main/CurveStableSwapNG.vy`, SHA-256
`dfe8f8a4dda288c41c630ea4be9bb0e5d61244c50dfdeaf1981a098b55ee12ab`.
The source is vendored under `../curve-models/ng/`. Vyper 0.3.10 compiles the
unmodified full pool. Only ERC20 balances and transfers use mocks. No pool
business math is replaced. `original-helpers.vy` adds constructor constants and
changes only visibility on exact original `exp`, `_get_p`, and `get_D` bodies for probes.

## API

`curve_ng::quote_full_exact_in` has the original `quote_exact_in` arguments plus
`state: &NgOracleState` last. It returns `NgFullQuote` with `amount_out`, effective
`next_balances`, complete `next_state`, `admin_fee_amount`, `fee_rate`, `invariant`.
`NgOracleState` fields: `stored_balances`, `admin_balances`, `last_prices_packed`
(`Vec<BigUint>`); `last_d_packed`, `ma_last_time` (`BigUint`); `ma_exp_time`,
`d_ma_time`, `timestamp` (`u64`). Existing APIs and the old 522-case corpus remain
unchanged.

The full API requires effective balances exactly equal raw stored minus admin
balances. Raw input storage grows by input; raw output storage shrinks by user
output only. Admin output balance accrues separately. Oracle upkeep uses the
solved `x`, `y` without fees, and the original pre-swap D. It must not normalize
`next_balances` or recompute D from fee-retaining balances. EMA uses previous
packed spots/EMAs and independent price/D times. Spot prices cap at 2e18;
zero spot preserves the old packed word. No state mutation escapes on error.

Supported production profile: standard asset type 0, fixed evaluated rates,
non-rebasing exact ERC20 transfers, u64 execution timestamp and EMA windows.
Original window fields are uint256; the caller must explicitly reject values
above u64 and prove rate/token metadata at its anchored block. External rate
providers, rebasing, fee-on-transfer and optimistic `exchange_received` are not
represented by this API. The corpus includes deliberately seeded zero windows
and future times to test original unsafe-div/clock semantics; these do not
broaden the production configuration profile.

## Runtime immutables and storage

Actual deployed runtime is **23,635 bytes**. Its first **21,491 bytes** exactly
match `bytecode_runtime` emitted from the original source. The remaining
**2,144 bytes** are the immutable block. Thus its start is precisely
`runtime.len() - 2144`. `execution-closure.json` records complete deployed
runtime hashes, immutable blobs, constructor arguments and public getter facts;
`runtime-*.hex` retains the deployed bytes. Never accept a suffix decode without
also proving the original runtime prefix and source/compiler profile.

Offsets below are relative to the immutable block, in bytes. Dynamic arrays
have a 32-byte length followed by eight 32-byte element words. `uint8` values
are not byte-packed. Used and unused words are checked in the reference.

| Immutable | Offset | Length |
|---|---:|---:|
| N_COINS | 0 | 32 |
| N_COINS_128 | 32 | 32 |
| factory | 64 | 32 |
| coins | 96 | 288 |
| asset_types (private) | 384 | 288 |
| pool_contains_rebasing_tokens | 672 | 32 |
| rate_multipliers | 704 | 288 |
| rate_oracles (private) | 992 | 288 |
| call_amount | 1280 | 288 |
| scale_factor | 1568 | 288 |
| name | 1856 | 96 |
| symbol | 1952 | 64 |
| NAME_HASH | 2016 | 32 |
| CACHED_CHAIN_ID | 2048 | 32 |
| salt | 2080 | 32 |
| CACHED_DOMAIN_SEPARATOR | 2112 | 32 |

| Private storage | Length slot / scalar slot | Element slots |
|---|---:|---|
| stored_balances | 1 | 2..9 |
| admin_balances (public indexed getter) | 16 | 17..24 |
| last_prices_packed | 25 | 26..33 |
| last_D_packed | 34 | scalar |
| ma_exp_time (public) | 35 | scalar |
| D_ma_time (public) | 36 | scalar |
| ma_last_time (public) | 37 | scalar |

`get_balances()` returns effective balances; `last_price(i)` and `ema_price(i)`
expose the low/high price halves. `D_oracle()` computes the current D EMA; it
does not expose the packed private D word. The generator verifies private price
storage against both getters and private D's high half against `D_oracle()`
after upkeep. `asset_types`, `rate_oracles`, `stored_balances`,
`last_prices_packed`, `last_D_packed` have no direct getters.

## Generation, verification and corpus format

From the xone root:

```
node integration/tools/ci/curve-ng-state-startup-test.cjs
node integration/tools/ci/curve-ng-state-golden.cjs
python3 integration/tools/ci/curve-ng-state-verify.py --recompile
python3 integration/tools/ci/curve-ng-state-test.py
python3 integration/tools/ci/curve-ng-state-verify-test.py
python3 integration/tools/ci/curve-multi-test.py ng
```

At module startup, before optional dependencies load or source pins are parsed,
generation atomically replaces both receipts with `generation_complete: false`,
`status: RUNNING`. Synchronous startup errors, rejected promises and later
compilation/execution failures atomically write `FAILED` receipts. The targeted
failure test first reproduced a surviving old success receipt, then passed for
missing dependencies, invalid pin JSON, compiler subprocess failure and execution
provider failure (including owned-provider teardown). Source pin verification and all compilation precede the singleton
Ganache provider. A try/finally owns provider teardown, including failed
initialization. A corpus is accepted only with a matching completed closure;
errors cannot leave an old receipt authorizing new or partial output.

`integration/test/contracts/curve-ng-state.json` and its byte-identical
`dex-router/test/contracts/` copy contain `scenarios`. Each scenario has a
`pool` metadata object suitable for building the bounded standard-token profile.
Each `steps[]` row separates math inputs (`balances`, `rates`, `amp`, `fee`,
`admin_fee`, `offpeg_fee_multiplier`, `input`, indices), `state`, anchored
`snapshot`, transaction `exchange` input/calldata, and `expected` output/full
poststate (or revert). Decimal integers, including EMA windows, use strings;
block numbers/timestamps and indices use JSON integers. Snapshots identify the
pre-exchange block; `execution` identifies the actual exchange transaction and
block with the same chosen timestamp. Sequential state continuity is checked at
every step. Reverts have failed transaction receipts and unchanged state.

`execution-closure.json` binds source, compiler command/output, creation bytecode,
runtime template, layout, loaded Node module file hashes and lockfile, Vyper
package/dependency files, token mock source/compiler input/output, execution
environment, runtime bytes, constructor arguments, all scenarios and fixture
hash. `compiler-artifact.json` retains creation bytecode, runtime template, ABI
and complete compiler layout. The offline verifier fails on mismatched source,
generator, fixture copies, compilation, runtime/immutable bytes or dependencies.
`startup-red-test.log` records the reproduced startup authorization bug.
`red-test.log` records the failing test before the new API was implemented.
The frozen corpus has 21 scenarios and 147 actual exchange transactions (144
successful, 3 reverted), plus signed exp, marginal-price and original internal D
probes. The controlled replay base timestamp is January 1, 2026; this corpus is
historical replay evidence and does not claim live snapshot freshness.

These are controlled local original-code executions, not evidence of an
arbitrary live pool's deployment identity. A real pool still needs independently
anchored code/source identity and original runtime/layout compatibility.
