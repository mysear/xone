package provenance

import (
	"bytes"
	"encoding/json"
	"errors"

	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/common/hexutil"
	"github.com/ethereum/go-ethereum/core/types"
	"github.com/ethereum/go-ethereum/params"
	"github.com/ethereum/go-ethereum/trie"
)

// VerifyMainnetWithdrawals authenticates an RPC block's ordered EIP-4895 list
// against its execution header. Callers select Ethereum mainnet explicitly.
func VerifyMainnetWithdrawals(header *types.Header, raw json.RawMessage) error {
	present := len(raw) != 0 && !bytes.Equal(raw, []byte("null"))
	if !params.MainnetChainConfig.IsShanghai(header.Number, header.Time) {
		if present || header.WithdrawalsHash != nil {
			return errors.New("WITHDRAWALS_BEFORE_SHANGHAI")
		}
		return nil
	}
	if header.WithdrawalsHash == nil || !present {
		return errors.New("WITHDRAWALS_MISSING")
	}
	decoder := json.NewDecoder(bytes.NewReader(raw))
	if token, err := decoder.Token(); err != nil || token != json.Delim('[') {
		return errors.New("WITHDRAWALS_ENCODING")
	}
	var withdrawals types.Withdrawals
	for decoder.More() {
		if len(withdrawals) == 16 { // mainnet payload limit in the supported forks
			return errors.New("WITHDRAWALS_LIMIT")
		}
		// Geth's JSON decoder defaults omitted quantities to zero. Require every
		// wire field, including zero amounts and validator/index zero.
		var row struct {
			Index     *hexutil.Uint64 `json:"index"`
			Validator *hexutil.Uint64 `json:"validatorIndex"`
			Address   *common.Address `json:"address"`
			Amount    *hexutil.Uint64 `json:"amount"`
		}
		if err := decoder.Decode(&row); err != nil || row.Index == nil || row.Validator == nil || row.Address == nil || row.Amount == nil {
			return errors.New("WITHDRAWALS_ENCODING")
		}
		withdrawals = append(withdrawals, &types.Withdrawal{Index: uint64(*row.Index), Validator: uint64(*row.Validator), Address: *row.Address, Amount: uint64(*row.Amount)})
	}
	if token, err := decoder.Token(); err != nil || token != json.Delim(']') {
		return errors.New("WITHDRAWALS_ENCODING")
	}
	if types.DeriveSha(withdrawals, trie.NewStackTrie(nil)) != *header.WithdrawalsHash {
		return errors.New("WITHDRAWALS_ROOT")
	}
	return nil
}
