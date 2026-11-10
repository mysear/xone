package evm

import (
	"context"
	"encoding/binary"
	"encoding/hex"
	"errors"
	"math/big"
	"sort"
	"strings"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/domain"
	"xone/chain-parser-go/internal/ingest"
)

// ReadFacts reads public execution facts for an explicit historical height.
// It never calls eth_call, getStorageAt or historical account APIs. Coverage is
// the configured V2 pool scope, not all protocols or arbitrary account state.
// The outer deadline includes all receipts, witnesses and the final anchor read.
func (a *Adapter) ReadFacts(ctx context.Context, tag string) (domain.FactBatch, error) {
	facts, _, err := a.ReadFactsAndCatalog(ctx, tag)
	return facts, err
}

// ReadFactsAndCatalog also returns creation-event hints from configured factory
// deployments. These are history candidates, not admitted current pool states.
func (a *Adapter) ReadFactsAndCatalog(ctx context.Context, tag string) (domain.FactBatch, []ingest.FactoryCandidate, error) {
	var empty domain.FactBatch
	if _, e := canonicalIndex(tag); e != nil {
		return empty, nil, errors.New("HISTORY_EXPLICIT_HEIGHT_REQUIRED")
	}
	if a.Client == nil || a.Client.HTTP == nil || a.Client.HTTP.Timeout <= 0 || a.Client.MaxBytes <= 0 {
		return empty, nil, errors.New("HISTORY_SOURCE_BUDGET")
	}
	ctx, cancel := context.WithTimeout(ctx, a.Client.HTTP.Timeout)
	defer cancel()
	b, e := a.Block(ctx, tag)
	if e != nil {
		return empty, nil, e
	}
	if b.Number != tag {
		return empty, nil, errors.New("HISTORY_BLOCK_HEIGHT")
	}
	ref, e := blockReference(a.Chain.Key, b)
	if e != nil {
		return empty, nil, e
	}
	var receipts []Receipt
	b, receipts, e = a.ReadQualifiedReceipts(ctx, b, false)
	if e != nil {
		return empty, nil, e
	}
	facts, e := DecodeSwapFacts(ref, receipts, a.Chain.Pools)
	if e != nil {
		return empty, nil, e
	}
	after, e := a.recheckBlock(ctx, b)
	if e != nil {
		return empty, nil, e
	}
	if !sameReceiptAnchor(b, after) {
		return empty, nil, errors.New("HISTORY_ANCHOR_CHANGED")
	}
	batch := domain.FactBatch{SchemaVersion: 1, DecoderVersion: "uniswap-v2/1", Blocks: []domain.FactBlock{{Block: ref, Swaps: facts}}}
	if e = ctx.Err(); e != nil {
		return empty, nil, e
	}
	if e = batch.Validate("BLOCK_COMMIT", ref); e != nil {
		return empty, nil, e
	}
	candidates, e := a.historyCandidates(ctx, ref, receipts)
	if e != nil {
		return empty, nil, e
	}
	if a.Chain.BlockIntegrity == ChainNativeCommitmentsV2 {
		if a.OnHistoryEvidence == nil {
			return empty, nil, errors.New("HISTORY_NATIVE_EVIDENCE_SINK_REQUIRED")
		}
		record, err := buildNativeHistoryEvidence(a.Chain, b, batch, candidates)
		if err != nil {
			return empty, nil, err
		}
		if err = a.OnHistoryEvidence(ctx, record); err != nil {
			return empty, nil, err
		}
	}
	return batch, candidates, nil
}

