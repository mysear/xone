# Independent singleton CL primary-source EVM oracle

Run from the XONE workspace root:

```sh
python3 integration/tools/ci/singleton-oracle-setup.py
node integration/tools/ci/singleton-oracle-test.cjs
```

The setup copies the committed package manifests into
`integration/.runtime/singleton-reference`, installs with `npm ci --ignore-scripts
--no-audit --no-fund`, downloads the pinned official repositories and their
fixed gitlink submodules, verifies every retained source SHA256, compiles the
unchanged library import closure, and executes the harnesses in a local in-process
EVM. It does not build services or start business infrastructure. All npm
versions, tarball integrity values, and transitive resolutions are in
`package-lock.json`; direct versions are solc 0.8.26, ethers 5.8.0, Ganache 7.9.2.

After the first setup, an offline rerun is:

```sh
python3 integration/tools/ci/singleton-oracle-setup.py --reuse-environment --skip-download
node integration/tools/ci/singleton-oracle-test.cjs
```

The generator still checks **every** source hash before compilation. The fetcher
can independently reverify downloads with
`python3 integration/tools/ci/singleton-reference-fetch.py`. Cached upstream
archives are reused, then their extracted files are checked against the committed
manifest. `--record-manifest` is only an explicit initial trust bootstrap and must
not be used for normal reproduction. Source URLs are derived from the repository,
revision, and path recorded for each file in `source-metadata.json`.

## Executed references and compiler boundary

- Uniswap/v4-core `bea7f6d1c11cd5dc4f0d5151ffa37a1c239feee6`:
  **original `src/libraries/Pool.sol:swap`**, its TickBitmap, SwapMath,
  SqrtPriceMath, TickMath, FullMath, LiquidityMath, fee and balance-delta libraries.
- pancakeswap/infinity-core `3c5ef1be1f71442d8bc60cc6fd11f9c677b714df`:
  **original `src/pool-cl/libraries/CLPool.sol:swap`** and native dependencies.
- Nine pinned repositories, including upstream gitlink dependencies; the source
  manifest records 1115 retained files. Only the 36 primary files in the harness
  import closures are compiled. Upstream Solidity files are never modified or
  mathematically transcribed into the wrapper.

The two Solidity harnesses call native initialize, then seed slot0 tick, active
liquidity, tick bitmap, and gross/net tick liquidity directly. They call native
swap with **negative** `amountSpecified`, zero fee override, and an explicit
coverage-bound sqrt-price limit. The wrapper only transports source balance
amounts and post-state into a result struct. It contains no swap loop or price
formula. State is transient to `eth_call`; calls start from pristine contract
storage. `sequence` instead executes three native swaps in one call with state
retained between steps. Those results are compared to per-step calls reseeded
with the previous post-state.

The fixture contains **416 cases, 208 per protocol**. The original 404 cases and
names are preserved in their original order. Twelve appended allocation cases
form **four same-direction native three-swap sequences** (two protocols × two
directions), in addition to the four existing mixed-direction continuation
sequences. Allocation cases are named
`{protocol}/allocation/{true|false}/{0..2}` and use inputs 20000000, 1000000,
3000000, always with the same direction and physical pool. Each sequence starts
from the same initial pool as its existing `sequential` counterpart. The native
sequence call retains storage, including tick crossings and fee growth, between
all three swaps; each per-step fixture then independently matches that result.
This supplies consistent sell/buy assets for allocation consumers. Sequence names
and inputs are recorded under `reference.allocation_sequences`; the report
records 12 allocation cases, four allocation sequences, and eight total native
sequences.

Both upstream Foundry configurations target solc 0.8.26 / Cancun. The original
pool-library closure also compiles unchanged for **Shanghai**, because this
closure has no manager transient-storage requirements. The generator compiles
both targets, executes the Shanghai artifacts on Ganache 7.9.2, and decodes PUSH
instructions to verify the executed creation/runtime bytecode contains no TLOAD,
TSTORE, MCOPY, BLOBHASH, or BLOBBASEFEE instructions. Cancun artifacts compile and
are hashed, but are **not executed**. This is evidence for pool-library arithmetic
and post-state, not Cancun manager execution. On the available Node 25 runtime,
Ganache reports its optional native uWS binary is unavailable and uses its JS
fallback; generation completes successfully.

## Fixture contract and rejection semantics

The identical files are `integration/test/contracts/singleton-quotes.json` and
`dex-router/test/contracts/singleton-quotes.json`. The top-level object is
`{schema_version: 1, reference: {...}, cases: [...]}`. Each case contains:

```json
{
  "name": "protocol/scenario/direction/input",
  "pool": {"singleton_cl": {"core": {}}},
  "input": "20000000",
  "zero_for_one": true,
  "tags": ["crossing-tick"],
  "expected": {
    "amount_out": "decimal",
    "consumed_in": "20000000",
    "remaining_in": "0",
    "sqrt_price_x96": "decimal",
    "tick": 0,
    "liquidity": "decimal",
    "next_state": {"sqrt_price_x96": "decimal", "tick": 0, "liquidity": "decimal"}
  },
  "reference": {
    "reverted": false,
    "amount_out": "decimal",
    "consumed_in": "20000000",
    "remaining_in": "0",
    "full_input_consumed": true,
    "partial": false,
    "zero_output": false,
    "sqrt_price_x96": "decimal",
    "tick": 0,
    "liquidity": "decimal",
    "next_state": {"sqrt_price_x96": "decimal", "tick": 0, "liquidity": "decimal"},
    "swap_fee": 3000,
    "protocol_amount": "decimal",
    "sqrt_price_limit_x96": "decimal"
  }
}
```

