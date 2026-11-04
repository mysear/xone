package domain

import (
	"encoding/hex"
	"errors"
	"strings"
	pb "xone/common/protoc/golang/dexpb"
)

func ManagedFeesToProto(s *BalancerManagedState) (*pb.BalancerManagedState, error) {
	if s == nil {
		return nil, nil
	}
	hash, e := hex.DecodeString(strings.TrimPrefix(s.BlockHash, "0x"))
	if e != nil || len(hash) != 32 {
		return nil, errors.New("MANAGED_FEE_BLOCK_HASH")
	}
	amounts := make([]*pb.Amount, 5)
	for i, a := range []Amount{s.TotalSupply, s.ProtocolSwapFee, s.ManagementSwapFee, s.OwnerBalance, s.ProtocolCollectorBalance} {
		raw, e := a.Bytes()
		if e != nil {
			return nil, e
		}
		amounts[i] = &pb.Amount{Uint256Be: raw}
	}
	return &pb.BalancerManagedState{TotalSupply: amounts[0], ProtocolSwapFee: amounts[1], ManagementSwapFee: amounts[2], Owner: s.Owner, ProtocolCollector: s.ProtocolCollector, OwnerBalance: amounts[3], ProtocolCollectorBalance: amounts[4], Timestamp: s.Timestamp, BlockNumber: s.BlockNumber, BlockHash: hash}, nil
}
func ManagedFeesFromProto(s *pb.BalancerManagedState) (*BalancerManagedState, error) {
	if s == nil {
		return nil, nil
	}
	if len(s.BlockHash) != 32 {
		return nil, errors.New("MANAGED_FEE_BLOCK_HASH")
	}
	amounts := make([]Amount, 5)
	for i, a := range []*pb.Amount{s.TotalSupply, s.ProtocolSwapFee, s.ManagementSwapFee, s.OwnerBalance, s.ProtocolCollectorBalance} {
		if a == nil {
			return nil, errors.New("MANAGED_FEE_AMOUNT_REQUIRED")
		}
		v, e := AmountFromBytes(a.Uint256Be)
		if e != nil {
			return nil, e
		}
		amounts[i] = v
	}
	return &BalancerManagedState{TotalSupply: amounts[0], ProtocolSwapFee: amounts[1], ManagementSwapFee: amounts[2], Owner: s.Owner, ProtocolCollector: s.ProtocolCollector, OwnerBalance: amounts[3], ProtocolCollectorBalance: amounts[4], Timestamp: s.Timestamp, BlockNumber: s.BlockNumber, BlockHash: "0x" + hex.EncodeToString(s.BlockHash)}, nil
}
