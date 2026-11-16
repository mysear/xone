# Zero-hook singleton CL output targets

This reference calls original Uniswap V4 `Pool.swap` and Pancake Infinity CL
`CLPool.swap` with positive `amountSpecified`. It uses the original fixed source
manifest and npm lockfile under `../singleton-cl`; `source-pins.json` commits their
identities and the legacy corpus used only to enumerate 46 pricing states.
All expected output results are newly executed in the original native libraries.

`ReferenceTypes.sol`, `V4Reference.sol`, and `InfinityReference.sol` transport the
original signed BalanceDelta and final price/tick/liquidity. Storage is seeded
explicitly; no original swap mathematics is copied into the wrapper. The Result
fields are `amountIn` and `remainingOut`, with target-output conservation. These
are core pricing projections: Manager settlement, ERC20/native transfer, hooks,
advanced dynamic fees, full LP fee ledgers and executable transactions are not
established by this reference.

Prepare `integration/.runtime/singleton-reference` as documented in
`../singleton-cl/README.md`, including the pinned source download. The generator
verifies every original source file and direct dependency version before use.

From the repository root:

```
node integration/tools/ci/singleton-exact-out-golden.cjs
```

It writes matching `singleton-exact-out.json` corpora to the two ignored contract
folders. It also records native mixed-direction and same-pool ordered sequences,
independent reseeded replay, two fee-tier pool grids and a generation receipt with
source/runtime hashes in ignored `integration/artifacts/local`.

Progress and remaining integration work are recorded in `DEVELOPMENT_PROGRESS.md`.

The full local acceptance target is:

```
make -C integration singleton-exact-out-current-check
```

It checks source/corpus identity, 10 mutation controls in normal and optimized
Python modes, existing Go parsing and Rust math/routes/API regressions, strict
Clippy and real Chromium. Capacity grids retain partial native outcomes and mark
those allocations infeasible; three of five four-part allocations are feasible
for each grid. No arbitrary continuous global optimum is claimed.
