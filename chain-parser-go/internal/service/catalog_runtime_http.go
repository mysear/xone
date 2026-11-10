package service

import (
	"context"
	"encoding/json"
	"fmt"
	"strconv"
	"strings"

	"xone/chain-parser-go/internal/chain/evm"
	"xone/chain-parser-go/internal/ingest"
	"xone/chain-parser-go/internal/rpc"
)

// factoryHTTPProof is the archived, complete HTTP block/receipt projection.
// Receipt-root verification proves log completeness relative to the RPC header;
// it does not assert consensus or independent provider ownership.
type factoryHTTPProof struct {
	Header            evm.RawBlock
	Receipts          []evm.Receipt
	NativeEvidence    *evm.NativeReceiptEvidence `json:",omitempty"`
	NativeSemanticsID string                     `json:",omitempty"`
}

// DiscoverHeight is the normal-source entry point: one bounded HTTP block and
// all of its receipts, verified before candidate decoding. Callers choose recent
// heights within their live budget; history work calls this in CatalogHistory.
// It never walks a downtime window or automatically admits candidates.
func (r *CatalogRuntime) DiscoverHeight(ctx context.Context, ns ingest.CatalogNamespace, height uint64) ([]ingest.FactoryCandidate, error) {
	ctx, done, revision, err := r.operation(ctx)
	if err != nil {
		return nil, err
	}
	defer done()
	if err = r.check(ctx, revision); err != nil {
		return nil, err
	}
	if ns != ingest.CatalogOnline && ns != ingest.CatalogHistory {
		return nil, ingest.ErrFactoryCandidate
	}
	r.mu.Lock()
	client := r.client
	anchor := r.anchor
	r.mu.Unlock()
	if height > anchor.Height {
		return nil, ingest.ErrFactoryStale
	}
	r.mu.Lock()
	var saved *ingest.FactoryBlock
	for _, b := range r.disk.Blocks {
		if b.Namespace == ns && b.Block.Anchor.Height == height {
			copy := b.Block
			saved = &copy
			break
		}
	}
	r.mu.Unlock()
	if saved != nil {
		return r.discover(ctx, ns, *saved, revision)
	}
	chain := r.options.Chain
	chain.Pools = nil
	chain.DynamicCatalog = &r.options.Config
	if chain.NativeProof != nil {
		limits := *chain.NativeProof
		limits.MaxBytes = min(limits.MaxBytes, int(r.options.Config.MaxBytes))
		chain.NativeProof = &limits
	}
	adapter := evm.Adapter{Client: client, Chain: chain}
	header, err := adapter.Block(ctx, fmt.Sprintf("0x%x", height))
	if err != nil {
		return nil, err
	}
	if header.Number != fmt.Sprintf("0x%x", height) || !runtimeDigest(header.Hash) || len(header.Transactions) > r.options.Config.MaxReceipts {
		return nil, ingest.ErrFactoryEvent
	}
	// Scope both per-response and aggregate receipt buffers to this catalog's
	// budget without changing the shared client's policy/recorder/observations.
	_, readers := client.ProviderClients()
	if len(readers) != 1 {
		return nil, ErrProviderGate
	}
	receiptClient := readers[0]
	if int64(receiptClient.MaxBytes) > r.options.Config.MaxBytes {
		receiptClient.MaxBytes = int(r.options.Config.MaxBytes)
	}
	receiptAdapter := evm.Adapter{Client: receiptClient, Chain: chain}
	var receipts []evm.Receipt
	if chain.BlockIntegrity == evm.ChainNativeCommitmentsV2 {
		header, receipts, err = receiptAdapter.ReadQualifiedReceipts(ctx, header, false)
	} else {
		receipts, err = receiptAdapter.ReadReceiptsWithMode(ctx, header, chain.ReceiptsMode)
	}
	if err != nil {
		return nil, err
	}
	if len(receipts) > r.options.Config.MaxReceipts {
		return nil, ingest.ErrBudget
	}
	if chain.BlockIntegrity == evm.ChainNativeCommitmentsV2 {
		if err = receiptAdapter.RecheckQualifiedBlock(ctx, header); err != nil {
			return nil, err
		}
	}
	proof := factoryHTTPProof{Header: header, Receipts: receipts}
	remaining := r.options.Config.MaxBytes
	logs := 0
	for _, receipt := range receipts {
		if ctx.Err() != nil {
			return nil, ctx.Err()
		}
		encoded, e := json.Marshal(receipt)
		if e != nil {
			return nil, e
		}
		remaining -= int64(len(encoded))
		logs += len(receipt.Logs)
		if remaining < 0 || logs > r.options.Config.MaxLogs {
			return nil, ingest.ErrBudget
		}
	}
	raw, err := json.Marshal(proof)
	if err != nil {
		return nil, err
	}
	if int64(len(raw)) > r.options.Config.MaxBytes {
		return nil, ingest.ErrBudget
	}
	block, err := r.projectHTTPProofContext(ctx, proof)
	if err != nil {
		return nil, err
	}
	creation := false
	for _, receipt := range block.Receipts {
		for _, log := range receipt.Logs {
			if r.observesFactoryCreation(height, log.Address, log.Topics) {
				creation = true
				break
			}
		}
	}
	// Empty verified blocks advance the caller's scan cursor without consuming
	// the bounded candidate directory or holding raw proof pins indefinitely.
	if !creation {
		return nil, ctx.Err()
	}

	if err = r.check(ctx, revision); err != nil {
		return nil, err
	}
	block.Raw, err = r.options.Archive.PutPinned(ingest.Owner{Chain: block.Chain, Height: height, Hash: header.Hash, ProviderID: client.Providers()[0].ID}, raw, r.pinReference(ns, header.Hash))
	if err != nil {
		return nil, err
	}
	return r.discover(ctx, ns, block, revision)
}

