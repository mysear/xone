package config

import (
	"bytes"
	"encoding/json"
	"errors"
	"io"
	"math/big"
	"reflect"
	"strings"
	"xone/chain-parser-go/internal/domain"
)

// Every configured digest is an expected identity; parsing must verify its origin/template.
type ManagedV2PoolConfig struct {
	ExpectedFactory                     string                       `json:"expected_factory"`
	ExpectedFactoryCodeSHA256           string                       `json:"expected_factory_code_sha256"`
	ExpectedPoolRuntimeTemplateSHA256   string                       `json:"expected_pool_runtime_template_sha256"`
	ExpectedFeeProvider                 string                       `json:"expected_fee_provider"`
	ExpectedFeeProviderCodeSHA256       string                       `json:"expected_fee_provider_code_sha256"`
	ExpectedProtocolCollector           string                       `json:"expected_protocol_collector"`
	ExpectedProtocolCollectorCodeSHA256 string                       `json:"expected_protocol_collector_code_sha256"`
	ExpectedManager                     string                       `json:"expected_manager"`
	ExpectedProviderFeeID               domain.Amount                `json:"expected_provider_fee_id"`
	ExpectedManagementAUMFee            domain.Amount                `json:"expected_management_aum_fee"`
	Dependencies                        []domain.ManagedV2Dependency `json:"dependencies"`
}

func (v *ManagedV2PoolConfig) UnmarshalJSON(raw []byte) error {
	type plain ManagedV2PoolConfig
	var p plain
	if e := decodeManagedV2ConfigJSON(raw, &p, true); e != nil {
		return e
	}
	*v = ManagedV2PoolConfig(p)
	return nil
}
func (v *MultiAssetConfig) UnmarshalJSON(raw []byte) error {
	type plain MultiAssetConfig
	var p plain
	if e := decodeManagedV2ConfigJSON(raw, &p, false); e != nil {
		return e
	}
	*v = MultiAssetConfig(p)
	return nil
}
func decodeManagedV2ConfigJSON(raw []byte, out any, required bool) error {
	typ := reflect.TypeOf(out).Elem()
	allowed := map[string]bool{}
	for i := 0; i < typ.NumField(); i++ {
		allowed[strings.Split(typ.Field(i).Tag.Get("json"), ",")[0]] = true
	}
	d := json.NewDecoder(bytes.NewReader(raw))
	start, e := d.Token()
	if e != nil || start != json.Delim('{') {
		return errors.New("MANAGED_V2_CONFIG_OBJECT")
	}
	seen := map[string]bool{}
	for d.More() {
		key, e := d.Token()
		if e != nil {
			return e
		}
		k, ok := key.(string)
		if !ok || !allowed[k] || seen[k] {
			return errors.New("MANAGED_V2_CONFIG_FIELD")
		}
		seen[k] = true
		var v json.RawMessage
		if e = d.Decode(&v); e != nil {
			return e
		}
		if (required || k == "managed_v2") && bytes.Equal(bytes.TrimSpace(v), []byte("null")) {
			return errors.New("MANAGED_V2_CONFIG_NULL")
		}
	}
	if _, e = d.Token(); e != nil {
		return e
	}
	if _, e = d.Token(); e != io.EOF {
		return errors.New("MANAGED_V2_CONFIG_TRAILING")
	}
	if required {
		for k := range allowed {
			if !seen[k] {
				return errors.New("MANAGED_V2_CONFIG_REQUIRED")
			}
		}
	}
	return json.Unmarshal(raw, out)
}
func (p PoolConfig) validateManagedV2(chain string) bool {
	v := p.MultiAsset
	if v == nil || v.ManagedV2 == nil {
		return false
	}
	m := v.ManagedV2
	if domain.ValidateChain(chain) != nil || !strings.HasPrefix(chain, "evm:") || domain.ValidateEVMIdentity(p.ID, v.ExpectedCodeSHA256) != nil || domain.ValidateEVMIdentity(v.ExpectedVault, v.ExpectedVaultCodeSHA256) != nil || domain.ValidateEVMIdentity(m.ExpectedFactory, m.ExpectedFactoryCodeSHA256) != nil || domain.ValidateEVMIdentity(m.ExpectedFeeProvider, m.ExpectedFeeProviderCodeSHA256) != nil || domain.ValidateEVMIdentity(m.ExpectedProtocolCollector, m.ExpectedProtocolCollectorCodeSHA256) != nil || domain.ValidateEVMAddress(m.ExpectedManager) != nil || domain.ValidateEVMIdentity(p.ID, m.ExpectedPoolRuntimeTemplateSHA256) != nil || domain.ValidateManagedV2Dependencies(m.Dependencies) != nil {
		return false
	}
	// ProtocolFeeCache encodes each provider fee ID in 8 bits.
	providerFeeID, e := domain.ParseAmount(m.ExpectedProviderFeeID.String())
	if e != nil || providerFeeID.Big().BitLen() > 8 {
		return false
	}
	fee, e := domain.ParseAmount(m.ExpectedManagementAUMFee.String())
	if e != nil || fee.Big().Cmp(big.NewInt(950000000000000000)) > 0 {
		return false
	}
	if p.InfinityBin != nil || p.SingletonAdvanced != nil || p.Weighted != nil || p.Stable != nil || p.V2 != nil || p.V3 != nil || p.SingletonCL != nil || p.RaydiumCPMM != nil || p.OrcaWhirlpool != nil || p.RaydiumCLMM != nil || p.ProgramID != "" || len(p.Accounts) != 0 || v.TokenPolicy != "standard-no-bpt" || v.ExpectedImplementation != "" || v.ExpectedImplementationCodeSHA256 != "" || v.ExpectedLPToken != "" || v.ExpectedLPTokenCodeSHA256 != "" || v.ExpectedBasePoolCodeSHA256 != "" || len(v.Tokens) < 2 || len(v.Tokens) > 50 || p.Token0 != v.Tokens[0].Token || p.Token1 != v.Tokens[1].Token || len(v.PoolID) != 66 || !strings.HasPrefix(v.PoolID, p.ID+"0001") || domain.ValidateEVMIdentity(p.ID, v.PoolID[2:]) != nil || p.FeeDenominator != 1000000000000000000 || p.FeeNumerator < 1000000000000 || p.FeeNumerator > 950000000000000000 {
		return false
	}
	for i, a := range v.Tokens {
		if domain.ValidateEVMIdentity(a.Token, a.ExpectedCodeSHA256) != nil || a.Token == p.ID || a.Token == v.ExpectedVault || i > 0 && v.Tokens[i-1].Token >= a.Token {
			return false
		}
	}
	return true
}
