package domain

import (
	"encoding/hex"
	"errors"
	"strings"
	pb "xone/common/protoc/golang/dexpb"
)

func CurveNGToProto(s *CurveNGState) (*pb.CurveNGState, error) {
	if s == nil {
		return nil, nil
	}
	amount := func(a Amount) (*pb.Amount, error) { b, e := a.Bytes(); return &pb.Amount{Uint256Be: b}, e }
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
	hash, e := hex.DecodeString(strings.TrimPrefix(s.BlockHash, "0x"))
	if e != nil || len(hash) != 32 {
		return nil, errors.New("CURVE_NG_BLOCK_HASH")
	}
	stored, e := values(s.StoredBalances)
	if e != nil {
		return nil, e
	}
	admin, e := values(s.AdminBalances)
	if e != nil {
		return nil, e
	}
	prices, e := values(s.LastPricesPacked)
	if e != nil {
		return nil, e
	}
	d, e := amount(s.LastDPacked)
	if e != nil {
		return nil, e
	}
	last, e := amount(s.MALastTime)
	if e != nil {
		return nil, e
	}
	offpeg, e := amount(s.OffpegFeeMultiplier)
	if e != nil {
		return nil, e
	}
	return &pb.CurveNGState{StoredBalances: stored, AdminBalances: admin, LastPricesPacked: prices, LastDPacked: d, MaLastTime: last, MaExpTime: s.MAExpTime, DMaTime: s.DMaTime, Timestamp: s.Timestamp, BlockNumber: s.BlockNumber, BlockHash: hash, OffpegFeeMultiplier: offpeg}, nil
}
func CurveNGFromProto(s *pb.CurveNGState) (*CurveNGState, error) {
	if s == nil {
		return nil, nil
	}
	amount := func(a *pb.Amount) (Amount, error) {
		if a == nil {
			return "", errors.New("CURVE_NG_AMOUNT_REQUIRED")
		}
		return AmountFromBytes(a.Uint256Be)
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
	if len(s.BlockHash) != 32 {
		return nil, errors.New("CURVE_NG_BLOCK_HASH")
	}
	stored, e := values(s.StoredBalances)
	if e != nil {
		return nil, e
	}
	admin, e := values(s.AdminBalances)
	if e != nil {
		return nil, e
	}
	prices, e := values(s.LastPricesPacked)
	if e != nil {
		return nil, e
	}
	d, e := amount(s.LastDPacked)
	if e != nil {
		return nil, e
	}
	last, e := amount(s.MaLastTime)
	if e != nil {
		return nil, e
	}
	offpeg, e := amount(s.OffpegFeeMultiplier)
	if e != nil {
		return nil, e
	}
	return &CurveNGState{StoredBalances: stored, AdminBalances: admin, LastPricesPacked: prices, LastDPacked: d, MALastTime: last, MAExpTime: s.MaExpTime, DMaTime: s.DMaTime, Timestamp: s.Timestamp, BlockNumber: s.BlockNumber, BlockHash: "0x" + hex.EncodeToString(s.BlockHash), OffpegFeeMultiplier: offpeg}, nil
}
