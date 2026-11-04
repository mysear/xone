# Original ManagedPool nonzero BPT fee reference

This is bounded arithmetic and callback evidence for
`balancer/balancer-v2-monorepo` revision
`e281ff40583c39bf1d46c322334b2b13936e0e33`. **It is reference-only and cannot
admit a live pool on an ordinary EVM.** The compiled original runtime is 27,545
bytes, above EIP-170's 24,576-byte limit. A separate Ganache instance with the
normal code-size limit rejects its creation transaction; see
`standard-evm-deployment.json`. Unlimited contract size is enabled only for the
explicit reference run. No production or VERIFIED admission follows from this
corpus.

The original `ManagedPool.sol` and all original dependencies are hash checked
against the existing pinned manifest in `../balancer-multi/`. They are compiled
without modifications. `ManagedFeesHarness.sol` provides a minimal Vault facade
and an independent observer of original WeightedMath. A separately deployed
original ManagedPool initializes by original `onJoinPool` INIT and executes
actual original `onSwap` transactions, including the private
`_payProtocolAndManagementFees`. This is not a kernel-only oracle. Token
transfers and a complete original Vault deployment are outside the facade.

The finite corpus has **357 cases, 30 sequential same-pool sequences (120
steps), and 2 rollback guards**, covering 2, 3, 8, 20, and 38 tokens; mixed
0/6/12/18 decimals; protocol-only, management-only, both, zero, full management
share, and coincident recipients; minimum and 80% swap fees; weight/fee schedules;
zero and tiny inputs; max input ratio; uint256 arithmetic; and Vault uint112 cash.
Each successful case records a transaction hash, before/after BPT supply and
recipient balances, full raw Vault balances, separate mint event amounts, and
exact selected scaled partial pre/post invariants. Native successful zero raw
outputs are retained, including positive fee minting before output downscaling.
Router rejection of an unusable zero output belongs above this math API.

The fee invariant reconstructs gross scaled input by
`upscaledNet.divUp(ONE - swapFee)`. It uses scaled output **before** raw-token
output downscaling. Raw Vault post-swap balances cannot replace these values.
Protocol shares are bounded by 50%; management shares by 100%; raw swap fees by
`1e12..=8e17`. Same-recipient state requires both balance fields equal and adds
the combined mint to both fields. Regular swaps do not collect AUM fees; nonzero
AUM configuration does not itself invalidate the arithmetic. AUM collection,
non-initialization joins, exits, and swaps involving BPT are outside this scope.

The overflow guard sets the initialized pool's totalSupply slot, identified by
the compiler storage layout, to max uint256 and uses a tiny management fee such
that invariant-growth `k` is exactly one fixed-point unit. This reaches an actual
ERC20 BPT mint addition overflow after successful fee math. The reverted
transaction leaves supply, recipient balances, and stored Vault cash unchanged.
The other guard confirms rollback after raw Vault uint112 overflow. Storage
injection is limited to this labeled negative case.

## Reproduce and verify

From the workspace root:

```sh
node integration/tools/ci/balancer-managed-fees.cjs --reference-only
python3 integration/tools/ci/balancer-managed-fees-verify.py
python3 integration/tools/ci/balancer-managed-fees-test.py
python3 -O integration/tools/ci/balancer-managed-fees-test.py
```

The default generator refuses the oversized runtime unless `--reference-only`
is explicit. The receipt is atomically invalidated before fallible dependency
loading, pin parsing, compilation, or provider startup. It becomes successful
only after source/dependency byte hashes are rechecked and both corpus copies
are written. `passed: true` means this reference generation succeeded;
`service_admission: false` remains explicit. The generator owns and disconnects
both providers and does not import the old golden module or share its provider.

Run Rust tests with the installed Rust 1.90 compiler:

```sh
RUSTC=/Users/mac/.rustup/toolchains/1.90.0-x86_64-apple-darwin/bin/rustc \
/Users/mac/.rustup/toolchains/1.90.0-x86_64-apple-darwin/bin/cargo test \
  --manifest-path dex-router/Cargo.toml -p xone-protocol-math \
  --test balancer_managed_fees --offline
```

The test imports the math source by path so central `lib.rs` wiring is not
required. Red phases observed the missing module, the absent original-pool
corpus, and the old kernel's erroneous zero-output rejection before the new
native callback implementation went green. A warning that `pow_down` is unused
in the central library is expected until main chooses to expose the module.

## Compiler and source identity

`runtime-metadata.json` holds compiler settings, storage layout, deployed runtime
template, immutableReferences, every immutable AST name/source/type, and byte
hashes for original source and runtime dependencies. `compiler-identity.json`
is the small projection. `original-deployment.json` retains actual runtime bytes,
immutable values, addresses, pool ID, initialization transaction and BPT state
from the explicitly unrestricted reference environment. Its raw runtime SHA-256
is `c795f61fe7e2099efa00473ab29744afd520a7474941309ef761ab3ebb44323a`.
Zeroing all 15 immutable spans gives normalized template SHA-256
`f28868b14502b7918b9ed67eca9a84716a15fc25d03e48db2eef634e1745ddb3`.
These hashes identify reference bytes, not a deployable service profile.

The pinned upstream configs are retained in `upstream-config/`: default solc
0.7.1 / 9999 runs, ManagedPoolFactory override 0.7.1 / 200 runs, and the weighted
package test network's unlimited-size setting. The compiler experiment reports
are `compiler-probe-0.7.1.json` and `compiler-probe-0.7.6.json`:

| Compiler | Runs | Yul | Runtime bytes |
|---|---:|---|---:|
| 0.7.6 | 200 | on | 27545 |
| 0.7.6 | 200 | off | 28689 |
| 0.7.6 | 1 | on | 27270 |
| 0.7.6 | 1 | off | 28414 |
| 0.7.1 | 200 | on | 27857 |
| 0.7.1 | 200 | off | 29089 |
| 0.7.1 | 1 | on | 27588 |
| 0.7.1 | 1 | off | 28818 |

`compiler/soljson-v0.7.1.js` is the npm solc 0.7.1 compiler byte payload used for
the upstream-version experiment; its acquisition URL and SHA-256 are retained
in `compiler/solc-0.7.1-origin.json`. These bounded failed configurations do not
prove that no conceivable compiler configuration is deployable, or establish
the historical deployment status of this commit. A deployable protocol version
requires a separately coordinated source/runtime investigation.
