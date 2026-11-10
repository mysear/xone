# Original Managed scheduled shared-Vault reference

This is a separate finite native observation scope for T34.11.6 M1. It replays
all 28 original signed prefix transactions with exact headers and receipts, then
creates two genuine three-token Managed pools through the pinned original
20230411 Factory. The pools share the same 18/6/8 underlying assets and Vault.
Normal EIP170/initcode limits remain enabled. There is no storage injection,
facade authorization or Go/Rust/SDK pricing oracle.

`mixed` explicitly requests scheduled Pool A and legacy static AUM Pool B;
`active` explicitly requests scheduled profiles for both pools. Old-profile
promotion is not inferred from an active getter. Each branch records start−1,
start, start+1, midpoint, end−1 and end as common-header checkpoints, with A/B/A/B
native swaps at each point and separate nonzero AUM collections for A and B.
The corpus contains 72 common-H banks, 144 physical Pool observations, 48 distinct
native swaps, four collections and 12 funded input budgets for later API split
selection. Swap expectations are actual ERC20 sender/recipient balance changes
cross-checked against original Vault Swap events. BPT expectations are actual
supply and owner/collector balances.

Every retained getter carries its exact source number/hash and canonical
original ABI input/output. Full source re-anchoring follows code, getter and
selected raw slot/mapping reads. Complete shared underlying wallets, each pool's
registered balances and separate BPT ledgers are retained. The checker binds
identities, original compiler templates/links/immutables and fixed constructor
contexts. It requires bijective native execution references, the complete
management/clock chain and unchanged same-clock sibling views. Structural
qualification is separate from fresh whole-corpus VM byte equality.

Use the original SHA-pinned Node v25.3.0 executable:

```sh
/usr/local/bin/node integration/tools/ci/managed-v2-scheduled-shared-golden.cjs
/usr/local/bin/node integration/tools/ci/managed-v2-scheduled-shared-golden.cjs --validate
/usr/local/bin/node integration/tools/ci/managed-v2-scheduled-shared-golden.cjs --check
/usr/local/bin/node integration/tools/ci/managed-v2-scheduled-shared-golden.cjs --output /tmp/new-shared-candidate.json
```

The source manifest is exact, rejects unknown files and inherits the closed
original compiler, executable and 359-file Node runtime authority from the
single-pool reference. Generation/checking uses only those registered sources.
Check/validate modes preserve input bytes and mtimes. Custom outputs are limited
to absolute temporary JSON paths outside the checkout; old raw names, aliases,
symlinks, hardlinks and unregistered repository destinations are refused.

This raw reference does not itself certify parser/math consumers, API-selected
split execution, live-chain trust, storage-trie completeness, gas model, public
VERIFIED/build, production operations or all design requirements. M2–M4 remain
separate implementation and verification steps. Read current task status only
in DEVELOPMENT_PROGRESS.md.
