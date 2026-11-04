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
	scope := map[string]config.PoolConfig{}
	for _, p := range pools {
		scope[strings.ToLower(p.ID)] = p
	}
	facts := []domain.SwapFact{}
	for _, receipt := range receipts {
		for _, log := range receipt.Logs {
			p, ok := scope[strings.ToLower(log.Address)]
			if !ok || (p.Protocol != "uniswap-v2" && p.Protocol != domain.V2CompleteProtocol) {
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
