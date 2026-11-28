package service

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"testing"
	"time"

	"xone/chain-parser-go/internal/chain/evm"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/ingest"
	"xone/chain-parser-go/internal/rpc"
)

type nativeServiceFixture struct {
	chain    config.Chain
	block    evm.RawBlock
	headers  map[string]json.RawMessage
	full     map[string]json.RawMessage
	receipts map[string]json.RawMessage
	client   *rpc.Client
}

func (f *nativeServiceFixture) height(t *testing.T) uint64 {
	t.Helper()
	height, err := strconv.ParseUint(f.block.Number, 0, 64)
	if err != nil {
		t.Fatal(err)
	}
	return height
}

func nativeServicePublicFixture(t *testing.T, name string) *nativeServiceFixture {
	t.Helper()
	switch name {
	case "avalanche-current":
		return serveNativeServiceFixture(t, avalancheNativeServiceFixture(t))
	case "arbitrum-current":
		fixture := readNativeServiceFixture(t, "arbitrum-528150486")
		return serveNativeServiceFixture(t, fixture.service(t, "evm:42161", "017-block-receipts.json"))
	case "polygon-synthetic":
	default:
		t.Fatalf("unknown native catalog fixture %s", name)
	}
	root := filepath.Join("../../test/fixtures/source/testdata/chain-receipt-vectors", name)
	read := func(file string, out any) {
		t.Helper()
		b, e := os.ReadFile(filepath.Join(root, file))
		if e != nil {
			t.Fatal(e)
		}
		if e = json.Unmarshal(b, out); e != nil {
			t.Fatal(e)
		}
	}
	var vector struct {
		ChainKey string `json:"chain_key"`
		Block    evm.RawBlock
		Receipts []evm.Receipt
	}
	read("vector.json", &vector)
	var full struct {
		Response struct{ Result json.RawMessage }
	}
	read("full-block.json", &full)
	var header map[string]json.RawMessage
	if e := json.Unmarshal(full.Response.Result, &header); e != nil {
		t.Fatal(e)
	}
	header["transactions"], _ = json.Marshal(vector.Block.Transactions)
	rawHeader, _ := json.Marshal(header)
	receipts, err := json.Marshal(vector.Receipts)
	if err != nil {
		t.Fatal(err)
	}
	f := &nativeServiceFixture{
		block:    vector.Block,
		full:     map[string]json.RawMessage{vector.Block.Number: full.Response.Result},
		headers:  map[string]json.RawMessage{vector.Block.Number: rawHeader},
		receipts: map[string]json.RawMessage{vector.Block.Number: receipts, vector.Block.Hash: receipts},
	}
	f.chain = nativeServiceChain(vector.ChainKey, "0xa9c28ce2141b56c474f1dc504bee9b01eb1bd7d1a507580d5519d4437a97de1b")
	return serveNativeServiceFixture(t, f)
}

func nativeServiceChain(key, genesis string) config.Chain {
	return config.Chain{Key: key, Genesis: genesis, BlockIntegrity: evm.ChainNativeCommitmentsV2, ReceiptsMode: "bulk-auto", NativeProof: &config.NativeProofConfig{MaxBytes: 16 << 20, MaxHeaders: 64, MaxWindowBlocks: 16, MaxTransactions: 4096, MaxLookahead: 32}}
}

