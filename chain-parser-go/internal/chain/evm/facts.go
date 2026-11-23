package evm

import (
	"encoding/hex"
	"errors"
	"sort"
	"strings"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/domain"
)

// keccak256("Swap(address,uint256,uint256,uint256,uint256,address)").
const V2SwapTopic = "0xd78ad95fa46c994b6551d0da85fc275fe613ce37657fb8d5e3d130840159d822"

func DecodeSwapFacts(block domain.BlockRef, receipts []Receipt, pools []config.PoolConfig) ([]domain.SwapFact, error) {
	return decodeSwapFacts(block, receipts, pools, domain.LegacyV2SwapDecoder)
}

func nativeBNBSwapScope(chain config.Chain) bool {
	return chain.Key == "evm:56" && chain.BlockIntegrity == ChainNativeCommitmentsV2
}

func pancakeV2FactPool(p config.PoolConfig) bool {
	return p.Protocol == "pancake-v2" && p.V2 != nil && domain.ValidateEVMIdentity(p.V2.ExpectedFactory, p.V2.ExpectedCodeSHA256) == nil
}

func swapDecoderVersion(chain config.Chain) string {
	if nativeBNBSwapScope(chain) {
		for _, p := range chain.Pools {
			if pancakeV2FactPool(p) {
				return domain.NativeBNBV2SwapDecoder
			}
		}
	}
	return domain.LegacyV2SwapDecoder
}

// V2JournalDecoder identifies the fact coverage of the admitted snapshot scope.
// Complete V2 state remains on its existing history decoder and is not part of
// the V2 online journal feed.
func V2JournalDecoder(chain config.Chain, pools []domain.Pool) string {
	var version string
	for _, p := range pools {
		if p.Protocol == "uniswap-v2" {
			version = domain.LegacyV2SwapDecoder
		}
		if nativeBNBSwapScope(chain) && p.ChainKey == chain.Key && p.Protocol == "pancake-v2" && p.V2 != nil {
			return domain.NativeBNBV2SwapDecoder
		}
	}
	return version
}

func V2JournalFacts(chain config.Chain, pools []domain.Pool, blocks []domain.FactBlock, reverted []domain.BlockRef) *domain.FactBatch {
	version := V2JournalDecoder(chain, pools)
	if version == "" {
		return nil
	}
	// Keep the history-only complete V2 projection outside the journal feed.
	// Unrecognized pool IDs still reach the journal's strict pool validation.
	historyOnly := map[string]bool{}
	for _, p := range pools {
		if p.Protocol == domain.V2CompleteProtocol {
			historyOnly[p.PoolID] = true
		}
	}
	if len(historyOnly) != 0 {
		blocks = append([]domain.FactBlock{}, blocks...)
		for i, b := range blocks {
			blocks[i].Swaps = []domain.SwapFact{}
			for _, swap := range b.Swaps {
				if !historyOnly[swap.PoolID] {
					blocks[i].Swaps = append(blocks[i].Swaps, swap)
				}
			}
		}
	}
	return &domain.FactBatch{SchemaVersion: 1, DecoderVersion: version, Blocks: blocks, RevertedBlocks: reverted}
}

func decodeSwapFacts(block domain.BlockRef, receipts []Receipt, pools []config.PoolConfig, version string) ([]domain.SwapFact, error) {
	scope := map[string]config.PoolConfig{}
	for _, p := range pools {
		if p.Protocol == "uniswap-v2" || p.Protocol == domain.V2CompleteProtocol ||
			(version == domain.NativeBNBV2SwapDecoder && pancakeV2FactPool(p)) {
			scope[strings.ToLower(p.ID)] = p
		}
	}
	facts := []domain.SwapFact{}
	for _, receipt := range receipts {
		for _, log := range receipt.Logs {
			p, ok := scope[strings.ToLower(log.Address)]
			if !ok {
				continue
			}
			if len(log.Topics) == 0 || strings.ToLower(log.Topics[0]) != V2SwapTopic {
				continue
			}
			if len(log.Topics) != 3 {
				return nil, errors.New("SWAP_ABI_TOPICS")
			}
			for _, topic := range log.Topics[1:] {
				b, e := hex.DecodeString(strings.TrimPrefix(topic, "0x"))
				if e != nil || len(b) != 32 || strings.Trim(hex.EncodeToString(b[:12]), "0") != "" {
					return nil, errors.New("SWAP_ABI_ADDRESS")
				}
			}
			raw, e := hex.DecodeString(strings.TrimPrefix(log.Data, "0x"))
			if e != nil || len(raw) != 128 {
				return nil, errors.New("SWAP_ABI_AMOUNTS")
			}
			tx, e := quantity(log.TxIndex)
			if e != nil {
				return nil, e
			}
			index, e := quantity(log.Index)
			if e != nil {
				return nil, e
			}
			amounts := []domain.Amount{}
			for i := 0; i < 4; i++ {
				a, e := domain.AmountFromBytes(raw[i*32 : (i+1)*32])
				if e != nil {
					return nil, e
				}
				amounts = append(amounts, a)
			}
			facts = append(facts, domain.SwapFact{EventID: domain.SwapEventID(block, tx, index), TxHash: log.TxHash, TxIndex: tx, LogIndex: index, PoolID: p.ID, Token0: p.Token0, Token1: p.Token1, Amount0In: amounts[0], Amount1In: amounts[1], Amount0Out: amounts[2], Amount1Out: amounts[3]})
		}
	}
	sort.Slice(facts, func(i, j int) bool { return facts[i].LogIndex < facts[j].LogIndex })
	return facts, nil
}
