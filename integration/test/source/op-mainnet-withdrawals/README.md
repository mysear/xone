# OP Mainnet block 159769967

`block-159769967.json.gz` contains the full execution block from PublicNode, the 66 receipts from Tenderly, and the USDC/WETH Pair runtime from PublicNode. The Pair account and seven storage proofs came from the OP Mainnet public RPC. The ordinary Go owner requested these values at block `0x82be41f91788d9eba982c7059d12e42475158b6bbbe3daa55490e04cfacc2e5d`. Its separate Factory proof request returned `-32603`.

Run `go test ./internal/chain/evm -run TestOPMainnet` from `chain-parser-go`. The test checks the transaction and receipt roots, Pair runtime and storage proofs. Changing the withdrawal list while retaining the same header hash exercises the body check.
