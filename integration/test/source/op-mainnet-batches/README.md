# OP Mainnet batch in Ethereum block 26453043

The execution block comes from the stage 208 owner archive. The matching light-client proof uses the Ethereum Mainnet checkpoint `0x48176a8eb6476fee86b5540b0764721cb93572a1d83234db95dbedcc845907c7`. The beacon API at `ethereum-beacon-api.publicnode.com` returned the block-ordered blobs for slot 15692864 via `/eth/v1/beacon/blobs/15692864`.

The transaction at index 193 has five blob references, starting at block-wide index 3. The fixture uses expected sender `0x6887246668a3b87f54deb3b94ba47a6f63f32985`. Epoch-specific sender authorization is an input to the check. The command verifies this transaction's sender, destination and five commitments against the signed execution block.

From the repository root, run the following commands to replay the capture.

```sh
scratch=$(mktemp -d)
gzip -dc integration/test/source/op-mainnet-batches/batch-26453043.json.gz > "$scratch/batch.json"
gzip -dc integration/test/source/op-mainnet-batches/beacon-26453043.json.gz > "$scratch/beacon.json"
(cd chain-parser-go && go run ./cmd/verify-op-batch \
  -beacon-root 0x48176a8eb6476fee86b5540b0764721cb93572a1d83234db95dbedcc845907c7 \
  -beacon-proof "$scratch/beacon.json" -evidence "$scratch/batch.json" \
  -sender 0x6887246668a3b87f54deb3b94ba47a6f63f32985)
```

Add `-channel` to the same command to reassemble the five ordered frames and decode the Brotli channel. The resulting 1202848 bytes have SHA-256 `d2d00254efa34182369af0b770589a2c08e63e48f8af3cb885d111144236bdf5` and contain a single RLP-wrapped span batch. The format references are Optimism's `op-service/eth/blob.go` and `op-node/rollup/derive/channel.go` at `v1.13.5`.

To verify the span contents, decompress `l2-159769779-159769799.json.gz` into `"$scratch/l2.json"`. Add `-span-blocks "$scratch/l2.json"` to the command above. This fixture contains the full OP Mainnet `eth_getBlockByNumber` results in ascending height order. The command verifies the headers and transaction tries, reads each first deposit's L1 attributes, and reproduces every byte of the batch from the 21 blocks and 1082 sequencer transactions. The resulting last observed L2 hash is `0x0d2168d78e0515855fc96e252a67cb20d68c23d726b305d2f8cddd4265af554e`. The report covers the sequencer inputs. Authentication of the deposit contents and replay of the execution state require additional evidence.

For the authenticated L1 origins, decompress `origins-26453043-26453032.json.gz` into `"$scratch/headers.json"`. Run the span command with `-origin-headers "$scratch/headers.json"`. These twelve execution headers were extracted from the stage 208 owner archive in descending height order. The verifier checks all parent links and matches the hashes and timestamps of epochs 26453032 through 26453035. The four matches appear in `channel.span.verified_l1_origins`.

### Submission authorized by SystemConfig at block 26453870

The signed execution head is block 26453871. Its parent contains the submission. The fixture carries the child header, the full parent block and the block-ordered blobs from beacon slot 15693695. The two EIP-1186 proofs are from the submission block's post-state. The verifier authenticates the SystemConfig proxy, the EIP-1967 implementation address and its runtime code hash before reading the batcher address. The storage layout of Sourcify's matched `src/L1/SystemConfig.sol:SystemConfig` at `0xade27248542c0f3e89b49e4eb76155e267f14377` declares `batcherHash` in slot 103.

```sh
for name in batch-26453870 beacon-26453871 authority-26453870; do
  gzip -dc "integration/test/source/op-mainnet-batches/$name.json.gz" > "$scratch/$name.json"
done
(cd chain-parser-go && go run ./cmd/verify-op-batch \
  -beacon-root 0x48176a8eb6476fee86b5540b0764721cb93572a1d83234db95dbedcc845907c7 \
  -beacon-proof "$scratch/beacon-26453871.json" -evidence "$scratch/batch-26453870.json" \
  -sender-proof "$scratch/authority-26453870.json")
```

The recovered sender matches the proved address and all five versioned blob hashes match their commitments. Supplying `-sender` as well additionally requires that address to match the proved value.

