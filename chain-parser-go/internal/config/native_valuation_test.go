package config

import (
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func oracleConfig(t *testing.T) Config {
	c := localRevmConfig(t)
	c.API.QuoteTimeoutMS = 1000
	c.API.QuoteConcurrency = 8
	c.Chains[0].Genesis = "0x" + strings.Repeat("ab", 32)
	c.Simulation.NativeValuation = &NativeValuationConfig{ObservationsFile: "fixtures/prices.json", Policies: []NativePricePolicy{{SourceID: "configured-oracle", ChainKey: c.Chains[0].Key, GenesisHash: c.Chains[0].Genesis, Token: "0x0000000000000000000000000000000000000010", Signers: map[string]string{"a": strings.Repeat("11", 32), "b": strings.Repeat("22", 32), "c": strings.Repeat("33", 32)}, Threshold: 2, MaxAgeMS: 60000, MaxValidityMS: 60000}}}
	return c
}
func TestOraclePolicyRejectsWrongChainsKeysQuorumsAndBackend(t *testing.T) {
	c := oracleConfig(t)
	if err := c.Validate("dex-router"); err != nil {
		t.Fatal(err)
	}
	for _, mode := range []string{"backend", "chain", "genesis", "keys", "single", "age", "duplicate", "path"} {
		c := oracleConfig(t)
		v := c.Simulation.NativeValuation
		p := &v.Policies[0]
		switch mode {
		case "backend":
			c.Simulation.Backend = "disabled"
		case "chain":
			p.ChainKey = "evm:999"
		case "genesis":
			p.GenesisHash = "0x" + strings.Repeat("cd", 32)
		case "keys":
			p.Signers["b"] = p.Signers["a"]
		case "single":
			p.Threshold = 1
		case "age":
			p.MaxAgeMS = 3600001
		case "duplicate":
			v.Policies = append(v.Policies, *p)
		case "path":
			v.ObservationsFile = ""
		}
		if c.Validate("dex-router") == nil {
			t.Fatal("oracle policy accepted", mode)
		}
	}
}
func TestOracleAuthoritiesArePartOfPublicAndEconomicIdentity(t *testing.T) {
	directory := os.Getenv("XONE_NATIVE_VALUATION_CONFIG_VECTORS")
	if directory == "" {
		directory = t.TempDir()
	}
	if err := os.MkdirAll(directory, 0700); err != nil {
		t.Fatal(err)
	}
	var vectors []map[string]any
	seenPublic, seenEconomic := map[string]bool{}, map[string]bool{}
	for i, mode := range []string{"absent", "configured", "rotated", "age", "candidate-costs", "candidate-limit"} {
		c := oracleConfig(t)
		switch mode {
		case "absent":
			c.Simulation.NativeValuation = nil
		case "rotated":
			c.Simulation.NativeValuation.Policies[0].Signers["a"] = strings.Repeat("44", 32)
		case "age":
			c.Simulation.NativeValuation.Policies[0].MaxAgeMS = 50000
		case "candidate-costs":
			c.Routing.MaxSplits = 1
			c.Simulation.CandidateCostMaxPlans = 16
		case "candidate-limit":
			c.Routing.MaxSplits = 1
			c.Simulation.CandidateCostMaxPlans = 1
		}
		name := "config-" + string(rune('0'+i)) + ".json"
		raw, err := json.Marshal(c)
		if err != nil {
			t.Fatal(err)
		}
		path := filepath.Join(directory, name)
		if err = os.WriteFile(path, raw, 0600); err != nil {
			t.Fatal(err)
		}
		loaded, err := Load(path, "dex-router")
		if err != nil {
			t.Fatal(err)
		}
		if seenPublic[loaded.PublicHash] || seenEconomic[loaded.EconomicHash] {
			t.Fatal("oracle authority omitted from identity", mode)
		}
		seenPublic[loaded.PublicHash], seenEconomic[loaded.EconomicHash] = true, true
		vectors = append(vectors, map[string]any{"file": name, "public_hash": loaded.PublicHash, "economic_hash": loaded.EconomicHash})
	}
	raw, err := json.MarshalIndent(map[string]any{"vectors": vectors}, "", "  ")
	if err != nil {
		t.Fatal(err)
	}
	if err = os.WriteFile(filepath.Join(directory, "vectors.json"), raw, 0600); err != nil {
		t.Fatal(err)
	}
}

func TestCandidateExecutionCostsRequireOptInBoundedConfiguration(t *testing.T) {
	for _, mode := range []string{"valid", "two-routes", "limit", "backend", "valuation", "splits"} {
		c := oracleConfig(t)
		c.Routing.MaxSplits = 1
		c.Simulation.CandidateCostMaxPlans = 16
		switch mode {
		case "limit":
			c.Simulation.CandidateCostMaxPlans = 17
		case "backend":
			c.Simulation.Backend = "disabled"
		case "valuation":
			c.Simulation.NativeValuation = nil
		case "two-routes":
			c.Routing.MaxSplits = 2
		case "splits":
			c.Routing.MaxSplits = 3
		}
		if (c.Validate("dex-router") == nil) != (mode == "valid" || mode == "two-routes") {
			t.Fatal("candidate cost configuration", mode)
		}
	}
}

func TestDurablePriceJournalConfiguration(t *testing.T) {
	dir := os.Getenv("XONE_PRICE_JOURNAL_CONFIGS")
	if dir == "" {
		dir = t.TempDir()
	}
	if err := os.MkdirAll(dir, 0700); err != nil {
		t.Fatal(err)
	}
	c := oracleConfig(t)
	var rows []map[string]string
	for _, name := range []string{"default", "durable"} {
		if name == "durable" {
			path := "state/prices.journal"
			c.Simulation.NativeValuation.PublicationJournal = &path
		}
		file := filepath.Join(dir, name+".json")
		data, err := json.Marshal(c)
		if err != nil {
			t.Fatal(err)
		}
		if err = os.WriteFile(file, data, 0600); err != nil {
			t.Fatal(err)
		}
		s, err := Load(file, "dex-router")
		if err != nil {
			t.Fatal(err)
		}
		rows = append(rows, map[string]string{"name": name, "public_hash": s.PublicHash, "economic_hash": s.EconomicHash})
	}
	for _, k := range []string{"public_hash", "economic_hash"} {
		if rows[0][k] == rows[1][k] {
			t.Fatal("journal omitted from", k)
		}
	}
	for _, path := range []string{"", "bad\x00path", strings.Repeat("p", 4097), c.Simulation.NativeValuation.ObservationsFile} {
		c.Simulation.NativeValuation.PublicationJournal = &path
		if c.Validate("dex-router") == nil {
			t.Fatal("invalid journal path accepted")
		}
	}
	b, err := json.Marshal(rows)
	if err != nil {
		t.Fatal(err)
	}
	if err = os.WriteFile(filepath.Join(dir, "hashes.json"), b, 0600); err != nil {
		t.Fatal(err)
	}
}
