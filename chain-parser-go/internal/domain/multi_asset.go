package domain

import (
	"errors"
	"math/big"
	"strings"
)

type MultiAsset struct {
	Token      string `json:"token"`
	Balance    Amount `json:"balance"`
	Scaling    Amount `json:"scaling"`
	CodeSHA256 string `json:"code_sha256"`
}
type MultiAssetState struct {
	ManagedV2       *BalancerManagedV2State `json:"managed_v2,omitempty"`
	CurveCrypto     *CurveCryptoState       `json:"curve_crypto,omitempty"`
	CurveMeta       *CurveMetaState         `json:"curve_meta,omitempty"`
	ManagedFees     *BalancerManagedState   `json:"managed_fees,omitempty"`
	CurveNG         *CurveNGState           `json:"curve_ng,omitempty"`
	CodeSHA256      string                  `json:"code_sha256"`
	Vault           string                  `json:"vault"`
	VaultCodeSHA256 string                  `json:"vault_code_sha256"`
	PoolID          string                  `json:"pool_id"`
	Assets          []MultiAsset            `json:"assets"`
	Weights         []Amount                `json:"weights"`
	Amplification   Amount                  `json:"amplification"`
	AdminFee        uint64                  `json:"admin_fee"`
}

func (p Pool) validateMultiAsset() error {
	s := p.MultiAsset
	profile, ok := Protocol(p.Protocol)
	if !ok || s == nil || p.Weighted != nil || p.Stable != nil || p.V2 != nil || p.V3 != nil || p.SingletonCL != nil || p.RaydiumCPMM != nil || p.OrcaWhirlpool != nil || p.RaydiumCLMM != nil {
		return errors.New("MULTI_ASSET_STATE_REQUIRED")
	}
	if p.Protocol == ManagedV2Protocol || p.Protocol == ManagedV2ScheduledProtocol || p.Protocol == ManagedV2BptProtocol || p.Protocol == ManagedV2BreakerProtocol {
		return p.validateManagedV2()
	}
	if s.ManagedV2 != nil {
		return errors.New("MANAGED_V2_PROFILE_MISMATCH")
	}
	if s.Weights == nil || s.Assets == nil {
		return errors.New("MULTI_ASSET_ARRAY_REQUIRED")
	}
	if ValidateChain(p.ChainKey) != nil || !strings.HasPrefix(p.ChainKey, "evm:") || ValidateEVMAddress(p.PoolID) != nil || p.AdapterVersion != profile.AdapterVersion || p.TokenBehavior != "standard" || !validDigest(s.CodeSHA256) || len(s.Assets) < 2 || len(s.Assets) > 38 || p.FeeDenominator != profile.FeeDenominator || p.FeeNumerator >= p.FeeDenominator {
		return errors.New("MULTI_ASSET_IDENTITY")
	}
	seen := map[string]bool{}
	for _, a := range s.Assets {
		if ValidateEVMAddress(a.Token) != nil || seen[a.Token] || (a.Token == p.PoolID && p.Protocol != "balancer-v2-composable-regular") || !validDigest(a.CodeSHA256) || !power10Between(a.Scaling, 18, 36) {
			return errors.New("MULTI_ASSET_TOKEN")
		}
		seen[a.Token] = true
		if _, e := ParseAmount(string(a.Balance)); e != nil {
			return e
		}
	}
	if p.Token0 != s.Assets[0].Token || p.Token1 != s.Assets[1].Token || p.Reserve0 != s.Assets[0].Balance || p.Reserve1 != s.Assets[1].Balance {
		return errors.New("MULTI_ASSET_DUPLICATE_FIELDS")
	}
	if p.Protocol != "curve-crypto2-full" && p.Protocol != "curve-crypto2-ramp" && s.CurveCrypto != nil {
		return errors.New("CURVE_CRYPTO_PROFILE_MISMATCH")
	}
	if p.Protocol == "curve-crypto2-full" || p.Protocol == "curve-crypto2-ramp" {
		return p.validateCurveCrypto()
	}
	if p.Protocol != "balancer-v2-managed-fees" && s.ManagedFees != nil {
		return errors.New("MANAGED_FEE_PROFILE_MISMATCH")
	}
	if p.Protocol == "balancer-v2-managed-fees" {
		if e := p.validateManagedFees(); e != nil {
			return e
		}
	}
	if p.Protocol != "curve-metausd-direct" && s.CurveMeta != nil {
		return errors.New("CURVE_META_PROFILE_MISMATCH")
	}
	if p.Protocol == "curve-metausd-direct" {
		return p.validateCurveMeta()
	}
	if p.Protocol == "curve-stableswap-ng" {
		return p.validateCurveNG()
	}
	if s.CurveNG != nil {
		return errors.New("CURVE_NG_PROFILE_MISMATCH")
	}
	if strings.HasPrefix(p.Protocol, "balancer-v2-") {
		managed := p.Protocol == "balancer-v2-managed-snapshot" || p.Protocol == "balancer-v2-managed-fees"
		maxFee := uint64(100000000000000000)
		if p.Protocol == "balancer-v2-managed-fees" {
			maxFee = 800000000000000000
		}
		composable := p.Protocol == "balancer-v2-composable-regular"
		if !managed && !composable && (len(s.Assets) < 3 || len(s.Assets) > 20) {
			return errors.New("MULTI_WEIGHTED_ASSETS")
		}
		if composable && (len(s.Assets) < 3 || len(s.Assets) > 6) {
			return errors.New("COMPOSABLE_ASSETS")
		}

		if ValidateEVMIdentity(s.Vault, s.VaultCodeSHA256) != nil || len(s.PoolID) != 66 || !strings.HasPrefix(s.PoolID, p.PoolID) || !validDigest(s.PoolID[2:]) || (s.PoolID[42:46] != "0001" && !composable || s.PoolID[42:46] != "0000" && composable) || (!composable && s.Amplification != "0") || s.AdminFee != 0 || (!composable && len(s.Weights) != len(s.Assets)) || (composable && len(s.Weights) != 0) || p.FeeNumerator < 1000000000000 || p.FeeNumerator > maxFee {
			return errors.New("MULTI_WEIGHTED_PARAMETERS")
		}
		total := big.NewInt(0)
		for i, w := range s.Weights {
			v, e := ParseAmount(string(w))
			if e != nil || (!managed && v.Big().Cmp(big.NewInt(10000000000000000)) < 0) || (managed && (v.Big().Sign() == 0 || v.Big().Cmp(big.NewInt(1000000000000000000)) > 0)) {
				return errors.New("MULTI_WEIGHTED_WEIGHTS")
			}
			total.Add(total, v.Big())
			if s.Assets[i].Balance.Big().BitLen() > 112 || s.Assets[i].Token == s.Vault || i > 0 && s.Assets[i-1].Token >= s.Assets[i].Token {
				return errors.New("MULTI_WEIGHTED_ASSETS")
			}
		}
		if !managed && !composable && total.Cmp(big.NewInt(1000000000000000000)) != 0 {
			return errors.New("MULTI_WEIGHTED_WEIGHTS")
		}

		bpt := 0
		for i, a := range s.Assets {
			if a.Balance.Big().BitLen() > 112 || a.Token == s.Vault || i > 0 && s.Assets[i-1].Token >= a.Token {
				return errors.New("MULTI_WEIGHTED_ASSETS")
			}
			if a.Token == p.PoolID {
				if a.CodeSHA256 != s.CodeSHA256 {
					return errors.New("COMPOSABLE_BPT_IDENTITY")
				}
				bpt++
				if a.Scaling != "1000000000000000000" {
					return errors.New("COMPOSABLE_BPT_SCALE")
				}
			}
		}
		if composable {
			amp, e := ParseAmount(string(s.Amplification))
			if bpt != 1 || e != nil || amp.Big().Cmp(big.NewInt(1000)) < 0 || amp.Big().Cmp(big.NewInt(5000000)) > 0 {
				return errors.New("COMPOSABLE_PARAMETERS")
			}
		} else if bpt != 0 {
			return errors.New("MULTI_WEIGHTED_ASSETS")
		}
	} else {
		n := 3
		if p.Protocol == "curve-plain4-basic" {
			n = 4
		}
		amp, e := ParseAmount(string(s.Amplification))
		if len(s.Assets) != n || len(s.Weights) != 0 || s.Vault != "" || s.VaultCodeSHA256 != "" || s.PoolID != "" || e != nil || amp.Big().Cmp(big.NewInt(100)) < 0 || amp.Big().Cmp(big.NewInt(100000000)) >= 0 || s.AdminFee != 5000000000 || p.FeeNumerator < 4000000 || p.FeeNumerator > 100000000 {
			return errors.New("MULTI_CURVE_PARAMETERS")
		}
	}
	return nil
}