func (a *Adapter) historyCandidates(ctx context.Context, ref domain.BlockRef, receipts []Receipt) ([]ingest.FactoryCandidate, error) {
	cfg := a.Chain.DynamicCatalog
	if cfg == nil {
		return nil, nil
	}
	if cfg.MaxCandidates < 1 || len(cfg.Sources) > 256 {
		return nil, errors.New("HISTORY_CATALOG_BUDGET")
	}
	max := cfg.MaxCandidates
	if max > 1024 {
		max = 1024
	}
	sources := map[string]int{}
	for i, s := range cfg.Sources {
		sources[strings.ToLower(s.Address)] = i
	}
	out := []ingest.FactoryCandidate{}
	for _, receipt := range receipts {
		for _, log := range receipt.Logs {
			if e := ctx.Err(); e != nil {
				return nil, e
			}
			i, ok := sources[strings.ToLower(log.Address)]
			if !ok || len(log.Topics) == 0 {
				continue
			}
			s := cfg.Sources[i]
			if ref.Number < s.Since || s.Until != 0 && ref.Number > s.Until {
				continue
			}
			topic := strings.ToLower(log.Topics[0])
			if topic != ingest.V2PairCreatedTopic && topic != ingest.V3PoolCreatedTopic {
				continue
			}
			v3 := s.Version == "uniswap-v3"
			if s.Version != "uniswap-v2" && !v3 || !v3 && topic != ingest.V2PairCreatedTopic || v3 && topic != ingest.V3PoolCreatedTopic {
				return nil, errors.New("HISTORY_FACTORY_VERSION")
			}
			count := 3
			if v3 {
				count = 4
			}
			if len(log.Topics) != count {
				return nil, errors.New("HISTORY_FACTORY_TOPICS")
			}
			token0, e := historyWordAddress(log.Topics[1])
			if e != nil {
				return nil, e
			}
			token1, e := historyWordAddress(log.Topics[2])
			if e != nil || token0 >= token1 {
				return nil, errors.New("HISTORY_FACTORY_TOKENS")
			}
			data, e := hex.DecodeString(strings.TrimPrefix(log.Data, "0x"))
			if e != nil || len(data) != 64 {
				return nil, errors.New("HISTORY_FACTORY_DATA")
			}
			tx, _ := canonicalIndex(log.TxIndex)
			index, _ := canonicalIndex(log.Index)
			c := ingest.FactoryCandidate{ID: domain.SwapEventID(ref, tx, index), Chain: ref.ChainKey, Factory: strings.ToLower(s.Address), Version: s.Version, ProvenanceID: s.ProvenanceID, Height: ref.Number, BlockHash: ref.Hash, TransactionHash: log.TxHash, TransactionIndex: tx, LogIndex: index, Token0: token0, Token1: token1}
			poolWord := data[:32]
			if v3 {
				fee, e := hex.DecodeString(strings.TrimPrefix(log.Topics[3], "0x"))
				if e != nil || len(fee) != 32 || new(big.Int).SetBytes(fee[:29]).Sign() != 0 || new(big.Int).SetBytes(data[:29]).Sign() != 0 {
					return nil, errors.New("HISTORY_FACTORY_V3_WORD")
				}
				c.Fee = binary.BigEndian.Uint32(fee[28:])
				c.TickSpacing = int32(binary.BigEndian.Uint32(data[28:32]))
				if c.Fee == 0 || c.Fee >= 1000000 || c.TickSpacing <= 0 || c.TickSpacing >= 16384 {
					return nil, errors.New("HISTORY_FACTORY_V3_RANGE")
				}
				poolWord = data[32:]
			} else {
				ordinal := new(big.Int).SetBytes(data[32:])
				if ordinal.Sign() == 0 {
					return nil, errors.New("HISTORY_FACTORY_ORDINAL")
				}
				c.Ordinal = ordinal.String()
			}
			c.Pool, e = historyWordAddress("0x" + hex.EncodeToString(poolWord))
			if e != nil {
				return nil, e
			}
			if len(out) >= max {
				return nil, errors.New("HISTORY_CATALOG_BUDGET")
			}
			out = append(out, c)
		}
	}
	return out, nil
}
func historyWordAddress(word string) (string, error) {
	b, e := hex.DecodeString(strings.TrimPrefix(word, "0x"))
	if e != nil || len(b) != 32 || new(big.Int).SetBytes(b[:12]).Sign() != 0 || new(big.Int).SetBytes(b[12:]).Sign() == 0 {
		return "", errors.New("HISTORY_FACTORY_ADDRESS")
	}
	return "0x" + hex.EncodeToString(b[12:]), nil
}

// HistoryDatasetID names the exact configured V2 facts/factory candidate scope.
// It includes decoder and integrity semantics, deployment intervals/provenance,
// tokens and pool identity. Reordering does not change it. It is not a market-
// completeness claim; pools discovered later require an explicit scope update.
func HistoryDatasetID(chain config.Chain) (string, error) {
	if domain.ValidateChain(chain.Key) != nil || len(chain.Pools) > 100000 {
		return "", errors.New("HISTORY_SCOPE_LIMIT")
	}
	pools := []config.PoolConfig{}
	seen := map[string]bool{}
	for _, p := range chain.Pools {
		if p.Protocol != "uniswap-v2" && p.Protocol != domain.V2CompleteProtocol {
			continue
		}
		key := strings.ToLower(p.ID)
		if key == "" || seen[key] {
			return "", errors.New("HISTORY_SCOPE_DUPLICATE")
		}
		seen[key] = true
		pools = append(pools, p)
	}
	sort.Slice(pools, func(i, j int) bool { return pools[i].ID < pools[j].ID })
	var factories *config.DynamicCatalogConfig
	if chain.DynamicCatalog != nil {
		copy := *chain.DynamicCatalog
		if len(copy.Sources) > 256 {
			return "", errors.New("HISTORY_FACTORY_SCOPE_LIMIT")
		}
		copy.Sources = append([]config.FactoryDeployment(nil), copy.Sources...)
		factoriesSeen := map[string]bool{}
		for _, source := range copy.Sources {
			key := strings.ToLower(source.Address)
			if key == "" || factoriesSeen[key] {
				return "", errors.New("HISTORY_FACTORY_SCOPE_DUPLICATE")
			}
			factoriesSeen[key] = true
		}
		sort.Slice(copy.Sources, func(i, j int) bool { return copy.Sources[i].Address < copy.Sources[j].Address })
		factories = &copy
	}
	scope, err := domain.Hash(struct {
		Version, Chain, Genesis, Integrity string
		Pools                              []config.PoolConfig
		Factories                          *config.DynamicCatalogConfig
	}{"uniswap-v2/1+factory-creation/1+chain-receipt-root-witness/1", chain.Key, chain.Genesis, chain.BlockIntegrity, pools, factories})
	if err != nil {
		return "", err
	}
	if chain.BlockIntegrity != ChainNativeCommitmentsV2 {
		return scope, nil
	}
	registry, err := nativeRegistryID(chain.Key)
	if err != nil {
		return "", err
	}
	return domain.Hash(struct{ Version, ClosedScope, NativeRegistry string }{"native-history-scope/2", scope, registry})

}
