package evm

import (
	"context"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"strconv"
	"strings"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/domain"
	"xone/chain-parser-go/internal/rpc"
)

type Adapter struct {
	Client *rpc.Client
	Chain  config.Chain
	// OnComplete runs only after receipts/root/witness, pool state, and the
	// final canonical anchor have all passed. An error backpressures the read.
	OnComplete        func(context.Context, RawBlock, []Receipt) error
	OnHistoryEvidence func(context.Context, NativeHistoryEvidence) error
}

// CopyWithChain preserves source callbacks when selecting a smaller scope.
func (a *Adapter) CopyWithChain(chain config.Chain) *Adapter {
	copy := *a
	copy.Chain = chain
	return &copy
}

type RawBlock struct {
	NativeBodyProof    *ChainBlockBodyProof     `json:"-"`
	NativeCoverage     *VerifiedReceiptCoverage `json:"-"`
	ObservedHead       *RawBlock                `json:"-"`
	CurrentStateAnchor *ExecutionStateAnchor    `json:"-"`
	nativeSession      *nativeReadSession
	nativeContext      context.Context
	Number             string   `json:"number"`
	Hash               string   `json:"hash"`
	ParentHash         string   `json:"parentHash"`
	Timestamp          string   `json:"timestamp"`
	Transactions       []string `json:"transactions"`
	ReceiptsRoot       string   `json:"receiptsRoot"`
}
type Log struct {
	BlockHash string   `json:"blockHash"`
	TxHash    string   `json:"transactionHash"`
	TxIndex   string   `json:"transactionIndex"`
	Index     string   `json:"logIndex"`
	Removed   bool     `json:"removed"`
	Address   string   `json:"address"`
	Topics    []string `json:"topics"`
	Data      string   `json:"data"`
}
type Receipt struct {
	invalidChainEncoding   bool
	GasUsed                string `json:"gasUsed,omitempty"`
	DepositNonce           string `json:"depositNonce,omitempty"`
	DepositReceiptVersion  string `json:"depositReceiptVersion,omitempty"`
	invalidTypeEncoding    bool
	invalidOutcomeEncoding bool
	BlockHash              string `json:"blockHash"`
	TxHash                 string `json:"transactionHash"`
	TxIndex                string `json:"transactionIndex"`
	Logs                   []Log  `json:"logs"`
	Type                   string `json:"type,omitempty"`
	Status                 string `json:"status,omitempty"`
	PostState              string `json:"root,omitempty"`
	CumulativeGasUsed      string `json:"cumulativeGasUsed"`
	LogsBloom              string `json:"logsBloom"`
}

func quantity(s string) (uint64, error) {
	if !strings.HasPrefix(s, "0x") {
		return 0, errors.New("INVALID_HEX_QUANTITY")
	}
	return strconv.ParseUint(s[2:], 16, 64)
}

// canonicalIndex rejects non-quantity spellings that otherwise alias the same
// position (for example 0x01 and 0x1).
func canonicalIndex(value string) (uint64, error) {
	index, err := quantity(value)
	if err != nil || value != fmt.Sprintf("0x%x", index) {
		return 0, errors.New("INVALID_HEX_QUANTITY")
	}
	return index, nil
}

func validateBlockTransactions(b RawBlock) error {
	if b.Transactions == nil {
		return errors.New("BLOCK_TRANSACTIONS_MISSING")
	}
	seen := make(map[string]bool, len(b.Transactions))
	for _, tx := range b.Transactions {
		if tx == "" || seen[tx] {
			return errors.New("BLOCK_TRANSACTION_IDENTITY")
		}
		seen[tx] = true
	}
	return nil
}

func receiptHex(value string, width int) bool {
	if !strings.HasPrefix(value, "0x") || len(value)%2 != 0 {
		return false
	}
	raw := value[2:]
	if width >= 0 && len(raw) != width*2 {
		return false
	}
	_, err := hex.DecodeString(raw)
	return err == nil
}
func validateReceiptLogEncoding(log Log) error {
	if !receiptHex(log.Address, 20) || !receiptHex(log.Data, -1) || log.Topics == nil || len(log.Topics) > 4 {
		return errors.New("LOG_ENCODING")
	}
	for _, topic := range log.Topics {
		if !receiptHex(topic, 32) {
			return errors.New("LOG_ENCODING")
		}
	}
	return nil
}

