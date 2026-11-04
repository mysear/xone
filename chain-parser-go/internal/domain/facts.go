package domain

import (
	"encoding/hex"
	"errors"
	"fmt"
	"strings"
)

type SwapFact struct {
	EventID    string `json:"event_id"`
	TxHash     string `json:"tx_hash"`
	TxIndex    uint64 `json:"tx_index"`
	LogIndex   uint64 `json:"log_index"`
	PoolID     string `json:"pool_id"`
	Token0     string `json:"token0"`
	Token1     string `json:"token1"`
	Amount0In  Amount `json:"amount0_in"`
	Amount1In  Amount `json:"amount1_in"`
	Amount0Out Amount `json:"amount0_out"`
	Amount1Out Amount `json:"amount1_out"`
}
type FactBlock struct {
	Block BlockRef   `json:"block"`
	Swaps []SwapFact `json:"swaps"`
}
type FactBatch struct {
	SchemaVersion  uint32      `json:"schema_version"`
	DecoderVersion string      `json:"decoder_version"`
	Blocks         []FactBlock `json:"blocks"`
	RevertedBlocks []BlockRef  `json:"reverted_blocks"`
}

func SwapEventID(b BlockRef, tx, log uint64) string {
	return fmt.Sprintf("%s/%s/%d/%d", b.ChainKey, b.Hash, tx, log)
}
func rawEVMHash(h string) bool {
	b, e := hex.DecodeString(strings.TrimPrefix(h, "0x"))
	return e == nil && len(b) == 32 && strings.HasPrefix(h, "0x") && h == strings.ToLower(h)
}
func validFactBlock(b BlockRef) bool {
	return ValidateChain(b.ChainKey) == nil && strings.HasPrefix(b.ChainKey, "evm:") && rawEVMHash(b.Hash) && rawEVMHash(b.ParentHash) && b.TimestampMS > 0
}
func (f FactBatch) Validate(kind string, tip BlockRef) error {
	if f.SchemaVersion != 1 || f.DecoderVersion != "uniswap-v2/1" || len(f.Blocks)+len(f.RevertedBlocks) > 100000 {
		return errors.New("FACT_SCHEMA_OR_LIMIT")
	}
	if kind != "BLOCK_COMMIT" && kind != "REORG_COMMIT" {
		return errors.New("FACT_KIND")
	}
	if kind == "BLOCK_COMMIT" && (len(f.Blocks) != 1 || len(f.RevertedBlocks) != 0) {
		return errors.New("FACT_BLOCK_REQUIRED")
	}
	seen := map[string]bool{}
	for i, b := range f.Blocks {
		if !validFactBlock(b.Block) || b.Block.ChainKey != tip.ChainKey {
			return errors.New("FACT_BLOCK_IDENTITY")
		}
		if i > 0 {
			p := f.Blocks[i-1].Block
			if b.Block.Number <= p.Number || b.Block.Number-p.Number != 1 || b.Block.ParentHash != p.Hash {
				return errors.New("FACT_BLOCK_SEQUENCE")
			}
		}
		for j, s := range b.Swaps {
			if s.EventID != SwapEventID(b.Block, s.TxIndex, s.LogIndex) || seen[s.EventID] || !rawEVMHash(s.TxHash) || s.PoolID == "" || s.Token0 == "" || s.Token1 == "" || s.Token0 == s.Token1 {
				return errors.New("FACT_EVENT_IDENTITY")
			}
			seen[s.EventID] = true
			if j > 0 {
				p := b.Swaps[j-1]
				if s.LogIndex <= p.LogIndex || s.TxIndex < p.TxIndex {
					return errors.New("FACT_EVENT_SEQUENCE")
				}
			}
			for _, amount := range []Amount{s.Amount0In, s.Amount1In, s.Amount0Out, s.Amount1Out} {
				if _, e := ParseAmount(string(amount)); e != nil {
					return e
				}
			}
		}
	}
	if len(f.Blocks) > 0 && f.Blocks[len(f.Blocks)-1].Block != tip {
		return errors.New("FACT_TIP_MISMATCH")
	}
	for i, b := range f.RevertedBlocks {
		if !validFactBlock(b) || b.ChainKey != tip.ChainKey {
			return errors.New("FACT_REVERT_IDENTITY")
		}
		if i > 0 {
			p := f.RevertedBlocks[i-1]
			if b.Number >= p.Number || p.Number-b.Number != 1 || p.ParentHash != b.Hash {
				return errors.New("FACT_REVERT_SEQUENCE")
			}
		}
	}
	if kind == "REORG_COMMIT" {
		if len(f.RevertedBlocks) == 0 {
			return errors.New("FACT_REVERT_REQUIRED")
		}
		last := f.RevertedBlocks[len(f.RevertedBlocks)-1]
		if len(f.Blocks) > 0 {
			first := f.Blocks[0].Block
			if first.Number != last.Number || first.ParentHash != last.ParentHash {
				return errors.New("FACT_REORG_ANCESTOR")
			}
		} else if tip.Hash != last.ParentHash || last.Number == 0 || tip.Number != last.Number-1 {
			return errors.New("FACT_REORG_ANCESTOR")
		}
	}
	return nil
}
