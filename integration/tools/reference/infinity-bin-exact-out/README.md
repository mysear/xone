# Static zero-hook Infinity Bin output targets

The original pinned BinPool `amountSpecified > 0` path delegates to
`BinHelper.getAmountsIn`. This reference calls that original library path, with
explicitly seeded pools and actual original `burn` calls to create active dust
that has been removed from the liquidity tree. The two previous reference corpora
supply fixed identities/scenarios; `source-pins.json` hashes both, the existing
1115-file upstream manifest and Node package lock. Every source hash and direct
runtime version is checked before compilation/execution.

`Reference.sol` transports original BalanceDelta, bin reserves, tree membership
and protocol amount. Application Vault cash and global protocol counters are
explicit pricing projections using the original observed delta, not observed
Manager/Vault settlement. The test is not a full Manager, ERC20/native transfer,
transaction builder, hook, dynamic fee, or execution-template proof.

Prepare the pinned Node/runtime/source tree as documented under
`../singleton-cl/README.md`. From the repository root:

```
make -C integration infinity-bin-exact-out-reference
make -C integration infinity-bin-exact-out-current-check
```

The generator runs actual original-library transactions and captures complete
initialized bins, detached active dust, tree words, swap delta/fee and post-state.
It covers several bin steps/fees, both directions, cross-bin targets, full-target
liquidity reverts, continuous and ordered swaps, two fee-tier pool grids and burn
scenarios. Current-check adds service math/routing/optimizer/API, source/evidence
controls, existing protocol regressions, strict Clippy and real Chrome.

Generated corpora, reports, logs, binaries and screenshots stay ignored. Progress
and remaining tasks are recorded only in `DEVELOPMENT_PROGRESS.md`.
