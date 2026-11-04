# Curve math: source identity, APIs, and native reference boundary

These are technical reconstruction notes and reproducible contract references.
No business-service math is imported by the native fixture generators. The
vendored `.vy` files are byte-for-byte original official sources. `sources.json`
records full revisions, original paths, and SHA-256. `verify_sources.py` fails
on any mismatch. Native fixture generators call it before compiling.

| Models | Official repository revision | Compiler | Native cases |
| --- | --- | --- | ---: |
| Plain3Basic, Plain4Basic, MetaUSD direct | curvefi/curve-factory `5272fd2b1199f185d9cd94822be9262c387dc2c2` | Vyper 0.3.1 | 720 |
| StableSwapNG, ERC4626 stored rates | curvefi/stableswap-ng `3332cd44656ec64b3c048885e1dd71955254b262` | Vyper 0.3.10 | 522 |
| CryptoSwap2 | curvefi/curve-crypto-contract `d7d04cd9ae038970e40be850df99de8c1ff7241b` | Vyper 0.3.3 | 288 |

All vectors include success or original native revert. 548 old-factory, 348 NG,
and 162 Crypto exchanges succeed. Successful vectors compare actual original
`exchange` output and poststate. Crypto also checks all 288 `get_dy` success/
revert outcomes (162 successes). There are 1,058 successful native exchanges.
ERC20 mocks supply transfer bookkeeping; Meta's fixed BASE_POOL address serves
only an explicit `get_virtual_price` rate. The NG lending fixture uses a real
original ERC4626-rate branch against an explicit `convertToAssets` provider
mock. Crypto starts from the original `add_liquidity`, with token/LP bookkeeping
mocks. Mocks contain no invariant/quote/fee/oracle algorithm.

## Parser and router contracts

### Plain3Basic and Plain4Basic

`curve_multi::quote_exact_in(balances: &[BigUint], amp: &BigUint,
rates: &[BigUint], fee: &BigUint, admin_fee: &BigUint, input: &BigUint,
index_in: usize, index_out: usize) -> xone_domain::Result<CurveQuote>`

`CurveQuote` has `amount_out`, `next_balances: Vec<BigUint>`, informational raw
`fee_amount`, and raw `admin_fee_amount`. Internal balances already exclude
admin fees. `amp` is current `A_precise()` (A_PRECISION=100); fee denominator
is 1e10; admin fee must be 5e9. `xp[k] = rates[k]*balances[k]/1e18`. Indices
refer to the complete coin array, not a remapped two-coin subset.

Profiles `curve-plain3-basic/factory-1` and `curve-plain4-basic/factory-1` refer
to real original implementations at the pinned factory revision. Native public
getters are `coins(uint256)`, `balances(uint256)`, `fee()`, `A_precise()`,
`admin_fee()`. Fixed rates are private and require storage reads at the same
block as every other field. `multi-layouts.json` contains compiler-derived
layouts:

| Field | Plain3Basic slot | Plain4Basic slot |
| --- | ---: | ---: |
| coins[0] | 2 | 2 |
| balances[0] | 5 | 6 |
| fee | 8 | 10 |
| initial_A | 9 | 11 |
| future_A | 10 | 12 |
| initial_A_time | 11 | 13 |
| future_A_time | 12 | 14 |
| rate_multipliers[0] | 13 | 15 |

The rate base is **2*N+7**, verified by Vyper's original layouts. For routing,
compute A_precise at quote time or ingest it directly. The module accepts
`100 <= amp < 100000000`, nonnegative fees through 1e10 and positive rates;
the source initialize function itself permits broader A/fee values than a
factory deployment profile. The dispatcher may retain its narrower deployment
fee/rate checks. Every checked arithmetic intermediate is limited to uint256.
The original NGMath probe xp=[215,130,257], amp=500 returns D=597,
while the old generic routine returns 598. Old generic D divides by `x*N` on each coin; substituting the old optimized
Plain2 D implementation changes rounding.

### MetaUSD direct and underlying boundary

