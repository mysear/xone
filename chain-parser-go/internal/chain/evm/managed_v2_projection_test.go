package evm

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"os"
	"reflect"
	"sort"
	"testing"
	"xone/chain-parser-go/internal/domain"
)

type managedV2OriginalCase struct {
	Name, Scenario, Operation string
	Source                    managedBankRef `json:"source_h"`
	Execution                 struct{ Before, After, Transaction managedBankRef }
}

// Compare every public JSON field, preserving exact numbers, known zeros and
// array order. Dependencies alone are a role-keyed set in the public model.
func managedV2PublicJSON(t *testing.T, raw []byte) any {
	t.Helper()
	canonical, e := managedCanonical(raw)
	if e != nil {
		t.Fatal(e)
	}
	var value map[string]any
	// Decode through the same lossless number representation for both sides.
	dec := json.NewDecoder(bytes.NewReader(canonical))
	dec.UseNumber()
	if e = dec.Decode(&value); e != nil {
		t.Fatal(e)
	}
	multi, ok := value["multi_asset"].(map[string]any)
	if !ok {
		t.Fatal("public multi_asset missing")
	}
	managed, ok := multi["managed_v2"].(map[string]any)
	if !ok {
		t.Fatal("public managed_v2 missing")
	}
	deps, ok := managed["dependencies"].([]any)
	if !ok {
		t.Fatal("public dependencies missing")
	}
	sort.Slice(deps, func(i, j int) bool {
		return deps[i].(map[string]any)["role"].(string) < deps[j].(map[string]any)["role"].(string)
	})
	return value
}

func managedV2JSONDifference(path string, got, want any) string {
	if reflect.DeepEqual(got, want) {
		return ""
	}
	switch expected := want.(type) {
	case map[string]any:
		actual, ok := got.(map[string]any)
		if !ok {
			break
		}
		keys := make([]string, 0, len(expected))
		for key := range expected {
			keys = append(keys, key)
		}
		sort.Strings(keys)
		for _, key := range keys {
			value, ok := actual[key]
			if !ok {
				return path + "/" + key + " missing"
			}
			if d := managedV2JSONDifference(path+"/"+key, value, expected[key]); d != "" {
				return d
			}
		}
		for key := range actual {
			if _, ok := expected[key]; !ok {
				return path + "/" + key + " unexpected"
			}
		}
	case []any:
		actual, ok := got.([]any)
		if !ok {
			break
		}
		if len(actual) != len(expected) {
			return fmt.Sprintf("%s length got %d want %d", path, len(actual), len(expected))
		}
		for i := range expected {
			if d := managedV2JSONDifference(fmt.Sprintf("%s/%d", path, i), actual[i], expected[i]); d != "" {
				return d
			}
		}
	}
	return fmt.Sprintf("%s got %v want %v", path, got, want)
}

func managedV2ComparePublicPool(t *testing.T, got domain.Pool, want json.RawMessage) {
	t.Helper()
	if e := got.Validate(); e != nil {
		t.Fatal(e)
	}
	raw, e := json.Marshal(got)
	if e != nil {
		t.Fatal(e)
	}
	if d := managedV2JSONDifference("", managedV2PublicJSON(t, raw), managedV2PublicJSON(t, want)); d != "" {
		t.Fatal(d)
	}
}

// Retain actual wallet observations in all 81 complete Pool comparisons,
// independently of their registered cash/managed balances and header origin.
func managedV2CompareNativeVaultWallets(t *testing.T, bank managedBank, ref managedBankRef, pool domain.Pool) {
	t.Helper()
	var snapshot struct {
		State struct {
			Vault      string
			Underlying []managedBankRef
			VaultBPT   struct {
				ActualBalance string `json:"actual_balance"`
			} `json:"vault_bpt"`
		}
	}
	bank.decode(t, ref, "snapshot", &snapshot)
	m := pool.MultiAsset.ManagedV2
	if m.VaultBPTActualBalance.String() != snapshot.State.VaultBPT.ActualBalance {
		t.Fatal("complete Pool lost original BPT Vault wallet")
	}
	for i, tokenRef := range snapshot.State.Underlying {
		var token struct {
			ActualBalances map[string]string `json:"actual_balances"`
		}
		bank.decode(t, tokenRef, "token_state", &token)
		if m.TokenInfo[i].VaultActualBalance.String() != token.ActualBalances[snapshot.State.Vault] {
			t.Fatalf("complete Pool lost original token %d Vault wallet", i)
		}
	}
}