// observesFactoryCreation limits retained proofs to the configured deployment
// range. The complete receipt set is still verified before this selection.
func (r *CatalogRuntime) observesFactoryCreation(height uint64, address string, topics []string) bool {
	if len(topics) == 0 {
		return false
	}
	for _, source := range r.options.Config.Sources {
		if !strings.EqualFold(source.Address, address) || height < source.Since || source.Until != 0 && height > source.Until {
			continue
		}
		topic := ingest.V2PairCreatedTopic
		if source.Version == ingest.UniswapV3Factory {
			topic = ingest.V3PoolCreatedTopic
		}
		return strings.EqualFold(topics[0], topic)
	}
	return false
}

func (r *CatalogRuntime) projectHTTPProofUnchecked(p factoryHTTPProof) (ingest.FactoryBlock, error) {
	height, err := strconv.ParseUint(trimRuntimeHex(p.Header.Number), 16, 64)
	if err != nil {
		return ingest.FactoryBlock{}, err
	}
	block := ingest.FactoryBlock{Chain: r.options.Chain.Key, Anchor: ingest.Cursor{Height: height, Hash: p.Header.Hash}, Receipts: make([]ingest.FactoryReceipt, 0, len(p.Receipts))}
	if len(p.Receipts) > r.options.Config.MaxReceipts {
		return block, ingest.ErrBudget
	}
	logs := 0
	for _, receipt := range p.Receipts {
		txIndex, err := strconv.ParseUint(trimRuntimeHex(receipt.TxIndex), 16, 64)
		if err != nil {
			return block, err
		}
		item := ingest.FactoryReceipt{BlockHash: receipt.BlockHash, BlockNumber: height, TransactionHash: receipt.TxHash, TransactionIndex: txIndex, Logs: make([]ingest.FactoryLog, 0, len(receipt.Logs))}
		logs += len(receipt.Logs)
		if logs > r.options.Config.MaxLogs {
			return block, ingest.ErrBudget
		}
		for _, l := range receipt.Logs {
			idx, err := strconv.ParseUint(trimRuntimeHex(l.Index), 16, 64)
			if err != nil {
				return block, err
			}
			index, err := strconv.ParseUint(trimRuntimeHex(l.TxIndex), 16, 64)
			if err != nil {
				return block, err
			}
			item.Logs = append(item.Logs, ingest.FactoryLog{Address: l.Address, BlockHash: l.BlockHash, BlockNumber: height, TransactionHash: l.TxHash, TransactionIndex: index, LogIndex: idx, Removed: l.Removed, Topics: append([]string{}, l.Topics...), Data: l.Data})
		}
		block.Receipts = append(block.Receipts, item)
	}
	return block, nil
}
func (r *CatalogRuntime) verifyHTTPBlock(ctx context.Context, client *rpc.Client, block ingest.FactoryBlock) error {
	raw, err := r.options.Archive.Read(block.Raw)
	if err != nil {
		return err
	}
	if int64(len(raw)) > r.options.Config.MaxBytes {
		return ingest.ErrBudget
	}
	var proof factoryHTTPProof
	if err = json.Unmarshal(raw, &proof); err != nil {
		return err
	}
	projected, err := r.projectHTTPProofContext(ctx, proof)
	if err != nil {
		return err
	}
	projected.Raw = block.Raw
	left, _ := json.Marshal(projected)
	right, _ := json.Marshal(block)
	if string(left) != string(right) {
		return ingest.ErrFactoryEvent
	}
	chain := r.options.Chain
	chain.Pools = nil
	chain.DynamicCatalog = &r.options.Config
	if chain.NativeProof != nil {
		limits := *chain.NativeProof
		limits.MaxBytes = min(limits.MaxBytes, int(r.options.Config.MaxBytes))
		chain.NativeProof = &limits
	}
	adapter := evm.Adapter{Client: client, Chain: chain}
	current, err := adapter.Block(ctx, proof.Header.Number)
	if err != nil {
		return err
	}
	// Bind the archived receipts root and transaction order to today's canonical
	// HTTP header at the creation height, including during restart/reorg replay.
	expected, _ := json.Marshal(proof.Header)
	actual, _ := json.Marshal(current)
	if string(expected) != string(actual) {
		return ingest.ErrFactoryStale
	}
	if chain.BlockIntegrity == evm.ChainNativeCommitmentsV2 {
		return adapter.RecheckNativeReceiptEvidence(ctx, current, *proof.NativeEvidence)
	}
	return ctx.Err()
}