`curve_multi::quote_meta_exact_in(balances, amp, meta_rate,
base_virtual_price, fee, admin_fee, input, i, j)` returns `CurveQuote`.
It models `exchange` between the meta token and base LP token. Obtain the
current base virtual price by calling the pinned MetaUSD's fixed BASE_POOL
`get_virtual_price()` at the quote block. The 72 native MetaUSD vectors use
this call with a non-unit virtual price.

`meta_underlying_boundary(i,j)` returns a typed `MetaUnderlyingBoundary`:
`BaseExchange`, `BaseDepositThenMeta`, or `MetaThenBaseWithdraw`. Those paths
require actual base-pool deposit/withdraw/exchange state, supply and fees.
They are not implemented as arbitrary precomputed output overrides. Do not
register this module as complete MetaUSD `exchange_underlying` support.

### StableSwapNG and lending-rate snapshot

`curve_ng::quote_exact_in(balances, amp, rates, fee, admin_fee,
offpeg_fee_multiplier, input, i, j) -> Result<CurveNgQuote>`

`CurveNgQuote` returns `amount_out`, effective `next_balances`, raw output-coin
`admin_fee_amount` increment, computed `fee_rate`, and pre-fee `invariant`.
The input balances are **effective LP balances**, from `get_balances()`, not
raw stored/ERC20 balances including admin fees. The source computes
`stored_balances[k] - admin_balances[k]` for standard tokens. The next effective
balance removes both output and admin increment. To replay actual storage,
raw next stored output balance removes only user output, while
admin_balances[j] increases by `admin_fee_amount`.

Additional central state required: `offpeg_fee_multiplier: BigUint` (or a
bounded u64). Existing current amp/scalings/fee/admin fields suffice otherwise.
Use `stored_rates()` from the original pool at a fixed block; it evaluates
oracle/4626 rates. Ingest `N_COINS()` and full `coins(uint256)` order. Each NG
pool has immutable rates/coin metadata; do not apply old private-slot rules.

NG's D divides by n**n after the coin loop. Dynamic fee is calculated at mean
pre/post normalized balances; original overflow/division order is retained.
The fee parameter domain is `fee <= 5e9` and
`fee*offpeg_fee_multiplier <= 5e9*1e10`. Zero exchange input reverts.

`stored_rates(multipliers, &[RateSnapshot])` implements Standard, Oracle,
Erc4626, and Rebasing rate snapshots. Oracle inputs are supplied `fetched_rate`
in 1e18 units. ERC4626 inputs are actual `convertToAssets(call_amount)` and
`scale_factor`; the original multiply/multiply/divide order is retained.
Rebasing rates are unchanged, but live balances need `balanceOf(pool) -
admin_balances` at the quote block. Static snapshots do not implement token
rebases, transfer taxes, changing external rates, or reentrant rate providers.
NG price/D EMA storage is outside this quote poststate: it does not feed this
NG implementation's swap pricing, but consumers requiring those exact oracle
outputs must execute/implement `upkeep_oracles` separately.

The ERC4626 test is lending-vault rate support. It does not establish legacy
Compound/Aave redemption/mint/exchange_underlying semantics. No such profile
should be registered based only on this module.

### CryptoSwap2

`quote_get_dy(balances, precisions, &CryptoParams, &CryptoState, input, i, j)`
returns `CryptoSwapAmounts` with `amount_out`, transfer-stage `next_balances`,
`fee_rate`, and `invariant_used`. This is original gamma/price/dynamic-fee math;
it does not assert that subsequent price/profit maintenance permits execution.

`quote_exact_in(balances, precisions, &CryptoParams, &CryptoState,
total_supply, timestamp, input, i, j) -> Result<CryptoQuote>` additionally
returns complete supported `next_state`. Precision multipliers are integer
`10**(18-decimals)` values, **not** the 1e18-scaled StableSwap rates.

`CryptoParams` fields:

- `amp`: current `A()` = ANN, A_MULTIPLIER=10000, allowed [4000, 4e9].
- `gamma`: current `gamma()`, allowed [1e10, 2e16].
- `mid_fee`, `out_fee`, `fee_gamma` from corresponding getters.
- `allowed_extra_profit`, `adjustment_step`, `admin_fee`, `ma_half_time`.

`CryptoState` fields and sources:

