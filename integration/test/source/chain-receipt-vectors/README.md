# Public chain receipt and body commitment evidence

This directory is a finite, read-only public RPC corpus. It does not establish consensus, trusted checkpoints, independent provider failure domains, current support for every fork, or production readiness. Sources are pinned by full upstream commit in `sources/*/pin.json`; `manifest.json` commits the retained bytes. RPC envelopes are normalized JSON; transport-response digests in `rpc.json` refer to acquisition bytes, not the pretty-printed envelope file.

## V1 own-header receipt profiles and retained before/after cases

`VerifyReceiptRoot` remains the existing strict Ethereum 0..4 verifier. `VerifyChainReceiptRoot(ctx, chainKey, block, receipts)` selects these explicit receipt rules:

| Chain key | Encoding authority and scope |
| --- | --- |
| `evm:1`, custom EVM / empty historical fixture key | Existing strict Ethereum profile; unknown native types are rejected |
| `evm:10`, `evm:8453` | op-geth: 0..4 plus deposit `0x7e`; no nonce in pre-Canyon trie encoding; when Canyon version 1 is present, nonce and version are both RLP fields; canonical quantities, no null/unknown versions |
| `evm:42161` | Nitro/geth: 0..4, `0x64`, `0x65`, `0x66`, `0x68`, `0x69`, `0x6a`; `0x78` uses unprefixed legacy RLP. `0x67` is not in the official list |
| `evm:56` | BSC standard receipt RLP; zero `gasUsed` may keep cumulative gas equal, never decrease it |
| `evm:137` | Bor standard RLP and post-Madhugiri `0x7f` (mainnet activation block 80,084,800). Deterministically derived RPC synthetic receipts are refused as `RECEIPT_BOR_SYNTHETIC_UNCOMMITTED` |
| `evm:43114` | Historical Coreth receipt root encoding, and the 0..3 list confirmed against current AvalancheGo v1.15.3's pinned libevm. V1 does not resolve Helicon successor commitments; the V2 SAE profile below does |

All profiles retain complete transaction/receipt identity, position, log, bloom, outcome, and root checks. Validating receipt RLP does not by itself authenticate transaction hashes: those hashes are not receipt RLP fields. Deposit nonces before Canyon are also not committed by the receipt trie. Unknown types are never mapped to legacy.

Nine positive public blocks, totaling 823 receipts, match both Go verification and the independent EthereumJS RLP/MPT oracle:

| Corpus | Block | Receipts |
| --- | ---: | ---: |
| Ethereum | 17,000,000 | 102 |
| Base Regolith | 1,000,000 | 1 |
| Base Canyon | 10,000,000 | 11 |
| OP Regolith | 110,000,000 | 4 |
| OP Canyon | 120,000,000 | 21 |
| Arbitrum | 100,000,000 | 4, including actual `0x6a` zero gas system receipt |
| BSC | 35,000,000 | 79 |
| Polygon | 50,000,000 | 564 |
| Avalanche historical | 10,000,000 | 37 |

Two actual public negative cases are retained; they are not counted as root matches:

- `polygon-synthetic`: block 50,000,144, 390 RPC receipts including the derived synthetic suffix. `oracle-results.json` checks the exact Bor derived hash and proves that removing that identified synthetic receipt gives the advertised root. Its logs are outside this root; the selector refuses it rather than certifying or silently dropping it. This cannot detect a provider omitting uncommitted synthetic data from both RPC lists; that coverage needs a separate authority.
- `avalanche-current`: block 99,642,059, timestamp 2026-11-07 04:02:17 UTC, seven transactions/receipts and an empty header receiptsRoot. The independently computed nonempty receipt root differs, and Go must return `RECEIPT_ROOT_MISMATCH`. AvalancheGo v1.15.3 SAE stores receipt roots in separate execution results. The strict V1 own-header rejection is preserved. V2 below authenticates these actual receipts against successor 99,642,066, and separately proves fresh 99m state.

`encoding-vectors.json` adds 17 independent constructed vectors: every listed Nitro native type, OP pre-Regolith/Regolith/Canyon field combinations including zero nonce, and native zero gas/equal cumulative cases. The actual BSC public sample does not contain zero gas receipts; its zero gas coverage is constructed and kept separate from the actual Arbitrum example.

## Optional bulk interface

```go
func (a *Adapter) ReadReceiptsWithMode(ctx context.Context, block RawBlock, mode string) ([]Receipt, error)
```

