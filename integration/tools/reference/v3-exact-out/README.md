# Standard V3 exact-output reference

This controlled reference admits `uniswap-v3` (official `@uniswap/v3-core@1.0.0`
Pool/Factory artifacts) and classic `pancake-v3` with its original LM pointer
zero (fixed `986847948755cba528324d41be19480731c36c2a` source). Pancake LM and
Slipstream origin-dependent models keep their separate adapters.

The test deploys original Factory/Pool programs, creates pools through the actual
factory, initializes prices and mints authentic positions through callbacks. It
never patches Pool bytecode or writes Pool storage. The self-owned test caller
funds callbacks with a controlled standard ERC20 and checks complete output and
maximum input. Successful and failed signed transactions execute in a local
`31337` EVM. Return transactions, deployed code hashes, actual callback payer
and recipient token deltas, immutable identity getters and all observed slot0,
liquidity, initialized ticks/bitmap, fee ledgers and observations are retained.

The production basic V3 model represents a **pricing projection** (price, tick,
active liquidity, proven bitmap and gross/net liquidity). It does not represent
all LP fee/outside/oracle counters or user balances. Tests compare that pricing
projection and separately verify original token flow and full observed rollback;
they do not claim full-contract state equivalence for the thin model.

Four original recursive-callback two-hop paths and two independently enumerated
fee-tier allocation grids cover routing and minimum raw input in the declared
four-part grid. Proof windows, steps, classic Token behavior, current snapshot
identity and same-factory/simple-path restrictions remain explicit.

`source-pins.json` binds official artifact bytes, npm lock integrity, the fixed
Pancake revision and compiler/runtime versions. The 1.0.0 and 1.0.1 official Pool
artifacts in the locked SDK tree are byte-identical; this reference selects the
1.0.0 nested package and records that exact identity. Solidity source bodies
remain unchanged; imports resolve only through the pinned monorepo/dependency
inventory. Newline/wrapper formatting does not create a production executor.

Run from the repository root:

```sh
node integration/tools/acceptance/v3_exact_out_reference.cjs
python3 integration/tools/acceptance/verify_v3_exact_out.py --reexecute
```

Replay accepts an absolute `--output-dir` and writes isolated `original.json` and
`corpus.json`, leaving canonical source inputs unchanged. Generated artifacts,
packages, code, logs, screenshots and banks stay ignored. Original reference
execution is not live-deployment qualification or an API-generated unsigned,
wallet-submitted or production-verified transaction.
