package config

import (
	"encoding/json"
	"fmt"
	"strings"
	"testing"
	"xone/chain-parser-go/internal/domain"
)

// Synthetic format/admission checks, not original-chain deployment evidence.
const managedV2ConfigJSON = `{"protocol":"balancer-v2-managed-aum","token0":"0x0000000000000000000000000000000000000001","token1":"0x0000000000000000000000000000000000000002","fee_numerator":1000000000000,"fee_denominator":1000000000000000000,"id":"0x0000000000000000000000000000000000000046","program_id":"","accounts":[],"multi_asset":{"expected_code_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","expected_vault":"0x0000000000000000000000000000000000000047","expected_vault_code_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","pool_id":"0x0000000000000000000000000000000000000046000100000000000000000000","tokens":[{"token":"0x0000000000000000000000000000000000000001","expected_code_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"},{"token":"0x0000000000000000000000000000000000000002","expected_code_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}],"token_policy":"standard-no-bpt","managed_v2":{"expected_factory":"0x0000000000000000000000000000000000000050","expected_factory_code_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","expected_pool_runtime_template_sha256":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","expected_fee_provider":"0x0000000000000000000000000000000000000051","expected_fee_provider_code_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","expected_protocol_collector":"0x0000000000000000000000000000000000000052","expected_protocol_collector_code_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","expected_manager":"0x0000000000000000000000000000000000000053","expected_provider_fee_id":"3","expected_management_aum_fee":"10000000000000000","dependencies":[{"role":"circuit-breaker-lib","address":"0x000000000000000000000000000000000000005a","code_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","template_sha256":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"},{"role":"managed-pool-add-remove-token-lib","address":"0x000000000000000000000000000000000000005b","code_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","template_sha256":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"},{"role":"managed-pool-amm-lib","address":"0x000000000000000000000000000000000000005c","code_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","template_sha256":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"},{"role":"external-weighted-math","address":"0x000000000000000000000000000000000000005d","code_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","template_sha256":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"},{"role":"recovery-mode-helper","address":"0x000000000000000000000000000000000000005e","code_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","template_sha256":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"}]}}}`

func TestManagedV2ConfigIndependentProfile(t *testing.T) {
	cfg, e := Load("../../config/config-test.yaml", "chain-parser-go")
	if e != nil {
		t.Fatal(e)
	}
	var p PoolConfig
	if e = json.Unmarshal([]byte(managedV2ConfigJSON), &p); e != nil {
		t.Fatal(e)
	}
	cfg.Config.Chains[0].Pools = []PoolConfig{p}
	if e = cfg.Config.Validate("chain-parser-go"); e != nil {
		t.Fatal(e)
	}
}

func TestManagedV2ConfigProviderFeeIDUint8Bounds(t *testing.T) {
	cfg, e := Load("../../config/config-test.yaml", "chain-parser-go")
	if e != nil {
		t.Fatal(e)
	}
	for _, tc := range []struct {
		name  string
		feeID string
		valid bool
	}{
		{"zero", "0", true},
		{"three", "3", true},
		{"max_uint8", "255", true},
		{"uint8_overflow", "256", false},
		{"large_u256", "1606938044258990275541962092341162602522202993782792835301376", false},
		{"max_u256", "115792089237316195423570985008687907853269984665640564039457584007913129639935", false},
	} {
		t.Run(tc.name, func(t *testing.T) {
			raw := strings.Replace(managedV2ConfigJSON, `"expected_provider_fee_id":"3"`, `"expected_provider_fee_id":"`+tc.feeID+`"`, 1)
			var p PoolConfig
			if e := json.Unmarshal([]byte(raw), &p); e != nil {
				t.Fatal(e)
			}
			cfg.Config.Chains[0].Pools = []PoolConfig{p}
			if e := cfg.Config.Validate("chain-parser-go"); (e == nil) != tc.valid {
				t.Fatalf("fee ID %s: valid = %t, want %t (error: %v)", tc.feeID, e == nil, tc.valid, e)
			}
		})
	}
}

