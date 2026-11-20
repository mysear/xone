# OP Mainnet fixed inputs

## Block 159769967

`block-159769967.json.gz` contains the full execution block from PublicNode, the 66 receipts from Tenderly, and the USDC/WETH Pair runtime from PublicNode. The Pair account and seven storage proofs came from the OP Mainnet public RPC. The ordinary Go owner requested these values at block `0x82be41f91788d9eba982c7059d12e42475158b6bbbe3daa55490e04cfacc2e5d`. Its separate Factory proof request returned `-32603`.

Run `go test ./internal/chain/evm -run TestOPMainnet` from `chain-parser-go`. The test checks the transaction and receipt roots, Pair runtime and storage proofs. Changing the withdrawal list while retaining the same header hash exercises the body check.

## USDC balance at block 159770518

`usdc-159770518.json.gz` contains the execution header, five account/storage proof responses for the USDC proxy and its implementation, and both runtime bytecodes from the owner run at `0x537edb4c73a257687c2ebde3f740eef497dc34484c5bbdf33362cec0a73a3f17`. The proof endpoint was Tenderly. The pair `0x4c43646304492a925e335f2b6d840c1489f17815` held 250054139 USDC base units at this block.

From `chain-parser-go`, run `go test -count=1 ./internal/chain/evm -run 'Test(OPUSDCDelegatedBalance159770518|CommittedViewRequiresSuccessfulSingleDelegate)'`. The fixed-input regression verifies accounts against the header state root, checks both code hashes, and recalculates the balance. Removing each proven proxy slot or changing the delegate target exercises the rejection paths. The companion unit test also covers a proxy that catches a reverted child call.

## Isthmus message passer at block 159769967

`passer-159769967.json.gz` retains the `eth_getProof` result from Tenderly for `0x4200000000000000000000000000000000000016` at the same block as `block-159769967.json.gz`. Run `go test -race -count=1 ./internal/chain/evm -run TestOPMessagePasserArchiveAccountRead` from `chain-parser-go`. The ordinary contract-state reader verifies the message passer before it returns the Pair account. The regression also substitutes another proven account and checks rejection of a mismatching storage root.

## OP block 144644465 and its Ethereum origin

`origin-144644465.json.gz` contains the OP Mainnet execution block returned by `optimism-rpc.publicnode.com` and the Ethereum Mainnet execution headers for heights 23942645 and 23942644 returned by `ethereum-rpc.publicnode.com`. The first deposit identifies origin 23942644 at sequence zero. The existing Electra/Fulu beacon fixture authenticates its immediate successor.

From the repository root, run the historical check with the fixture checkpoint below. The report covers the Ethereum origin referenced by this OP block. L2 derivation requires separate batch and execution evidence.

```sh
scratch=$(mktemp -d)
gzip -dc integration/test/source/op-mainnet-withdrawals/origin-144644465.json.gz > "$scratch/origin.json"
gzip -dc integration/test/source/beacon-fork-handover/electra-fulu.json.gz > "$scratch/beacon.json"
(cd chain-parser-go && go run ./cmd/verify-op-origin -beacon-root 0xd203fe26634fb6d972b70fdc29d9684963183549cab12cae1ce60aea9aa2533c -beacon-proof "$scratch/beacon.json" -evidence "$scratch/origin.json")
```
