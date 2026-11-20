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
