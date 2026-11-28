package service

import (
	"compress/gzip"
	"encoding/json"
	"io"
	"os"
	"path/filepath"
	"testing"

	"xone/chain-parser-go/internal/chain/evm"
)

func avalancheNativeServiceFixture(t *testing.T) *nativeServiceFixture {
	t.Helper()
	fixture := readNativeServiceFixture(t, "avalanche-101597996")
	f := fixture.service(t, "evm:43114", "023-block-receipts.json")
	for _, name := range []string{"007-sae-successor.json", "008-sae-successor.json", "009-sae-successor.json", "010-sae-successor.json", "011-sae-successor.json"} {
		var block evm.RawBlock
		if err := json.Unmarshal(fixture[name], &block); err != nil {
			t.Fatal(err)
		}
		f.headers[block.Number] = fixture[name]
	}
	previousFull := fixture["021-sae-receipt-window-block.json"]
	var previous struct {
		Number, Hash string
		Transactions []struct{ Hash string }
	}
	var header map[string]json.RawMessage
	if err := json.Unmarshal(previousFull, &previous); err != nil {
		t.Fatal(err)
	}
	if err := json.Unmarshal(previousFull, &header); err != nil {
		t.Fatal(err)
	}
	hashes := make([]string, len(previous.Transactions))
	for i, tx := range previous.Transactions {
		hashes[i] = tx.Hash
	}
	header["transactions"], _ = json.Marshal(hashes)
	compact, err := json.Marshal(header)
	if err != nil {
		t.Fatal(err)
	}
	f.headers[previous.Number] = compact
	f.full[previous.Number] = previousFull
	f.receipts[previous.Number] = fixture["022-block-receipts.json"]
	f.receipts[previous.Hash] = fixture["022-block-receipts.json"]
	return f
}

type nativeServiceResults map[string]json.RawMessage

func (r nativeServiceResults) service(t *testing.T, key, receiptFile string) *nativeServiceFixture {
	t.Helper()
	var genesis, current evm.RawBlock
	if err := json.Unmarshal(r["002-genesis.json"], &genesis); err != nil {
		t.Fatal(err)
	}
	if err := json.Unmarshal(r["006-witness-header.json"], &current); err != nil {
		t.Fatal(err)
	}
	return &nativeServiceFixture{
		chain:    nativeServiceChain(key, genesis.Hash),
		block:    current,
		headers:  map[string]json.RawMessage{current.Number: r["006-witness-header.json"]},
		full:     map[string]json.RawMessage{current.Number: r["005-finalized-header.json"]},
		receipts: map[string]json.RawMessage{current.Number: r[receiptFile], current.Hash: r[receiptFile]},
	}
}

func readNativeServiceFixture(t *testing.T, name string) nativeServiceResults {
	t.Helper()
	var results nativeServiceResults
	readCatalogReplayFixture(t, name, &results)
	return results
}

func readCatalogReplayFixture(t *testing.T, name string, dest any) {
	t.Helper()
	root := "../../test/fixtures/native-catalog-replay"
	source, err := os.Open(filepath.Join(root, name+".json.gz"))
	if err != nil {
		t.Fatal(err)
	}
	defer source.Close()
	compressed, err := gzip.NewReader(source)
	if err != nil {
		t.Fatal(err)
	}
	defer compressed.Close()
	const maxFixtureBytes = 16 << 20
	raw, err := io.ReadAll(io.LimitReader(compressed, maxFixtureBytes+1))
	if err != nil || len(raw) > maxFixtureBytes {
		t.Fatalf("invalid native catalog fixture %s: %v", name, err)
	}
	if err := json.Unmarshal(raw, dest); err != nil {
		t.Fatal(err)
	}
}
