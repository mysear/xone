package config

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"xone/chain-parser-go/internal/domain"
)

func approvedMarketConfig(t *testing.T) Config {
	c := oracleConfig(t)
	c.Chains[0].Key = "evm:1"
	c.Chains[0].Genesis = "0xd4e56740f876aef8c010b86a40d5f56745a118d0906a34e69aec8c0db1cb8fa3"
	p := &c.Simulation.NativeValuation.Policies[0]
	p.ChainKey, p.GenesisHash = c.Chains[0].Key, c.Chains[0].Genesis
	f := ChainlinkFeedPolicy{Proxy: "0x" + strings.Repeat("01", 20), ProxyCodeSHA256: strings.Repeat("11", 32), Aggregator: "0x" + strings.Repeat("02", 20), AggregatorCodeSHA256: strings.Repeat("22", 32), Description: "ETH / USD", Decimals: 8, MinAnswer: "1", MaxAnswer: "100000000000000000", MaxAgeMS: 3600000}
	g := f
	g.Proxy, g.Aggregator, g.Description = "0x"+strings.Repeat("03", 20), "0x"+strings.Repeat("04", 20), "TOKEN / USD"
	p.MarketFeed = &ChainlinkUSDPairPolicy{SchemaVersion: 1, NativeDecimals: 18, TokenDecimals: 6, NativeUSD: f, TokenUSD: g}
	return c
}

func TestApprovedMarketFeedPolicyIdentityAndInvalidConfigurations(t *testing.T) {
	directory := os.Getenv("XONE_MARKET_FEED_CONFIG_VECTORS")
	if directory == "" {
		directory = t.TempDir()
	}
	if err := os.MkdirAll(directory, 0700); err != nil {
		t.Fatal(err)
	}
	var valid, invalid []map[string]any
	seenPublic, seenEconomic := map[string]bool{}, map[string]bool{}
	for i, mode := range []string{"base", "plain", "proxy", "aggregator", "code", "age", "precision", "bounds"} {
		c := approvedMarketConfig(t)
		p := &c.Simulation.NativeValuation.Policies[0]
		switch mode {
		case "plain":
			p.MarketFeed = nil
		case "proxy":
			p.MarketFeed.TokenUSD.Proxy = "0x" + strings.Repeat("05", 20)
		case "aggregator":
			p.MarketFeed.TokenUSD.Aggregator = "0x" + strings.Repeat("06", 20)
		case "code":
			p.MarketFeed.TokenUSD.AggregatorCodeSHA256 = strings.Repeat("33", 32)
		case "age":
			p.MarketFeed.NativeUSD.MaxAgeMS = 3000000
		case "precision":
			p.MarketFeed.TokenDecimals = 18
		case "bounds":
			p.MarketFeed.NativeUSD.MinAnswer = domain.Amount("2")
		}
		name := fmt.Sprintf("market-%02d.json", i)
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
			t.Fatal(mode, err)
		}
		if seenPublic[loaded.PublicHash] || seenEconomic[loaded.EconomicHash] {
			t.Fatal("market policy omitted from identity", mode)
		}
		seenPublic[loaded.PublicHash], seenEconomic[loaded.EconomicHash] = true, true
		valid = append(valid, map[string]any{"file": name, "public_hash": loaded.PublicHash, "economic_hash": loaded.EconomicHash})
	}
	for i, mode := range []string{"chain", "genesis", "native-decimals", "token-decimals", "feed-decimals", "zero-address", "upper-hash", "zero-hash", "description", "min", "max", "age", "same-proxy", "version"} {
		c := approvedMarketConfig(t)
		p := &c.Simulation.NativeValuation.Policies[0]
		f := p.MarketFeed
		switch mode {
		case "chain":
			c.Chains[0].Key = "evm:10"
			p.ChainKey = c.Chains[0].Key
		case "genesis":
			c.Chains[0].Genesis = "0x" + strings.Repeat("ab", 32)
			p.GenesisHash = c.Chains[0].Genesis
		case "native-decimals":
			f.NativeDecimals = 17
		case "token-decimals":
			f.TokenDecimals = 37
		case "feed-decimals":
			f.TokenUSD.Decimals = 19
		case "zero-address":
			f.NativeUSD.Aggregator = "0x" + strings.Repeat("0", 40)
		case "upper-hash":
			f.NativeUSD.ProxyCodeSHA256 = strings.Repeat("AB", 32)
		case "zero-hash":
			f.NativeUSD.ProxyCodeSHA256 = strings.Repeat("0", 64)
		case "description":
			f.TokenUSD.Description = "TOKEN / EUR"
		case "min":
			f.NativeUSD.MinAnswer = "0"
		case "max":
			f.NativeUSD.MaxAnswer = "57896044618658097711785492504343953926634992332820282019728792003956564819968"
		case "age":
			f.NativeUSD.MaxAgeMS = 86400001
		case "same-proxy":
			f.NativeUSD.Proxy = f.TokenUSD.Proxy
		case "version":
			f.SchemaVersion = 2
		}
		name := fmt.Sprintf("invalid-market-%02d.json", i)
		raw, err := json.Marshal(c)
		if err != nil {
			t.Fatal(err)
		}
		path := filepath.Join(directory, name)
		if err = os.WriteFile(path, raw, 0600); err != nil {
			t.Fatal(err)
		}
		if _, err = Load(path, "dex-router"); err == nil {
			t.Fatal("invalid market config accepted", mode)
		}
		invalid = append(invalid, map[string]any{"file": name, "mode": mode})
	}
	raw, err := json.MarshalIndent(map[string]any{"valid": valid, "invalid": invalid}, "", "  ")
	if err != nil {
		t.Fatal(err)
	}
	if err = os.WriteFile(filepath.Join(directory, "market-vectors.json"), raw, 0600); err != nil {
		t.Fatal(err)
	}
}
