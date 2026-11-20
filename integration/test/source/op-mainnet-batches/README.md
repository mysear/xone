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
