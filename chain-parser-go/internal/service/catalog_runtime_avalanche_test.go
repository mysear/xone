package service

import (
	"archive/tar"
	"compress/gzip"
	"encoding/json"
	"errors"
	"io"
	"os"
	"path"
	"testing"

	"xone/chain-parser-go/internal/chain/evm"
)

func avalancheNativeServiceFixture(t *testing.T) *nativeServiceFixture {
	t.Helper()
	const directory = "integration/test/source/deployment-catalog/evm-continuation-v2/avalanche-getter-anchor.43nmhr50"
	fixture := readAvalancheServiceArchive(t, directory, []string{
		"002-genesis.json", "005-finalized-header.json", "006-witness-header.json",
		"007-sae-successor.json", "008-sae-successor.json", "009-sae-successor.json",
		"010-sae-successor.json", "011-sae-successor.json", "021-sae-receipt-window-block.json",
		"022-block-receipts.json", "023-block-receipts.json",
	})
	var genesis, current evm.RawBlock
	if err := json.Unmarshal(fixture["002-genesis.json"], &genesis); err != nil {
		t.Fatal(err)
	}
	if err := json.Unmarshal(fixture["006-witness-header.json"], &current); err != nil {
		t.Fatal(err)
	}
	f := &nativeServiceFixture{
		chain:    nativeServiceChain("evm:43114", genesis.Hash),
		block:    current,
		headers:  map[string]json.RawMessage{current.Number: fixture["006-witness-header.json"]},
		full:     map[string]json.RawMessage{current.Number: fixture["005-finalized-header.json"]},
		receipts: map[string]json.RawMessage{current.Number: fixture["023-block-receipts.json"], current.Hash: fixture["023-block-receipts.json"]},
	}
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

func readAvalancheServiceArchive(t *testing.T, directory string, files []string) map[string]json.RawMessage {
	t.Helper()
	const archive = "../../../integration/test/source/deployment-catalog/evidence/avalanche-uniswap-v3-101597996.tar.gz"
	source, err := os.Open(archive)
	if err != nil {
		t.Fatal(err)
	}
	defer source.Close()
	compressed, err := gzip.NewReader(source)
	if err != nil {
		t.Fatal(err)
	}
	defer compressed.Close()
	pending := make(map[string]bool, len(files))
	for _, file := range files {
		pending[path.Join(directory, file)] = true
	}
	results := make(map[string]json.RawMessage, len(files))
	reader := tar.NewReader(compressed)
	for len(pending) > 0 {
		entry, err := reader.Next()
		if errors.Is(err, io.EOF) {
			t.Fatalf("missing Avalanche archive entries: %v", pending)
		}
		if err != nil {
			t.Fatal(err)
		}
		if !pending[entry.Name] {
			continue
		}
		var record struct {
			Response struct{ Result json.RawMessage }
		}
		if entry.Typeflag != tar.TypeReg || entry.Size > 16<<20 {
			t.Fatalf("invalid Avalanche archive entry %s", entry.Name)
		}
		if err := json.NewDecoder(reader).Decode(&record); err != nil {
			t.Fatal(err)
		}
		if len(record.Response.Result) == 0 || string(record.Response.Result) == "null" {
			t.Fatalf("missing RPC result in %s", entry.Name)
		}
		results[path.Base(entry.Name)] = record.Response.Result
		delete(pending, entry.Name)
	}
	return results
}