`expected: {"reverted": true}` means the consumer must reject the quote. It can
mean a source EVM revert, **or** a successful raw source return that is partial,
zero-output, combined-fee 100%, or has requested positive input above `2^127`.
The actual source behavior is always kept in `reference` with an explicit
`rejection_reason`. Source reverts carry their actual `revert_data`. Unknown EVM
or infrastructure errors abort generation instead of being recorded as reverts.
Do not replace actual `consumed_in` with requested input or interpret `partial`
as full exact-in success. Negative payer deltas allow input **equal to `2^127`**;
source-confirmed successful cases at that boundary are accepted. Output must be
below `2^127`.

The bitmap words are continuous and include every initialized tick. The native
swap price limit is bounded to the known bitmap window, so omitted words outside
that window are not asserted to be empty. A source return at that limit with
remaining input is recorded as partial and rejected by the consumer expectation.
The fixtures' 512-step budget exceeds all possible iterations for these windows;
the upstream swap libraries themselves have no configurable `max_steps`.

Pool IDs use ethers ABI encoding followed by Keccak256: five words for V4
(token0, token1, uint24 fee, int24 spacing, zero hooks), six for Infinity (token0,
token1, zero hooks, manager, uint24 fee, bytes32 parameters). Infinity parameters
are exactly tickSpacing shifted left 16. V4 parameters are the empty string.
Managers and tokens are canonical synthetic addresses; token0 < token1, both
nonzero. Outer pool IDs are `manager/rawPoolId`. The synthetic
`manager_code_sha256` is the hash of the **pinned manager source file**, and this
choice is labeled in fixture metadata. It is not an attestation of deployed
manager runtime code and must never be admitted as a production code pin.

## Critical primary-source audit findings

1. Both pinned maxGross functions use compressed **floor** negative MIN_TICK:
   `(2^128 - 1) / (floor(887272 / s) - floor(-887272 / s) + 1)`.
   V3's truncation toward zero differs when spacing does not divide 887272.
   Native helper calls for spacings 1, 10, 60, 200, 16383, and 32767 are asserted
   and recorded under `reference.boundary_checks`.
2. Protocol fees pack 0→1 into the low 12 bits and 1→0 into the next 12. V4's
   authoritative ProtocolFeeLibrary cap is 1000; Infinity's is 4000. Infinity's
   CLSlot0 prose still mentions 1000, so use its actual validation library.
3. Both combine the selected fee as `p + lp - floor(p*lp/1e6)`. This rounds the
   combined percentage up. The protocol cut of each step is rounded down from
   gross consumed input; with LP fee zero, all step fees go to protocol. There
   is no independent whole-input LP/protocol precharge.
4. Exact-in uses negative `amountSpecified`. The non-target branch in both pinned
   SwapMath libraries keeps `amountIn = floor(remaining*(1e6-fee)/1e6)` and takes
   the remainder as fee. V3 SwapMath instead recomputes input from the rounded
   next sqrt price. Target-reaching steps charge
   `ceil(amountIn*fee/(1e6-fee))`; do not combine per-step rounding into a single
   global fee calculation. LP below 1e6 can still combine with protocol fee to
   exactly 1e6, producing zero output.
5. Native return packing casts the consumed payer delta and received output to
   signed int128. The payer can be `-2^127`; the positive received delta cannot
   reach `2^127`. Cases cover both directions with fee 0 and 3000 at
   `2^127-1`, `2^127`, `2^127+1`, and `2^128-1`. Overflow returns actual
   `SafeCastOverflow()` selector `0x93dafdf1`.
6. Both SqrtPriceMath libraries preserve V3's token0 add-input **uint256 product
   wrap detection** and denominator overflow fallback, followed by checked
   fallback denominator addition. Arbitrary-width math that always evaluates
   the main rational formula can differ. Token0 rounds next sqrt up; token1
   exact-input increments round down, with uint160 checked output. Zero amount
   has an explicit early return; zero price/liquidity and invalid subtraction
   paths revert with the new libraries' custom errors. Unsigned amount deltas
   remain input-up/output-down. Their **signed liquidity amount-delta helpers
   reverse V3's signs** (adding liquidity returns negative currency deltas).
   These signed helpers are not called by the scoped swap path.
7. At a leftward boundary, tick becomes `tickNext - 1`, even when sqrt equals
   the next tick price. Rightward boundary uses `tickNext`. Initialized crossing
   negates liquidityNet only when moving left. Empty words and zero active
   liquidity can traverse with no consumed amount; this does not establish a
   complete positive-output quote.
8. Both original pool loops execute with their native tick crossing, fee growth,
   and balance casts. Static zero-hook library parity does not establish native
   manager execution, hooks, custom accounting, paused checks, external code
   pinning, special tokens, or settlement correctness. Direct storage seeding
   does not exercise position construction or manager initialization validation.

## Outputs and validation

The generator writes the two fixture files and
`integration/artifacts/local/singleton-golden-generation.json`, including counts,
fixture hash, source and npm-lock hashes, compiler identity, bytecode hashes,
maxGross checks, and sequence provenance. A captured successful execution is kept
in `integration/artifacts/local/singleton-golden-generation.log`. The independent
fixture test checks mirror identity, canonical shape, directional/fee/tick/word
coverage, raw input accounting, partial/zero rejection, exact `2^127` acceptance,
and post-state aliases. Allocation checks require the exact case names and input
order, consistent direction, initial pool identity, prior post-state continuation,
and the four native sequence references. No production calculation or fake V3
state is imported.
