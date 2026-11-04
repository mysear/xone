package domain

import (
	"encoding/hex"
	"errors"
	"strings"
	pb "xone/common/protoc/golang/dexpb"
)

func WeightedToProto(s *WeightedState) (*pb.WeightedState, error) {
	if s == nil {
		return nil, nil
	}
	raw := func(v string) ([]byte, error) {
		b, e := hex.DecodeString(strings.TrimPrefix(v, "0x"))
		if e != nil || len(b) != 32 {
			return nil, errors.New("WEIGHTED_BYTES_WIDTH")
		}
		return b, nil
	}
	vals := []string{s.CodeSHA256, s.VaultCodeSHA256, s.PoolID, s.Token0CodeSHA256, s.Token1CodeSHA256}
	b := make([][]byte, len(vals))
	for i, v := range vals {
		var e error
		b[i], e = raw(v)
		if e != nil {
			return nil, e
		}
	}
	a := make([]*pb.Amount, 4)
	for i, v := range []Amount{s.Scaling0, s.Scaling1, s.Weight0, s.Weight1} {
		x, e := v.Bytes()
		if e != nil {
			return nil, e
		}
		a[i] = &pb.Amount{Uint256Be: x}
	}
	return &pb.WeightedState{CodeSha256: b[0], Vault: s.Vault, VaultCodeSha256: b[1], PoolId: b[2], Scaling0: a[0], Scaling1: a[1], Weight0: a[2], Weight1: a[3], Token0CodeSha256: b[3], Token1CodeSha256: b[4]}, nil
}
func WeightedFromProto(s *pb.WeightedState) (*WeightedState, error) {
	if s == nil {
		return nil, nil
	}
	vals := [][]byte{s.CodeSha256, s.VaultCodeSha256, s.PoolId, s.Token0CodeSha256, s.Token1CodeSha256}
	v := make([]string, len(vals))
	for i, b := range vals {
		if len(b) != 32 {
			return nil, errors.New("WEIGHTED_BYTES_WIDTH")
		}
		v[i] = hex.EncodeToString(b)
	}
	a := make([]Amount, 4)
	for i, b := range []*pb.Amount{s.Scaling0, s.Scaling1, s.Weight0, s.Weight1} {
		if b == nil {
			return nil, errors.New("WEIGHTED_AMOUNT_REQUIRED")
		}
		var e error
		a[i], e = AmountFromBytes(b.Uint256Be)
		if e != nil {
			return nil, e
		}
	}
	return &WeightedState{CodeSHA256: v[0], Vault: s.Vault, VaultCodeSHA256: v[1], PoolID: "0x" + v[2], Scaling0: a[0], Scaling1: a[1], Weight0: a[2], Weight1: a[3], Token0CodeSHA256: v[3], Token1CodeSHA256: v[4]}, nil
}