Accepted modes: `""`, `"per-tx"`, `"bulk-auto"`. Empty mode keeps the legacy per-transaction path. Bulk pins `eth_getBlockReceipts` to the block hash, uses the existing RPC wire ceiling plus aggregate retained-receipt budget, and verifies every identity and the selected chain root. Only method-not-found (`-32601`, existing `rpc.Unsupported`) permits per-transaction fallback. Null, empty/incomplete arrays, malformed data, invalid params, HTTP/network failure, budget overflow, and root mismatch do not trigger fallback. The retained 564-transaction Polygon block exercises 564 per-tx RPCs versus one bulk RPC in controlled replay.

Main has added `Chain.ReceiptsMode`, validation and ordinary/history routing. The witness consumes `a.Chain.ReceiptsMode`. Keep private `readReceipts` as per-tx so bulk fallback does not recurse. This task does not edit `adapter.go` or `config.go`.

## Explicit transaction/body commitment profile

```go
const ChainBodyCommitmentsV1 = "known-evm-header-raw-transactions-v1"
func (a *Adapter) ReadChainCommittedBlock(ctx context.Context, tag string) (RawBlock, error)
```

This opt-in profile uses `Chain.BlockIntegrity` and requires the fixed known mainnet genesis fingerprint for Ethereum, Base, OP, BSC, or Bor. One healthy endpoint is held for the complete proof, chain ID and genesis are checked, the standard header RLP hash is recomputed, each `eth_getRawTransactionByHash` payload is Keccak-bound to its txID, and the ordered raw bytes must match the header's transactionsRoot. Native bytes are opaque trie values; no geth transaction parser is asked to interpret native OP or Arbitrum envelopes. Every request, retained header/body and raw payload shares a decreasing total byte budget, and the complete operation has one deadline. Unsupported raw methods fail, with no acceptance fallback.

Actual BSC block 35,000,000 (79 raw transactions) and OP block 110,000,000 (four raw transactions, including `0x7e`) match both Go and the independent JS oracle for every txID, transactionsRoot, and standard header hash; see `body-oracle-results.json`. Tests demonstrate a coordinated block/receipt/log txHash substitution that passes the receipt-only verifier but fails this body proof.

Nitro and Coreth native header hashing is now implemented by `VerifyChainHeaderJSON` and shared by V1 and V2. Nitro follows the exact official standard Header serializer with committed send-root/ArbOS metadata; Coreth uses its ExtDataHash slot and Apricot/Granite/Helicon optional tail. V1 raw-only historical RPCs returning empty `0x` still fail; V2 explicitly reconstructs complete signed JSON and enumerated native transaction layouts. Unknown fingerprints, missing required methods/material, native header mismatch, raw tx mismatch and aggregate exhaustion all fail closed. These proofs remain anchored to RPC hashes, not independently authenticated consensus checkpoints.

Main-thread hook: admit this exact profile for the five known keys in config validation, and dispatch from `Adapter.Block` to `ReadChainCommittedBlock` **before** its existing generic nonempty-profile Ethereum dispatch. Default legacy and `ethereum-header-transactions-v1` remain unchanged. Other readers must use the resulting committed block, then the chain receipt selector. The two remaining strict global receipt calls at initial handoff were `CatalogRuntime` capture/replay in `internal/service/catalog_runtime_http.go`; both need `r.options.Chain.Key` passed to `VerifyChainReceiptRoot`. `runtime_sources.go`, history and normal Adapter calls are owned by main; witness selector changes are in this task's write set.

## V2 native coverage, SAE successor authority and archive replay

```go
const ChainNativeCommitmentsV2 = "known-evm-native-commitments-v2"

type ReceiptProofLimits struct {
    MaxBytes, MaxHeaders, MaxWindowBlocks, MaxTransactions int
    MaxLookahead uint64
}

(*Adapter).ReadNativeBlockProof(ctx, tag, limits) (ChainBlockBodyProof, error)
(*Adapter).ReadNativeSettledBlockProof(ctx, headTag, limits) (ChainBlockBodyProof, error)
(*Adapter).ReadNativeReceiptCoverage(ctx, block, receiptsMode, limits) (VerifiedReceiptCoverage, error)
(*Adapter).ReadNativeAccountProof(ctx, coverage, address, storageKeys, limits) (VerifiedAccountCommitment, error)
```

These are explicit opt-in paths using existing `Chain.BlockIntegrity`. Main must admit V2 for the seven fixed EVM mainnet fingerprints, route block and receipt reads through these APIs, and retain coverage through online/history/factory admission. Normal SAE head ingestion can explicitly select the head's already-settled execution with `ReadNativeSettledBlockProof`; it returns `ObservedHead` and selected `Block` separately. Exact historical reads never silently change requested height. Budget admission is caller supplied; tested limits are 16 MiB, 64 headers, 16 window blocks, 4096 transactions and 32-block lookahead. One selected provider and one overall deadline cover acquisition. A response-body counting transport charges even error envelopes and internal RPC retries against a shared decreasing byte budget. Bulk fallback is only method-not-found.

