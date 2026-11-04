# T34.9 native shared Vault reference

This reference replays the authenticated original 28 signed transactions, normally
creates PoolB through the original ManagedPoolFactory, initializes it, and records
two physical pools using one Vault and the same two underlying tokens. PoolA uses
its original 80/20 weights; PoolB uses 50/50. All limits and chain options come
from the original reference, including EIP170 and initcode size enforcement.

The default fixture includes four native Vault swaps in A/B/A/B order and a
PoolA AUM collection after a deterministic 30 day advance. Seven common-header
banks H38–H44 each contain two complete original getter scopes (152 getters per
bank, 1,064 total). The January 1 / January 31, 2026 fixture timeline and February
1 exclusive fixture bound are constants; generation never consults wall clock.
Every pool independently observes the full ERC20 Vault wallet, while registered
balances and BPT accounting stay attached to each physical pool. Amounts expected
from swaps come from actual actor wallet differences and Vault Swap events;
collection amounts come from native BPT balance/supply differences and mint events.
The current local reference epoch uses the two existing signed admin `mint` transactions to fund the trader with 10,000 tokens per underlying for live API selection of two routes followed by native VM replay, changing the raw hash and requiring consumer pins and gates to be revalidated.

Use an absolute wrapper path to run from any directory. The commands below are
shown from the repository root. Use the pinned Node v25.3.0 binary. The requested binary
is `/usr/local/bin/node25.3.0`; if absent, `/usr/local/bin/node` must match the
original executable SHA and version. All 359 runtime files are authenticated; a
native generation also checks the exact loaded module closure.

```sh
/usr/local/bin/node integration/tools/ci/managed-v2-shared-vault-golden.cjs
/usr/local/bin/node integration/tools/ci/managed-v2-shared-vault-golden.cjs --output /tmp/shared.json
/usr/local/bin/node integration/tools/ci/managed-v2-shared-vault-golden.cjs --check
/usr/local/bin/node integration/tools/ci/managed-v2-shared-vault-golden.cjs --check --input /tmp/shared.json
/usr/local/bin/node integration/tools/ci/managed-v2-shared-vault-golden.cjs --validate /tmp/shared.json
/usr/local/bin/node integration/tools/reference/balancer-managed-v2-shared/validate.cjs /tmp/shared.json
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s tests -p test_managed_v2_shared_reference.py -v
```

Default generation writes identical `test/contracts/managed-v2-shared-vault.json`
files in `integration`, `chain-parser-go` and `dex-router`. Explicit `--output`
accepts an absolute temporary JSON path only. `--check` authenticates all three
copies, regenerates in an owned temporary directory and requires byte equality.
`--check --input` authenticates and compares exactly one explicit absolute input.
Both check modes preserve input contents and mtime. `--validate` and the standalone
validator only authenticate and decode; they never instantiate or execute Ganache.
Modes cannot be combined. Symlinks, hardlinks and custom repository output paths
are refused before any native execution or write.

The original `environment.cjs::verifyInputs()` defines the immutable input
inventory and rejects unknown members. The original manifest itself is also SHA
bound. Only its explicitly declared mutable output names are excluded by that
original authority. New script SHA records use logical repository-relative paths;
the generated raw contains the producer/validator/checker source hashes without
embedding its own hash in those sources. This avoids a source/output hash cycle.
The original bank is fully validated once per process, with its exact bytes and
input inventory checked on each use. No prototype directory, generated report,
copied compiled source or broad catalog
preservation inventory is an input.

This is a finite local original-bytecode reference, with signed transaction,
receipt, header, ABI, link, constructor, immutable, code storage and getter
observations. Offline decoding authenticates those recorded relationships; native
exact regeneration additionally re-executes the chain. Neither mode certifies a
live process, production routing, optimizer behavior, or general protocol scope.

The two ordinary admin mint transactions fund the local trader with 10,000 of each token. This permits an API order larger than either single pool's input capacity to execute as two native Vault swaps; pool reserves and original four swap outputs are unchanged.
