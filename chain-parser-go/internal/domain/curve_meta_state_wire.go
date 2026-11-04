package domain

import (
	"encoding/hex"
	"errors"
	"strings"
	pb "xone/common/protoc/golang/dexpb"
)

func CurveMetaToProto(s *CurveMetaState) (*pb.CurveMetaState, error) {
	if s == nil {
		return nil, nil
	}
	bytes := func(v string) ([]byte, error) {
		raw, e := hex.DecodeString(strings.TrimPrefix(v, "0x"))
		if e != nil || len(raw) != 32 {
			return nil, errors.New("CURVE_META_BYTES_WIDTH")
		}
		return raw, nil
	}
	amount := func(v Amount) (*pb.Amount, error) {
		raw, e := v.Bytes()
		if e != nil {
			return nil, e
		}
		return &pb.Amount{Uint256Be: raw}, nil
	}
	values := func(v []Amount) ([]*pb.Amount, error) {
		out := make([]*pb.Amount, len(v))
		for i, a := range v {
			var e error
			out[i], e = amount(a)
			if e != nil {
				return nil, e
			}
		}
		return out, nil
	}
	impl, e := bytes(s.ImplementationCodeSHA256)
	if e != nil {
		return nil, e
	}
	code, e := bytes(s.BasePoolCodeSHA256)
	if e != nil {
		return nil, e
	}
	hash, e := bytes(s.BlockHash)
	if e != nil {
		return nil, e
	}
	vp, e := amount(s.BaseVirtualPrice)
	if e != nil {
		return nil, e
	}
	stored, e := values(s.StoredBalances)
	if e != nil {
		return nil, e
	}
	admin, e := values(s.AdminBalances)
	if e != nil {
		return nil, e
	}
	return &pb.CurveMetaState{Implementation: s.Implementation, ImplementationCodeSha256: impl, BasePool: s.BasePool, BasePoolCodeSha256: code, BaseVirtualPrice: vp, StoredBalances: stored, AdminBalances: admin, Timestamp: s.Timestamp, BlockNumber: s.BlockNumber, BlockHash: hash}, nil
}
func CurveMetaFromProto(s *pb.CurveMetaState) (*CurveMetaState, error) {
	if s == nil {
		return nil, nil
	}
	if len(s.BasePoolCodeSha256) != 32 || len(s.BlockHash) != 32 || len(s.ImplementationCodeSha256) != 32 {
		return nil, errors.New("CURVE_META_BYTES_WIDTH")
	}
	amount := func(v *pb.Amount) (Amount, error) {
		if v == nil {
			return "", errors.New("CURVE_META_AMOUNT_REQUIRED")
		}
		return AmountFromBytes(v.Uint256Be)
	}
	values := func(v []*pb.Amount) ([]Amount, error) {
		out := make([]Amount, len(v))
		for i, a := range v {
			var e error
			out[i], e = amount(a)
			if e != nil {
				return nil, e
			}
		}
		return out, nil
	}
	vp, e := amount(s.BaseVirtualPrice)
	if e != nil {
		return nil, e
	}
	stored, e := values(s.StoredBalances)
	if e != nil {
		return nil, e
	}
	admin, e := values(s.AdminBalances)
	if e != nil {
		return nil, e
	}
	return &CurveMetaState{Implementation: s.Implementation, ImplementationCodeSHA256: hex.EncodeToString(s.ImplementationCodeSha256), BasePool: s.BasePool, BasePoolCodeSHA256: hex.EncodeToString(s.BasePoolCodeSha256), BaseVirtualPrice: vp, StoredBalances: stored, AdminBalances: admin, Timestamp: s.Timestamp, BlockNumber: s.BlockNumber, BlockHash: "0x" + hex.EncodeToString(s.BlockHash)}, nil
}
