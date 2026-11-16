# Immutable WeightedPool output targets

The source manifest in `../weighted-model/source-metadata.json` pins Balancer V2
revision `e281ff40583c39bf1d46c322334b2b13936e0e33`. This reference compiles the
unaltered WeightedPool, BaseMinimalSwapInfoPool, WeightedMath, FixedPoint and
LogExpMath using the existing locked solc07/ethers/Ganache runtime.

`Harness.sol` calls the original `GIVEN_OUT` callback and original packed
BalanceAllocation cash operations. It is a test Vault facade: no ERC20 transfer,
full Vault, production deployment, unsigned transaction or wallet execution is
proved. All synthetic model identities in the generated corpus are test values.

From the repository root:

```
make -C integration weighted-exact-out-reference
make -C integration weighted-exact-out-current-check
```

The reference target verifies/downloads pinned sources with the existing fetcher.
Its Node dependencies use `integration/.runtime/stable-reference/node_modules`,
installed from `../stable-models/package-lock.json` as described in that reference.
The check target regenerates its corpus and runs math, routing, allocation,
API/plan/view/limit tests and the real Chromium frontend with a loopback Router.
Set `PLAYWRIGHT_MODULE` if Playwright is not on Node's import path; the bundled
Codex runtime is discovered when available. Chrome must already be installed.

Generated datasets, logs, reports and screenshots stay in ignored runtime/test
and local artifact directories. Test sources and reproduction commands are kept.
Progress and remaining scope are maintained in `DEVELOPMENT_PROGRESS.md`.
