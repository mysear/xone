package config

import "testing"

func TestRPCReadRetryBounds(t *testing.T) {
	cfg, err := Load("../../config/config-test.yaml", "chain-parser-go")
	if err != nil {
		t.Fatal(err)
	}
	for _, tc := range []struct {
		cfg   RPCRetryConfig
		valid bool
	}{
		{RPCRetryConfig{3, 1, 10}, true}, {RPCRetryConfig{0, 1, 10}, false}, {RPCRetryConfig{17, 1, 10}, false}, {RPCRetryConfig{2, 0, 5}, false}, {RPCRetryConfig{2, 10, 5}, false}, {RPCRetryConfig{2, 1, cfg.Config.Journal.TimeoutMS}, false},
	} {
		cfg.Config.Chains[0].RPCRetry = &tc.cfg
		err = cfg.Config.Validate("chain-parser-go")
		if (err == nil) != tc.valid {
			t.Fatalf("%+v: %v", tc.cfg, err)
		}
	}
}
