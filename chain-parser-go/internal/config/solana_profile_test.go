package config

import "testing"

// Exercise the shared router configuration contract without another service's files.
func solanaProfileRouterConfig(t *testing.T) *Snapshot {
	t.Helper()
	cfg, err := Load("../../config/config-test.yaml", "chain-parser-go")
	if err != nil {
		t.Fatal(err)
	}
	cfg.Config.Service.Name = "dex-router"
	cfg.Config.State.Engine = "memory"
	cfg.Config.State.Path = ""
	cfg.Config.Routing = Routing{MaxHops: 3, MaxCandidates: 32, MaxExpansions: 1000, SplitParts: 20, MaxSplits: 2, ActivationCost: "0", MaxAge: 180000, TTL: 5000}
	cfg.Config.Simulation = Simulation{Backend: "disabled", MaxGas: 15000000, MaxReads: 4096, TimeoutMS: 1000}
	return cfg
}
