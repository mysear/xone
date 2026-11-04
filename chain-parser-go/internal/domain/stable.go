package domain

import (
	"encoding/hex"
	"errors"
	"math/big"
	"strings"
)

// StableState is a typed parameter envelope; protocol profiles select independent
// Aero cubic, Curve Plain2, or Balancer StableMath validation and algorithms.
type StableState struct {
	CodeSHA256               string `json:"code_sha256"`
	Implementation           string `json:"implementation,omitempty"`
	ImplementationCodeSHA256 string `json:"implementation_code_sha256,omitempty"`
	Factory                  string `json:"factory,omitempty"`
	FactoryCodeSHA256        string `json:"factory_code_sha256,omitempty"`
	Vault                    string `json:"vault,omitempty"`
	VaultCodeSHA256          string `json:"vault_code_sha256,omitempty"`
	PoolID                   string `json:"pool_id,omitempty"`
	Rate0                    Amount `json:"rate0"`
	Rate1                    Amount `json:"rate1"`
	Amplification            Amount `json:"amplification"`
	AdminFee                 uint64 `json:"admin_fee"`
	MaxIterations            uint32 `json:"max_iterations"`
}

func validDigest(s string) bool {
	b, e := hex.DecodeString(s)
	return e == nil && len(b) == 32 && s == strings.ToLower(s)
}
func power10Between(a Amount, low, high int) bool {
	n, e := ParseAmount(string(a))
	if e != nil {
		return false
	}
	v := big.NewInt(1)
	for i := 0; i <= high; i++ {
		if i >= low && n.Big().Cmp(v) == 0 {
			return true
		}
		v.Mul(v, big.NewInt(10))
	}
	return false
}
func (s StableState) Validate(family string) error {
	if !validDigest(s.CodeSHA256) || s.MaxIterations != 255 {
		return errors.New("STABLE_CODE_OR_BUDGET")
	}
	if s.Implementation != "" {
		if ValidateEVMAddress(s.Implementation) != nil || !validDigest(s.ImplementationCodeSHA256) {
			return errors.New("STABLE_IMPLEMENTATION_IDENTITY")
		}
	} else if s.ImplementationCodeSHA256 != "" {
		return errors.New("STABLE_IMPLEMENTATION_IDENTITY")
	}
	amp, e := ParseAmount(string(s.Amplification))
	if e != nil {
		return e
	}
	switch family {
	case "aero":
		if ValidateEVMIdentity(s.Factory, s.FactoryCodeSHA256) != nil || !power10Between(s.Rate0, 0, 18) || !power10Between(s.Rate1, 0, 18) || amp.Big().Sign() != 0 || s.AdminFee != 0 || s.Vault != "" || s.VaultCodeSHA256 != "" || s.PoolID != "" {
			return errors.New("AERO_STATE_PARAMETERS")
		}
	case "curve":
		if !power10Between(s.Rate0, 18, 36) || !power10Between(s.Rate1, 18, 36) || amp.Big().Cmp(big.NewInt(100)) < 0 || amp.Big().Cmp(big.NewInt(100000000)) >= 0 || s.AdminFee != 5000000000 || s.Factory != "" || s.FactoryCodeSHA256 != "" || s.Vault != "" || s.VaultCodeSHA256 != "" || s.PoolID != "" {
			return errors.New("CURVE_STATE_PARAMETERS")
		}
	case "balancer":
		b, e := hex.DecodeString(strings.TrimPrefix(s.PoolID, "0x"))
		if !power10Between(s.Rate0, 18, 36) || !power10Between(s.Rate1, 18, 36) || amp.Big().Cmp(big.NewInt(1000)) < 0 || amp.Big().Cmp(big.NewInt(5000000)) > 0 || s.AdminFee != 0 || s.Factory != "" || s.FactoryCodeSHA256 != "" || ValidateEVMIdentity(s.Vault, s.VaultCodeSHA256) != nil || e != nil || len(b) != 32 || !strings.HasPrefix(s.PoolID, "0x") || s.PoolID != strings.ToLower(s.PoolID) {
			return errors.New("BALANCER_STATE_PARAMETERS")
		}
	default:
		return errors.New("STABLE_PROTOCOL_UNSUPPORTED")
	}
	return nil
}
