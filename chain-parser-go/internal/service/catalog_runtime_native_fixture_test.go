package service

import (
	"encoding/json"
	"path/filepath"
	"testing"

	"xone/chain-parser-go/internal/chain/evm"
	"xone/chain-parser-go/internal/testfixture"
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
	return nativeServiceFromResults(t, key, r["002-genesis.json"], r["006-witness-header.json"], r["005-finalized-header.json"], r[receiptFile])
}

func nativeServiceFromResults(t *testing.T, key string, genesisRaw, header, body, receipts json.RawMessage) *nativeServiceFixture {
	t.Helper()
	var genesis, current evm.RawBlock
	if err := json.Unmarshal(genesisRaw, &genesis); err != nil {
		t.Fatal(err)
	}
	if err := json.Unmarshal(header, &current); err != nil {
		t.Fatal(err)
	}
	return &nativeServiceFixture{
		chain:    nativeServiceChain(key, genesis.Hash),
		block:    current,
		headers:  map[string]json.RawMessage{current.Number: header},
		full:     map[string]json.RawMessage{current.Number: body},
		receipts: map[string]json.RawMessage{current.Number: receipts, current.Hash: receipts},
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
	testfixture.ReadGzipJSON(t, filepath.Join(root, name+".json.gz"), dest)
}
