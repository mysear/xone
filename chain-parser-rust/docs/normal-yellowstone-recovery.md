# Normal Yellowstone and owner scope transactions

Implementation status and remaining work are tracked only in the repository's
DEVELOPMENT_PROGRESS.md, maintained by the main thread. This document defines the
Rust interfaces, authority rules and operating constraints.

## Inputs and authority

`Config.yellowstone` is optional per chain and consumes the pinned upstream
`10f0af69d150637d8e3eaaf83b179d640988dd3c` interface. Normal ingestion requests
finalized full blocks and commitment-filtered finalized slot barriers. A public
self-hosted source needs no token; hosted token_file is optional. Original proto
files are unmodified. Transport ACK is exact durable protobuf reception, not
account completeness. Neither provider counts nor RPC context is a bank proof.

The public native source implementation is `crates/bank-source`. Its Agave
v4.3.0 patch observes native account callbacks before Geyser delivery, then reads
scope bytes from the immutable Bank after the freeze commit-excluding lock. It
checks the actual native genesis, bank ID, PoH blockhash, state hash and parent.
A standard Geyser-only process cannot supply this native read and refuses the
independent completion contract. See that crate's README and source-pin.json.

The normal subscription atomically registers an expiring `.request` containing
its source/session, current admitted catalog identity/revision and exact public
account keys. The native observer generates independent bank manifests; no
user-authored manifest or private supplier is required for the self-hosted path.
The first observed frozen bank seeds parent state. Retained immutable native
banks can be re-attested for a new receipt session. The producer bank/byte window
must cover validator finality lag and receiver buffering. During initial source
bootstrap, old raw frames without a native manifest remain durable but cannot
become ready; the same session waits for a qualified current bank. Pre-observation parent rows
use explicit anchor-state version 0, not a historical native-write claim. Later
changes require observed native versions and independently matching Bank bytes.
The native Bank destructor calls `drop_bank(bank_id)` for the precise destroyed
identity, releasing only its pending unfrozen observations. Frozen parent proof
caches survive destruction of the Bank object; integrity/resource poison is never
cleared by cleanup. Dead/root pruning reaches this hook through actual native
Arc destruction, without slot-order or root-count inference.
Native manifest V2 also contains the complete synchronous notifier write set.
The pinned provider's full block deduplicates accounts to their final version;
the assembler admits the native observations as separate authority evidence,
without inventing received gRPC frames or weakening its write-count barrier.
The consumer independently checks dependency coverage, final fingerprints,
source/session, parent ancestry, transaction assembly and finalized barrier.

## Same owner RocksDB transaction

`owner_scope::OwnerScopeStore` implements `ProductionTransactions` on the existing
Store; normal service does not publish through FileScopeStore.

- Admission persists deduplication, aggregate job/account reservations and identity.
- Claim persists attempt/session/deadline. Checkpoints use exact attempt/fence CAS
  and legal phase transitions. A reclaimed attempt restarts complete restoration.
- Prepare writes the verified publication/attempt and the exact owner Unit into
  the SAME WriteBatch as online/prepared. Prepared is never READY.
- After Kafka commit, the SAME owner apply WriteBatch writes snapshot, local
  journal, broker cursor, demand queue changes, READY ledger and commit proof,
  and removes prepared. Embedded development uses its actual local owner journal
  cursor through that same path; it does not pretend to have a Kafka transaction.
- PostgreSQL authority checks read the real registry/lease namespace, including
  owner, assignment, generation and canonical fence. READY additionally checks the
  committed journal sequence so an externally committed revocation fences old
  bootstrap state. Embedded authority checks read
  the durable current owner-session/fence row under the exclusive RocksDB handle.
  A cached renewal deadline is not sufficient authority.
- Freshness is rechecked after final I/O, including ProductionStore reads and
  bootstrap serialization. A READY read must match the committed reference,
  current authority, exact job attempt and original bank timestamp. After the
  final authority I/O, phase LIVE, receipt session, WAL generation, catalog and
  current owner reference are revalidated through a synchronous local read barrier.
- Any ambiguous write/publish result seals the owner. Reopen retires the prior
  owner session; the exact prepared outcome is reconciled against the journal
  before allocating another generation. Reconciled historical data alone is not
  live READY. A failed attempt cannot erase a possibly committed prepared intent.

`service_scope_runtime::publish_scope_unit` bridges synchronous ScopeController
publication to the owned asynchronous operation. The blocking worker sends one
publish request; the chain actor services it and remains held until commit or
bounded reconciliation outcome. HTTP cancellation cannot drop an in-flight
publication. Graceful close drains that actor before releasing writers. The current Parser
uses its shared recovery actor, so slow publication can serialize other configured
chains; per-chain parallel publication is not a property of this implementation.

