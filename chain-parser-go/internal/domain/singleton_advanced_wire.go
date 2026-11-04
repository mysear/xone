package domain

import (
	"encoding/hex"
	"errors"
	"strings"
	pb "xone/common/protoc/golang/dexpb"
)

func (p Pool) validateSingletonAdvanced() error {
	s := p.SingletonAdvanced
	profile, ok := Protocol(p.Protocol)
	if !ok || s == nil || p.MultiAsset != nil || p.Weighted != nil || p.SingletonCL != nil || p.Stable != nil || p.V2 != nil || p.V3 != nil || p.OrcaWhirlpool != nil || p.RaydiumCLMM != nil || p.RaydiumCPMM != nil {
		return errors.New("SINGLETON_ADVANCED_STATE_REQUIRED")
	}
	if ValidateChain(p.ChainKey) != nil || !strings.HasPrefix(p.ChainKey, "evm:") || s.Protocol+"-advanced" != p.Protocol || p.PoolID != s.Snapshot.Manager+"/"+s.Snapshot.PoolID || p.Token0 != s.Currency0 || p.Token1 != s.Currency1 || p.Reserve0 != "0" || p.Reserve1 != "0" || p.AdapterVersion != profile.AdapterVersion || p.TokenBehavior != "standard" || p.FeeDenominator != 1000000 || p.FeeNumerator != uint64(s.Snapshot.LPFee) {
		return errors.New("SINGLETON_ADVANCED_IDENTITY")
	}
	return s.Validate()
}
func SingletonAdvancedToProto(s *SingletonAdvancedState) (*pb.SingletonAdvancedState, error) {
	if s == nil {
		return nil, nil
	}
	if e := s.Validate(); e != nil {
		return nil, e
	}
	hash, e := hex.DecodeString(strings.TrimPrefix(s.SnapshotBlockHash, "0x"))
	if e != nil || len(hash) != 32 {
		return nil, errors.New("SINGLETON_BLOCK_HASH_REQUIRED")
	}
	snapshot, e := singletonCLToProto(&s.Snapshot, true)
	if e != nil {
		return nil, e
	}
	return &pb.SingletonAdvancedState{Protocol: s.Protocol, Currency0: s.Currency0, Currency1: s.Currency1, KeyFee: s.KeyFee, SnapshotBlockHash: hash, Snapshot: snapshot}, nil
}
func SingletonAdvancedFromProto(s *pb.SingletonAdvancedState) (*SingletonAdvancedState, error) {
	if s == nil {
		return nil, nil
	}
	if len(s.SnapshotBlockHash) != 32 || s.Snapshot == nil {
		return nil, errors.New("SINGLETON_ADVANCED_WIRE_REQUIRED")
	}
	snapshot, e := singletonCLFromProto(s.Snapshot, true)
	if e != nil {
		return nil, e
	}
	out := &SingletonAdvancedState{Protocol: s.Protocol, Currency0: s.Currency0, Currency1: s.Currency1, KeyFee: s.KeyFee, SnapshotBlockHash: "0x" + hex.EncodeToString(s.SnapshotBlockHash), Snapshot: *snapshot}
	return out, out.Validate()
}