func serveNativeServiceFixture(t *testing.T, f *nativeServiceFixture) *nativeServiceFixture {
	t.Helper()
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		var q struct {
			ID     any
			Method string
			Params []json.RawMessage
		}
		if e := json.NewDecoder(r.Body).Decode(&q); e != nil {
			t.Error(e)
			return
		}
		var result any
		switch q.Method {
		case "eth_chainId":
			id, _ := strconv.ParseUint(strings.TrimPrefix(f.chain.Key, "evm:"), 10, 64)
			result = fmt.Sprintf("0x%x", id)
		case "eth_getBlockByNumber":
			var tag string
			var full bool
			json.Unmarshal(q.Params[0], &tag)
			json.Unmarshal(q.Params[1], &full)
			if tag == "0x0" {
				result = map[string]string{"hash": f.chain.Genesis}
			} else if full {
				result = f.full[tag]
			} else {
				result = f.headers[tag]
			}
		case "eth_getBlockReceipts":
			var selector string
			json.Unmarshal(q.Params[0], &selector)
			result = f.receipts[selector]
		default:
			t.Errorf("unexpected public native method %s", q.Method)
		}
		json.NewEncoder(w).Encode(map[string]any{"jsonrpc": "2.0", "id": q.ID, "result": result})
	}))
	t.Cleanup(server.Close)
	f.client = rpc.New([]string{server.URL}, 10*time.Second, 16<<20)
	return f
}
func nativeServiceCatalog(t *testing.T, f *nativeServiceFixture) *CatalogRuntime {
	t.Helper()
	archive, e := ingest.OpenArchive(filepath.Join(t.TempDir(), "raw"), ingest.ArchiveLimits{MaxBytes: 64 << 20, MaxRecords: 64, MaxRecordBytes: 16 << 20})
	if e != nil {
		t.Fatal(e)
	}
	t.Cleanup(func() { archive.Close() })
	return &CatalogRuntime{
		options: CatalogRuntimeOptions{
			Chain: f.chain, Client: f.client, Archive: archive,
			Config:     config.DynamicCatalogConfig{TimeoutMS: 10000, MaxBytes: 16 << 20, MaxReceipts: 4096, MaxLogs: 10000},
			CheckFence: func(ctx context.Context, _ CatalogFence) error { return ctx.Err() },
		},
		client: f.client,
		active: true,
		anchor: ingest.CatalogAnchor{Chain: f.chain.Key, Height: f.height(t), Hash: f.block.Hash},
	}
}
func TestNativeFactoryActualPublicProofNormalAndReplay(t *testing.T) {
	for _, name := range []string{"avalanche-current", "arbitrum-current", "polygon-synthetic"} {
		t.Run(name, func(t *testing.T) {
			f := nativeServicePublicFixture(t, name)
			r := nativeServiceCatalog(t, f)
			ctx := context.Background()
			height := f.height(t)
			if candidates, e := r.DiscoverHeight(ctx, ingest.CatalogOnline, height); e != nil || len(candidates) != 0 {
				t.Fatalf("normal discovery: %v %v", candidates, e)
			}
			a := evm.Adapter{Client: f.client, Chain: f.chain}
			b, e := a.Block(ctx, f.block.Number)
			if e != nil {
				t.Fatal(e)
			}
			b, receipts, e := a.ReadQualifiedReceipts(ctx, b, false)
			if e != nil {
				t.Fatal(e)
			}
			raw, e := json.Marshal(factoryHTTPProof{Header: b, Receipts: receipts})
			if e != nil {
				t.Fatal(e)
			}
			var proof factoryHTTPProof
			if e = json.Unmarshal(raw, &proof); e != nil {
				t.Fatal(e)
			}
			if proof.NativeEvidence == nil || proof.NativeSemanticsID == "" {
				t.Fatal("normal callback archive lost native successor/system evidence")
			}
			projected, e := r.projectHTTPProofContext(ctx, proof)
			if e != nil {
				t.Fatal(e)
			}
			projected.Raw, e = r.options.Archive.Put(ingest.Owner{Chain: f.chain.Key, Height: height, Hash: f.block.Hash, ProviderID: "public-native-test"}, raw)
			if e != nil {
				t.Fatal(e)
			}
			if e = r.verifyHTTPBlock(ctx, f.client, projected); e != nil {
				t.Fatal("archive replay", e)
			}
			if name == "arbitrum-current" {
				if len(proof.Receipts) == 0 || proof.Receipts[0].Type != "0x6a" {
					t.Fatal("captured ArbOS system receipt missing")
				}
				altered := proof
				altered.Receipts = append([]evm.Receipt(nil), proof.Receipts...)
				altered.Receipts[0].Type = "0x2"
				if e = r.verifyFactoryHTTPProof(ctx, altered); e == nil {
					t.Fatal("rewritten ArbOS system receipt passed replay")
				}
			}
			proof.NativeEvidence = nil
			if e = r.verifyFactoryHTTPProof(ctx, proof); e == nil {
				t.Fatal("missing native proof passed replay")
			}
		})
	}
}
func TestNativeFactoryActualPublicRejectsWrongSuccessorAndAffectedSystem(t *testing.T) {
	for _, name := range []string{"avalanche-current", "polygon-synthetic"} {
		t.Run(name, func(t *testing.T) {
			f := nativeServicePublicFixture(t, name)
			r := nativeServiceCatalog(t, f)
			ctx := context.Background()
			a := evm.Adapter{Client: f.client, Chain: f.chain}
			b, e := a.Block(ctx, f.block.Number)
			if e != nil {
				t.Fatal(e)
			}
			b, receipts, e := a.ReadQualifiedReceipts(ctx, b, false)
			if e != nil {
				t.Fatal(e)
			}
			raw, _ := json.Marshal(factoryHTTPProof{Header: b, Receipts: receipts})
			var proof factoryHTTPProof
			json.Unmarshal(raw, &proof)
			if name == "avalanche-current" {
				proof.NativeEvidence.SAE.Headers = proof.NativeEvidence.SAE.Headers[:1]
			} else {
				r.options.Config.Sources = []config.FactoryDeployment{{Address: "0x1fdeaf938267ca43388ed1fdb879eaf91e920c7a", Since: 1, Version: ingest.UniswapV2Factory}}
			}
			if e = r.verifyFactoryHTTPProof(ctx, proof); e == nil {
				t.Fatal("unproven native factory scope accepted")
			}
		})
	}
}