### Ordered submissions for a single channel

Pass an array of `{ "evidence": <batch evidence>, "authority": <SystemConfig proof> }` objects to `-submissions` with one beacon checkpoint and finality proof. Supply the objects in L1 block and transaction order. Each object's `descendants` connects its inclusion block to the same finalized head. `-span-blocks` optionally checks the resulting span. For a reproducible single-submission replay of the same entry point, run this after the previous example.

```sh
jq -n --slurpfile evidence "$scratch/batch-26453870.json" \
  --slurpfile authority "$scratch/authority-26453870.json" \
  '[{evidence:$evidence[0],authority:$authority[0]}]' > "$scratch/submissions.json"
(cd chain-parser-go && go run ./cmd/verify-op-batch \
  -beacon-root 0x48176a8eb6476fee86b5540b0764721cb93572a1d83234db95dbedcc845907c7 \
  -beacon-proof "$scratch/beacon-26453871.json" -submissions "$scratch/submissions.json")
```

This verifies the selected submissions and ordered frames within the Granite timeout. For each subsequent submission, supply `intervening` as an ascending array of `{ "block": <full Ethereum block>, "authority": <optional SystemConfig proof> }` entries for every height strictly between the two submission blocks. The verifier checks their parent links, recomputes their transaction roots, and scans the open transaction interval, including both boundary blocks. An inbox candidate in an intermediate block requires its own authority proof. An omitted authorized inbox transaction fails the channel check. The retained live submission in this example contains the entire channel.

### Owner verification before reading pool state

Set the optional `op_l1_origin.batch_proof_file` to an observation containing `submissions` and an ordered `blocks` array. Relative paths resolve from the directory containing the configured beacon proof. Replace the file atomically when publishing the next observation. The owner reads it under its native session byte budget and checks the observation against the same signed L1 head as its RPC origin checks. Each submission needs the SystemConfig proofs for its inclusion block. The owner checks its requested L2 height and hash against the corresponding verified span member.

The sample for Ethereum submission 26453870 covers OP heights 159774755 through 159774778. Reproduce the owner check with the retained L1 headers and 24 L2 bodies from this directory.

```sh
(cd chain-parser-go && go test ./internal/chain/evm -run '^TestOPOwnerSpanMembership$' -count=1)
```

### Refresh the owner's observation from the configured L2 RPC

With `op_l1_origin.batch_proof_file` and a fresh beacon proof configured, run `chain-parser --config "$release" --refresh-op-span "$submissions"`. Supply the ordered submission array described above. The command authenticates the L1 submissions, reads the first L2 height and block count from the committed span, and requests those full bodies from the configured OP RPC. It compares every byte of the span and checks the referenced epochs against the configured L1 RPC. A successful command publishes the observation by atomic rename and prints its report. The refresh supports up to 128 L2 blocks per span and 32 MiB per observation, subject to the configured proof byte budget.

### Discover the latest finalized OP submission

An OP-only owner release can use `chain-parser --config "$release" --refresh-beacon-proof "$beacon_api"` to update its signed Ethereum observation. Run `chain-parser --config "$release" --refresh-latest-op-span "$beacon_api"` to walk backwards from that execution head. The command checks each full Ethereum body, looks for the newest authorized OP inbox blob transaction, obtains both SystemConfig account proofs, and fetches the block-ordered blobs for its beacon slot. Set `op_l1_origin.l1_proof_rpc_url` if a separate Ethereum endpoint supplies `eth_getProof`. The span refresh verifies the commitments, L2 transactions and L1 origins before publishing the owner file. This discovery mode handles complete channels in one submission. Use the submission array for a channel spanning multiple transactions.

### Replay withdrawal and recovery at an unchanged OP head

Build `chain-parser` and run the following against the stage 222 capture while its configured clock limits admit the archived block and beacon proof. The replay uses loopback RPC, a temporary copy of the channel observation and a fresh state directory.

```sh
capture=integration/artifacts/local/op-owner-run-222
python3 integration/tools/acceptance/replay_op_span_owner.py \
  --binary "$scratch/chain-parser" --config "$capture/owner-config.json" \
  --archive "$capture/complete-state/raw-evidence/evm-10" --baseline "$capture/baseline.json"
```

