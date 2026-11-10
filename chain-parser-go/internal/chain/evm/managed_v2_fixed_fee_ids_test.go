package evm

import (
	"context"
	"encoding/hex"
	"fmt"
	"math/big"
	"strings"
	"testing"
)

func TestManagedV2RejectsForgedFixedFeeIDContext(t *testing.T) {
	for _, scheduled := range []bool{false, true} {
		for _, kind := range []int64{0, 2} {
			t.Run(fmt.Sprintf("scheduled-%t-kind-%d", scheduled, kind), func(t *testing.T) {
				mutate := func(s *managedTestState) {
					p := &s.Pool
					code, e := hex.DecodeString(strings.TrimPrefix(s.Codes[p.ID], "0x"))
					if e != nil {
						t.Fatal(e)
					}
					offsets := managedV2Templates["ManagedPool"].Immutables["_feeIds"]
					if len(offsets) == 0 {
						t.Fatal("original compiler fee-ID immutable absent")
					}
					value := new(big.Int).SetBytes(code[offsets[0] : offsets[0]+32])
					shift := uint(0)
					if kind == 2 {
						shift = 8
					}
					mask := new(big.Int).Lsh(big.NewInt(255), shift)
					value.AndNot(value, mask)
					value.Or(value, new(big.Int).Lsh(big.NewInt(kind+1), shift))
					for _, offset := range offsets {
						copy(code[offset:offset+32], managedV2Word(value))
					}
					s.Codes[p.ID] = "0x" + hex.EncodeToString(code)
					p.MultiAsset.ExpectedCodeSHA256 = managedDigest(s.Codes[p.ID])
					data := managedV2Selector("getProviderFeeId(uint256)") + hex.EncodeToString(managedV2Word(big.NewInt(kind)))
					s.Calls[p.ID+data] = fmt.Sprintf("0x%064x", kind+1)
				}
				var a Adapter
				if scheduled {
					a, _ = scheduledNativeFixture(t, mutate)
				} else {
					a, _ = managedNativeFixture(t, 0, mutate)
				}
				_, pools, e := a.ReadState(context.Background(), "latest", false)
				if e == nil || pools != nil {
					t.Fatal("getter and forged runtime agreed on an impossible original constructor fee ID", e)
				}
				if !strings.Contains(e.Error(), "FIXED_PROTOCOL_FEE_IDS") {
					t.Fatal("missing explicit constructor fee-ID rejection", e)
				}
			})
		}
	}
}
