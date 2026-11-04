package domain

import (
	"crypto/sha256"
	"encoding/hex"
	"errors"
	"math/big"
	"strings"
)

const CurveMetaRuntimeSHA256 = "c5351fd332fdf0f04486a838b2bceba15d7a45965e3df61e9a0ccedc674d450d"
const CurveMetaBasePool = "0xbebc44782c7db0a1a60cb6fe97d0b483032ff1c7"
const CurveMetaBaseLP = "0x6c3f90f043a72fa612cbac8115ee7e52bde6e490"

// CurveMetaState preserves physical balances and the explicitly read-only base.
type CurveMetaState struct {
	Implementation           string   `json:"implementation"`
	ImplementationCodeSHA256 string   `json:"implementation_code_sha256"`
	BasePool                 string   `json:"base_pool"`
	BasePoolCodeSHA256       string   `json:"base_pool_code_sha256"`
	BaseVirtualPrice         Amount   `json:"base_virtual_price"`
	StoredBalances           []Amount `json:"stored_balances"`
	AdminBalances            []Amount `json:"admin_balances"`
	Timestamp                uint64   `json:"timestamp"`
	BlockNumber              uint64   `json:"block_number"`
	BlockHash                string   `json:"block_hash"`
}

func (p Pool) validateCurveMeta() error {
	s := p.MultiAsset
	if s == nil || s.CurveMeta == nil {
		return errors.New("CURVE_META_STATE_REQUIRED")
	}
	m := s.CurveMeta
	if s.CurveNG != nil || s.ManagedFees != nil || len(s.Assets) != 2 || len(s.Weights) != 0 || s.Vault != "" || s.VaultCodeSHA256 != "" || s.PoolID != "" || len(m.StoredBalances) != 2 || len(m.AdminBalances) != 2 || s.Assets[1].Token != CurveMetaBaseLP || s.Assets[1].Scaling != "1000000000000000000" || m.BasePool != CurveMetaBasePool || p.PoolID == m.BasePool || !validDigest(m.BasePoolCodeSHA256) || m.Timestamp == 0 || m.Timestamp > ^uint64(0)/1000 || len(m.BlockHash) != 66 || !strings.HasPrefix(m.BlockHash, "0x") || !validDigest(m.BlockHash[2:]) || strings.Trim(m.BlockHash[2:], "0") == "" {
		return errors.New("CURVE_META_PARAMETERS")
	}
	amp, e := ParseAmount(string(s.Amplification))
	if e != nil || amp.Big().Cmp(big.NewInt(100)) < 0 || amp.Big().Cmp(big.NewInt(100000000)) >= 0 || s.AdminFee != 5000000000 || p.FeeNumerator < 4000000 || p.FeeNumerator > 100000000 {
		return errors.New("CURVE_META_FEE_OR_AMP")
	}
	if ValidateEVMAddress(m.Implementation) != nil || m.Implementation == p.PoolID || m.ImplementationCodeSHA256 != CurveMetaRuntimeSHA256 {
		return errors.New("CURVE_META_IMPLEMENTATION")
	}
	code, _ := hex.DecodeString("363d3d373d3d3d363d73" + m.Implementation[2:] + "5af43d82803e903d91602b57fd5bf3")
	sum := sha256.Sum256(code)
	if hex.EncodeToString(sum[:]) != s.CodeSHA256 {
		return errors.New("CURVE_META_CLONE_CODE")
	}
	vp, e := ParseAmount(string(m.BaseVirtualPrice))
	if e != nil || vp.Big().Sign() == 0 {
		return errors.New("CURVE_META_VIRTUAL_PRICE")
	}
	for i := range s.Assets {
		stored, e := ParseAmount(string(m.StoredBalances[i]))
		if e != nil {
			return e
		}
		admin, e := ParseAmount(string(m.AdminBalances[i]))
		if e != nil {
			return e
		}
		sum := s.Assets[i].Balance.Big()
		sum.Add(sum, admin.Big())
		if sum.BitLen() > 256 || sum.Cmp(stored.Big()) != 0 {
			return errors.New("CURVE_META_BALANCE_ACCOUNTING")
		}
	}
	return nil
}

// A base virtual price remains valid through local swaps only when the base
// pool itself cannot be mutated by the same quote plan.
func ValidateCurveMetaDependencies(pools []Pool) error {
	ids := map[string]bool{}
	for _, p := range pools {
		ids[p.PoolID] = true
	}
	var observed *CurveMetaState
	var lpCode string
	for _, p := range pools {
		if p.MultiAsset == nil || p.MultiAsset.CurveMeta == nil {
			continue
		}
		m := p.MultiAsset.CurveMeta
		if ids[m.BasePool] {
			return errors.New("CURVE_META_MUTABLE_BASE_DEPENDENCY")
		}
		if len(p.MultiAsset.Assets) != 2 {
			return errors.New("CURVE_META_PARAMETERS")
		}
		if observed != nil && (observed.BasePool != m.BasePool || observed.BasePoolCodeSHA256 != m.BasePoolCodeSHA256 || observed.BaseVirtualPrice != m.BaseVirtualPrice || observed.BlockNumber != m.BlockNumber || observed.BlockHash != m.BlockHash || observed.Timestamp != m.Timestamp || lpCode != p.MultiAsset.Assets[1].CodeSHA256) {
			return errors.New("CURVE_META_DEPENDENCY_CONFLICT")
		}
		observed = m
		lpCode = p.MultiAsset.Assets[1].CodeSHA256
	}
	return nil
}
