package config

import (
	"encoding/json"
	"testing"
)

func TestOfficialV3TemplateConfigRejectsArbitraryPropertiesAndProfiles(t *testing.T) {
	for _, raw := range []string{`{"profile":"attacker-template"}`, `{"profile":"uniswap-v3-core-1.0.1-solc-0.7.6-800-istanbul","offsets":[0]}`, `{"profile":"uniswap-v3-core-1.0.1-solc-0.7.6-800-istanbul","profile":"uniswap-v3-core-1.0.1-solc-0.7.6-800-istanbul"}`, `null`, `{}`} {
		var c V3PoolTemplateConfig
		if json.Unmarshal([]byte(raw), &c) == nil {
			t.Fatal("unreviewed template configuration accepted", raw)
		}
	}
	var c V3PoolTemplateConfig
	if err := json.Unmarshal([]byte(`{"profile":"uniswap-v3-core-1.0.1-solc-0.7.6-800-istanbul"}`), &c); err != nil {
		t.Fatal(err)
	}
}
func TestOfficialV3TemplateFactoryAndGenesisAreFrozen(t *testing.T) {
	profile := V3PoolTemplateConfig{Profile: OfficialUniswapV3Pool076}
	known, err := profile.Deployment("evm:1", "0xd4e56740f876aef8c010b86a40d5f56745a118d0906a34e69aec8c0db1cb8fa3", "0x1f98431c8ad98523631ae4a59f267346ea31f984", "fd5e0ec0633afeb92b2f6b6cac36e13a9732bd1914f8158a78f127d51d54a572")
	if err != nil || known.SourceSHA256 == "" || known.TemplateSHA256 != OfficialV3RuntimeTemplateSHA256 {
		t.Fatal(known, err)
	}
	for _, bad := range []struct{ chain, genesis, factory, code string }{{"evm:999", known.Genesis, known.Factory, known.FactoryCodeSHA256}, {"evm:1", "0x00", known.Factory, known.FactoryCodeSHA256}, {"evm:1", known.Genesis, "0x0000000000000000000000000000000000000001", known.FactoryCodeSHA256}, {"evm:1", known.Genesis, known.Factory, "bad"}} {
		if _, err := profile.Deployment(bad.chain, bad.genesis, bad.factory, bad.code); err == nil {
			t.Fatal("unreviewed deployment accepted", bad)
		}
	}
}

func TestOfficialV3TemplateIsTheOnlyPoolHashException(t *testing.T) {
	known := officialV3Deployments["evm:1"]
	profile := &V3PoolTemplateConfig{Profile: OfficialUniswapV3Pool076}
	for _, test := range []struct {
		name, chain, version, factory, factoryCode, poolCode string
		profile                                              *V3PoolTemplateConfig
		valid                                                bool
	}{
		{"official", "evm:1", "uniswap-v3", known.Factory, known.FactoryCodeSHA256, "", profile, true},
		{"optional-extra-pin", "evm:1", "uniswap-v3", known.Factory, known.FactoryCodeSHA256, OfficialV3RuntimeTemplateSHA256, profile, true},
		{"legacy-hash", "evm:1", "uniswap-v3", known.Factory, known.FactoryCodeSHA256, OfficialV3RuntimeTemplateSHA256, nil, true},
		{"legacy-no-hash", "evm:1", "uniswap-v3", known.Factory, known.FactoryCodeSHA256, "", nil, false},
		{"v2-template", "evm:1", "uniswap-v2", known.Factory, known.FactoryCodeSHA256, "", profile, false},
		{"unknown-chain", "evm:999", "uniswap-v3", known.Factory, known.FactoryCodeSHA256, "", profile, false},
		{"unknown-factory", "evm:1", "uniswap-v3", "0x0000000000000000000000000000000000000001", known.FactoryCodeSHA256, "", profile, false},
		{"unknown-code", "evm:1", "uniswap-v3", known.Factory, OfficialV3RuntimeTemplateSHA256, "", profile, false},
		{"invalid-extra-pin", "evm:1", "uniswap-v3", known.Factory, known.FactoryCodeSHA256, "invalid", profile, false},
	} {
		t.Run(test.name, func(t *testing.T) {
			err := ValidateFactoryPoolCode(test.chain, test.version, test.factory, test.factoryCode, test.poolCode, test.profile)
			if (err == nil) != test.valid {
				t.Fatal(err)
			}
		})
	}
}
