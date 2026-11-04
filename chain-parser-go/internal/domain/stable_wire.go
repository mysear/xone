package domain

import (
	"encoding/hex"
	"errors"
	"strings"
	pb "xone/common/protoc/golang/dexpb"
)

func StableToProto(s *StableState) (*pb.StableState, error) {
	if s == nil {
		return nil, nil
	}
	raw := func(s string) []byte { b, _ := hex.DecodeString(strings.TrimPrefix(s, "0x")); return b }
	amount := func(a Amount) (*pb.Amount, error) { b, e := a.Bytes(); return &pb.Amount{Uint256Be: b}, e }
	r0, e := amount(s.Rate0)
	if e != nil {
		return nil, e
	}
	r1, e := amount(s.Rate1)
	if e != nil {
		return nil, e
	}
	amp, e := amount(s.Amplification)
	if e != nil {
		return nil, e
	}
	return &pb.StableState{CodeSha256: raw(s.CodeSHA256), Implementation: s.Implementation, ImplementationCodeSha256: raw(s.ImplementationCodeSHA256), Factory: s.Factory, FactoryCodeSha256: raw(s.FactoryCodeSHA256), Vault: s.Vault, VaultCodeSha256: raw(s.VaultCodeSHA256), PoolId: raw(s.PoolID), Rate0: r0, Rate1: r1, Amplification: amp, AdminFee: s.AdminFee, MaxIterations: s.MaxIterations}, nil
}
func StableFromProto(s *pb.StableState) (*StableState, error) {
	if s == nil {
		return nil, nil
	}
	digest := func(b []byte, optional bool) (string, error) {
		if len(b) != 32 && !(optional && len(b) == 0) {
			return "", errors.New("STABLE_DIGEST_WIDTH")
		}
		return hex.EncodeToString(b), nil
	}
	amount := func(a *pb.Amount) (Amount, error) {
		if a == nil {
			return "", errors.New("STABLE_AMOUNT_REQUIRED")
		}
		return AmountFromBytes(a.Uint256Be)
	}
	code, e := digest(s.CodeSha256, false)
	if e != nil {
		return nil, e
	}
	impl, e := digest(s.ImplementationCodeSha256, true)
	if e != nil {
		return nil, e
	}
	factory, e := digest(s.FactoryCodeSha256, true)
	if e != nil {
		return nil, e
	}
	vault, e := digest(s.VaultCodeSha256, true)
	if e != nil {
		return nil, e
	}
	id, e := digest(s.PoolId, true)
	if e != nil {
		return nil, e
	}
	if id != "" {
		id = "0x" + id
	}
	r0, e := amount(s.Rate0)
	if e != nil {
		return nil, e
	}
	r1, e := amount(s.Rate1)
	if e != nil {
		return nil, e
	}
	amp, e := amount(s.Amplification)
	if e != nil {
		return nil, e
	}
	return &StableState{CodeSHA256: code, Implementation: s.Implementation, ImplementationCodeSHA256: impl, Factory: s.Factory, FactoryCodeSHA256: factory, Vault: s.Vault, VaultCodeSHA256: vault, PoolID: id, Rate0: r0, Rate1: r1, Amplification: amp, AdminFee: s.AdminFee, MaxIterations: s.MaxIterations}, nil
}
