package evm

import (
	"context"
	"encoding/json"
	"fmt"
	"os"
	"testing"

	"xone/chain-parser-go/internal/domain"
)

type completeV2GetterBank struct {
	BlockHash   string      `json:"block_hash"`
	BlockNumber uint64      `json:"block_number"`
	Pool        domain.Pool `json:"pool"`
	Calls       []struct {
		To          string `json:"to"`
		Data        string `json:"data"`
		Result      string `json:"result"`
		BlockHash   string `json:"block_hash"`
		BlockNumber uint64 `json:"block_number"`
	} `json:"calls"`
	Lock struct {
		Address     string `json:"address"`
		Slot        string `json:"slot"`
		Value       string `json:"value"`
		BlockHash   string `json:"block_hash"`
		BlockNumber uint64 `json:"block_number"`
	} `json:"lock_observation"`
}

func TestV2CompleteAllNativeBanksIncludingReverts(t *testing.T) {
	raw, err := os.ReadFile("../../../test/contracts/v2-complete.json")
	if err != nil {
		t.Fatal(err)
	}
	var bank struct {
		Profile        string            `json:"profile"`
		AdapterVersion string            `json:"adapter_version"`
		Cases          []json.RawMessage `json:"cases"`
	}
	if err = json.Unmarshal(raw, &bank); err != nil {
		t.Fatal(err)
	}
	if bank.Profile != completeV2Protocol || bank.AdapterVersion != completeV2Version || len(bank.Cases) < 24 {
		t.Fatal("incomplete native bank inventory")
	}
	successes, reverts, projections := 0, 0, 0
	for _, row := range bank.Cases {
		var c completeV2Case
		var native struct {
			ExecutionEnvironment struct {
				Number     uint64 `json:"block_number"`
				Hash       string `json:"block_hash"`
				ParentHash string `json:"parent_hash"`
				Timestamp  uint64 `json:"timestamp"`
			} `json:"execution_environment"`
			Expected struct {
				Status       string              `json:"status"`
				PoolAfter    domain.Pool         `json:"pool_after"`
				PostAccounts []completeV2Account `json:"post_accounts"`
			} `json:"expected"`
			Evidence struct {
				Pre  []completeV2GetterBank `json:"pre_getters"`
				Post []completeV2GetterBank `json:"post_getters"`
			} `json:"evidence"`
		}
		if err = json.Unmarshal(row, &c); err != nil {
			t.Fatal(err)
		}
		if err = json.Unmarshal(row, &native); err != nil {
			t.Fatal(err)
		}
		switch native.Expected.Status {
		case "success":
			successes++
		case "revert":
			reverts++
		default:
			t.Fatalf("unknown execution status: %s", native.Expected.Status)
		}
		if len(native.Evidence.Pre) == 0 || len(native.Evidence.Pre) != len(native.Evidence.Post) {
			t.Fatalf("missing banks: %s", c.Name)
		}
		for _, phase := range []string{"before", "after"} {
			getters := native.Evidence.Pre
			if phase == "after" {
				getters = native.Evidence.Post
			}
			for index, getter := range getters {
				t.Run(fmt.Sprintf("%s/%s/pair-%d", c.Name, phase, index), func(t *testing.T) {
					// A failed swap still leaves every physical Pair readable. The
					// parser does not execute a future swap or assume raw==reserve.
					observed := c
					observed.Pool = getter.Pool
					if phase == "after" {
						observed.Accounts = native.Expected.PostAccounts
						observed.StateAnchor.Number = native.ExecutionEnvironment.Number
						observed.StateAnchor.Hash = native.ExecutionEnvironment.Hash
						observed.StateAnchor.ParentHash = native.ExecutionEnvironment.ParentHash
						observed.StateAnchor.Timestamp = native.ExecutionEnvironment.Timestamp
					}
					if getter.Pool.PoolID == c.Pool.PoolID {
						want := c.Pool
						if phase == "after" {
							want = native.Expected.PoolAfter
						}
						gotJSON, _ := json.Marshal(getter.Pool)
						wantJSON, _ := json.Marshal(want)
						if string(gotJSON) != string(wantJSON) {
							t.Fatal("case pool and native getter pool disagree")
						}
					}
					expected := getter.Pool
					// Recorded post-swap projections retain the input's provenance;
					// a fresh parser observation carries the actual post header.
					identity := *expected.V2
					swap := *identity.Swap
					swap.BankBlockNumber = observed.StateAnchor.Number
					swap.BankBlockHash = observed.StateAnchor.Hash
					swap.BankUnixTimestamp = observed.StateAnchor.Timestamp
					identity.Swap = &swap
					expected.V2 = &identity
					observed.Getters = &getter
					if getter.BlockHash != observed.StateAnchor.Hash || getter.BlockNumber != observed.StateAnchor.Number || len(getter.Calls) != 10 {
						t.Fatal("getter bank identity/inventory")
					}
					a, _ := newCompleteV2RPC(t, observed, nil)
					_, pools, err := a.ReadState(context.Background(), "latest", false)
					if err != nil || len(pools) != 1 {
						t.Fatalf("native %s state rejected for execution %s: %v", phase, native.Expected.Status, err)
					}
					assertCompleteV2Projection(t, observed, pools[0])
					got, _ := json.Marshal(pools[0])
					want, _ := json.Marshal(expected)
					if string(got) != string(want) {
						t.Fatalf("native pool projection differs\nwant %s\ngot  %s", want, got)
					}
				})
				projections++
			}
		}
	}
	if successes < 20 || reverts < 4 || projections < 2*len(bank.Cases) {
		t.Fatalf("native inventory: successes=%d reverts=%d projections=%d", successes, reverts, projections)
	}
	t.Logf("verified %d native cases: %d success, %d revert, %d complete before/after bank projections", len(bank.Cases), successes, reverts, projections)
}