func TestManagedV2NativePublicProjectionMatrix(t *testing.T) {
	bank := managedLoadBank(t)
	raw, e := os.ReadFile("../../../test/contracts/managed-v2-aum-pools.json")
	if e != nil {
		t.Fatal(e)
	}
	var projection struct {
		SchemaVersion      int `json:"schema_version"`
		Profile, Authority string
		AdapterVersion     string `json:"adapter_version"`
		Original           struct {
			Filename, SHA256 string
			Bytes            int
		} `json:"original_bank"`
		Cases []struct {
			Name, Scenario, Operation string
			Source                    json.RawMessage `json:"source_pool"`
			Observed                  json.RawMessage `json:"observed_pool_after"`
			Derived                   json.RawMessage `json:"expected_pool_after"`
			Authority                 struct {
				Source   string `json:"source_pool"`
				Observed string `json:"observed_pool_after"`
				Derived  string `json:"expected_pool_after"`
			}
			Refs struct {
				Bank                       string `json:"bank_sha256"`
				Before, After, Transaction managedBankRef
				Scenario                   managedBankRef `json:"scenario_source_h"`
			} `json:"source_refs"`
		}
	}
	if e = json.Unmarshal(raw, &projection); e != nil {
		t.Fatal(e)
	}
	const bankSHA = "1ee7ec5dd981994af6dd42eb75717cf83a5b209857883fa2923885ce7181fb66"
	if projection.SchemaVersion != 1 || projection.Profile != domain.ManagedV2Protocol || projection.AdapterVersion != domain.ManagedV2AdapterVersion || projection.Authority != "derived-public-pool-projection-of-frozen-original-bank" || projection.Original.Filename != "managed-v2-aum.json" || projection.Original.Bytes != 7905624 || projection.Original.SHA256 != bankSHA {
		t.Fatal("projection original-bank identity changed")
	}
	if len(bank.Cases) != 27 || len(projection.Cases) != len(bank.Cases) {
		t.Fatal("original/public case inventory differs")
	}
	originals := map[string]managedV2OriginalCase{}
	for _, c := range bank.Cases {
		if _, ok := originals[c.Name]; ok {
			t.Fatal("duplicate original case")
		}
		originals[c.Name] = c
	}
	seen := map[string]bool{}
	for _, c := range projection.Cases {
		original, ok := originals[c.Name]
		if !ok || seen[c.Name] {
			t.Fatal("unknown/duplicate public case", c.Name)
		}
		seen[c.Name] = true
		t.Run(c.Name, func(t *testing.T) {
			if c.Scenario != original.Scenario || c.Operation != original.Operation || c.Refs.Bank != bankSHA || c.Refs.Before != original.Execution.Before || c.Refs.After != original.Execution.After || c.Refs.Transaction != original.Execution.Transaction || c.Refs.Scenario != original.Source {
				t.Fatal("projection reference is not the original execution")
			}
			if c.Authority.Source != "original-local-node-observation" || c.Authority.Observed != "original-local-node-observation" || c.Authority.Derived != "derived-after-state-at-source-header" {
				t.Fatal("source/derived authority lost")
			}
			index := -1
			for i, s := range bank.Scenarios {
				if s.Name == original.Scenario {
					index = i
					break
				}
			}
			if index < 0 {
				t.Fatal("original scenario missing")
			}
			parse := func(ref managedBankRef) domain.Pool {
				a, state := managedNativeFixtureAt(t, index, ref, nil)
				header, pools, e := a.ReadState(context.Background(), state.Header["number"].(string), false)
				if e != nil || len(pools) != 1 {
					t.Fatalf("native getter parse: %v", e)
				}
				m := pools[0].MultiAsset.ManagedV2
				if m.BlockHash != header.Hash || m.BlockNumber != header.Number || m.Timestamp*1000 != header.TimestampMS {
					t.Fatal("native parser source header differs")
				}
				return pools[0]
			}
			before, after := parse(original.Execution.Before), parse(original.Execution.After)
			managedV2CompareNativeVaultWallets(t, bank, original.Execution.Before, before)
			managedV2CompareNativeVaultWallets(t, bank, original.Execution.After, after)
			managedV2ComparePublicPool(t, before, c.Source)
			managedV2ComparePublicPool(t, after, c.Observed)
			// A derived execution result retains its before-header identity and
			// carries the original after execution clock separately.
			derived := after
			multi := *after.MultiAsset
			managed := *multi.ManagedV2
			derived.MultiAsset = &multi
			multi.ManagedV2 = &managed
			source := before.MultiAsset.ManagedV2
			managed.ExecutionContext = &domain.ManagedV2ExecutionContext{BlockNumber: managed.BlockNumber, Timestamp: managed.Timestamp}
			managed.BlockHash, managed.BlockNumber, managed.Timestamp = source.BlockHash, source.BlockNumber, source.Timestamp
			managedV2CompareNativeVaultWallets(t, bank, original.Execution.After, derived)
			managedV2ComparePublicPool(t, derived, c.Derived)
		})
	}
}

