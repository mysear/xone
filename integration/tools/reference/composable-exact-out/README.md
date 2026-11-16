# Composable regular-token output targets

The fixed `e91a2b643a49856f51a648d175667c1b48cf3377` StableMath import closure is
read from the existing `balancer-multi` manifest and checked before/after use.
`source-pins.json` also commits the existing Node lockfile. Solc07 0.7.6 compiles
unchanged original `_calculateInvariant` and `_calcInGivenOut`.

`Output.sol` is a fee/scaling transport wrapper. It removes the registered BPT
balance before the invariant/solve, remaps ordinary token indices, and returns
raw balances after gross input and chosen output. Its explicit uint112 cash
checks are test boundary checks. No unlisted BalanceAllocation source is loaded;
no complete ComposableStablePool, Vault, ERC20 transfer, BPT supply, pending-fee
minting, or rate-cache execution is claimed. Model identities are synthetic.
Only ordinary-token swaps under the existing no-rate-provider model are admitted.
BPT remains in the full state vector and is never an input/output route token.

From the repository root, after preparing the existing stable-reference Node
runtime and `balancer-multi` fixed source closure:

```
make -C integration composable-exact-out-reference
make -C integration composable-exact-out-current-check
```

Cases cover 2/3/5 underlying tokens plus BPT at the first, middle and last index,
18/6/0/12 decimal scales, low/high amplification, both directions, outputs above
30%, complete target failure, evolving state, same-pool ordered allocation and
three independent two-pool grids. Current-check verifies source/corpus identity,
10 mutation controls in normal and optimized modes, Go admission, Rust math,
routing, optimizer, API/plan/view/limit checks, strict Clippy and real Chrome.
Generated corpora, logs, screenshots, reports and binaries stay ignored. Progress
and remaining work are maintained only in DEVELOPMENT_PROGRESS.md.
