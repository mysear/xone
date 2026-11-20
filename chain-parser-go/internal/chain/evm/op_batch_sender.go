package evm

import (
	"context"
	"encoding/json"
	"errors"
	"math/big"

	"github.com/ethereum/go-ethereum/common"
)

// This deployment of the OP Mainnet SystemConfig stores batcherHash at slot 103.
// Check both runtime hashes before interpreting storage through the proxy.
const (
	opConfigProxy     = "0x229047fed2591dbec1ef1118d64f7af3db9eb290"
	opConfigProxyCode = "0x1f958654ab06a152993e7a0ae7b6dbb0d4b19265cc9337b8789fe1353bd9dc35"
	opConfigImplCode  = "0xbbdfcdc98278949febb0882c5352ab3f589ad519ea5a91a3db9daeea67b9445e"
	opConfigImplSlot  = "0x360894a13ba1a3210667c828492db98dca3e2076cc3735a920a3ca505d382bbc"
)

type OPBatchSenderProof struct {
	Proxy          json.RawMessage `json:"proxy"`
	Implementation json.RawMessage `json:"implementation"`
}

func provenAddress(account VerifiedAccountCommitment, key string) (common.Address, error) {
	raw, err := account.StorageValue(key)
	if err != nil {
		return common.Address{}, err
	}
	word, ok := new(big.Int).SetString(raw, 0)
	if !ok || word.Sign() <= 0 || word.BitLen() > 160 {
		return common.Address{}, errors.New("OP_CONFIG_ADDRESS_WORD")
	}
	return common.BigToAddress(word), nil
}

func (proof OPBatchSenderProof) sender(ctx context.Context, anchor ExecutionStateAnchor) (common.Address, error) {
	const budget = 2 << 20
	proxy, err := VerifyExecutionAccountProof(ctx, anchor, proof.Proxy, budget)
	if err != nil {
		return common.Address{}, err
	}
	if !proxy.Exists || proxy.Address != opConfigProxy || proxy.CodeHash != opConfigProxyCode {
		return common.Address{}, errors.New("OP_CONFIG_PROXY")
	}
	addr, err := provenAddress(proxy, opConfigImplSlot)
	if err != nil {
		return common.Address{}, err
	}
	implementation, err := VerifyExecutionAccountProof(ctx, anchor, proof.Implementation, budget)
	if err != nil {
		return common.Address{}, err
	}
	if !implementation.Exists || implementation.CodeHash != opConfigImplCode || common.HexToAddress(implementation.Address) != addr {
		return common.Address{}, errors.New("OP_CONFIG_IMPLEMENTATION")
	}
	return provenAddress(proxy, "0x67")
}

func opConfigAnchor(body ChainBlockBodyProof, height uint64) ExecutionStateAnchor {
	return ExecutionStateAnchor{Height: height, BlockHash: body.Block.Hash, StateRoot: body.HeaderCommitment.StateRoot, SettlingHeight: height, SettlingHash: body.Block.Hash, AccountEncodingID: "ethereum-account-rlp/v1", Authority: "native-post-execution-header-state-root"}
}
