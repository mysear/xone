# ManagedPool snapshot GIVEN_OUT

Uses Balancer V2 revision `e281ff40583c39bf1d46c322334b2b13936e0e33` from the
existing `balancer-multi` source manifest. All files are SHA256 checked before
compilation and after reference execution. Dependencies are the locked
`stable-models` solc07 0.7.6, ethers 5.8.0 and Ganache runtime.

The reference deploys the original ManagedPool, schedules weights and swap fees
through original public methods, reads actual `getNormalizedWeights()` and
`getSwapFeePercentage()` at each selected block, then calls original GIVEN_OUT.
The retained weights are not renormalized: compression and interpolation do not
preserve an exact sum of 1e18. The output math keeps the original 30% scaled
output guard, rounding, scaling and fee order. Guards cover disabled swaps;
protocol and management swap fees remain zero as required by this model.

`OutputVault.sol` is a test Vault facade with original BalanceAllocation cash
operations. Reference ManagedPool code uses explicitly unlimited contract size,
as did the previous legacy Managed reference. This does not establish a valid
production deployment, a full Vault, ERC20 transfer or BPT/protocol/AUM execution.
The synthetic model identities and cash vectors are explicit pricing projections.
Other managed fee/scheduled/AUM/BPT profiles are separate.

Prepare `integration/.runtime/stable-reference` from the existing lockfile and
fetch legacy Managed sources with `integration/tools/ci/balancer-multi-fetch.py`
as documented by that reference. From the repository root:

```
make -C integration managed-snapshot-exact-out-reference
make -C integration managed-snapshot-exact-out-current-check
```

Current-check regenerates the reference, verifies recorded blocks, weights, fees,
full balance vectors, sequences and grids, then checks math/routing/optimizer/API,
old static Weighted and managed-input regressions, strict Clippy, and real Chrome.
Set PLAYWRIGHT_MODULE if an existing Playwright module is not on the Node path.
Generated corpora, logs, screenshots and reports stay in ignored paths. Progress
and remaining tasks are maintained only in DEVELOPMENT_PROGRESS.md.
