package evm

import (
	"context"
	"crypto/sha256"
	"encoding/json"
	"errors"
	"math/big"

	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/core/types"
	"github.com/ethereum/go-ethereum/crypto/kzg4844"
	"xone/chain-parser-go/internal/provenance"
)

const opBatchInbox = "0xff00000000000000000000000000000000000010"

type OPBatchEvidence struct {
	Block  json.RawMessage `json:"block"`
	TxHash string          `json:"tx_hash"`
	Blobs  []*kzg4844.Blob `json:"blobs"`
}

type OPBatchReport struct {
	Channel         *OPChannelReport `json:"channel,omitempty"`
	L1Number        uint64           `json:"l1_number"`
	L1Hash          string           `json:"l1_hash"`
	TxHash          string           `json:"tx_hash"`
	Sender          string           `json:"sender"`
	FirstBlobIndex  int              `json:"first_blob_index"`
	VersionedHashes []common.Hash    `json:"versioned_hashes"`
}

// VerifyOPBatchBlobs authenticates an inbox submission in the signed L1 head.
// The expected batch sender comes from the caller's epoch-specific policy.
func VerifyOPBatchBlobs(ctx context.Context, finality *provenance.VerifiedExecutionFinality, expectedSender common.Address, e OPBatchEvidence) (OPBatchReport, error) {
	return verifyOPBatch(ctx, finality, expectedSender, e, false)
}

func VerifyOPBatchChannel(ctx context.Context, finality *provenance.VerifiedExecutionFinality, expectedSender common.Address, e OPBatchEvidence) (OPBatchReport, error) {
	return verifyOPBatch(ctx, finality, expectedSender, e, true)
}

func verifyOPBatch(ctx context.Context, finality *provenance.VerifiedExecutionFinality, expectedSender common.Address, e OPBatchEvidence, channel bool) (OPBatchReport, error) {
	var empty OPBatchReport
	if finality == nil || expectedSender == (common.Address{}) || !receiptHex(e.TxHash, 32) {
		return empty, errors.New("OP_BATCH_IDENTITY")
	}
	body, err := VerifyChainBlockJSON(ctx, "evm:1", e.Block, 32<<20)
	if err != nil {
		return empty, err
	}
	head, hash := finality.ExecutionBlock()
	height, err := canonicalIndex(body.Block.Number)
	if err != nil || head == 0 || height != head || body.Block.Hash != hash {
		return empty, errors.New("OP_BATCH_FINALIZED_L1_BLOCK")
	}
	first := 0
	for i, raw := range body.RawTransactions {
		if err = ctx.Err(); err != nil {
			return empty, err
		}
		var tx types.Transaction
		if err = tx.UnmarshalBinary(common.FromHex(raw)); err != nil {
			return empty, err
		}
		hashes := tx.BlobHashes()
		if body.Block.Transactions[i] != e.TxHash {
			first += len(hashes)
			continue
		}
		if tx.Type() != types.BlobTxType || tx.ChainId().Cmp(big.NewInt(1)) != 0 || tx.To() == nil || *tx.To() != common.HexToAddress(opBatchInbox) {
			return empty, errors.New("OP_BATCH_INBOX_TRANSACTION")
		}
		sender, err := types.Sender(types.NewCancunSigner(big.NewInt(1)), &tx)
		if err != nil || sender != expectedSender {
			return empty, errors.New("OP_BATCH_SENDER")
		}
		if err = verifyOPBatchData(ctx, first, hashes, e.Blobs); err != nil {
			return empty, err
		}
		var decoded *OPChannelReport
		if channel {
			if timestamp, err := canonicalIndex(body.Block.Timestamp); err != nil || timestamp < opFjordTime {
				return empty, errors.New("OP_CHANNEL_FJORD_REQUIRED")
			}
			decoded, err = decodeOPChannel(ctx, e.Blobs[first:first+len(hashes)])
			if err != nil {
				return empty, err
			}
		}
		return OPBatchReport{Channel: decoded, L1Number: height, L1Hash: hash, TxHash: e.TxHash, Sender: sender.Hex(), FirstBlobIndex: first, VersionedHashes: hashes}, nil
	}
	return empty, errors.New("OP_BATCH_TRANSACTION_MISSING")
}

// Blobs are in block order, as returned by the Beacon blobs endpoint.
func verifyOPBatchData(ctx context.Context, first int, hashes []common.Hash, blobs []*kzg4844.Blob) error {
	if len(hashes) == 0 || first > len(blobs) || len(blobs)-first < len(hashes) {
		return errors.New("OP_BATCH_BLOB_COUNT")
	}
	for i, hash := range hashes {
		if err := ctx.Err(); err != nil {
			return err
		}
		blob := blobs[first+i]
		if blob == nil {
			return errors.New("OP_BATCH_BLOB_MISSING")
		}
		commitment, err := kzg4844.BlobToCommitment(blob)
		if err != nil || kzg4844.CalcBlobHashV1(sha256.New(), &commitment) != hash {
			return errors.New("OP_BATCH_BLOB_COMMITMENT")
		}
	}
	return ctx.Err()
}
