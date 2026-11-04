package config

import "testing"

func localRevmConfig(t *testing.T) Config {
	t.Helper()
	c := solanaProfileRouterConfig(t).Config
	c.Simulation = Simulation{Backend: "revm-fixture", MaxGas: 15000000, MaxReads: 4096, TimeoutMS: 1000, AllowedTargets: []string{"0x0000000000000000000000000000000000000001"}, StateFile: "fixtures/evm-state.json", MaxStateBytes: 8388608, MaxInstructions: 2000000, MaxMemoryBytes: 67108864, Concurrency: 2}
	return c
}
func TestLocalRevmConfigurationExplicitBoundsAndDisabledIsolation(t *testing.T) {
	c := localRevmConfig(t)
	if err := c.Validate("dex-router"); err != nil {
		t.Fatal(err)
	}
	for _, mutate := range []func(*Simulation){
		func(s *Simulation) { s.StateFile = "" }, func(s *Simulation) { s.RPCURL = "http://127.0.0.1:8545" },
		func(s *Simulation) { s.MaxStateBytes = 0 }, func(s *Simulation) { s.MaxStateBytes = 16777217 },
		func(s *Simulation) { s.MaxInstructions = 0 }, func(s *Simulation) { s.MaxInstructions = 100000001 },
		func(s *Simulation) { s.MaxMemoryBytes = 0 }, func(s *Simulation) { s.MaxMemoryBytes = 67108865 },
		func(s *Simulation) { s.Concurrency = 0 }, func(s *Simulation) { s.Concurrency = 9 },
		func(s *Simulation) { s.AllowedTargets = nil }, func(s *Simulation) { s.AllowedTargets = []string{"invalid"} },
		func(s *Simulation) { s.AllowedTargets = []string{"0x0000000000000000000000000000000000000000"} },
		func(s *Simulation) { s.AllowedTargets = append(s.AllowedTargets, s.AllowedTargets[0]) },
		func(s *Simulation) { s.Backend = "disabled" }, func(s *Simulation) { s.Backend = "evm-rpc" },
	} {
		q := localRevmConfig(t)
		mutate(&q.Simulation)
		if q.Validate("dex-router") == nil {
			t.Fatal("invalid local EVM execution settings accepted", q.Simulation)
		}
	}
}
