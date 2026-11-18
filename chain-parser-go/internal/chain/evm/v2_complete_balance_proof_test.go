package evm

import (
	"context"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"testing"
)

func TestV2CompleteNativeBalanceProofBindsOriginalTokenCodeAndReachedSlots(t *testing.T) {
	f := newV2ProofBank(t, "")
	a := f.adapter(t, "")
	ref, pools, err := a.ReadState(context.Background(), fmt.Sprintf("0x%x", f.bank.StateAnchor.Number), true)
	if err != nil || len(pools) != 1 || f.proofReads.Load() != 6 {
		t.Fatal(err, len(pools), f.proofReads.Load())
	}
	if pools[0].V2.Swap.RawBalance0 != f.bank.Pool.V2.Swap.RawBalance0 || pools[0].V2.Swap.RawBalance1 != f.bank.Pool.V2.Swap.RawBalance1 {
		t.Fatal("independent storage-view execution changed original raw balances")
	}
	if out := os.Getenv("XONE_V2_NATIVE_BALANCE_OUTPUT"); out != "" {
		if err = os.MkdirAll(out, 0700); err != nil {
			t.Fatal(err)
		}
		raw, _ := json.MarshalIndent(map[string]any{"scope": "ORIGINAL_TOKEN_CODE_REAL_MPT_SINGLE_ACCOUNT_VIEW_SYNTHETIC_HEADER", "reference": ref, "pool": pools[0], "proof_requests": f.proofReads.Load(), "token_balances_proven": true, "token_transfer_behavior_verified": false, "consensus_verified": false, "view_gas_cap": committedViewGas, "view_slot_cap": committedViewSlots}, "", "  ")
		if err = os.WriteFile(filepath.Join(out, "normal-token-balance-proof.json"), raw, 0600); err != nil {
			t.Fatal(err)
		}
	}
}
func TestV2CompleteNativeBalanceProofRejectsGetterStateCodeAndUnsupportedView(t *testing.T) {
	for _, fault := range []string{"getter-token-balance", "committed-token-balance", "unsupported-token-view", "missing-token-storage", "token-code-mismatch", "slot-limit-token"} {
		t.Run(fault, func(t *testing.T) {
			f := newV2ProofBank(t, fault)
			a := f.adapter(t, fault)
			_, pools, err := a.ReadState(context.Background(), fmt.Sprintf("0x%x", f.bank.StateAnchor.Number), true)
			if err == nil || len(pools) != 0 {
				t.Fatal("unproved token balance published", fault, err, pools)
			}
		})
	}
}

func TestV2CompleteNativeBalanceProofUsesSealedEmptyRootWithoutUnnecessarySlotIO(t *testing.T) {
	f := newV2ProofBank(t, "empty-token-storage")
	a := f.adapter(t, "empty-token-storage")
	_, pools, err := a.ReadState(context.Background(), fmt.Sprintf("0x%x", f.bank.StateAnchor.Number), true)
	if err != nil || len(pools) != 1 || pools[0].V2.Swap.RawBalance0 != "0" || f.proofReads.Load() != 5 {
		t.Fatal(err, len(pools), f.proofReads.Load())
	}
}
