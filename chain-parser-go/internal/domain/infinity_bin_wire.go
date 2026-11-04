package domain

import (
	"encoding/hex"
	"errors"
	"strings"
	pb "xone/common/protoc/golang/dexpb"
)

func InfinityBinToProto(s *InfinityBinState) (*pb.InfinityBinState, error) {
	if s == nil {
		return nil, nil
	}
	if err := s.validateWire(); err != nil {
		return nil, err
	}
	raw := func(v string) []byte { b, _ := hex.DecodeString(strings.TrimPrefix(v, "0x")); return b }
	amount := func(v Amount) *pb.Amount { b, _ := v.Bytes(); return &pb.Amount{Uint256Be: b} }
	out := &pb.InfinityBinState{Manager: s.Manager, ManagerCodeSha256: raw(s.ManagerCodeSHA256), Vault: s.Vault, VaultCodeSha256: raw(s.VaultCodeSHA256), PoolId: raw(s.PoolID), Hooks: s.Hooks, Parameters: raw(s.Parameters), Token0CodeSha256: raw(s.Token0CodeSHA256), Token1CodeSha256: raw(s.Token1CodeSHA256), BinStep: s.BinStep, ActiveId: s.ActiveID, LpFee: s.LPFee, ProtocolFee0: s.ProtocolFee0, ProtocolFee1: s.ProtocolFee1, MaxBins: s.MaxBins, MaxTreeWords: s.MaxTreeWords, MaxSteps: s.MaxSteps, TreeLevel0: amount(s.TreeLevel0), AppReserve0: amount(s.AppReserve0), AppReserve1: amount(s.AppReserve1), ProtocolFees0: amount(s.ProtocolFees0), ProtocolFees1: amount(s.ProtocolFees1), BlockNumber: s.BlockNumber, Timestamp: s.Timestamp, BlockHash: raw(s.BlockHash)}
	for _, w := range s.TreeLevel1 {
		out.TreeLevel1 = append(out.TreeLevel1, &pb.InfinityBinTreeWord{Index: w.Index, Value: amount(w.Value)})
	}
	for _, w := range s.TreeLevel2 {
		out.TreeLevel2 = append(out.TreeLevel2, &pb.InfinityBinTreeWord{Index: w.Index, Value: amount(w.Value)})
	}
	for _, b := range s.Bins {
		out.Bins = append(out.Bins, &pb.InfinityBinEntry{Id: b.ID, Reserve0: amount(b.Reserve0), Reserve1: amount(b.Reserve1), TotalShares: amount(b.TotalShares)})
	}
	if b := s.DetachedActive; b != nil {
		out.DetachedActive = &pb.InfinityBinEntry{Id: b.ID, Reserve0: amount(b.Reserve0), Reserve1: amount(b.Reserve1), TotalShares: amount(b.TotalShares)}
	}
	return out, nil
}
func InfinityBinFromProto(s *pb.InfinityBinState) (*InfinityBinState, error) {
	if s == nil {
		return nil, nil
	}
	for _, raw := range [][]byte{s.ManagerCodeSha256, s.VaultCodeSha256, s.PoolId, s.Parameters, s.Token0CodeSha256, s.Token1CodeSha256, s.BlockHash} {
		if len(raw) != 32 {
			return nil, errors.New("INFINITY_BIN_WIRE_DIGEST_WIDTH")
		}
	}
	amount := func(v *pb.Amount) (Amount, error) {
		if v == nil {
			return "", errors.New("INFINITY_BIN_WIRE_AMOUNT_REQUIRED")
		}
		return AmountFromBytes(v.Uint256Be)
	}
	out := &InfinityBinState{Manager: s.Manager, ManagerCodeSHA256: hex.EncodeToString(s.ManagerCodeSha256), Vault: s.Vault, VaultCodeSHA256: hex.EncodeToString(s.VaultCodeSha256), PoolID: "0x" + hex.EncodeToString(s.PoolId), Hooks: s.Hooks, Parameters: "0x" + hex.EncodeToString(s.Parameters), Token0CodeSHA256: hex.EncodeToString(s.Token0CodeSha256), Token1CodeSHA256: hex.EncodeToString(s.Token1CodeSha256), BinStep: s.BinStep, ActiveID: s.ActiveId, LPFee: s.LpFee, ProtocolFee0: s.ProtocolFee0, ProtocolFee1: s.ProtocolFee1, MaxBins: s.MaxBins, MaxTreeWords: s.MaxTreeWords, MaxSteps: s.MaxSteps, BlockNumber: s.BlockNumber, Timestamp: s.Timestamp, BlockHash: "0x" + hex.EncodeToString(s.BlockHash), TreeLevel1: []InfinityBinTreeWord{}, TreeLevel2: []InfinityBinTreeWord{}, Bins: []InfinityBinEntry{}}
	for _, field := range []struct {
		src *pb.Amount
		dst *Amount
	}{{s.TreeLevel0, &out.TreeLevel0}, {s.AppReserve0, &out.AppReserve0}, {s.AppReserve1, &out.AppReserve1}, {s.ProtocolFees0, &out.ProtocolFees0}, {s.ProtocolFees1, &out.ProtocolFees1}} {
		a, err := amount(field.src)
		if err != nil {
			return nil, err
		}
		*field.dst = a
	}
	for i, words := range [][]*pb.InfinityBinTreeWord{s.TreeLevel1, s.TreeLevel2} {
		for _, w := range words {
			if w == nil {
				return nil, errors.New("INFINITY_BIN_WIRE_WORD_REQUIRED")
			}
			a, err := amount(w.Value)
			if err != nil {
				return nil, err
			}
			word := InfinityBinTreeWord{Index: w.Index, Value: a}
			if i == 0 {
				out.TreeLevel1 = append(out.TreeLevel1, word)
			} else {
				out.TreeLevel2 = append(out.TreeLevel2, word)
			}
		}
	}
	entry := func(b *pb.InfinityBinEntry) (InfinityBinEntry, error) {
		if b == nil {
			return InfinityBinEntry{}, errors.New("INFINITY_BIN_WIRE_BIN_REQUIRED")
		}
		bin := InfinityBinEntry{ID: b.Id}
		for _, field := range []struct {
			src *pb.Amount
			dst *Amount
		}{{b.Reserve0, &bin.Reserve0}, {b.Reserve1, &bin.Reserve1}, {b.TotalShares, &bin.TotalShares}} {
			a, err := amount(field.src)
			if err != nil {
				return InfinityBinEntry{}, err
			}
			*field.dst = a
		}
		return bin, nil
	}
	for _, b := range s.Bins {
		bin, err := entry(b)
		if err != nil {
			return nil, err
		}
		out.Bins = append(out.Bins, bin)
	}
	if s.DetachedActive != nil {
		bin, err := entry(s.DetachedActive)
		if err != nil {
			return nil, err
		}
		out.DetachedActive = &bin
	}

	if err := out.validateWire(); err != nil {
		return nil, err
	}
	return out, nil
}