func ValidateReceipts(b RawBlock, receipts []Receipt) error {
	if err := validateBlockTransactions(b); err != nil {
		return err
	}
	if len(receipts) != len(b.Transactions) {
		return errors.New("INCOMPLETE_RECEIPTS")
	}
	seen := map[uint64]bool{}
	ordered := make([]*Receipt, len(receipts))
	for i := range receipts {
		r := &receipts[i]
		idx, e := canonicalIndex(r.TxIndex)
		if e != nil || idx >= uint64(len(b.Transactions)) || r.TxHash != b.Transactions[idx] || r.BlockHash != b.Hash || ordered[idx] != nil {
			return errors.New("RECEIPT_IDENTITY")
		}
		if r.Logs == nil {
			return errors.New("RECEIPT_LOGS_MISSING")
		}
		ordered[idx] = r
		for _, l := range r.Logs {
			tx, e := canonicalIndex(l.TxIndex)
			if e != nil || l.BlockHash != b.Hash || l.TxHash != r.TxHash || tx != idx || l.Removed {
				return errors.New("LOG_IDENTITY")
			}
			if e := validateReceiptLogEncoding(l); e != nil {
				return e
			}
			li, e := canonicalIndex(l.Index)
			if e != nil {
				return errors.New("LOG_INDEX")
			}
			if seen[li] {
				return errors.New("DUPLICATE_LOG")
			}
			seen[li] = true
		}
	}
	// JSON-RPC logIndex is the position in the whole block, not the receipt.
	// Normalize receipt order without sorting or modifying the caller's data.
	// This detects internal holes and position inversions; detecting a provider
	// dropping an entire trailing suffix still requires a receipts-root proof or
	// an independent source, which this check does not claim to provide.
	var next uint64
	for _, r := range ordered {
		if r == nil {
			return errors.New("INCOMPLETE_RECEIPTS")
		}
		for _, l := range r.Logs {
			index, _ := canonicalIndex(l.Index) // validated above
			if index != next {
				return errors.New("LOG_INDEX_GAP")
			}
			next++
		}
	}
	return nil
}
func (a *Adapter) Block(ctx context.Context, tag string) (RawBlock, error) {
	if a.Chain.BlockIntegrity == ChainNativeCommitmentsV2 {
		return a.nativeRuntimeBlock(ctx, tag)
	}
	if a.Chain.BlockIntegrity == ChainBodyCommitmentsV1 {
		return a.ReadChainCommittedBlock(ctx, tag)
	}
	if a.Chain.BlockIntegrity != "" {
		return a.ethereumBlock(ctx, tag)
	}
	var b RawBlock
	e := a.Client.Call(ctx, "eth_getBlockByNumber", []any{tag, false}, &b)
	if e == nil && (b.Hash == "" || b.ParentHash == "") {
		e = errors.New("BLOCK_IDENTITY")
	}
	if e == nil {
		for _, p := range a.Chain.Pools {
			if p.Protocol == domain.ManagedV2Protocol || p.Protocol == domain.ManagedV2ScheduledProtocol || p.Protocol == domain.ManagedV2BptProtocol || p.Protocol == domain.ManagedV2BreakerProtocol {
				e = validateManagedV2Header(b)
				break
			}
			if p.Protocol == domain.V2CompleteProtocol {
				e = validateV2CompleteHeader(b)
				break
			}
		}
	}
	if e == nil {
		// Preserve absent/null transaction arrays for explicit layout rejection.
		if b.Transactions != nil {
			b = canonicalReceiptBlock(b)
		}
		e = validateBlockTransactions(b)
	}
	return b, e
}
func (a *Adapter) ReadState(ctx context.Context, tag string, complete bool) (domain.BlockRef, []domain.Pool, error) {
	b, p, _, e := a.readState(ctx, tag, complete)
	return b, p, e
}
func (a *Adapter) ReadComplete(ctx context.Context, tag string) (domain.BlockRef, []domain.Pool, []domain.SwapFact, error) {
	return a.readState(ctx, tag, true)
}
func (a *Adapter) readState(ctx context.Context, tag string, completeBlock bool) (domain.BlockRef, []domain.Pool, []domain.SwapFact, error) {
	ctx, cancel := context.WithTimeout(ctx, a.Client.HTTP.Timeout)
	defer cancel()
	b, e := a.Block(ctx, tag)
	if e != nil {
		return domain.BlockRef{}, nil, nil, e
	}
	ref, e := blockReference(a.Chain.Key, b)
	if e != nil {
		return domain.BlockRef{}, nil, nil, e
	}
	facts := []domain.SwapFact{}
	var completedReceipts []Receipt
	if completeBlock || a.Chain.BlockIntegrity == ChainNativeCommitmentsV2 {
		var receipts []Receipt
		b, receipts, e = a.ReadQualifiedReceipts(ctx, b, true)
		if e != nil {
			return ref, nil, nil, e
		}
		completedReceipts = receipts
		if completeBlock {
			facts, e = DecodeSwapFacts(ref, receipts, a.Chain.Pools)
			if e != nil {
				return ref, nil, nil, e
			}
		}
	}

	stateReader, e := a.nativeStateReader(ctx, b)
	if e != nil {
		return ref, nil, nil, e
	}
	pools := make([]domain.Pool, 0, len(a.Chain.Pools))
	remainingPoolBytes := a.Client.MaxBytes
	anchor := map[string]any{"blockHash": b.Hash, "requireCanonical": true}
	for _, p := range a.Chain.Pools {
		profile, ok := domain.Protocol(p.Protocol)
		if !ok {
			return ref, nil, nil, errors.New("PROTOCOL_UNSUPPORTED")
		}
		var pool domain.Pool
		if profile.Family == "infinity-bin" {
			pool, e = stateReader.readInfinityBin(ctx, p, anchor, ref)
		} else if profile.Family == "singleton-advanced" {
			v := p.SingletonAdvanced
			if v == nil {
				return ref, nil, nil, errors.New("SINGLETON_ADVANCED_CONFIG_REQUIRED")
			}
			state := domain.SingletonAdvancedState{Protocol: v.Protocol, Currency0: v.Currency0, Currency1: v.Currency1, KeyFee: v.KeyFee, SnapshotBlockHash: b.Hash, Snapshot: domain.SingletonCLState{Manager: v.Manager, ManagerCodeSHA256: v.ExpectedManagerCodeSHA256, PoolID: v.PoolID, LPFee: func() uint32 {
				if v.KeyFee == domain.SingletonDynamicFeeFlag {
					return 0
				}
				return v.KeyFee
			}(), Hooks: v.Hooks, Parameters: v.Parameters, Core: domain.ConcentratedState{TickSpacing: v.TickSpacing}}}
			observed, err := stateReader.ReadSingletonAdvanced(ctx, SingletonAdvancedReadConfig{State: state, TickWordRadius: v.TickWordRadius, MaxInitializedTicks: v.MaxInitializedTicks, MaxSteps: v.MaxSteps})
			e = err
			if e == nil {
				pool = domain.Pool{SingletonAdvanced: &observed, ChainKey: a.Chain.Key, PoolID: p.ID, Protocol: p.Protocol, Token0: p.Token0, Token1: p.Token1, Reserve0: "0", Reserve1: "0", FeeNumerator: uint64(observed.Snapshot.LPFee), FeeDenominator: 1000000, AdapterVersion: profile.AdapterVersion, TokenBehavior: "standard"}
				e = pool.Validate()
			}
		} else if p.Protocol == domain.PancakeBscLmProtocol {
			pool, e = stateReader.readPancakeBscLm(ctx, p, anchor, ref)
		} else if p.Protocol == domain.PancakeLmProtocol {
			pool, e = stateReader.readPancakeLm(ctx, p, anchor, ref)
		} else if p.Protocol == domain.SlipstreamContextProtocol {
			pool, e = stateReader.readSlipstreamContext(ctx, p, anchor, ref)
		} else if p.Protocol == domain.LfjProtocol {
			pool, e = stateReader.readLfj(ctx, p, anchor, ref)
		} else if p.Protocol == "curve-metausd-direct" {
			pool, e = stateReader.readCurveMeta(ctx, p, anchor, ref)
		} else if p.Protocol == "curve-crypto2-full" || p.Protocol == "curve-crypto2-ramp" {
			pool, e = stateReader.readCurveCrypto(ctx, p, anchor, ref)
		} else if p.Protocol == "curve-stableswap-ng" {
			pool, e = stateReader.readCurveNG(ctx, p, anchor, ref)
		} else if p.Protocol == domain.ManagedV2Protocol || p.Protocol == domain.ManagedV2ScheduledProtocol || p.Protocol == domain.ManagedV2BptProtocol || p.Protocol == domain.ManagedV2BreakerProtocol {
			pool, e = stateReader.readManagedV2(ctx, p, anchor, ref)
		} else if profile.Family == "multi" {
			pool, e = stateReader.readMultiAsset(ctx, p, anchor, ref.Number)
		} else if profile.Family == "singleton" {
			pool, e = stateReader.readSingletonCL(ctx, p, anchor)
		} else if profile.Family == "weighted" {
			pool, e = stateReader.readWeighted(ctx, p, anchor, ref.Number)
		} else if profile.Family == "aero" || profile.Family == "curve" || profile.Family == "balancer" {
			pool, e = stateReader.readStable(ctx, p, anchor)
		} else if profile.Family == "v3" {
			pool, e = stateReader.readV3(ctx, p, anchor)
		} else if p.Protocol == domain.V2CompleteProtocol {
			pool, e = stateReader.readV2Complete(ctx, p, anchor, ref)
		} else {
			pool, e = stateReader.readV2(ctx, p, anchor)
		}
		if e != nil {
			if b.nativeSession != nil {
				e = b.nativeSession.stateReadError(e)
			}
			return ref, nil, nil, e
		}
		encoded, err := json.Marshal(pool)
		if err != nil {
			return ref, nil, nil, err
		}
		if len(encoded) > remainingPoolBytes-256 {
			return ref, nil, nil, errors.New("POOL_STATE_BUDGET")
		}
		remainingPoolBytes -= len(encoded) + 256
		if b.nativeSession != nil {
			if err = b.nativeSession.retainPool(encoded); err != nil {
				return ref, nil, nil, err
			}
		}
		pools = append(pools, pool)
	}
	// The anchor must still be canonical after all state calls; unsupported EIP-1898 is a hard failure.
	check, e := a.recheckBlock(ctx, b)
	if e != nil {
		return ref, nil, nil, e
	}
	if !sameReceiptAnchor(b, check) {
		return ref, nil, nil, errors.New("ANCHOR_REORGED")
	}
	if completeBlock && a.OnComplete != nil {
		if ctx.Err() != nil {
			return ref, nil, nil, ctx.Err()
		}
		if e = a.OnComplete(ctx, b, completedReceipts); e != nil {
			return ref, nil, nil, e
		}
	}
	return ref, pools, facts, nil
}

