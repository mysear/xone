package config

import (
	"strings"
	"testing"
)

func TestProviderCatalogOptionalAndPublicConfiguration(t *testing.T) {
	var probe *ProviderProbeConfig
	if err := probe.Validate("evm:1"); err != nil {
		t.Fatal(err)
	}
	p := ProviderProbeConfig{HistoricalHeight: 1, HistoricalHash: "0x" + strings.Repeat("a", 64), Account: "0x" + strings.Repeat("1", 40), TimeoutMS: 1000, IntervalMS: 1000, MaxBackoffMS: 4000, MaxAgeMS: 5000, MaxProviders: 2}
	if err := p.Validate("evm:1"); err != nil {
		t.Fatal(err)
	}
	p.TimeoutMS = ^uint64(0)
	if p.Validate("evm:1") == nil {
		t.Fatal("overflow accepted")
	}
}
func TestProviderCatalogPinnedVersionsAndBounds(t *testing.T) {
	c := DynamicCatalogConfig{TimeoutMS: 1000, MaxCandidates: 8, MaxPools: 8, MaxBytes: 1 << 20, MaxReceipts: 8, MaxLogs: 16, Sources: []FactoryDeployment{{Address: "0x" + strings.Repeat("1", 40), Version: "uniswap-v2", ProvenanceID: "public-deployment", FactoryCodeSHA256: strings.Repeat("a", 64), PoolCodeSHA256: strings.Repeat("b", 64)}}}
	if err := c.Validate("evm:1"); err != nil {
		t.Fatal(err)
	}
	c.Sources[0].PoolCodeSHA256 = ""
	if c.Validate("evm:1") == nil {
		t.Fatal("unpinned pool code accepted")
	}
	c.Sources[0].PoolCodeSHA256 = strings.Repeat("b", 64)
	c.Sources = append(c.Sources, c.Sources[0])
	if c.Validate("evm:1") == nil {
		t.Fatal("ambiguous factory versions accepted")
	}
}