## Scope selection and tail

The current admitted catalog is read dynamically. Startup restores the configured
initial pool subset (or the first admitted pool when none is selected). A request
can expand that subset from an immutable certified bank cache. Unknown/unavailable
scope is durably coalesced and admitted on a current bank, never by importing old
numeric state. The publication is a complete baseline for the selected scope.
The native request and required proof/recovery boundary use that actual target
before assembly; unrequested catalog dependencies do not gate readiness. The
full catalog identity/revision still binds every request and manifest. Full-block
transport may include unrelated accounts. V2 independently enumerates native
writes, so the consumer selects target keys and derives their exact write count,
including repeated writes. Legacy V1 has only an aggregate count: when it supplies
a larger explicitly complete scope, assembly retains that entire declared proof
rather than inventing a smaller count. No dependency is required merely because
it belongs to another admitted pool. A requested expansion absent from the
certified bank stays pending and starts a fresh receipt/native request session;
it cannot become READY from a smaller cached proof.

ManifestRecoverySource supports a real nonzero bank tail when the previous
certified bank is the actual parent, source/session and WAL generation match,
raw receipt and source-sequence deltas agree, and the entire scope is present.
It restores baseline shards, verifies those bytes, and returns the independently
certified final scope. Events count the exact receipt-cursor interval; tail bytes
use a conservative per-frame cap. Session changes, unknown ancestry or exceeded
budgets select a current-bank HEAD_FIRST baseline. There is no cross-session
write_version comparison. Initial and reconnect history gaps remain explicit and
never gate current scope readiness.

## WAL checkpoint and whole-segment retention

Only `Store::committed_wal` can mint the normal committed capability from the
actual same-batch owner proof and provenance. The capture worker checks chain,
source/session, WAL generation, cursor and record digest before checkpointing.
A segment rotates only at a completed committed bank, with no staged bank refs
or uncommitted tail. The single capture writer closes before the segment is
verified and atomically renamed into an archive. Native observation manifests are copied, digest-checked and fsynced into the
segment before releasing their active-segment references. New active segments use fresh
source sessions; archive/checkpoint state cannot restore live scope by itself.

Current snapshot, prepared Unit, admitted/running jobs and unresolved publication
intents protect sessions from eviction. Terminal job/commit audit metadata alone
cannot resume recovery and does not retain old WAL sessions forever; current
committed state remains independently protected by its snapshot reference. Segments containing protected sessions
are never removed to make a quota pass. Optional archive_retention supplies independent max_segments/max_bytes caps.
When absent, archive count uses max_stream_slots and total archive physical
file bytes use max_wal_bytes. Native sidecars count toward that physical budget.
A protected-capacity conflict fails closed. Rotation also uses a conservative
combined raw/native byte threshold and a completed-bank-count threshold. A crash during rename is reconciled
by immutable archive/active paths; no uncommitted records are discarded.
Owner ledger retention derives from max_catalog_pools and max_wal_records;
account/snapshot budgets derive from max_stream_bytes. These are explicit caps,
not RSS or throughput measurements. RocksDB physical amplification remains an
operator disk-quota concern for the active database.

The authenticated source-refs endpoint returns a coherent union of owner DB
references and ingress staging references. A native exporter configured with its
loopback refs_endpoint can remove only manifests outside its bank window and
outside that live reference set and no newer than the receiver's acknowledged
committed slot. Unavailable/uncertain receiver state does not
authorize GC. Without that endpoint the exporter retains files and stops at its
explicit directory budget instead of deleting unknown references.

## Reproducible checks and scope of evidence

Use Rust 1.90.0 through rustup (the host also has an older system rustc):

```sh
export PATH="$HOME/.cargo/bin:$PATH"
cargo +1.90.0 test --locked -p xone-solana-ingest --test owner_scope --test manifest_tail --test normal_yellowstone
cargo +1.90.0 test --locked -p xone-solana-ingest --lib --test scope_recovery --test async_frame_wal --test stream_wal
cargo +1.90.0 clippy --locked -p xone-config -p xone-solana-ingest -p xone-bank-source --all-targets -- -D warnings
cargo +1.90.0 fmt -p xone-config -p xone-solana-ingest -p xone-bank-source -- --check
```

Controlled tests use real local TCP/gRPC, normal Parser, actual RocksDB, owner
prepared recovery and the native observer's separate Bank-read fixture. Native
patch applicability is verified against the pinned public source files. Full
patched Agave binary build/run, a live PostgreSQL/Kafka failure matrix, mainnet
qualification and capacity SLOs require their respective operating environments;
the controlled checks do not establish those properties.
