package provenance

import (
	"bytes"
	"encoding/json"

	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/common/hexutil"
	"github.com/ethereum/go-ethereum/core/types"
	"github.com/ethereum/go-ethereum/params"
)

// verifyMainnetTransactionRows cross-checks the redundant JSON representation
// when retained. Older header-only evidence carries the signed bytes separately.
func verifyMainnetTransactionRows(header *types.Header, raw json.RawMessage, txs types.Transactions) error {
	if raw == nil {
		return nil
	}
	decoder := json.NewDecoder(bytes.NewReader(raw))
	if token, err := decoder.Token(); err != nil || token != json.Delim('[') {
		return invalid("BLOCK_TRANSACTION_LIST")
	}
	blockHash := header.Hash()
	signer := types.MakeSigner(params.MainnetChainConfig, header.Number, header.Time)
	for i, tx := range txs {
		var value json.RawMessage
		if !decoder.More() || decoder.Decode(&value) != nil {
			return invalid("BLOCK_TRANSACTION_LIST")
		}
		if value[0] == '"' { // hash-only RPC response
			var hash common.Hash
			if json.Unmarshal(value, &hash) != nil || hash != tx.Hash() {
				return invalid("BLOCK_TRANSACTION_HASH")
			}
			continue
		}
		var position struct {
			Hash        *common.Hash    `json:"hash"`
			BlockHash   *common.Hash    `json:"blockHash"`
			BlockNumber *hexutil.Uint64 `json:"blockNumber"`
			Index       *hexutil.Uint64 `json:"transactionIndex"`
			From        json.RawMessage `json:"from"`
		}
		if json.Unmarshal(value, &position) != nil || position.Hash == nil || *position.Hash != tx.Hash() || position.BlockHash == nil || *position.BlockHash != blockHash || position.BlockNumber == nil || uint64(*position.BlockNumber) != header.Number.Uint64() || position.Index == nil || uint64(*position.Index) != uint64(i) {
			return invalid("BLOCK_TRANSACTION_POSITION")
		}
		var decoded types.Transaction
		if json.Unmarshal(value, &decoded) != nil || decoded.Hash() != tx.Hash() {
			return invalid("BLOCK_TRANSACTION_BODY")
		}
		if position.From != nil {
			var from *common.Address
			sender, err := types.Sender(signer, tx)
			if json.Unmarshal(position.From, &from) != nil || from == nil || err != nil || sender != *from {
				return invalid("BLOCK_TRANSACTION_SENDER")
			}
		}
	}
	if token, err := decoder.Token(); err != nil || token != json.Delim(']') {
		return invalid("BLOCK_TRANSACTION_LIST")
	}
	return nil
}
