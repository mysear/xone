package config

import (
	"encoding/json"
	"os"
	"testing"
	"xone/chain-parser-go/internal/domain"
)

func TestMetaConfigRequiresPinnedCloneAndReadOnlyBaseProof(t *testing.T) {
	cfg, e := Load("../../config/config-test.yaml", "chain-parser-go")
	if e != nil {
		t.Fatal(e)
	}
	raw, e := os.ReadFile("../../test/contracts/curve-meta-wire.json")
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
	m := s.CurveMeta
	doc := map[string]any{"id": p.PoolID, "protocol": p.Protocol, "token0": p.Token0, "token1": p.Token1, "fee_numerator": p.FeeNumerator, "fee_denominator": p.FeeDenominator, "multi_asset": map[string]any{"expected_code_sha256": s.CodeSHA256, "expected_implementation": m.Implementation, "expected_implementation_code_sha256": m.ImplementationCodeSHA256, "expected_base_pool_code_sha256": m.BasePoolCodeSHA256, "tokens": []any{map[string]any{"token": s.Assets[0].Token, "expected_code_sha256": s.Assets[0].CodeSHA256}, map[string]any{"token": s.Assets[1].Token, "expected_code_sha256": s.Assets[1].CodeSHA256}}, "token_policy": "standard-base-lp"}}
	for _, bad := range []string{"", "base-proof", "implementation", "template", "clone", "policy", "lp", "length", "legacy", "mutable-base"} {
		t.Run(bad, func(t *testing.T) {
			b, _ := json.Marshal(doc)
			var raw map[string]any
			json.Unmarshal(b, &raw)
			v := raw["multi_asset"].(map[string]any)
			switch bad {
			case "base-proof":
				v["expected_base_pool_code_sha256"] = ""
			case "implementation":
				v["expected_implementation"] = ""
			case "template":
				v["expected_implementation_code_sha256"] = s.CodeSHA256
			case "clone":
				v["expected_code_sha256"] = m.ImplementationCodeSHA256
			case "policy":
				v["token_policy"] = "standard-no-bpt"
			case "lp":
				v["tokens"].([]any)[1].(map[string]any)["token"] = p.Token0
			case "length":
				v["tokens"] = v["tokens"].([]any)[:1]
			case "legacy":
				raw["protocol"] = "curve-plain3-basic"
			}
			var pool PoolConfig
			b, _ = json.Marshal(raw)
			json.Unmarshal(b, &pool)
			cfg.Config.Chains[0].Pools = []PoolConfig{pool}
			if bad == "mutable-base" {
				other := pool
				other.ID = domain.CurveMetaBasePool
				cfg.Config.Chains[0].Pools = append(cfg.Config.Chains[0].Pools, other)
			}
			err := cfg.Config.Validate("chain-parser-go")
			if (bad == "") != (err == nil) {
				t.Fatal(bad, err)
			}
		})
	}
}
