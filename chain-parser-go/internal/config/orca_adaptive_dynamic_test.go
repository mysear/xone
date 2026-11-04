package config

import "testing"

func TestOrcaAdaptiveDynamicConfigBoundaries(t *testing.T) {
	c := orcaAdaptiveConfig(t)
	c.Chains[0].Pools[0].Protocol = "orca-whirlpools-adaptive-dynamic"
	if e := c.Validate("dex-router"); e != nil {
		t.Fatal(e)
	}
	for _, starts := range [][]int32{{-5632, 0}, {-5632, 0, 5632, 11264}, {-5632, 0, 5720}, {-5631, 1, 5633}} {
		q := orcaAdaptiveConfig(t)
		q.Chains[0].Pools[0].Protocol = "orca-whirlpools-adaptive-dynamic"
		q.Chains[0].Pools[0].OrcaWhirlpool.TickArrayStarts = starts
		if q.Validate("dex-router") == nil {
			t.Fatal("invalid envelope accepted", starts)
		}
	}
}
