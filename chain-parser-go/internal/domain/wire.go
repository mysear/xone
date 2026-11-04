package domain

import (
	"encoding/hex"
	"errors"
	"math/big"
	"strings"
	pb "xone/common/protoc/golang/dexpb"
)

const base58Alphabet = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"

func decode58(s string) ([]byte, error) {
	if len(s) < 32 || len(s) > 44 {
		return nil, errors.New("BLOCK_HASH_WIDTH")
	}
	n := new(big.Int)
	for _, c := range s {
		i := strings.IndexRune(base58Alphabet, c)
		if i < 0 {
			return nil, errors.New("BLOCK_HASH_ENCODING")
		}
		n.Mul(n, big.NewInt(58))
		n.Add(n, big.NewInt(int64(i)))
	}
	raw := n.Bytes()
	zeros := 0
	for zeros < len(s) && s[zeros] == '1' {
		zeros++
	}
	if zeros+len(raw) != 32 {
		return nil, errors.New("BLOCK_HASH_WIDTH")
	}
	return append(make([]byte, zeros), raw...), nil
}
func encode58(raw []byte) string {
	n := new(big.Int).SetBytes(raw)
	b := big.NewInt(58)
	remainder := new(big.Int)
	out := ""
	for n.Sign() != 0 {
		n.QuoRem(n, b, remainder)
		out = string(base58Alphabet[remainder.Int64()]) + out
	}
	for _, v := range raw {
		if v != 0 {
			break
		}
		out = "1" + out
	}
	return out
}
func digest32(s string) ([]byte, error) {
	raw, e := hex.DecodeString(strings.TrimPrefix(s, "0x"))
	if e != nil || len(raw) != 32 {
		return nil, errors.New("WIRE_DIGEST_WIDTH")
	}
	return raw, nil
}
func blockHash(chain, s string) ([]byte, error) {
	if strings.HasPrefix(chain, "evm:") {
		return digest32(s)
	}
	if strings.HasPrefix(chain, "solana:") {
		return decode58(s)
	}
	return nil, errors.New("INVALID_CHAIN_KEY")
}
func RefToProto(r SnapshotRef) (*pb.SnapshotRef, error) {
	h, e := blockHash(r.Block.ChainKey, r.Block.Hash)
	if e != nil {
		return nil, e
	}
	p, e := blockHash(r.Block.ChainKey, r.Block.ParentHash)
	if e != nil {
		return nil, e
	}
	a, e := digest32(r.AdapterSetHash)
	if e != nil {
		return nil, e
	}
	scope, e := digest32(r.ScopeHash)
	if e != nil {
		return nil, e
	}
	return &pb.SnapshotRef{Block: &pb.BlockRef{ChainKey: r.Block.ChainKey, NumberOrSlot: r.Block.Number, Hash: h, ParentHash: p, TimestampMs: r.Block.TimestampMS}, CanonicalEpoch: r.CanonicalEpoch, JournalSeq: r.JournalSeq, SchemaVersion: r.SchemaVersion, AdapterSetHash: a, OnlineGeneration: r.OnlineGeneration, ScopeHash: scope, WriterEpoch: r.WriterEpoch, OwnerService: r.OwnerService, OwnerAssignmentVersion: r.OwnerAssignmentVersion}, nil
}
func RefFromProto(r *pb.SnapshotRef) (SnapshotRef, error) {
	if r == nil || r.Block == nil || len(r.Block.Hash) != 32 || len(r.Block.ParentHash) != 32 || len(r.AdapterSetHash) != 32 || len(r.ScopeHash) != 32 {
		return SnapshotRef{}, errors.New("WIRE_REFERENCE_REQUIRED")
	}
	if ValidateChain(r.Block.ChainKey) != nil {
		return SnapshotRef{}, errors.New("INVALID_CHAIN_KEY")
	}
	h, p := "0x"+hex.EncodeToString(r.Block.Hash), "0x"+hex.EncodeToString(r.Block.ParentHash)
	if strings.HasPrefix(r.Block.ChainKey, "solana:") {
		h, p = encode58(r.Block.Hash), encode58(r.Block.ParentHash)
	}
	return SnapshotRef{Block: BlockRef{ChainKey: r.Block.ChainKey, Number: r.Block.NumberOrSlot, Hash: h, ParentHash: p, TimestampMS: r.Block.TimestampMs}, CanonicalEpoch: r.CanonicalEpoch, JournalSeq: r.JournalSeq, SchemaVersion: r.SchemaVersion, AdapterSetHash: hex.EncodeToString(r.AdapterSetHash), OnlineGeneration: r.OnlineGeneration, ScopeHash: hex.EncodeToString(r.ScopeHash), WriterEpoch: r.WriterEpoch, OwnerService: r.OwnerService, OwnerAssignmentVersion: r.OwnerAssignmentVersion}, nil
}
func SnapshotToProto(s Snapshot) (*pb.OnlineSnapshot, error) {
	r, e := RefToProto(s.Reference)
	if e != nil {
		return nil, e
	}
	out := &pb.OnlineSnapshot{Reference: r, Source: s.Source}
	for _, p := range s.Pools {
		bin, e := InfinityBinToProto(p.InfinityBin)
		if e != nil {
			return nil, e
		}
		r0, e := p.Reserve0.Bytes()
		if e != nil {
			return nil, e
		}
		r1, e := p.Reserve1.Bytes()
		if e != nil {
			return nil, e
		}
		clmm, e := RaydiumCLMMToProto(p.RaydiumCLMM)
		if e != nil {
			return nil, e
		}
		orca, e := OrcaWhirlpoolToProto(p.OrcaWhirlpool)
		if e != nil {
			return nil, e
		}
		raydium, e := RaydiumCPMMToProto(p.RaydiumCPMM)
		if e != nil {
			return nil, e
		}
		singleton, e := SingletonCLToProto(p.SingletonCL)
		if e != nil {
			return nil, e
		}
		v3, e := V3ToProto(p.V3)
		if e != nil {
			return nil, e
		}
		advanced, e := SingletonAdvancedToProto(p.SingletonAdvanced)
		if e != nil {
			return nil, e
		}
		multi, e := MultiAssetToProto(p.MultiAsset)
		if e != nil {
			return nil, e
		}
		weighted, e := WeightedToProto(p.Weighted)
		if e != nil {
			return nil, e
		}
		stable, e := StableToProto(p.Stable)
		if e != nil {
			return nil, e
		}
		var v2 *pb.V2State
		if p.V2 != nil {
			if e = p.V2.Validate(); e != nil {
				return nil, e
			}
			code, _ := hex.DecodeString(p.V2.CodeSHA256)
			swap, err := V2SwapToProto(p.V2.Swap)
			if err != nil {
				return nil, err
			}
			v2 = &pb.V2State{Factory: p.V2.Factory, CodeSha256: code, Swap: swap}
		}
		out.Pools = append(out.Pools, &pb.PoolState{InfinityBin: bin, SingletonAdvanced: advanced, MultiAsset: multi, Weighted: weighted, RaydiumClmm: clmm, OrcaWhirlpool: orca, RaydiumCpmm: raydium, SingletonCl: singleton, Stable: stable, V2: v2, V3: v3, ChainKey: p.ChainKey, PoolId: p.PoolID, Protocol: p.Protocol, Token0: p.Token0, Token1: p.Token1, Reserve0: &pb.Amount{Uint256Be: r0}, Reserve1: &pb.Amount{Uint256Be: r1}, FeeNumerator: p.FeeNumerator, FeeDenominator: p.FeeDenominator, AdapterVersion: p.AdapterVersion, TokenBehavior: p.TokenBehavior})
	}
	for _, g := range s.HistoryGaps {
		out.HistoryGaps = append(out.HistoryGaps, &pb.HistoryGap{From: g.From, To: g.To, Reason: g.Reason})
	}
	return out, nil
}
func SnapshotFromProto(s *pb.OnlineSnapshot) (Snapshot, error) {
	if s == nil {
		return Snapshot{}, errors.New("WIRE_SNAPSHOT_REQUIRED")
	}
	r, e := RefFromProto(s.Reference)
	if e != nil {
		return Snapshot{}, e
	}
	out := Snapshot{Reference: r, Source: s.Source, Pools: []Pool{}, HistoryGaps: []Gap{}}
	for _, p := range s.Pools {
		if p == nil || p.Reserve0 == nil || p.Reserve1 == nil {
			return Snapshot{}, errors.New("WIRE_POOL_REQUIRED")
		}
		bin, e := InfinityBinFromProto(p.InfinityBin)
		if e != nil {
			return Snapshot{}, e
		}
		a, e := AmountFromBytes(p.Reserve0.Uint256Be)
		if e != nil {
			return Snapshot{}, e
		}
		b, e := AmountFromBytes(p.Reserve1.Uint256Be)
		if e != nil {
			return Snapshot{}, e
		}
		clmm, e := RaydiumCLMMFromProto(p.RaydiumClmm)
		if e != nil {
			return Snapshot{}, e
		}
		orca, e := OrcaWhirlpoolFromProto(p.OrcaWhirlpool)
		if e != nil {
			return Snapshot{}, e
		}
		raydium, e := RaydiumCPMMFromProto(p.RaydiumCpmm)
		if e != nil {
			return Snapshot{}, e
		}
		singleton, e := SingletonCLFromProto(p.SingletonCl)
		if e != nil {
			return Snapshot{}, e
		}
		v3, e := V3FromProto(p.V3)
		if e != nil {
			return Snapshot{}, e
		}
		advanced, e := SingletonAdvancedFromProto(p.SingletonAdvanced)
		if e != nil {
			return Snapshot{}, e
		}
		multi, e := MultiAssetFromProto(p.MultiAsset)
		if e != nil {
			return Snapshot{}, e
		}
		weighted, e := WeightedFromProto(p.Weighted)
		if e != nil {
			return Snapshot{}, e
		}
		stable, e := StableFromProto(p.Stable)
		if e != nil {
			return Snapshot{}, e
		}
		var v2 *V2State
		if p.V2 != nil {
			if len(p.V2.CodeSha256) != 32 {
				return Snapshot{}, errors.New("POOL_CODE_IDENTITY")
			}
			swap, err := V2SwapFromProto(p.V2.Swap)
			if err != nil {
				return Snapshot{}, err
			}
			v2 = &V2State{Factory: p.V2.Factory, CodeSHA256: hex.EncodeToString(p.V2.CodeSha256), Swap: swap}
		}
		out.Pools = append(out.Pools, Pool{InfinityBin: bin, SingletonAdvanced: advanced, MultiAsset: multi, Weighted: weighted, RaydiumCLMM: clmm, OrcaWhirlpool: orca, RaydiumCPMM: raydium, SingletonCL: singleton, Stable: stable, V2: v2, V3: v3, ChainKey: p.ChainKey, PoolID: p.PoolId, Protocol: p.Protocol, Token0: p.Token0, Token1: p.Token1, Reserve0: a, Reserve1: b, FeeNumerator: p.FeeNumerator, FeeDenominator: p.FeeDenominator, AdapterVersion: p.AdapterVersion, TokenBehavior: p.TokenBehavior})
	}
	for _, g := range s.HistoryGaps {
		if g == nil {
			return Snapshot{}, errors.New("WIRE_GAP_REQUIRED")
		}
		out.HistoryGaps = append(out.HistoryGaps, Gap{From: g.From, To: g.To, Reason: g.Reason})
	}
	return out, out.Validate(r.OwnerService, r.OwnerAssignmentVersion)
}