The test checks startup rejection with an empty observation. After a successful start, it withdraws the observation, checks the revocation journal and HTTP endpoints, then restores the original observation and checks the newly published generation. For confirmation reads that request transaction hashes, the replay projects each endpoint's own recorded full block into the hash-only response.

### Follow submissions in L1 order

For a bounded gap, use a release with a fresh signed head covering the window. Start at the first unprocessed transaction and invoke the command once per channel.

```sh
chain-parser --config "$release" --refresh-latest-op-span "$beacon_api" \
  --op-scan-from 26454408:0 > report.json
next=$(jq -r '.submissions[-1] | "\(.l1_number):\(.transaction_index+1)"' report.json)
chain-parser --config "$release" --refresh-latest-op-span "$beacon_api" \
  --op-scan-from "$next"
```

The decimal transaction index is inclusive. The start must be within the configured header window (at most 129 heights). The collector checks bodies in transaction order, gathers frames and intervening blocks, and stops at the first closed channel. An incomplete channel or an unsupported authorized submission leaves the previous observation available. When a previous file exists, the next verified span must extend its last height and full block hash. Keep the report cursor with its observation. Use a separate observation path when replaying an older window.

For Holocene L1 origins, recovery is supported at transaction boundaries: a new transaction starting at frame zero replaces an unfinished channel, orphan-only transactions without an active channel are skipped, and a channel expires after its inclusive 50-block window. Replaced evidence is discarded while cumulative response bytes (including SystemConfig proofs) and the 128-candidate allowance remain charged. A transaction mixing discarded frames and a new channel, or an invalid continuation of an active channel, remains unsupported. The selected channel still passes the full submission and intervening-transaction verifier. The recovery tests use retained mainnet blob bytes with synthetic submission boundaries; they do not establish a captured mainnet multi-submission positive.

```sh
(cd chain-parser-go && go test -race ./internal/chain/evm \
  -run 'Test(OPForward|CollectOPSubmissionChecksEveryHeight|OPChannelSubmission)' -count=1)
```

### Replay an OP owner across two consecutive spans

Use the retained stage 229 span observations, historical beacon proof and endpoint-specific RPC cache. Build the ordinary owner and choose a fresh output directory for each run. Run these commands from the repository root.

```sh
window=integration/artifacts/local/op-owner-window-229
scratch=$(mktemp -d)
(cd chain-parser-go && go build -o "$scratch/owner" ./cmd/chain-parser)
python3 integration/tools/acceptance/verify_op_owner_window.py \
  --config integration/artifacts/local/op-signed-owner-213/owner-config.json \
  --binary "$scratch/owner" --beacon "$window/beacon.json" \
  --spans "$window/159778200.span" "$window/159778224.span" \
  --first 159778222 --last 159778226 --max-age-ms 86400000 \
  --rpc-cache "$window/boundary-retry/rpc" --cache-only --out "$scratch/replay"
```

The driver maps L2 head tags to historical heights, preserves source identity and checks the owner’s HTTP journal from the initial baseline through the last commit. Compare `baseline` and `commits` in the two reports when reproducing a run. For acquisition, omit `--cache-only` to fetch missing responses from each configured endpoint. The 24-hour age budget applies only to this historical acceptance run. The owner still evaluates proof freshness against its clock.

### OP Mainnet user deposits from Ethereum receipts

The stage 231 fixture includes OP block 159778224, the descending Ethereum headers from finalized block 26454447 to origin 26454436, and the origin block’s complete receipts in `origin_receipts`. The verifier reconstructs each successful OptimismPortal deposit event in receipt and log order. It checks the full encoded transaction against the deposit prefix after the L1 attributes transaction. The report includes the verified transaction hashes for this first block of the Jovian epoch.

```sh
for name in beacon-26454447 portal-origin-26454436; do
  gzip -dc "integration/test/source/op-mainnet-batches/$name.json.gz" > "$scratch/$name.json"
done
(cd chain-parser-go && go run ./cmd/verify-op-origin \
  -beacon-root 0x48176a8eb6476fee86b5540b0764721cb93572a1d83234db95dbedcc845907c7 \
  -beacon-proof "$scratch/beacon-26454447.json" -evidence "$scratch/portal-origin-26454436.json")
```
