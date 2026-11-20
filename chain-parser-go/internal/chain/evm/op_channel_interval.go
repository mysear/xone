package evm

import (
	"context"
	"encoding/json"
	"errors"
	"math/big"

	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/core/types"
)

// Intervening blocks are full Ethereum bodies between the two inclusion
// heights, in ascending order. Authority is needed only for an inbox candidate.
type OPChannelIntervalBlock struct {
	Block     json.RawMessage     `json:"block"`
	Authority *OPBatchSenderProof `json:"authority,omitempty"`
}

// Empty bounds select the start or end of the block. Nonempty bounds identify
// the submitted transactions themselves and are excluded from the scan.
func scanOPInboxRange(ctx context.Context, body ChainBlockBodyProof, after, before string, sender common.Address, authority *OPBatchSenderProof) error {
	start, end := 0, len(body.RawTransactions)
	for i, hash := range body.Block.Transactions {
		if hash == after {
			start = i + 1
		}
		if hash == before {
			end = i
		}
	}
	if (after != "" && start == 0) || (before != "" && end == len(body.RawTransactions)) || start > end {
		return errors.New("OP_CHANNEL_INTERVAL_BOUNDARY")
	}
	for _, raw := range body.RawTransactions[start:end] {
		if err := ctx.Err(); err != nil {
			return err
		}
		var tx types.Transaction
		if err := tx.UnmarshalBinary(common.FromHex(raw)); err != nil {
			return err
		}
		if tx.To() == nil || *tx.To() != common.HexToAddress(opBatchInbox) {
			continue
		}
		if sender == (common.Address{}) {
			if authority == nil {
				return errors.New("OP_CHANNEL_INTERVAL_AUTHORITY")
			}
			h, err := canonicalIndex(body.Block.Number)
			if err != nil {
				return err
			}
			sender, err = authority.sender(ctx, opConfigAnchor(body, h))
			if err != nil {
				return err
			}
		}
		from, err := types.Sender(types.LatestSignerForChainID(big.NewInt(1)), &tx)
		if err != nil {
			return err
		}
		if from == sender {
			return errors.New("OP_CHANNEL_INTERVENING_SUBMISSION")
		}
	}
	return nil
}

func verifyOPChannelInterval(ctx context.Context, prev OPChannelSubmission, left OPBatchReport, next OPChannelSubmission, right OPBatchReport) error {
	if right.L1Number < left.L1Number {
		return errors.New("OP_CHANNEL_SUBMISSION_ORDER")
	}
	gap := right.L1Number - left.L1Number
	required := uint64(0)
	if gap > 0 {
		required = gap - 1
	}
	if required != uint64(len(next.Intervening)) {
		return errors.New("OP_CHANNEL_INTERVAL_LENGTH")
	}
	body, err := VerifyChainBlockJSON(ctx, "evm:1", next.Evidence.Block, 32<<20)
	if err != nil {
		return err
	}
	if body.Block.Hash != right.L1Hash {
		return errors.New("OP_CHANNEL_INTERVAL_LINK")
	}
	if gap == 0 {
		if left.L1Hash != right.L1Hash {
			return errors.New("OP_CHANNEL_INTERVAL_LINK")
		}
		return scanOPInboxRange(ctx, body, left.TxHash, right.TxHash, common.HexToAddress(right.Sender), nil)
	}
	if err := scanOPInboxRange(ctx, body, "", right.TxHash, common.HexToAddress(right.Sender), nil); err != nil {
		return err
	}
	expected := body.Block.ParentHash
	for i := len(next.Intervening) - 1; i >= 0; i-- {
		item := next.Intervening[i]
		body, err := VerifyChainBlockJSON(ctx, "evm:1", item.Block, 32<<20)
		if err != nil {
			return err
		}
		n, err := canonicalIndex(body.Block.Number)
		if err != nil || n != left.L1Number+1+uint64(i) || body.Block.Hash != expected {
			return errors.New("OP_CHANNEL_INTERVAL_LINK")
		}
		if err := scanOPInboxRange(ctx, body, "", "", common.Address{}, item.Authority); err != nil {
			return err
		}
		expected = body.Block.ParentHash
	}
	if expected != left.L1Hash {
		return errors.New("OP_CHANNEL_INTERVAL_LINK")
	}
	body, err = VerifyChainBlockJSON(ctx, "evm:1", prev.Evidence.Block, 32<<20)
	if err != nil {
		return err
	}
	if body.Block.Hash != expected {
		return errors.New("OP_CHANNEL_INTERVAL_LINK")
	}
	return scanOPInboxRange(ctx, body, left.TxHash, "", common.HexToAddress(left.Sender), nil)
}