- `invariant` from `D()`; `price_scale` from `price_scale()`.
- `price_oracle`: **stored** private `_price_oracle`, not time-decayed
  `price_oracle()` getter. Read the compiler-derived pinned slot.
- `last_prices`, `last_prices_timestamp`, `xcp_profit`, `xcp_profit_a`,
  `virtual_price`, `future_a_gamma_time` from corresponding source fields.
- `not_adjusted`: private storage bool; see `evidence/crypto-layout.json`.

Use actual LP token `totalSupply()` and quote block timestamp. All values must
be from one block. Current A/gamma are supplied; the caller owns ramp
interpolation. Ramp D refresh and the finished-ramp sentinel are modeled.
The 288 native vectors cover static ramps only. The exchange assumes a live
pool, exact ERC20 transfers, no callback, and a valid nonzero receiver.
Killed state / min-output / token failures remain execution preconditions.

The 162 successful full native vectors match raw balances, D, price_scale,
stored price_oracle, last_prices and timestamp, virtual_price, xcp_profit,
xcp_profit_a, not_adjusted and future-ramp sentinel. Cases exercise three
gammas, two price scales, 18/18 and 18/6 precision, dust, both directions and
three elapsed times including non-integer halfpow EMA updates. Small inputs
exercise the original derivative-price Newton path.

Exchange poststate supports the **non-adjusting, no-admin-claim branch**.
Unsupported branches return distinct codes from `CryptoPoststateBoundary`:

- `CURVE_CRYPTO_BOUNDARY_PRICE_SCALE_ADJUSTMENT`
- `CURVE_CRYPTO_BOUNDARY_ADMIN_FEE_CLAIM_AND_GULP`

Do not register this as unrestricted CryptoSwap2 execution. Admin claim can
mint LP fees, gulp live ERC20 balances, and change D/virtual price; returning
only the transfer-stage balances as full poststate would be incorrect.
`newton_d`, `newton_y`, and `halfpow` are public, checked/bounded helpers;
explicit Vyper unsafe add/multiply/divide operations use uint256 wrapping or
EVM zero-division behavior as appropriate.

## Reproduction and evidence

Run from repository root, retaining existing stable-reference npm runtime and
existing `integration/.runtime/curve-vyper` Vyper 0.3.1 environment:

```sh
python3 integration/tools/reference/curve-models/verify_sources.py
node integration/tools/ci/curve-multi-golden.cjs
node integration/tools/ci/curve-ng-golden.cjs
node integration/tools/ci/curve-crypto-golden.cjs
```

The NG compiler executable is
`curve-models/.runtime/vyper-0.3.10-py313/bin/vyper`; Crypto's is
`curve-models/.runtime/vyper-0.3.3/bin/vyper`. They are separate ignored venvs,
not mutations of the old runtime. See `evidence/compiler-environments.json`
for exact executed interpreter/compiler/package versions. The installed NG
compiler runs under Python 3.13 with `--ignore-requires-python` installation
because its metadata declares Python <3.12. A normal Python 3.10/3.11 runtime
is preferable when rebuilding elsewhere; preserve the compiler version and
update the executable path in the generator. No Vyper source was patched.

The generators verify sources/compiler and write only the new
`curve-multi-quotes.json`, `curve-ng-quotes.json`, and
`curve-crypto-quotes.json` fixture names under integration/test/contracts and
dex-router/test/contracts. Existing stable fixtures are not rewritten.
Storage seeding is explicit and compiler-layout-derived. Generator digests,
layouts and TDD red/green captures are in `evidence/`. Fixtures include
source/compiler identity; stored timestamps are explicit for Crypto replay.

Bounded Rust verification (Rust 1.90):

```sh
cd dex-router
cargo test -p xone-protocol-math --test curve_multi_standalone \
  --test curve_ng_standalone --test curve_crypto_standalone
```

If a shared schema migration temporarily prevents domain compilation,
`python3 integration/tools/ci/curve-multi-test.py multi` (or `ng` / `crypto`)
links the real cached domain/BigUint libraries using rustc. It is a documented
fallback, not the final verification authority. `evidence/cargo-bounded.txt`
records the final coherent Cargo run with all 15 tests passing.
