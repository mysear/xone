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
