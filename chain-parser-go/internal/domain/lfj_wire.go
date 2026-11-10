package domain

import (
	"encoding/hex"
	"errors"
	"strings"
	pb "xone/common/protoc/golang/dexpb"
)

func LfjToProto(s *LfjState) (*pb.LfjState, error) {
	if s == nil {
		return nil, nil
	}
	if e := s.ValidateWire(); e != nil {
		return nil, e
	}
	raw := func(value string) []byte { b, _ := hex.DecodeString(strings.TrimPrefix(value, "0x")); return b }
	amount := func(value Amount) *pb.Amount { b, _ := value.Bytes(); return &pb.Amount{Uint256Be: b} }
	ptr := func(value uint64) *uint64 { return &value }
	out := &pb.LfjState{
		CodeSha256:               raw(s.CodeSHA256),
		Implementation:           s.Implementation,
		ImplementationCodeSha256: raw(s.ImplementationCodeSHA256),
		Factory:                  s.Factory,
		FactoryCodeSha256:        raw(s.FactoryCodeSHA256),
		Token0CodeSha256:         raw(s.Token0CodeSHA256),
		Token1CodeSha256:         raw(s.Token1CodeSHA256),
		BlockHash:                raw(s.BlockHash),
		BlockNumber:              ptr(s.BlockNumber),
		Timestamp:                ptr(s.Timestamp),
		BinStep:                  s.BinStep,
		Parameters:               amount(s.Parameters),
		RawReserve0:              amount(s.RawReserve0),
		RawReserve1:              amount(s.RawReserve1),
		ProtocolFees0:            amount(s.ProtocolFees0),
		ProtocolFees1:            amount(s.ProtocolFees1),
		MaxBins:                  s.MaxBins,
		MaxSteps:                 s.MaxSteps,
	}
	for _, b := range s.Bins {
		out.Bins = append(out.Bins, &pb.LfjBin{Id: b.ID, Reserve0: amount(b.Reserve0), Reserve1: amount(b.Reserve1), TotalShares: amount(b.TotalShares)})
	}
	for _, o := range s.OracleSamples {
		out.OracleSamples = append(out.OracleSamples, &pb.LfjOracle{Index: o.Index, Data: amount(o.Data)})
	}
	return out, nil
}
func LfjFromProto(s *pb.LfjState) (*LfjState, error) {
	if s == nil {
		return nil, nil
	}
	if len(s.CodeSha256) != 32 {
		return nil, errors.New("LFJ_WIRE_DIGEST_WIDTH")
	}
	if len(s.ImplementationCodeSha256) != 32 {
		return nil, errors.New("LFJ_WIRE_DIGEST_WIDTH")
	}
	if len(s.FactoryCodeSha256) != 32 {
		return nil, errors.New("LFJ_WIRE_DIGEST_WIDTH")
	}
	if len(s.Token0CodeSha256) != 32 {
		return nil, errors.New("LFJ_WIRE_DIGEST_WIDTH")
	}
	if len(s.Token1CodeSha256) != 32 {
		return nil, errors.New("LFJ_WIRE_DIGEST_WIDTH")
	}
	if len(s.BlockHash) != 32 {
		return nil, errors.New("LFJ_WIRE_DIGEST_WIDTH")
	}
	if s.BlockNumber == nil || s.Timestamp == nil {
		return nil, errors.New("LFJ_WIRE_BANK_REQUIRED")
	}
	amount := func(value *pb.Amount) (Amount, error) {
		if value == nil {
			return "", errors.New("LFJ_WIRE_AMOUNT_REQUIRED")
		}
		return AmountFromBytes(value.Uint256Be)
	}
	out := &LfjState{Bins: []LfjBin{}, OracleSamples: []LfjOracle{},
		CodeSHA256:               hex.EncodeToString(s.CodeSha256),
		Implementation:           s.Implementation,
		ImplementationCodeSHA256: hex.EncodeToString(s.ImplementationCodeSha256),
		Factory:                  s.Factory,
		FactoryCodeSHA256:        hex.EncodeToString(s.FactoryCodeSha256),
		Token0CodeSHA256:         hex.EncodeToString(s.Token0CodeSha256),
		Token1CodeSHA256:         hex.EncodeToString(s.Token1CodeSha256),
		BlockHash:                "0x" + hex.EncodeToString(s.BlockHash),
		BlockNumber:              *s.BlockNumber,
		Timestamp:                *s.Timestamp,
		BinStep:                  s.BinStep,
		MaxBins:                  s.MaxBins,
		MaxSteps:                 s.MaxSteps,
	}
	var e error
	out.Parameters, e = amount(s.Parameters)
	if e != nil {
		return nil, e
	}
	out.RawReserve0, e = amount(s.RawReserve0)
	if e != nil {
		return nil, e
	}
	out.RawReserve1, e = amount(s.RawReserve1)
	if e != nil {
		return nil, e
	}
	out.ProtocolFees0, e = amount(s.ProtocolFees0)
	if e != nil {
		return nil, e
	}
	out.ProtocolFees1, e = amount(s.ProtocolFees1)
	if e != nil {
		return nil, e
	}
	for _, b := range s.Bins {
		if b == nil {
			return nil, errors.New("LFJ_WIRE_BIN_REQUIRED")
		}
		x, e := amount(b.Reserve0)
		if e != nil {
			return nil, e
		}
		y, e := amount(b.Reserve1)
		if e != nil {
			return nil, e
		}
		shares, e := amount(b.TotalShares)
		if e != nil {
			return nil, e
		}
		out.Bins = append(out.Bins, LfjBin{ID: b.Id, Reserve0: x, Reserve1: y, TotalShares: shares})
	}
	for _, o := range s.OracleSamples {
		if o == nil {
			return nil, errors.New("LFJ_WIRE_ORACLE_REQUIRED")
		}
		data, e := amount(o.Data)
		if e != nil {
			return nil, e
		}
		out.OracleSamples = append(out.OracleSamples, LfjOracle{Index: o.Index, Data: data})
	}
	if e := out.ValidateWire(); e != nil {
		return nil, e
	}
	return out, nil
}
