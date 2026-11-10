# BSC fourth-generation Pancake LM swap reference

The current Pool, LM, MasterChef, Factory and first/second/third predecessor sources
are retained public records with original compiler settings. Acquisition receipts
bind each source response. `pancake-v3-bsc-artifacts.cjs` rebuilds the current three
contracts and compares complete immutable-normalized runtime bytes.
`pancake-bsc-predecessor-artifacts.cjs` rebuilds the predecessors and Factory;
its per-role `full_runtime_match` is separate from normalized execution-body
agreement. The first predecessor has a retained CBOR metadata mismatch, while its
executable body matches. This is not claimed as a metadata-exact reconstruction.

`pancake-bsc4-reference.cjs` executes the current original runtime and all three
original predecessor runtimes in controlled banks. The first predecessor has no
`lmTicksFlag` getter. The second exposes `OldLMPool`; the third exposes first and
second predecessor immutables. Only fourth-generation swap-relevant state is
projected into `pancake-v3-bsc-lm-v4`, field34. Unvisited local slots remain raw;
`crossLmTick` selects third, second or first and marks a local tick even when the
inherited gross liquidity is zero. `initialization` is preserved by swap.

The controlled Factory, Pool and LM identities are rebound at documented immutable
locations for local banks; these are not public historical state claims. Standard
reference Tokens and callback code provide local funding and recipient balance
observations. No wallet keys, private order flow or paid RPC credentials are
required. Public bank/consensus qualification and production transaction execution
are separate obligations. Legacy field33 admission remains unchanged.

Commands from the repository root:

- `make -C integration pancake-bsc4-reference-refresh`
- `make -C integration pancake-bsc4-differential-test`
- `make -C integration pancake-bsc4-quote-test`

The fixed original-source licensing remains GPL-2.0-or-later where declared by the
original contracts. Source records retain their per-file notices. This reference
covers swap, fee/reward accumulation and lazy crossing, not position modification
or reward claiming.
