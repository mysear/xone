package domain

import (
	"encoding/hex"
	"errors"
	"strings"
	pb "xone/common/protoc/golang/dexpb"
)

func MultiAssetToProto(s *MultiAssetState) (*pb.MultiAssetState, error) {
	if s == nil {
		return nil, nil
	}
	raw := func(v string, optional bool) ([]byte, error) {
		b, e := hex.DecodeString(strings.TrimPrefix(v, "0x"))
		if e != nil || len(b) != 32 && !(optional && len(b) == 0) {
			return nil, errors.New("MULTI_ASSET_BYTES_WIDTH")
		}
		return b, nil
	}
	amount := func(a Amount) (*pb.Amount, error) { b, e := a.Bytes(); return &pb.Amount{Uint256Be: b}, e }
	code, e := raw(s.CodeSHA256, false)
	if e != nil {
		return nil, e
	}
	vault, e := raw(s.VaultCodeSHA256, true)
	if e != nil {
		return nil, e
	}
	id, e := raw(s.PoolID, true)
	if e != nil {
		return nil, e
	}
	amp, e := amount(s.Amplification)
	if e != nil {
		return nil, e
	}
	ng, e := CurveNGToProto(s.CurveNG)
	if e != nil {
		return nil, e
	}
	managed, e := ManagedFeesToProto(s.ManagedFees)
	if e != nil {
		return nil, e
	}
	meta, e := CurveMetaToProto(s.CurveMeta)
	if e != nil {
		return nil, e
	}
	crypto, e := CurveCryptoToProto(s.CurveCrypto)
	if e != nil {
		return nil, e
	}
	v2, e := ManagedV2ToProto(s.ManagedV2)
	if e != nil {
		return nil, e
	}
	out := &pb.MultiAssetState{ManagedV2: v2, CurveCrypto: crypto, CurveMeta: meta, ManagedFees: managed, CurveNg: ng, CodeSha256: code, Vault: s.Vault, VaultCodeSha256: vault, PoolId: id, Amplification: amp, AdminFee: s.AdminFee}
	for _, a := range s.Assets {
		balance, e := amount(a.Balance)
		if e != nil {
			return nil, e
		}
		scale, e := amount(a.Scaling)
		if e != nil {
			return nil, e
		}
		code, e := raw(a.CodeSHA256, false)
		if e != nil {
			return nil, e
		}
		out.Assets = append(out.Assets, &pb.MultiAsset{Token: a.Token, Balance: balance, Scaling: scale, CodeSha256: code})
	}
	for _, w := range s.Weights {
		v, e := amount(w)
		if e != nil {
			return nil, e
		}
		out.Weights = append(out.Weights, v)
	}
	return out, nil
}
func MultiAssetFromProto(s *pb.MultiAssetState) (*MultiAssetState, error) {
	if s == nil {
		return nil, nil
	}
	raw := func(b []byte, optional bool) (string, error) {
		if len(b) != 32 && !(optional && len(b) == 0) {
			return "", errors.New("MULTI_ASSET_BYTES_WIDTH")
		}
		return hex.EncodeToString(b), nil
	}
	amount := func(v *pb.Amount) (Amount, error) {
		if v == nil {
			return "", errors.New("MULTI_ASSET_AMOUNT_REQUIRED")
		}
		return AmountFromBytes(v.Uint256Be)
	}
	code, e := raw(s.CodeSha256, false)
	if e != nil {
		return nil, e
	}
	vault, e := raw(s.VaultCodeSha256, true)
	if e != nil {
		return nil, e
	}
	id, e := raw(s.PoolId, true)
	if e != nil {
		return nil, e
	}
	if id != "" {
		id = "0x" + id
	}
	amp, e := amount(s.Amplification)
	if e != nil {
		return nil, e
	}
	ng, e := CurveNGFromProto(s.CurveNg)
	if e != nil {
		return nil, e
	}
	managed, e := ManagedFeesFromProto(s.ManagedFees)
	if e != nil {
		return nil, e
	}
	meta, e := CurveMetaFromProto(s.CurveMeta)
	if e != nil {
		return nil, e
	}
	crypto, e := CurveCryptoFromProto(s.CurveCrypto)
	if e != nil {
		return nil, e
	}
	v2, e := ManagedV2FromProto(s.ManagedV2)
	if e != nil {
		return nil, e
	}
	out := &MultiAssetState{ManagedV2: v2, CurveCrypto: crypto, CurveMeta: meta, ManagedFees: managed, CurveNG: ng, CodeSHA256: code, Vault: s.Vault, VaultCodeSHA256: vault, PoolID: id, Amplification: amp, AdminFee: s.AdminFee, Assets: []MultiAsset{}, Weights: []Amount{}}
	for _, a := range s.Assets {
		if a == nil {
			return nil, errors.New("MULTI_ASSET_REQUIRED")
		}
		balance, e := amount(a.Balance)
		if e != nil {
			return nil, e
		}
		scale, e := amount(a.Scaling)
		if e != nil {
			return nil, e
		}
		code, e := raw(a.CodeSha256, false)
		if e != nil {
			return nil, e
		}
		out.Assets = append(out.Assets, MultiAsset{Token: a.Token, Balance: balance, Scaling: scale, CodeSHA256: code})
	}
	for _, w := range s.Weights {
		v, e := amount(w)
		if e != nil {
			return nil, e
		}
		out.Weights = append(out.Weights, v)
	}
	return out, nil
}
