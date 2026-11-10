# Public self-hosted frozen-bank source

Progress and remaining work are tracked in the repository DEVELOPMENT_PROGRESS.md.
This is an implementation/interface description, not a separate progress ledger.

The pinned Yellowstone commit uses Agave 4.3.0. Its bank-id block reconstruction
uses transaction/entry/sysvar conditions; it does not independently attest a
requested scope's final account bytes. The standard public Geyser interface has
bank-aware account/status callbacks, but no complete frozen-Bank scope read.
This crate therefore does NOT promote a provider's block counts, Completed,
Rooted alone, getBlock or getMultipleAccounts context into a scope manifest.

The supplied exact-version native patch installs three public open-source paths:

1. Native ledger initialization provides the actual GenesisConfig hash.
2. AccountsUpdateNotifierImpl passes every native account callback synchronously
   before transport delivery. The observer keeps bounded per-bank write versions.
3. Bank::freeze releases its commit-excluding write lock, then exports scope
   bytes through the immutable Bank::get_account, native bank_id, PoH blockhash,
   state hash and actual parent Bank. Missing callbacks/current-byte conflicts,
   wrong genesis, resource exhaustion and unknown parents refuse export.

The normal Parser writes bounded expiring `.request` session/scope records.
The observer derives the requested public account keys from these records,
reads the real frozen Bank and writes normal independent manifests atomically.
The Parser still independently verifies its admitted dependency set, exact
source/session/bank ancestry, final versions and finalized stream barrier.

The first complete native frozen scope seeds parent state. For an unobserved account, zero in
that parent snapshot is an anchor-state token rather than a claim about a
historical native write. An actual notifier callback may itself have version
zero: Agave starts the counter there, and the exact observed value is retained. Every subsequent change
must have an observed native version and independently matching Bank bytes.
Account tombstones/missing dependencies refuse the scope. The observer must be
paired with the Geyser endpoint from the same patched validator process.

No private provider or token is required. Kernel dependencies deliberately omit
the parser, RocksDB and Solana SDK, avoiding Agave's native `links=rocksdb` conflict.

## Native install

Use the exact public source tag `v4.3.0` from `anza-xyz/agave`. Source URLs and
SHA-256s are recorded in `agave-4.3.0/source-pin.json`. The checker refuses other
inputs and materializes the local crate's absolute path into Cargo dependencies.

```sh
python3 agave-4.3.0/apply.py --agave-root /path/to/agave
python3 agave-4.3.0/apply.py --agave-root /path/to/agave --apply
# Build the patched Agave validator normally using its supported toolchain.
export XONE_BANK_SOURCE_CONFIG=/path/to/native-source.json
```

SourceConfig has `chain_key`, `genesis_hash`, `source_id`, `catalog_file`,
`manifest_directory`, `max_bytes`, `max_updates`, `max_banks` (>=2), and
`max_sessions`. Its paths must be shared with the normal Parser on the self-hosted
node. Its source_id/genesis must match the configured Geyser chain exactly.

This hook is synchronous and bounded. Select a small admitted scope; no node
throughput claim follows from the controlled tests. The first parent seed and a
subsequent bank are required. Genesis/archive boot are patched explicitly;
other validator boot paths must supply the same native genesis initialization
and refuse export if it is absent. No callback is inferred from wall-clock time.

Tests exercise the native observer independently of received protobuf bytes,
including repeated writes that the pinned Geyser full block deduplicates. V2
manifests carry those native observations as a separate durable evidence channel;
consumer receipt sequence numbers still count only actual protobuf frames.
Tests then send actual gRPC frames through the normal Parser and RocksDB owner ledger.
Patch applicability is checked against the public source files. These tests do
not build/run the full patched Agave validator or establish mainnet deployment
qualification; that requires the validator's build and operating environment.

For long-running manifest retention, configure optional `refs_endpoint` to the
normal Parser's loopback `/internal/chains/{chain}/source-refs`; optional
`refs_token_file` authenticates that local control read if required. This is not
a private chain-data provider. GC refuses deletion on a failed/uncertain reference
read. The directory entry cap is derived from max_banks/max_sessions; size each
exporter bank window/session budget to cover the receiver's rotation and audit
windows. Insufficient budgets stop export rather than delete protected proofs.

Retained native banks are re-attested for new receiver sessions within the bank
and byte window; no old receiver sequence is reused. Configure that window to
cover the validator's finality lag plus receiver buffering. Initial manifest
availability is not inferred from a slot notification: the Parser keeps old raw
frames durable and waits for a qualified current manifest on the same session.
