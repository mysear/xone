package config

import (
	"encoding/json"
	"os"
	"testing"
	"xone/chain-parser-go/internal/domain"
)

func TestCryptoConfigRequiresPinnedLPAndDedicatedProfile(t *testing.T) {
	raw, e := os.ReadFile("../../test/contracts/curve-crypto-wire.json")
	if e != nil {
		t.Fatal(e)
	}
	var f struct {
		Snapshot domain.Snapshot `json:"snapshot"`
	}
	if e = json.Unmarshal(raw, &f); e != nil {
		t.Fatal(e)
	}
	p := f.Snapshot.Pools[0]
	s := p.MultiAsset
	v := &MultiAssetConfig{ExpectedCodeSHA256: s.CodeSHA256, TokenPolicy: "standard-no-bpt"}
	b, _ := json.Marshal(v)
	var fields map[string]any
	json.Unmarshal(b, &fields)
	fields["expected_lp_token"] = s.CurveCrypto.LPToken
	fields["expected_lp_token_code_sha256"] = s.CurveCrypto.LPTokenCodeSHA256
	b, _ = json.Marshal(fields)
	json.Unmarshal(b, v)
	for _, a := range s.Assets {
		v.Tokens = append(v.Tokens, MultiTokenConfig{Token: a.Token, ExpectedCodeSHA256: a.CodeSHA256})
	}
	pc := PoolConfig{ID: p.PoolID, Protocol: p.Protocol, Token0: p.Token0, Token1: p.Token1, FeeNumerator: p.FeeNumerator, FeeDenominator: p.FeeDenominator, MultiAsset: v}
	cfg, e := Load("../../config/config-test.yaml", "chain-parser-go")
	if e != nil {
		t.Fatal(e)
	}
	cfg.Config.Chains[0].Pools = []PoolConfig{pc}
	if e = cfg.Config.Validate("chain-parser-go"); e != nil {
		t.Fatal("Crypto config missing", e)
	}
	for _, bad := range []string{"missing", "lp-code", "proxy", "legacy"} {
		t.Run(bad, func(t *testing.T) {
			copy := *v
			cfg.Config.Chains[0].Pools[0].MultiAsset = &copy
			cfg.Config.Chains[0].Pools[0].Protocol = p.Protocol
			b, _ := json.Marshal(copy)
			var m map[string]any
			json.Unmarshal(b, &m)
			switch bad {
			case "missing":
				m["expected_lp_token"] = ""
			case "lp-code":
				m["expected_lp_token_code_sha256"] = ""
			case "proxy":
				m["expected_implementation"] = s.CurveCrypto.LPToken
			case "legacy":
				cfg.Config.Chains[0].Pools[0].Protocol = "curve-plain3-basic"
			}
			b, _ = json.Marshal(m)
			json.Unmarshal(b, &copy)
			if cfg.Config.Validate("chain-parser-go") == nil {
				t.Fatal("invalid LP admitted")
			}
		})
	}
}

func TestCryptoOriginalServiceConfigurations(t *testing.T) {
	for _, fixture := range []string{"../../test/contracts/curve-crypto-service.json", "../../test/contracts/curve-crypto-ramp-service.json"} {
		raw, e := os.ReadFile(fixture)
		if e != nil {
			t.Fatal(e)
		}
		var f struct {
			Cases []struct {
				Config PoolConfig `json:"config"`
			} `json:"cases"`
		}
		if e = json.Unmarshal(raw, &f); e != nil {
			t.Fatal(e)
		}
		cfg, e := Load("../../config/config-test.yaml", "chain-parser-go")
		if e != nil {
			t.Fatal(e)
		}
		for _, c := range f.Cases {
			cfg.Config.Chains[0].Pools = []PoolConfig{c.Config}
			if e = cfg.Config.Validate("chain-parser-go"); e != nil {
				t.Fatal(e)
			}
		}
	}
}
