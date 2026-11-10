package evm

import (
	"context"
	"encoding/hex"
	"encoding/json"
	"errors"
	"strings"

	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/core/types"
	"github.com/ethereum/go-ethereum/crypto"
	"github.com/ethereum/go-ethereum/trie"
)

type ReceiptProofLimits struct {
	MaxBytes        int    `json:"max_bytes"`
	MaxHeaders      int    `json:"max_headers"`
	MaxWindowBlocks int    `json:"max_window_blocks"`
	MaxTransactions int    `json:"max_transactions"`
	MaxLookahead    uint64 `json:"max_lookahead"`
}

func (l ReceiptProofLimits) validate() error {
	if l.MaxBytes <= 0 || l.MaxBytes > 64<<20 || l.MaxHeaders < 1 || l.MaxHeaders > 512 || l.MaxWindowBlocks < 1 || l.MaxWindowBlocks > 128 || l.MaxTransactions < 1 || l.MaxTransactions > 100000 || l.MaxLookahead < 1 || l.MaxLookahead > 512 {
		return errors.New("RECEIPT_PROOF_LIMITS_INVALID")
	}
	return nil
}

type receiptProofBudget struct{ remaining int }

func (b *receiptProofBudget) charge(n int) error {
	if n < 0 || n > b.remaining {
		return errors.New("RECEIPT_PROOF_BUDGET")
	}
	b.remaining -= n
	return nil
}

type ExecutionStateAnchor struct {
	AccountEncodingID string `json:"account_encoding_id"`
	Height            uint64 `json:"height"`
	BlockHash         string `json:"block_hash"`
	StateRoot         string `json:"state_root"`
	SettlingHeight    uint64 `json:"settling_height"`
	SettlingHash      string `json:"settling_hash"`
	Authority         string `json:"authority"`
}

type SystemReceiptObservation struct {
	Receipt              Receipt `json:"receipt"`
	Authority            string  `json:"authority"`
	LogsContentCommitted bool    `json:"logs_content_committed"`
}

// VerifiedReceiptCoverage deliberately separates receipt membership, whole
// block event coverage, state height and non-member system observations. A
// canonical prefix proof never upgrades excluded observations to proven logs.
type VerifiedReceiptCoverage struct {
	ProofBytesUsed                int                        `json:"proof_bytes_used"`
	Evidence                      *NativeReceiptEvidence     `json:"evidence,omitempty"`
	NativeSemantics               *NativeSemanticsDescriptor `json:"native_semantics,omitempty"`
	ChainKey                      string                     `json:"chain_key"`
	Block                         RawBlock                   `json:"block"`
	CanonicalBlock                RawBlock                   `json:"canonical_block"`
	CanonicalReceipts             []Receipt                  `json:"canonical_receipts"`
	CanonicalBodyProven           bool                       `json:"canonical_body_proven"`
	CanonicalReceiptsProven       bool                       `json:"canonical_receipts_proven"`
	WholeBlockReceiptEventsProven bool                       `json:"whole_block_receipt_events_proven"`
	SystemObservations            []SystemReceiptObservation `json:"system_observations"`
	SystemObservationCoverage     string                     `json:"system_observation_coverage"`
	NativeSemanticsID             string                     `json:"native_semantics_id"`
	ActivationID                  string                     `json:"activation_id"`
	ReceiptAuthority              string                     `json:"receipt_authority"`
	ReceiptRoot                   string                     `json:"receipt_root"`
	WindowStart                   uint64                     `json:"window_start"`
	WindowEnd                     uint64                     `json:"window_end"`
	StateAnchor                   *ExecutionStateAnchor      `json:"state_anchor,omitempty"`
}

