package evm

import (
	"encoding/json"
	"errors"
	"math/big"

	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/core/types"
	"github.com/ethereum/go-ethereum/params"
)

// verifyMainnetRPCSender checks optional redundant RPC metadata. The signed
// encoding commits no separate "from" field, so transaction/root checks cannot
// authenticate that declaration. This helper applies only to Ethereum mainnet;
// native deposits and other chains retain their own transaction rules.
func verifyMainnetRPCSender(block RawBlock, tx *types.Transaction, declared json.RawMessage) error {
	if declared == nil {
		return nil // Older full-transaction rows may omit the redundant sender.
	}
	var address *common.Address
	if json.Unmarshal(declared, &address) != nil || address == nil {
		return errors.New("BLOCK_INTEGRITY_TRANSACTION_SENDER")
	}
	height, heightErr := canonicalIndex(block.Number)
	timestamp, timeErr := canonicalIndex(block.Timestamp)
	if heightErr != nil || timeErr != nil {
		return errors.New("BLOCK_INTEGRITY_TRANSACTION_SENDER")
	}
	// Frontier/Homestead signature rules and typed-transaction activation depend
	// on the actual committed height/time, not the latest supported signer.
	signer := types.MakeSigner(params.MainnetChainConfig, new(big.Int).SetUint64(height), timestamp)
	recovered, err := types.Sender(signer, tx)
	if err != nil || recovered != *address {
		return errors.New("BLOCK_INTEGRITY_TRANSACTION_SENDER")
	}
	return nil
}
