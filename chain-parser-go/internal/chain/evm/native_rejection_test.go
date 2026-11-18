package evm

import (
	"context"
	"errors"
	"fmt"
	"strings"
	"testing"
)

func TestNativeSourceRejectionPreservesCauseWithoutInventingBudgetOverrun(t *testing.T) {
	for _, control := range []struct{ fault, want string }{{"getter-reserves", "V2_COMPLETE_STATE_PROOF_VALUE"}, {"getter-token-balance", "V2_COMPLETE_TOKEN_PROOF_BALANCE"}, {"bad-node", "STATE_ACCOUNT_PROOF_INVALID"}} {
		t.Run(control.fault, func(t *testing.T) {
			f := newV2ProofBank(t, control.fault)
			a := f.adapter(t, control.fault)
			_, pools, err := a.ReadState(context.Background(), fmt.Sprintf("0x%x", f.bank.StateAnchor.Number), true)
			if err == nil || !strings.Contains(err.Error(), control.want) || strings.Contains(err.Error(), "BUDGET") || len(pools) != 0 {
				t.Fatal("source rejection mislabeled", err, pools)
			}
		})
	}
}
func TestNativeRejectedSessionCannotBeRevivedByRefillingItsAllowance(t *testing.T) {
	rejected := errors.New("STATE_ACCOUNT_PROOF_INVALID")
	s := &nativeReadSession{remaining: 1024}
	s.reject()
	if s.stateReadError(rejected) != rejected {
		t.Fatal("source cause lost")
	}
	s.remaining = 1024
	if _, err := s.available(); err == nil || err.Error() != "NATIVE_RUNTIME_SESSION_REJECTED" {
		t.Fatal("rejected session revived", err)
	}
	if err := s.consume(1); err == nil {
		t.Fatal("rejected session spent refilled quota")
	}
	quota := &nativeReadSession{remaining: 0}
	err := quota.stateReadError(errors.New("RPC_NETWORK"))
	if err == nil || !strings.Contains(err.Error(), "NATIVE_RUNTIME_PROOF_BUDGET") {
		t.Fatal("real exhaustion hidden", err)
	}
}