func (c VerifiedReceiptCoverage) RequireWholeBlockEvents() error {
	if !c.CanonicalBodyProven || !c.CanonicalReceiptsProven || !c.WholeBlockReceiptEventsProven {
		return errors.New("RECEIPT_WHOLE_BLOCK_EVENTS_UNPROVEN")
	}
	return nil
}
func (c VerifiedReceiptCoverage) TargetStateAnchor() (*ExecutionStateAnchor, error) {
	height, err := canonicalIndex(c.Block.Number)
	if err != nil || c.StateAnchor == nil || c.StateAnchor.Height != height || !strings.EqualFold(c.StateAnchor.BlockHash, c.Block.Hash) {
		return nil, errors.New("EXECUTION_STATE_TARGET_NOT_DIRECTLY_SETTLED")
	}
	copy := *c.StateAnchor
	return &copy, nil
}

// A raw native byte string is not interpreted through geth.Transaction. The
// classic 0x78 wrapper has an explicit upstream HashOverride; treating it as a
// Keccak txID proof would be false. Nitro's ordinary typed native envelopes are
// Keccak-bound and can be handled without a native transaction interpreter.
func verifyRawBodyCommitment(ctx context.Context, chainKey string, header ChainHeaderCommitment, raws []string, budget *receiptProofBudget) error {
	if err := validateBlockTransactions(header.Block); err != nil {
		return err
	}
	if len(raws) != len(header.Block.Transactions) {
		return errors.New("BLOCK_BODY_INCOMPLETE")
	}
	encoded := make(encodedChainReceipts, 0, len(raws))
	for i, value := range raws {
		if err := ctx.Err(); err != nil {
			return err
		}
		if !receiptHex(value, -1) || len(value) <= 2 {
			return errors.New("BLOCK_INTEGRITY_TRANSACTION_ENCODING")
		}
		if err := budget.charge(len(value) + (len(value)-2)/2 + 128); err != nil {
			return err
		}
		payload, err := hex.DecodeString(value[2:])
		if err != nil {
			return err
		}
		if chainKey == "evm:42161" && payload[0] == 0x78 {
			return errors.New("ARBITRUM_CLASSIC_TX_ID_AUTHORITY_REQUIRED")
		}
		if !receiptHex(header.Block.Transactions[i], 32) || crypto.Keccak256Hash(payload) != common.HexToHash(header.Block.Transactions[i]) {
			return errors.New("BLOCK_INTEGRITY_TRANSACTION_HASH")
		}
		encoded = append(encoded, payload)
	}
	if types.DeriveSha(encoded, trie.NewStackTrie(nil)) != common.HexToHash(header.TransactionsRoot) {
		return errors.New("BLOCK_INTEGRITY_TRANSACTIONS_ROOT")
	}
	return nil
}

type ExecutionReceiptBlock struct {
	Header           json.RawMessage   `json:"header"`
	Receipts         []Receipt         `json:"receipts"`
	RawTransactions  []string          `json:"raw_transactions"`
	TransactionsJSON []json.RawMessage `json:"transactions_json,omitempty"`
}
type SAEExecutionProof struct {
	ChainKey    string                  `json:"chain_key"`
	GenesisHash string                  `json:"genesis_hash"`
	Headers     []json.RawMessage       `json:"headers"`
	Window      []ExecutionReceiptBlock `json:"window"`
}

func verifyExecutionBody(ctx context.Context, chainKey string, header ChainHeaderCommitment, block ExecutionReceiptBlock, budget *receiptProofBudget) error {
	if len(block.TransactionsJSON) == 0 {
		return verifyRawBodyCommitment(ctx, chainKey, header, block.RawTransactions, budget)
	}
	if len(block.RawTransactions) > 0 || len(block.TransactionsJSON) != len(header.Block.Transactions) {
		return errors.New("BLOCK_BODY_AUTHORITY_AMBIGUOUS")
	}
	raws := make([]string, 0, len(block.TransactionsJSON))
	for i, raw := range block.TransactionsJSON {
		if err := budget.charge(2*len(raw) + 128); err != nil {
			return err
		}
		payload, err := encodeChainTransactionJSON(chainKey, header.Block, uint64(i), raw)
		if err != nil {
			return err
		}
		raws = append(raws, "0x"+hex.EncodeToString(payload))
	}
	return verifyRawBodyCommitment(ctx, chainKey, header, raws, budget)
}