// This separate original branch proves Vault and Pool pause semantics. Its
// runtime/getter bytes are native observations; no synthetic ABI fallback exists.
func TestManagedV2OriginalPausedVaultBranchFullState(t *testing.T) {
	managedV2CompareOriginalControlBranch(t, "../../../test/contracts/managed-v2-aum-paused-vault.json", 3)
}
func TestManagedV2OriginalRecoveryBranchFullState(t *testing.T) {
	managedV2CompareOriginalControlBranch(t, "../../../test/contracts/managed-v2-aum-recovery.json", 7)
}
func managedV2CompareOriginalControlBranch(t *testing.T, filename string, count int) {
	t.Helper()
	raw, err := os.ReadFile(filename)
	if err != nil {
		t.Fatal(err)
	}
	var branch struct {
		Adapter string `json:"public_adapter_version"`
		Bank    string `json:"original_bank_sha256"`
		Cases   []struct {
			Name                  string
			Source                json.RawMessage     `json:"source_pool"`
			Observed              json.RawMessage     `json:"observed_pool_after"`
			ExecutionSource       json.RawMessage     `json:"execution_source_pool"`
			BeforeHeader          map[string]any      `json:"before_header"`
			SourceHeader          map[string]any      `json:"source_header"`
			AfterHeader           map[string]any      `json:"execution_header"`
			ExecutionSourceHeader map[string]any      `json:"execution_source_header"`
			Before                []managedBankGetter `json:"getters_before"`
			After                 []managedBankGetter `json:"getters_after"`
			ExecutionGetters      []managedBankGetter `json:"execution_source_getters"`
		} `json:"cases"`
	}
	if err = json.Unmarshal(raw, &branch); err != nil {
		t.Fatal(err)
	}
	if branch.Adapter != domain.ManagedV2AdapterVersion || branch.Bank != "1ee7ec5dd981994af6dd42eb75717cf83a5b209857883fa2923885ce7181fb66" || len(branch.Cases) != count {
		t.Fatal("unqualified original pause branch")
	}
	for _, c := range branch.Cases {
		rows := []struct {
			label    string
			expected json.RawMessage
			header   map[string]any
			getters  []managedBankGetter
		}{
			{"source", c.Source, c.SourceHeader, c.Before}, {"observed", c.Observed, c.AfterHeader, c.After},
		}
		if len(c.ExecutionSource) > 0 {
			rows = append(rows, struct {
				label    string
				expected json.RawMessage
				header   map[string]any
				getters  []managedBankGetter
			}{"execution-source", c.ExecutionSource, c.ExecutionSourceHeader, c.ExecutionGetters})
		}
		for _, row := range rows {
			t.Run(c.Name+"/"+row.label, func(t *testing.T) {
				var expected domain.Pool
				if err := json.Unmarshal(row.expected, &expected); err != nil {
					t.Fatal(err)
				}
				a, _ := managedNativeFixture(t, 0, func(s *managedTestState) {
					for key := range s.Header {
						delete(s.Header, key)
					}
					for key, value := range row.header {
						s.Header[key] = value
					}
					for key := range s.Calls {
						delete(s.Calls, key)
					}
					for _, g := range row.getters {
						s.Calls[g.To+g.Data] = g.Result
					}
				})
				_, pools, err := a.ReadState(context.Background(), "latest", false)
				if err != nil {
					t.Fatal(err)
				}
				if len(pools) != 1 {
					t.Fatal("original paused branch scope lost")
				}
				managedV2ComparePublicPool(t, pools[0], row.expected)
			})
		}
	}
}