V2 complete JSON body verification reconstructs signed standard 0..4 transactions, native Nitro `0x6a` as `[chainId,data]`, and OP `0x7e` in the official field order. Every encoding must match txID and native-header transactionsRoot. Other enumerated native types require nonempty raw bytes; they cannot silently become legacy. Arbitrum Classic `0x78` transaction HashOverride requires its own historical identity authority and is not admitted as a Keccak txID proof. Its receipt RLP remains covered by the separate receipt codec. `VerifyChainBlockJSON` provides the finite offline body verifier.

The actual Arbitrum 100,000,000 and historical Avalanche 10,000,000 public raw methods returned `0x`; these are retained negative material observations in `body.json`. Their full public JSON blocks now prove txIDs and body roots under V2. Avalanche 99,642,059 has seven actual nonempty raw transactions and also verifies through full JSON. `native-oracle-results.json` independently recomputes header and body roots for these cases and the 389 canonical Bor body transactions.

### Actual SAE settlement windows

AvalancheGo v1.15.3 `blocks.Settles` defines the flattened receipt interval as `(parent.settledHeight, child.settledHeight]`, in block height order. The same native child header's stateRoot commits the state **after the final height** of that window. The verifier checks native header RLP hashes, contiguous ancestry, monotonic settled heights, first successor crossing the target, exact window membership, receipt identities/order/blooms, every block's transaction body, flattened receipt root and aggregate bloom. A target state anchor is available only if the target is the final height; `TargetStateAnchor()` refuses stale/intermediate state attribution.

| Retained proof | Target | Settling header | Receipt window | Receipts | Result |
| --- | ---: | ---: | --- | ---: | --- |
| `settlement.json` | 99,642,059 | 99,642,066 | 99,642,059 | 7 | Native header chain, body and receipt root match |
| `settlement-multi.json` | 99,642,060 | 99,642,068 | 99,642,060–99,642,061 | 3 | Target receipt events prove; state anchor correctly names 99,642,061 |
| `settlement-fresh.json` | 99,645,675 | 99,645,682 | 99,645,675 | 1 | Header/body/receipt root plus public WAVAX account MPT proof match |

The fresh actual EIP-1186 proof has nine nodes and verifies the five-field Coreth account RLP, including `heldXChainAsset`. `VerifyExecutionAccountProof` also checks requested storage proofs, duplicate slots, nonce, balance, storageRoot and codeHash against a correctly scoped `ExecutionStateAnchor`. Public old state at 99,642,059 was pruned (`missing trie node`); the newer public retained proof is used rather than pretending old state was supplied. Arbitrary `eth_call` output is not authenticated by this proof.

### Bor typed isolation and business qualification

`VerifyBorReceiptCoverage` verifies 389 canonical transaction/receipt members of actual block 50,000,144. The official derived final synthetic identity determines the split before root verification; there is no root-mismatch retry or arbitrary suffix deletion. The additional 41 logs are returned as `SystemObservations` with `LogsContentCommitted:false`, separate from `CanonicalReceipts`. Derived txID is identity evidence, never log-content commitment.

`coverage.SelectBusinessLogs(addresses, CanonicalTransactionEvents)` can return proven events for unrelated canonical business addresses. Matching unproved system logs cause `RECEIPT_BUSINESS_SYSTEM_EVENTS_UNPROVEN`; system-dependent pool/factory observations therefore remain unqualified. `WholeBlockEvents` and `RequireWholeBlockEvents()` remain closed for pre-Madhugiri, including when a provider omits the synthetic suffix: canonical root membership does not prove absence of omitted system logs. Post-Madhugiri native `0x7f` remains a normal root member. This preserves useful proved canonical scope while keeping all system event claims explicitly unproved until an independent execution authority is supplied.

### Activation/native semantics and replay

`ResolveNativeSemantics(chainKey, verifiedHeader)` returns `NativeSemanticsDescriptor` with fixed genesis fingerprint, source commits, concrete activation boundary, header/receipt/account encoding IDs and receipt authority. Nitro's ArbOS version is decoded from committed mixDigest bytes; the actual sample is ArbOS 10. Coreth Helicon activation is fixed at 2026-09-22 15:00 UTC; Granite, earlier Avalanche forks, BSC forks, OP/Base Canyon and later pinned forks, and Bor Madhugiri block 80,084,800 are separately identified. OP pre-Bedrock and Arbitrum Classic transaction identity authority are not relabeled as supported Nitro/Bedrock sources. `ID` is a deterministic SHA-256 of this descriptor, not a universal Ethereum string. Main must bind `coverage.NativeSemanticsID` into HistoryDatasetID/source activation and preserve it across replay.

