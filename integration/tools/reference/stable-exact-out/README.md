# Standard Balancer V2 Stable2 output targets

Pins the existing `stable-models/source-metadata.json` StableMath and imports at
Balancer V2 revision `e281ff40583c39bf1d46c322334b2b13936e0e33`. Original Vault
BalanceAllocation dependencies come from `weighted-model/source-metadata.json`.
All source files are hash checked; solc 0.7.6 is selected from the existing locked
stable-reference Node runtime.

`Harness.sol` wraps original `_calculateInvariant`, `_calcInGivenOut`, fixed point
scale/fee operations and packed cash changes. It is a math wrapper, with synthetic
model identities; it does not deploy StablePool or the Vault, transfer ERC20s, or
establish production executable transaction support. Runtime amp/scaling and
admission bounds remain the existing Stable2 model's responsibility.

The corpus includes both directions, 18/6/0 decimal scales, fractional precise
amplification, raw fee rounding, outputs above 30%, reserve boundaries, cash
overflow, evolving state, two-pool four-part grids and same-pool ordered targets.

From the repository root:

```
make -C integration stable-exact-out-reference
make -C integration stable-exact-out-current-check
```

Prepare the Node runtime using the lockfile under `../stable-models` as documented
there. The reference target fetches pinned sources; current-check regenerates data,
runs current math/routing/optimizer/API regressions and the real Chrome UI against
a loopback Router. `PLAYWRIGHT_MODULE` selects an existing Playwright module; the
bundled Codex module is discovered when available. Chrome must already be installed.

Generated corpora, reports, logs, binaries and screenshots stay in ignored paths.
Progress and remaining work are recorded only in `DEVELOPMENT_PROGRESS.md`.
