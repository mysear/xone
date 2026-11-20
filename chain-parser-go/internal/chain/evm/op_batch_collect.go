package evm

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"math/big"
	"net/http"
	"net/url"
	"strings"
	"time"

	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/core/types"
	"github.com/ethereum/go-ethereum/crypto/kzg4844"
	"xone/chain-parser-go/internal/provenance"
	"xone/chain-parser-go/internal/rpc"
)

// Search from the signed execution head backwards. Verify the complete body
// before using even a negative inbox result to advance to its parent.
func collectOPSubmission(ctx context.Context, finality *provenance.VerifiedExecutionFinality, l1, state *rpc.Client, window, budget int, blobsAt func(context.Context, uint64) ([]*kzg4844.Blob, error)) ([]OPChannelSubmission, error) {
	if finality == nil || window < 1 || window > 129 || budget < 1 {
		return nil, errors.New("OP_BATCH_SEARCH_WINDOW")
	}
	head, expected := finality.ExecutionBlock()
	var descendants []json.RawMessage
	for offset := 0; offset < window && uint64(offset) <= head; offset++ {
		height := head - uint64(offset)
		tag := fmt.Sprintf("0x%x", height)
		var block json.RawMessage
		if err := l1.Call(ctx, "eth_getBlockByNumber", []any{tag, true}, &block); err != nil {
			return nil, err
		}
		if len(block) > budget {
			return nil, errors.New("OP_BATCH_SEARCH_BUDGET")
		}
		budget -= len(block)
		body, err := VerifyChainBlockJSON(ctx, "evm:1", block, 32<<20)
		if err != nil {
			return nil, err
		}
		if body.Block.Number != tag || body.Block.Hash != expected {
			return nil, errors.New("OP_BATCH_ANCESTRY_LINK")
		}
		var authority *OPBatchSenderProof
		var sender common.Address
		for i := len(body.RawTransactions) - 1; i >= 0; i-- {
			var tx types.Transaction
			if err := tx.UnmarshalBinary(common.FromHex(body.RawTransactions[i])); err != nil {
				return nil, err
			}
			if tx.Type() != types.BlobTxType || tx.To() == nil || *tx.To() != common.HexToAddress(opBatchInbox) {
				continue
			}
			if authority == nil {
				authority, sender, err = collectOPAuthority(ctx, state, body, height)
				if err != nil {
					return nil, err
				}
			}
			from, err := types.Sender(types.NewCancunSigner(big.NewInt(1)), &tx)
			if err != nil {
				return nil, err
			}
			if from != sender {
				continue
			}
			timestamp, err := canonicalIndex(body.Block.Timestamp)
			if err != nil || timestamp < 1606824023 || (timestamp-1606824023)%12 != 0 {
				return nil, errors.New("OP_BATCH_BEACON_SLOT")
			}
			blobs, err := blobsAt(ctx, (timestamp-1606824023)/12)
			if err != nil {
				return nil, err
			}
			return []OPChannelSubmission{{Evidence: OPBatchEvidence{Block: block, TxHash: tx.Hash().Hex(), Blobs: blobs, Descendants: descendants}, Authority: *authority}}, nil
		}
		descendants = append(descendants, body.Header)
		expected = body.Block.ParentHash
	}
	return nil, errors.New("OP_BATCH_SEARCH_EXHAUSTED")
}

func collectOPAuthority(ctx context.Context, client *rpc.Client, body ChainBlockBodyProof, height uint64) (*OPBatchSenderProof, common.Address, error) {
	var proof OPBatchSenderProof
	anchor := opConfigAnchor(body, height)
	tag := body.Block.Number
	slots := []string{common.HexToHash("0x67").Hex(), opConfigImplSlot}
	if err := client.Call(ctx, "eth_getProof", []any{opConfigProxy, slots, tag}, &proof.Proxy); err != nil {
		return nil, common.Address{}, err
	}
	impl, err := proof.implementationAddress(ctx, anchor)
	if err != nil {
		return nil, common.Address{}, err
	}
	if err = client.Call(ctx, "eth_getProof", []any{impl.Hex(), []string{}, tag}, &proof.Implementation); err != nil {
		return nil, common.Address{}, err
	}
	sender, err := proof.sender(ctx, anchor)
	return &proof, sender, err
}

func opBeaconBlobs(endpoint string) (func(context.Context, uint64) ([]*kzg4844.Blob, error), error) {
	base, err := url.Parse(endpoint)
	if err != nil || base.Scheme != "https" || base.Hostname() == "" || base.User != nil || base.RawQuery != "" || base.Fragment != "" {
		return nil, errors.New("OP_BATCH_BEACON_ENDPOINT")
	}
	client := http.Client{Timeout: 20 * time.Second, CheckRedirect: func(*http.Request, []*http.Request) error { return http.ErrUseLastResponse }}
	return func(ctx context.Context, slot uint64) ([]*kzg4844.Blob, error) {
		path := fmt.Sprintf("%s/eth/v1/beacon/blobs/%d", strings.TrimRight(endpoint, "/"), slot)
		req, err := http.NewRequestWithContext(ctx, http.MethodGet, path, nil)
		if err != nil {
			return nil, err
		}
		req.Header.Set("Accept", "application/json")
		res, err := client.Do(req)
		if err != nil {
			return nil, err
		}
		defer res.Body.Close()
		if res.StatusCode != http.StatusOK {
			return nil, fmt.Errorf("OP_BATCH_BEACON_HTTP_%d", res.StatusCode)
		}
		const limit = 32 << 20
		raw, err := io.ReadAll(io.LimitReader(res.Body, limit+1))
		if err != nil {
			return nil, err
		}
		if len(raw) > limit {
			return nil, errors.New("OP_BATCH_BLOB_BYTES")
		}
		var envelope struct {
			Data []*kzg4844.Blob `json:"data"`
		}
		if err = json.Unmarshal(raw, &envelope); err != nil {
			return nil, err
		}
		return envelope.Data, nil
	}, nil
}
