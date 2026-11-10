package evm

import (
	"context"
	"fmt"
	"strings"
	"testing"
)

func TestNativeSemanticsActualActivationAndDatasetBinding(t *testing.T) {
	ids := map[string]bool{}
	for _, name := range []string{"ethereum", "base-regolith", "base-canyon", "optimism-regolith", "optimism-canyon", "arbitrum", "bsc", "polygon", "avalanche", "avalanche-current"} {
		t.Run(name, func(t *testing.T) {
			v := loadChainReceiptVector(t, name)
			header, err := VerifyChainHeaderJSON(context.Background(), v.ChainKey, retainedChainHeader(t, name))
			if err != nil {
				t.Fatal(err)
			}
			sem, err := ResolveNativeSemantics(v.ChainKey, header)
			if err != nil {
				t.Fatal(err)
			}
			if sem.ID == "" || sem.GenesisHash == "" || sem.ActivationID == "" || len(sem.SourcePins) == 0 || !sem.Admitted {
				t.Fatal(sem)
			}
			if ids[sem.ID] {
				t.Fatal("chain/activation scopes collapsed into one dataset semantics", name, sem.ID)
			}
			ids[sem.ID] = true
			again, _ := ResolveNativeSemantics(v.ChainKey, header)
			if again.ID != sem.ID {
				t.Fatal("unstable native semantics ID")
			}
			if name == "arbitrum" && (sem.ArbOSVersion == nil || *sem.ArbOSVersion != 10) {
				t.Fatal("ArbOS version must come from committed header", sem)
			}
			if name == "polygon" && sem.ReceiptCoverageScope != "canonical-transactions/system-isolated" {
				t.Fatal(sem)
			}
			if name == "avalanche-current" && sem.ReceiptAuthority != "successor-sae-settling-window" {
				t.Fatal(sem)
			}
		})
	}
}
func TestNativeSemanticsForkBoundarySeparatesDatasetIDs(t *testing.T) {
	v := loadChainReceiptVector(t, "avalanche-current")
	header, err := VerifyChainHeaderJSON(context.Background(), v.ChainKey, retainedChainHeader(t, "avalanche-current"))
	if err != nil {
		t.Fatal(err)
	}
	after, _ := ResolveNativeSemantics(v.ChainKey, header)
	header.Block.Timestamp = fmt.Sprintf("0x%x", avalancheHeliconTime-1)
	header.SettledHeight = nil
	before, err := ResolveNativeSemantics(v.ChainKey, header)
	if err != nil {
		t.Fatal(err)
	}
	if before.ID == after.ID || strings.Contains(before.ActivationID, "helicon") {
		t.Fatal("activation boundary did not change dataset identity", before, after)
	}
}
