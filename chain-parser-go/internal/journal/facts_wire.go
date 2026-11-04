package journal

import (
	"encoding/hex"
	"errors"
	"strings"
	"xone/chain-parser-go/internal/domain"
	pb "xone/common/protoc/golang/dexpb"
)

func factBlockToProto(b domain.BlockRef) (*pb.BlockRef, error) {
	h, e := hex.DecodeString(strings.TrimPrefix(b.Hash, "0x"))
	if e != nil || len(h) != 32 {
		return nil, errors.New("FACT_HASH_WIDTH")
	}
	p, e := hex.DecodeString(strings.TrimPrefix(b.ParentHash, "0x"))
	if e != nil || len(p) != 32 {
		return nil, errors.New("FACT_HASH_WIDTH")
	}
	return &pb.BlockRef{ChainKey: b.ChainKey, NumberOrSlot: b.Number, Hash: h, ParentHash: p, TimestampMs: b.TimestampMS}, nil
}
func factBlockFromProto(b *pb.BlockRef) (domain.BlockRef, error) {
	if b == nil || len(b.Hash) != 32 || len(b.ParentHash) != 32 {
		return domain.BlockRef{}, errors.New("FACT_HASH_WIDTH")
	}
	return domain.BlockRef{ChainKey: b.ChainKey, Number: b.NumberOrSlot, Hash: "0x" + hex.EncodeToString(b.Hash), ParentHash: "0x" + hex.EncodeToString(b.ParentHash), TimestampMS: b.TimestampMs}, nil
}
func factsToProto(f *domain.FactBatch) (*pb.FactBatch, error) {
	if f == nil {
		return nil, nil
	}
	out := &pb.FactBatch{SchemaVersion: f.SchemaVersion, DecoderVersion: f.DecoderVersion}
	for _, block := range f.Blocks {
		b, e := factBlockToProto(block.Block)
		if e != nil {
			return nil, e
		}
		entry := &pb.FactBlock{Block: b}
		for _, s := range block.Swaps {
			tx, e := hex.DecodeString(strings.TrimPrefix(s.TxHash, "0x"))
			if e != nil || len(tx) != 32 {
				return nil, errors.New("FACT_TX_WIDTH")
			}
			amounts := []*pb.Amount{}
			for _, amount := range []domain.Amount{s.Amount0In, s.Amount1In, s.Amount0Out, s.Amount1Out} {
				raw, e := amount.Bytes()
				if e != nil {
					return nil, e
				}
				amounts = append(amounts, &pb.Amount{Uint256Be: raw})
			}
			entry.Swaps = append(entry.Swaps, &pb.SwapFact{EventId: s.EventID, TxHash: tx, TxIndex: s.TxIndex, LogIndex: s.LogIndex, PoolId: s.PoolID, Token0: s.Token0, Token1: s.Token1, Amount0In: amounts[0], Amount1In: amounts[1], Amount0Out: amounts[2], Amount1Out: amounts[3]})
		}
		out.Blocks = append(out.Blocks, entry)
	}
	for _, b := range f.RevertedBlocks {
		v, e := factBlockToProto(b)
		if e != nil {
			return nil, e
		}
		out.RevertedBlocks = append(out.RevertedBlocks, v)
	}
	return out, nil
}
func factsFromProto(f *pb.FactBatch) (*domain.FactBatch, error) {
	if f == nil {
		return nil, nil
	}
	out := &domain.FactBatch{SchemaVersion: f.SchemaVersion, DecoderVersion: f.DecoderVersion, Blocks: []domain.FactBlock{}, RevertedBlocks: []domain.BlockRef{}}
	for _, b := range f.Blocks {
		block, e := factBlockFromProto(b.Block)
		if e != nil {
			return nil, e
		}
		entry := domain.FactBlock{Block: block, Swaps: []domain.SwapFact{}}
		for _, s := range b.Swaps {
			if len(s.TxHash) != 32 {
				return nil, errors.New("FACT_TX_WIDTH")
			}
			amounts := []domain.Amount{}
			for _, a := range []*pb.Amount{s.Amount0In, s.Amount1In, s.Amount0Out, s.Amount1Out} {
				if a == nil {
					return nil, errors.New("FACT_AMOUNT_REQUIRED")
				}
				v, e := domain.AmountFromBytes(a.Uint256Be)
				if e != nil {
					return nil, e
				}
				amounts = append(amounts, v)
			}
			entry.Swaps = append(entry.Swaps, domain.SwapFact{EventID: s.EventId, TxHash: "0x" + hex.EncodeToString(s.TxHash), TxIndex: s.TxIndex, LogIndex: s.LogIndex, PoolID: s.PoolId, Token0: s.Token0, Token1: s.Token1, Amount0In: amounts[0], Amount1In: amounts[1], Amount0Out: amounts[2], Amount1Out: amounts[3]})
		}
		out.Blocks = append(out.Blocks, entry)
	}
	for _, b := range f.RevertedBlocks {
		v, e := factBlockFromProto(b)
		if e != nil {
			return nil, e
		}
		out.RevertedBlocks = append(out.RevertedBlocks, v)
	}
	return out, nil
}
func (u Unit) ValidateFacts() error {
	if u.NormalizedFacts == nil {
		return nil
	}
	if e := u.NormalizedFacts.Validate(u.Kind, u.Snapshot.Reference.Block); e != nil {
		return e
	}
	pools := map[string]domain.Pool{}
	for _, p := range u.Snapshot.Pools {
		pools[p.PoolID] = p
	}
	for _, b := range u.NormalizedFacts.Blocks {
		for _, f := range b.Swaps {
			p, ok := pools[f.PoolID]
			if !ok || p.Protocol != "uniswap-v2" || p.Token0 != f.Token0 || p.Token1 != f.Token1 {
				return errors.New("FACT_POOL_SCOPE")
			}
		}
	}
	return nil
}
