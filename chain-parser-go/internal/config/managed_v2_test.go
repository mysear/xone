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
	for _, protocol := range []string{domain.ManagedV2Protocol, domain.ManagedV2ScheduledProtocol} {
		t.Run(protocol, func(t *testing.T) {
			managedV2ConfigJSON := strings.Replace(managedV2ConfigJSON, domain.ManagedV2Protocol, protocol, 1)

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
			if cfg.Config.Chains[0].Pools[0].Protocol != protocol {
				t.Fatal("protocol changed")
			}
		})
	}
}

func TestManagedV2ConfigProviderFeeIDUint8Bounds(t *testing.T) {
	for _, protocol := range []string{domain.ManagedV2Protocol, domain.ManagedV2ScheduledProtocol} {
		t.Run(protocol, func(t *testing.T) {
			managedV2ConfigJSON := strings.Replace(managedV2ConfigJSON, domain.ManagedV2Protocol, protocol, 1)

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
		})
	}
}

func TestManagedV2ConfigBoundaries(t *testing.T) {
	for _, protocol := range []string{domain.ManagedV2Protocol, domain.ManagedV2ScheduledProtocol} {
		t.Run(protocol, func(t *testing.T) {
			managedV2ConfigJSON := strings.Replace(managedV2ConfigJSON, domain.ManagedV2Protocol, protocol, 1)

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
			for name, mut := range map[string]func(*PoolConfig){
				"custom_protocol":     func(p *PoolConfig) { p.Protocol = "balancer-v2-managed-scheduled-custom" },
				"pool_hash":           func(p *PoolConfig) { p.MultiAsset.ExpectedCodeSHA256 = "" },
				"vault_hash":          func(p *PoolConfig) { p.MultiAsset.ExpectedVaultCodeSHA256 = "" },
				"factory_hash":        func(p *PoolConfig) { p.MultiAsset.ManagedV2.ExpectedFactoryCodeSHA256 = "" },
				"runtime_template":    func(p *PoolConfig) { p.MultiAsset.ManagedV2.ExpectedPoolRuntimeTemplateSHA256 = "" },
				"collector_hash":      func(p *PoolConfig) { p.MultiAsset.ManagedV2.ExpectedProtocolCollectorCodeSHA256 = "" },
				"manager":             func(p *PoolConfig) { p.MultiAsset.ManagedV2.ExpectedManager = "" },
				"missing_role":        func(p *PoolConfig) { p.MultiAsset.ManagedV2.Dependencies = p.MultiAsset.ManagedV2.Dependencies[:4] },
				"dependency_hash":     func(p *PoolConfig) { p.MultiAsset.ManagedV2.Dependencies[0].CodeSHA256 = "" },
				"dependency_template": func(p *PoolConfig) { p.MultiAsset.ManagedV2.Dependencies[0].TemplateSHA256 = "" },
				"minimum_fee":         func(p *PoolConfig) { p.FeeNumerator = 999999999999 },
				"fee_denominator":     func(p *PoolConfig) { p.FeeDenominator = 1000000 },
				"first_token":         func(p *PoolConfig) { p.Token0 = p.Token1 },
				"second_token":        func(p *PoolConfig) { p.Token1 = p.Token0 },
				"duplicate_tokens":    func(p *PoolConfig) { p.MultiAsset.Tokens[1] = p.MultiAsset.Tokens[0] },
				"unsorted_tokens": func(p *PoolConfig) {
					p.MultiAsset.Tokens[0], p.MultiAsset.Tokens[1] = p.MultiAsset.Tokens[1], p.MultiAsset.Tokens[0]
					p.Token0, p.Token1 = p.Token1, p.Token0
				},
				"token_hash":             func(p *PoolConfig) { p.MultiAsset.Tokens[0].ExpectedCodeSHA256 = "" },
				"non_evm_token":          func(p *PoolConfig) { p.MultiAsset.Tokens[0].Token = "foreign"; p.Token0 = "foreign" },
				"specialization":         func(p *PoolConfig) { p.MultiAsset.PoolID = p.ID + "0002" + strings.Repeat("0", 20) },
				"implementation":         func(p *PoolConfig) { p.MultiAsset.ExpectedImplementation = p.ID },
				"implementation_hash":    func(p *PoolConfig) { p.MultiAsset.ExpectedImplementationCodeSHA256 = strings.Repeat("a", 64) },
				"lp_token":               func(p *PoolConfig) { p.MultiAsset.ExpectedLPToken = p.ID },
				"lp_token_hash":          func(p *PoolConfig) { p.MultiAsset.ExpectedLPTokenCodeSHA256 = strings.Repeat("a", 64) },
				"base_pool_hash":         func(p *PoolConfig) { p.MultiAsset.ExpectedBasePoolCodeSHA256 = strings.Repeat("a", 64) },
				"foreign_weighted":       func(p *PoolConfig) { p.Weighted = &WeightedPoolConfig{} },
				"foreign_stable":         func(p *PoolConfig) { p.Stable = &StablePoolConfig{} },
				"foreign_program":        func(p *PoolConfig) { p.ProgramID = p.ID },
				"foreign_accounts":       func(p *PoolConfig) { p.Accounts = []string{p.ID} },
				"legacy_without_payload": func(p *PoolConfig) { p.Protocol = "balancer-v2-managed-fees"; p.MultiAsset.ManagedV2 = nil },
				"old_profile":            func(p *PoolConfig) { p.Protocol = "balancer-v2-managed-fees" }, "foreign_model": func(p *PoolConfig) { p.Protocol = "curve-stableswap-ng" }, "missing": func(p *PoolConfig) { p.MultiAsset.ManagedV2 = nil }, "fee": func(p *PoolConfig) { p.FeeNumerator = 950000000000000001 }, "aum": func(p *PoolConfig) { p.MultiAsset.ManagedV2.ExpectedManagementAUMFee = "950000000000000001" }, "provider": func(p *PoolConfig) { p.MultiAsset.ManagedV2.ExpectedFeeProviderCodeSHA256 = "" }, "role": func(p *PoolConfig) {
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
		})
	}
}
func TestManagedV2ConfigStrictFields(t *testing.T) {
	for _, protocol := range []string{domain.ManagedV2Protocol, domain.ManagedV2ScheduledProtocol} {
		t.Run(protocol, func(t *testing.T) {
			managedV2ConfigJSON := strings.Replace(managedV2ConfigJSON, domain.ManagedV2Protocol, protocol, 1)

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
		})
	}
}

func TestManagedV2ConfigKnownZero(t *testing.T) {
	for _, protocol := range []string{domain.ManagedV2Protocol, domain.ManagedV2ScheduledProtocol} {
		t.Run(protocol, func(t *testing.T) {
			managedV2ConfigJSON := strings.Replace(managedV2ConfigJSON, domain.ManagedV2Protocol, protocol, 1)

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
		})
	}
}

func TestManagedV2ScheduledConfigIndependentProfile(t *testing.T) {
	cfg, err := Load("../../config/config-test.yaml", "chain-parser-go")
	if err != nil {
		t.Fatal(err)
	}
	var p PoolConfig
	if err = json.Unmarshal([]byte(managedV2ConfigJSON), &p); err != nil {
		t.Fatal(err)
	}
	p.Protocol = domain.ManagedV2ScheduledProtocol
	cfg.Config.Chains[0].Pools = []PoolConfig{p}
	if err = cfg.Config.Validate("chain-parser-go"); err != nil {
		t.Fatal(err)
	}
	if cfg.Config.Chains[0].Pools[0].Protocol != domain.ManagedV2ScheduledProtocol {
		t.Fatal("protocol changed")
	}
}

func TestManagedV2ConfigRejectsScheduleInput(t *testing.T) {
	for _, protocol := range []string{domain.ManagedV2Protocol, domain.ManagedV2ScheduledProtocol} {
		for _, scope := range []string{"multi_asset", "managed_v2"} {
			for _, field := range []string{"schedule", "raw_schedule", "source_storage", "source_storage9", "source_storage11", "profile_version", "config_version"} {
				t.Run(protocol+"/"+scope+"/"+field, func(t *testing.T) {
					var v map[string]any
					if err := json.Unmarshal([]byte(managedV2ConfigJSON), &v); err != nil {
						t.Fatal(err)
					}
					v["protocol"] = protocol
					m := v["multi_asset"].(map[string]any)
					if scope == "managed_v2" {
						m = m["managed_v2"].(map[string]any)
					}
					m[field] = "forged"
					raw, err := json.Marshal(v)
					if err != nil {
						t.Fatal(err)
					}
					var p PoolConfig
					if json.Unmarshal(raw, &p) == nil {
						t.Fatal("unknown schedule/version/storage field accepted")
					}
				})
			}
		}
	}
}

func TestManagedV2BptConfigRequiresExplicitObservationScope(t *testing.T) {
	var value map[string]any
	if e := json.Unmarshal([]byte(managedV2ConfigJSON), &value); e != nil {
		t.Fatal(e)
	}
	value["protocol"] = domain.ManagedV2BptProtocol
	multi := value["multi_asset"].(map[string]any)
	multi["token_policy"] = "standard-bpt"
	managed := multi["managed_v2"].(map[string]any)
	managed["bpt"] = map[string]any{"allowlist_subjects": []string{"0x0000000000000000000000000000000000000001"}}
	base, _ := json.Marshal(value)
	cfg, e := Load("../../config/config-test.yaml", "chain-parser-go")
	if e != nil {
		t.Fatal(e)
	}
	check := func(raw []byte) bool {
		var p PoolConfig
		if json.Unmarshal(raw, &p) != nil {
			return false
		}
		cfg.Config.Chains[0].Pools = []PoolConfig{p}
		return cfg.Config.Validate("chain-parser-go") == nil
	}
	if !check(base) {
		t.Fatal("explicit BPT source observation scope rejected")
	}
	for kind := 0; kind < 7; kind++ {
		var v map[string]any
		_ = json.Unmarshal(base, &v)
		m := v["multi_asset"].(map[string]any)
		c := m["managed_v2"].(map[string]any)
		switch kind {
		case 0:
			delete(c, "bpt")
		case 1:
			c["bpt"] = nil
		case 2:
			m["token_policy"] = "standard-no-bpt"
		case 3:
			v["protocol"] = domain.ManagedV2ScheduledProtocol
		case 4:
			c["bpt"] = map[string]any{}
		case 5:
			c["bpt"] = map[string]any{"allowlist_subjects": nil}
		case 6:
			c["bpt"] = map[string]any{"allowlist_subjects": []string{"0x0000000000000000000000000000000000000001", "0x0000000000000000000000000000000000000001"}}
		}
		raw, _ := json.Marshal(v)
		if check(raw) {
			t.Fatalf("invalid BPT scope %d accepted", kind)
		}
	}
}

func TestManagedV2BreakerConfigHasIndependentProfileAndObservationPolicy(t *testing.T) {
	raw := strings.Replace(managedV2ConfigJSON, domain.ManagedV2Protocol, domain.ManagedV2BreakerProtocol, 1)
	raw = strings.Replace(raw, "standard-no-bpt", "standard-breaker-bpt", 1)
	raw = strings.Replace(raw, `"expected_factory":`, `"bpt":{"allowlist_subjects":[]},"expected_factory":`, 1)
	cfg, e := Load("../../config/config-test.yaml", "chain-parser-go")
	if e != nil {
		t.Fatal(e)
	}
	check := func(input string) bool {
		var p PoolConfig
		if json.Unmarshal([]byte(input), &p) != nil {
			return false
		}
		cfg.Config.Chains[0].Pools = []PoolConfig{p}
		return cfg.Config.Validate("chain-parser-go") == nil
	}
	if !check(raw) {
		t.Fatal("breaker independent source config rejected")
	}
	for _, bad := range []string{strings.Replace(raw, "standard-breaker-bpt", "standard-bpt", 1), strings.Replace(raw, domain.ManagedV2BreakerProtocol, domain.ManagedV2BptProtocol, 1), strings.Replace(raw, `"bpt":{"allowlist_subjects":[]},`, "", 1)} {
		if check(bad) {
			t.Fatal("breaker config downgrade or incomplete state accepted")
		}
	}
}
