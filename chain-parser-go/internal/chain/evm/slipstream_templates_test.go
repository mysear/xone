package evm

import (
	"encoding/hex"
	"os"
	"strings"
	"testing"

	"xone/chain-parser-go/internal/domain"
)

func TestSlipstreamUnstakedStorageFactoryRuntime(t *testing.T) {
	// Base block 63180533, CustomUnstakedFeeModule at 0x0ad08370c76ff426f534bb2affd9b5555338ee68.
	fixture, err := os.ReadFile("testdata/slipstream-custom-unstaked-base.hex")
	if err != nil {
		t.Fatal(err)
	}
	runtime, err := hex.DecodeString(strings.TrimSpace(string(fixture)))
	if err != nil {
		t.Fatal(err)
	}
	bindings := map[string]string{"factory": "0x5e7bb104d84c7cb9b682aac2f3d509f5f406809a"}
	template, err := validateSlipstreamRuntime("custom_unstaked", runtime, bindings)
	if err != nil || template != domain.SlipstreamCustomUnstakedStorageFactoryTemplate {
		t.Fatalf("observed template %q, error %v", template, err)
	}
	for _, kind := range []string{"custom_swap", "dynamic_swap"} {
		if _, err := validateSlipstreamRuntime(kind, runtime, bindings); err == nil {
			t.Fatalf("unstaked module admitted as %s", kind)
		}
	}
	for _, offset := range []int{0, 848} {
		changed := append([]byte(nil), runtime...)
		changed[offset] ^= 1
		if _, err := validateSlipstreamRuntime("custom_unstaked", changed, bindings); err == nil {
			t.Fatalf("modified deployment admitted at offset %d", offset)
		}
	}
}
