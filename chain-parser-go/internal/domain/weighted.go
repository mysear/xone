package domain

import (
	"errors"
	"math/big"
	"strings"
)

// WeightedState describes versioned immutable two-token Balancer V2
// pools with no asset managers. Reserves are raw pool-specific Vault cash.
type WeightedState struct {
	CodeSHA256       string `json:"code_sha256"`
	Vault            string `json:"vault"`
	VaultCodeSHA256  string `json:"vault_code_sha256"`
	PoolID           string `json:"pool_id"`
	Scaling0         Amount `json:"scaling0"`
	Scaling1         Amount `json:"scaling1"`
	Weight0          Amount `json:"weight0"`
	Weight1          Amount `json:"weight1"`
	Token0CodeSHA256 string `json:"token0_code_sha256"`
	Token1CodeSHA256 string `json:"token1_code_sha256"`
}

func (p Pool) validateWeighted() error {
	s := p.Weighted
	if s == nil || p.Stable != nil || p.V2 != nil || p.V3 != nil || p.SingletonCL != nil || p.RaydiumCPMM != nil || p.OrcaWhirlpool != nil || p.RaydiumCLMM != nil {
		return errors.New("WEIGHTED_STATE_REQUIRED")
	}
	if ValidateChain(p.ChainKey) != nil || !strings.HasPrefix(p.ChainKey, "evm:") || ValidateEVMAddress(p.PoolID) != nil || ValidateEVMAddress(p.Token0) != nil || ValidateEVMAddress(p.Token1) != nil || p.Token0 >= p.Token1 || p.Token0 == p.PoolID || p.Token1 == p.PoolID || p.Token0 == s.Vault || p.Token1 == s.Vault || p.PoolID == s.Vault {
		return errors.New("WEIGHTED_IDENTITY")
	}
	profile, known := Protocol(p.Protocol)
	if !known || profile.Family != "weighted" || p.AdapterVersion != profile.AdapterVersion || p.TokenBehavior != "standard" || p.FeeDenominator != 1000000000000000000 || p.FeeNumerator < 1000000000000 || p.FeeNumerator > 100000000000000000 {
		return errors.New("WEIGHTED_PROFILE")
	}
	if !validDigest(s.CodeSHA256) || ValidateEVMIdentity(s.Vault, s.VaultCodeSHA256) != nil || !validDigest(s.Token0CodeSHA256) || !validDigest(s.Token1CodeSHA256) || len(s.PoolID) != 66 || !strings.HasPrefix(s.PoolID, p.PoolID) || s.PoolID[42:46] != "0002" || !validDigest(s.PoolID[2:]) {
		return errors.New("WEIGHTED_CODE_OR_POOL_ID")
	}
	w0, e0 := ParseAmount(string(s.Weight0))
	w1, e1 := ParseAmount(string(s.Weight1))
	if e0 != nil || e1 != nil {
		return errors.New("WEIGHTED_PARAMETERS")
	}
	if w0.Big().Cmp(big.NewInt(10000000000000000)) < 0 || w1.Big().Cmp(big.NewInt(10000000000000000)) < 0 || new(big.Int).Add(w0.Big(), w1.Big()).Cmp(big.NewInt(1000000000000000000)) != 0 {
		return errors.New("WEIGHTED_PARAMETERS")
	}
	if p.Protocol == "balancer-v2-weighted2-equal" && (s.Weight0 != "500000000000000000" || s.Weight1 != "500000000000000000") {
		return errors.New("WEIGHTED_PARAMETERS")
	}
	if !power10Between(s.Scaling0, 18, 36) || !power10Between(s.Scaling1, 18, 36) {
		return errors.New("WEIGHTED_PARAMETERS")
	}
	for _, a := range []Amount{p.Reserve0, p.Reserve1} {
		n, e := ParseAmount(string(a))
		if e != nil || n.Big().BitLen() > 112 {
			return errors.New("WEIGHTED_VAULT_BALANCE")
		}
	}
	return nil
}