`coverage.Evidence` carries complete `NativeReceiptEvidence`: native body, receipt, SAE successor window or Bor isolation inputs. Archive it alongside the existing raw evidence owner references. `DecodeNativeReceiptEvidence` bounds archive JSON size and rejects duplicate fields; `VerifyNativeReceiptEvidence` rechecks all commitments and requires the same native semantics ID. Serialized truth flags alone are never trusted. Main's actualReader/FactoryRuntime/history hooks need these richer values; own-header selectors should not be reapplied to the full uncommitted Bor list or the un-settled SAE target header.

## Offline verification and reproducible acquisition

Run from repository root:

```sh
node integration/test/source/chain-receipt-vectors/oracle.cjs
node integration/test/source/chain-receipt-vectors/native-oracle.cjs
go -C chain-parser-go test -race ./internal/chain/evm -run '^Test(ChainReceipts|ChainHeaders|ChainBodyCommitments|Receipt|ReadComplete|RPCBlock|SAE|Bor|Native)' -count=1
```

JS uses the existing pinned `integration/.runtime/receipt-root-reference/node_modules` (`@ethereumjs/trie@6.2.1`, `@ethereumjs/rlp@5.0.2`, `ethereum-cryptography@2.2.1`). Ordinary Go tests are offline. Read-only acquisition scripts are `capture.py`, `capture-body.py` and `capture-native.py`; they do not submit transactions or use private services/accounts/keys/tokens, and do not replace existing evidence. Old pruned account-state refreshes require a public archive retaining the original state. `manifest.py` regenerates the complete local digest inventory after source/vector updates stop. Tests enforce every retained artifact digest without weakening the check.

V2 witness hook: `(*Adapter).VerifyNativeReceiptWitnesses(ctx, coverage, mode, limits)` reacquires the native body/receipt/SAE window from every healthy configured provider under one outer deadline and aggregate proof budget. It re-verifies expected archived evidence, compares only proved canonical content and native/state/window authorities, and performs a final body-anchor read. Bor's uncommitted system observations are not promoted by agreement or used to outvote committed content. Do not call the V1 own-header witness after a V2 SAE proof. Main must attach this hook to V2 actualReader/history/factory paths alongside the existing source qualification policy.


## Normal V2 runtime integration

`Chain.NativeProof` is a typed finite configuration (`max_bytes`, `max_headers`, `max_window_blocks`, `max_transactions`, `max_lookahead`). The normal Adapter `Block`/`Header` chooses SAE's settled execution height for named head tags and retains `ObservedHead`; exact hexadecimal history tags retain their height. `ReadQualifiedReceipts` consumes the cached native body and verifies complete canonical coverage, business system-event scope and native witnesses. `ReadState`/`ReadComplete` use `TargetStateAnchor` and recheck the target and SAE settling successor under the same decreasing body/receipt/witness/final-anchor budget. The original callback and responsibility context are retained. A multi-height window proves receipt facts at each member, while its state root is used only for its final execution height.

`factoryHTTPProof` serializes `NativeEvidence` and `NativeSemanticsID` from normal callback coverage. Factory HTTP discovery and context-aware replay reverify the sidecar; replay also rechecks successor canonicality. The history source's `installNativeHistoryEvidence` hook writes a dedicated `native-history-proof/v2` artifact, held by the same responsibility group's pins. `ReplayNativeHistoryEvidence` verifies the complete successor/native input, closed pool/factory scope and actual activation dataset ID. These derived artifacts are not labelled JSON-RPC responses.

`TestNormalNative*`, `TestNativeFactoryActualPublic*` and `TestNativeHistoryActualPublic*` exercise the retained SAE 99,642,059/99,642,060 windows, Nitro 100,000,000 and Bor 50,000,144 through the ordinary entry points and owned archive replay. Their HTTP servers replay the captured public bytes; controlled pool getter responses are used only to test normal wiring. Canonical body/receipt commitments and EIP-1186 values have their explicitly stated header authority. The codec does not independently validate chain consensus or prove arbitrary `eth_call` computation. Pre-Madhugiri Bor system logs remain typed uncommitted observations; a matching business emitter closes admission, unrelated system observations preserve only canonical event coverage.


Native SPEC regression closure: every independently acquired witness coverage passes the current business-address gate before canonical agreement can qualify the source. An omitted primary synthetic suffix cannot hide a witness's actual affected system logs; unrelated observations preserve canonical scope. The native normal-state budget now includes all state RPC request/response bodies (failed attempts and failover included) plus each retained pool JSON and 256 bytes overhead. A scoped RPC `CopyWithHTTP` preserves all endpoints, policy, shared live provider state and recorder without replacing the parent Adapter/HTTP client. The original full-scope retained-pool limit remains an additional independent check. Own-header archived evidence reports its local verification cost through `ProofBytesUsed`, so replay and witnesses deduct it from their enclosing session.