func TestManagedV2ConfigBoundaries(t *testing.T) {
	cfg, e := Load("../../config/config-test.yaml", "chain-parser-go")
	if e != nil {
		t.Fatal(e)
	}
	for _, n := range []int{2, 39, 50, 51} {
		var p PoolConfig
		_ = json.Unmarshal([]byte(managedV2ConfigJSON), &p)
		p.MultiAsset.Tokens = nil
		for i := 0; i < n; i++ {
			p.MultiAsset.Tokens = append(p.MultiAsset.Tokens, MultiTokenConfig{Token: fmt.Sprintf("0x%040x", i+1), ExpectedCodeSHA256: strings.Repeat("a", 64)})
		}
		p.FeeNumerator = 950000000000000000
		cfg.Config.Chains[0].Pools = []PoolConfig{p}
		if e := cfg.Config.Validate("chain-parser-go"); (e == nil) != (n <= 50) {
			t.Fatal(n, e)
		}
	}
	for name, mut := range map[string]func(*PoolConfig){"old_profile": func(p *PoolConfig) { p.Protocol = "balancer-v2-managed-fees" }, "foreign_model": func(p *PoolConfig) { p.Protocol = "curve-stableswap-ng" }, "missing": func(p *PoolConfig) { p.MultiAsset.ManagedV2 = nil }, "fee": func(p *PoolConfig) { p.FeeNumerator = 950000000000000001 }, "aum": func(p *PoolConfig) { p.MultiAsset.ManagedV2.ExpectedManagementAUMFee = "950000000000000001" }, "provider": func(p *PoolConfig) { p.MultiAsset.ManagedV2.ExpectedFeeProviderCodeSHA256 = "" }, "role": func(p *PoolConfig) {
		p.MultiAsset.ManagedV2.Dependencies[1].Role = p.MultiAsset.ManagedV2.Dependencies[0].Role
	}, "policy": func(p *PoolConfig) { p.MultiAsset.TokenPolicy = "standard-with-own-bpt" }, "identity": func(p *PoolConfig) {
		p.MultiAsset.ManagedV2.ExpectedFactory = strings.ToUpper(p.MultiAsset.ManagedV2.ExpectedFactory)
	}, "u256": func(p *PoolConfig) {
		p.MultiAsset.ManagedV2.ExpectedProviderFeeID = domain.Amount(strings.Repeat("9", 79))
	}} {
		t.Run(name, func(t *testing.T) {
			var p PoolConfig
			_ = json.Unmarshal([]byte(managedV2ConfigJSON), &p)
			mut(&p)
			cfg.Config.Chains[0].Pools = []PoolConfig{p}
			if cfg.Config.Validate("chain-parser-go") == nil {
				t.Fatal("invalid accepted")
			}
		})
	}
}
func TestManagedV2ConfigStrictFields(t *testing.T) {
	var v map[string]any
	_ = json.Unmarshal([]byte(managedV2ConfigJSON), &v)
	for key := range v["multi_asset"].(map[string]any)["managed_v2"].(map[string]any) {
		for _, null := range []bool{false, true} {
			var d map[string]any
			_ = json.Unmarshal([]byte(managedV2ConfigJSON), &d)
			m := d["multi_asset"].(map[string]any)["managed_v2"].(map[string]any)
			if null {
				m[key] = nil
			} else {
				delete(m, key)
			}
			raw, _ := json.Marshal(d)
			var p PoolConfig
			if json.Unmarshal(raw, &p) == nil {
				t.Error("missing/null", key)
			}
		}
	}
	for _, key := range []string{"expected_manager", "expected_provider_fee_id"} {
		raw := strings.Replace(managedV2ConfigJSON, `"`+key+`":`, `"UNKNOWN":0,"`+key+`":`, 1)
		var p PoolConfig
		if json.Unmarshal([]byte(raw), &p) == nil {
			t.Fatal("unknown field")
		}
	}
	for _, raw := range []string{strings.Replace(managedV2ConfigJSON, `"expected_manager":`, `"Expected_Manager":`, 1), strings.Replace(managedV2ConfigJSON, `"managed_v2":`, `"Managed_V2":`, 1), strings.Replace(managedV2ConfigJSON, `"expected_management_aum_fee":"10000000000000000"`, `"expected_management_aum_fee":"0","expected_management_aum_fee":"0"`, 1)} {
		var p PoolConfig
		if json.Unmarshal([]byte(raw), &p) == nil {
			t.Fatal("case alias/duplicate")
		}
	}
}

func TestManagedV2ConfigKnownZero(t *testing.T) {
	cfg, e := Load("../../config/config-test.yaml", "chain-parser-go")
	if e != nil {
		t.Fatal(e)
	}
	var p PoolConfig
	if e = json.Unmarshal([]byte(managedV2ConfigJSON), &p); e != nil {
		t.Fatal(e)
	}
	p.MultiAsset.ManagedV2.ExpectedProviderFeeID = "0"
	p.MultiAsset.ManagedV2.ExpectedManagementAUMFee = "0"
	cfg.Config.Chains[0].Pools = []PoolConfig{p}
	if e = cfg.Config.Validate("chain-parser-go"); e != nil {
		t.Fatal(e)
	}
}
