# Uniswap V2 complete Swap reference

Profile `uniswap-v2-complete`, adapter `uniswap-v2/core-1.0.1-full-swap-1`.
Only unchanged published Factory, Pair, Router02, WETH9 and official standard
ERC20 bytecode is executed. All 93 original package/source/LICENSE pins remain
at `../evm-execution/source-pins.json`; packages and LICENSE files are referenced
there without copying or editing. Runtime dependencies, Node executable,
package locks, registries and every new script are separately SHA256 pinned.

Run from the repository root:

```
node integration/tools/ci/v2-complete-golden.cjs
python3 integration/tools/ci/v2-complete-check.py --check
python3 integration/tools/ci/v2-complete-guards.py
python3 -OO integration/tools/ci/v2-complete-guards.py
```

The checker first invalidates `verified.json`, validates the three identical
fixture copies and source closure, regenerates the entire original-contract
history and traces into a temporary directory, and compares exact JSON bytes.
Every setup/deployment and exchange transaction includes signed calldata,
original receipt and headers (independently RLP-hashed including PREVRANDAO), complete declared pre/post account images
(nonce, native balance, code and explicit storage), and raw getter calldata /
return bytes at the same block. Raw opcode traces are losslessly gzip encoded
in `traces/`, named by their uncompressed SHA256 and independently hashed as
compressed bytes. Headers and PREVRANDAO are never normalized. Fixed genesis,
wallet seed, deterministic signed nonces and explicit timestamp increments
make the original headers reproducible. EIP170 remains enabled.

24 exchanges cover 20 successes and 4 actual Pair reverts. Four sequences
retain every intermediate complete Swap expected state. Ordinary cases use
execution timestamp equal to H; future uint32/cumulative wrap and subsequent
cases are explicitly excluded from normal current-headclock service admission.
Shared sequences retain their first source H in all derived steps, with explicit
per-step execution timestamps and original case links. Each standalone post Pool retains H's source bank fields while its observed getters are
read at the execution block H+1. Timestamp is explicitly supplied per case.

The generator never calculates a swap quote or fills expected values from
service math. Amount out is actual recipient balance delta. Router getAmountsOut
is a supplementary original EVM view. Independent validation checks observed
Swap amountIn, conservation, K, capacity and cumulative updates. Cumulative wrap
is reached by minting a 1:(2^112-1) reserve ratio and two public time advances
with sync/swap. Unsynced surplus comes from actual token transfers; sync/skim
boundaries and overflow recovery use public Pair calls. No storage writes,
custom AMM, unlimited code size, fee/rebase tokens or invented standard-token
deficit banks are used. These are synthetic local canonical histories and do
not establish deployed mainnet identity or future-block execution guarantees.
