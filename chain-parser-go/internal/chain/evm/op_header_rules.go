package evm

import (
	"errors"

	"github.com/ethereum/go-ethereum/core/types"
	"github.com/ethereum/go-ethereum/params"
)

// These parent-independent Bedrock rules follow the pinned op-geth beacon
// engine. OP Mainnet's transition is block-number based, including its migrated
// genesis at opBedrockHeight. Legacy headers retain their existing handling.
// Parent-dependent fees, timestamps and execution outputs are outside this check.
func verifyOPMainnetHeaderRules(header *types.Header) error {
	if header.Number.Uint64() < opBedrockHeight {
		return nil
	}
	if header.Difficulty == nil || header.Difficulty.Sign() != 0 {
		return errors.New("OP_HEADER_DIFFICULTY")
	}
	if header.Nonce != (types.BlockNonce{}) {
		return errors.New("OP_HEADER_NONCE")
	}
	if header.UncleHash != types.EmptyUncleHash {
		return errors.New("OP_HEADER_UNCLE_HASH")
	}
	if header.GasLimit > params.MaxGasLimit {
		return errors.New("OP_HEADER_GAS_LIMIT")
	}
	if header.GasUsed > header.GasLimit {
		return errors.New("OP_HEADER_GAS_USED")
	}
	return nil
}
