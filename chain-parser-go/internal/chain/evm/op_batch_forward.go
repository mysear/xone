package evm

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"math/big"
	"strconv"
	"strings"

	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/core/types"
	"github.com/ethereum/go-ethereum/crypto/kzg4844"
	"xone/chain-parser-go/internal/provenance"
	"xone/chain-parser-go/internal/rpc"
)

// The index is inclusive and may point one past the last transaction. That
// cursor resumes in the next block without losing later transactions at a height.
type opScanCursor struct{ height, index uint64 }

func parseOPScanCursor(text string) (opScanCursor, error) {
	h, i, ok := strings.Cut(text, ":")
	height, eh := strconv.ParseUint(h, 10, 64)
	index, ei := strconv.ParseUint(i, 10, 64)
	if !ok || eh != nil || ei != nil || height == 0 {
		return opScanCursor{}, errors.New("OP_BATCH_SCAN_CURSOR")
	}
	return opScanCursor{height, index}, nil
}

func collectOPForward(ctx context.Context, proof *provenance.VerifiedExecutionFinality, l1, state *rpc.Client, window, budget int, cursor opScanCursor, blobsAt func(context.Context, uint64) ([]*kzg4844.Blob, error)) ([]OPChannelSubmission, error) {
	if proof == nil || window < 1 || window > 129 || budget < 1 {
		return nil, errors.New("OP_BATCH_SEARCH_WINDOW")
	}
	head, expected := proof.ExecutionBlock()
	if cursor.height > head || head-cursor.height >= uint64(window) {
		return nil, errors.New("OP_BATCH_SEARCH_WINDOW")
	}
	encodedBudget := min(budget, 32<<20)
	read := func(h uint64, full bool) (json.RawMessage, error) {
		var raw json.RawMessage
		if err := l1.Call(ctx, "eth_getBlockByNumber", []any{fmt.Sprintf("0x%x", h), full}, &raw); err != nil {
			return nil, err
		}
		if len(raw) > budget {
			return nil, errors.New("OP_BATCH_SEARCH_BUDGET")
		}
		budget -= len(raw)
		return raw, nil
	}
	// Pin every expected body hash to the signed head before scanning forwards.
	var descendants []json.RawMessage
	hashes := []string{expected}
	for h := head; h > cursor.height; h-- {
		raw, err := read(h, false)
		if err != nil {
			return nil, err
		}
		header, err := VerifyChainHeaderJSON(ctx, "evm:1", raw)
		if err != nil {
			return nil, err
		}
		if header.Block.Number != fmt.Sprintf("0x%x", h) || header.Block.Hash != expected {
			return nil, errors.New("OP_BATCH_ANCESTRY_LINK")
		}
		descendants = append(descendants, raw)
		expected = header.Block.ParentHash
		hashes = append(hashes, expected)
	}
	var submissions []OPChannelSubmission
	var between []OPChannelIntervalBlock
	var frames opFrames
	var first uint64
	for h := cursor.height; ; h++ {
		if len(submissions) > 0 && h-first > 50 {
			return nil, errors.New("OP_CHANNEL_TIMEOUT")
		}
		raw, err := read(h, true)
		if err != nil {
			return nil, err
		}
		body, err := VerifyChainBlockJSON(ctx, "evm:1", raw, 32<<20)
		if err != nil {
			return nil, err
		}
		if body.Block.Number != fmt.Sprintf("0x%x", h) || body.Block.Hash != hashes[head-h] {
			return nil, errors.New("OP_BATCH_ANCESTRY_LINK")
		}
		if h == cursor.height && cursor.index > uint64(len(body.RawTransactions)) {
			return nil, errors.New("OP_BATCH_SCAN_CURSOR")
		}
		var authority *OPBatchSenderProof
		var sender common.Address
		var blobs []*kzg4844.Blob
		before := len(submissions)
		for i, encoded := range body.RawTransactions {
			if h == cursor.height && uint64(i) < cursor.index {
				continue
			}
			if err := ctx.Err(); err != nil {
				return nil, err
			}
			var tx types.Transaction
			if err := tx.UnmarshalBinary(common.FromHex(encoded)); err != nil {
				return nil, err
			}
			if tx.To() == nil || *tx.To() != common.HexToAddress(opBatchInbox) {
				continue
			}
			if authority == nil {
				authority, sender, err = collectOPAuthority(ctx, state, body, h)
				if err != nil {
					return nil, err
				}
			}
			from, err := types.Sender(types.LatestSignerForChainID(big.NewInt(1)), &tx)
			if err != nil {
				return nil, err
			}
			if from != sender {
				continue
			}
			if tx.Type() != types.BlobTxType {
				return nil, errors.New("OP_BATCH_INBOX_TRANSACTION")
			}
			if len(submissions) >= 128 {
				return nil, errors.New("OP_CHANNEL_SUBMISSION_COUNT")
			}
			if blobs == nil {
				timestamp, err := canonicalIndex(body.Block.Timestamp)
				if err != nil || timestamp < 1606824023 || (timestamp-1606824023)%12 != 0 {
					return nil, errors.New("OP_BATCH_BEACON_SLOT")
				}
				blobs, err = blobsAt(ctx, (timestamp-1606824023)/12)
				if err != nil {
					return nil, err
				}
				if len(blobs) > budget/len(kzg4844.Blob{}) {
					return nil, errors.New("OP_BATCH_SEARCH_BUDGET")
				}
				budget -= len(blobs) * len(kzg4844.Blob{})
			}
			e := OPBatchEvidence{Block: raw, TxHash: tx.Hash().Hex(), Blobs: blobs, Descendants: descendants[:head-h], SenderProof: authority}
			report, err := VerifyOPBatchBlobs(ctx, proof, common.Address{}, e)
			if err != nil {
				return nil, err
			}
			start := report.FirstBlobIndex
			if err = frames.appendBlobs(ctx, blobs[start:start+len(report.VersionedHashes)]); err != nil {
				return nil, err
			}
			if len(submissions) == 0 {
				first = h
			}
			item := OPChannelSubmission{Evidence: e, Authority: *authority, Intervening: between}
			// The output repeats block and blob evidence for transactions sharing
			// one inclusion height. Bound that expansion before retaining the item.
			wire, err := json.Marshal(item)
			if err != nil {
				return nil, err
			}
			if len(wire) > encodedBudget {
				return nil, errors.New("OP_BATCH_OBSERVATION_FILE")
			}
			encodedBudget -= len(wire)
			submissions = append(submissions, item)
			between = nil
			if frames.closed {
				return submissions, nil
			}
		}
		if before > 0 && before == len(submissions) {
			between = append(between, OPChannelIntervalBlock{Block: raw, Authority: authority})
		}
		if h == head {
			break
		}
	}
	if len(submissions) > 0 {
		return nil, errors.New("OP_CHANNEL_INCOMPLETE")
	}
	return nil, errors.New("OP_BATCH_SEARCH_EXHAUSTED")
}
