package evm

import (
	"bytes"
	"compress/gzip"
	"context"
	"crypto/sha256"
	"encoding/json"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"testing"

	"google.golang.org/protobuf/proto"
	"xone/chain-parser-go/internal/domain"
)

func TestManagedV2GivenOutParserWire(t *testing.T) {
	bankFile, err := os.Open("../../../test/contracts/managed-v2-vault-exact-out.json.gz")
	if err != nil {
		t.Fatal(err)
	}
	defer bankFile.Close()
	gz, err := gzip.NewReader(bankFile)
	if err != nil {
		t.Fatal(err)
	}
	defer gz.Close()
	raw, err := io.ReadAll(io.LimitReader(gz, 2<<20))
	if err != nil {
		t.Fatal(err)
	}
	if digest := fmt.Sprintf("%x", sha256.Sum256(raw)); digest != "3c5f14fa613ebfe360f9fae8c854fcf7ad92aeb90754cdcaa4167df5f9c1a8f6" {
		t.Fatalf("recorded GIVEN_OUT bank digest changed: %s", digest)
	}
	var bank managedBank
	if err := json.Unmarshal(raw, &bank); err != nil {
		t.Fatal(err)
	}
	if len(bank.Scenarios) != 1 || len(bank.Cases) != 2 || bank.Cases[0].Execution.After != bank.Cases[1].Execution.Before {
		t.Fatal("expected one pool and two continuous Vault swaps")
	}
	fixtureDir := "../../../../dex-router/crates/quote-api/tests/fixtures"
	for index, bankRef := range []managedBankRef{
		bank.Scenarios[0].Source, bank.Cases[0].Execution.After, bank.Cases[1].Execution.After,
	} {
		adapter, state := managedNativeFixtureFromBank(t, bank, 0, bankRef, nil)
		header, pools, err := adapter.ReadState(context.Background(), state.Header["number"].(string), false)
		if err != nil || len(pools) != 1 {
			t.Fatalf("step %d could not parse captured getters: %v", index, err)
		}
		managedV2CompareNativeVaultWallets(t, bank, bankRef, pools[0])
		snapshot, err := domain.NewCurrentStateBaseline(header, pools, "chain-parser-go", 1, header.Number)
		if err != nil {
			t.Fatal(err)
		}
		snapshot.Source = "LOCAL_MANAGED_V2_VAULT_REPLAY"
		snapshot.Reference.JournalSeq = uint64(index) + 1
		snapshot.Reference.WriterEpoch = 1
		snapshot.Reference.OnlineGeneration = 1
		if err = snapshot.Validate("chain-parser-go", 1); err != nil {
			t.Fatal(err)
		}
		wire, err := domain.SnapshotToProto(snapshot)
		if err != nil {
			t.Fatal(err)
		}
		encoded, err := (proto.MarshalOptions{Deterministic: true}).Marshal(wire)
		if err != nil {
			t.Fatal(err)
		}
		name := fmt.Sprintf("managed_v2_vault_snapshot_%d.pb", index)
		if output := os.Getenv("XONE_MANAGED_V2_WIRE_OUTPUT"); output != "" {
			if err := os.WriteFile(filepath.Join(output, name), encoded, 0o600); err != nil {
				t.Fatal(err)
			}
			continue
		}
		expected, err := os.ReadFile(filepath.Join(fixtureDir, name))
		if err != nil {
			t.Fatal(err)
		}
		if !bytes.Equal(encoded, expected) {
			t.Fatalf("step %d Go snapshot differs from the Rust API fixture", index)
		}
	}
}