func (a *Adapter) ConfirmBlock(ctx context.Context, block domain.BlockRef) error {
	return a.Client.ConfirmEVMBlock(ctx, block.Number, block.Hash, block.ParentHash)
}

// checkedEVMTimestampMS bounds the source seconds before multiplying. Testing
// the resulting milliseconds for alignment cannot detect an overflow that wraps
// to another multiple of 1000 (for example, (2^61 + 1) seconds wraps to 1000ms).
func checkedEVMTimestampMS(encoded string) (uint64, error) {
	seconds, e := quantity(encoded)
	if e != nil || seconds > ^uint64(0)/1000 {
		return 0, errors.New("TIMESTAMP_INVALID")
	}
	return seconds * 1000, nil
}

// Both state and header readers construct their clock through this conversion.
// Block deliberately retains the raw timestamp for receipt-only reads.
func blockReference(chain string, b RawBlock) (domain.BlockRef, error) {
	number, e := quantity(b.Number)
	if e != nil {
		return domain.BlockRef{}, e
	}
	timestampMS, e := checkedEVMTimestampMS(b.Timestamp)
	if e != nil {
		return domain.BlockRef{}, e
	}
	ref := domain.BlockRef{ChainKey: chain, Number: number, Hash: b.Hash, ParentHash: b.ParentHash, TimestampMS: timestampMS}
	return ref, nil
}
func (a *Adapter) Header(ctx context.Context, tag string) (domain.BlockRef, error) {
	b, e := a.Block(ctx, tag)
	if e != nil {
		return domain.BlockRef{}, e
	}
	return blockReference(a.Chain.Key, b)
}
