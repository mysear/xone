package evm

import (
	"bytes"
	"compress/gzip"
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"reflect"
	"regexp"
	"strings"
	"sync/atomic"
	"testing"
	"time"

	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/domain"
	"xone/chain-parser-go/internal/rpc"
)

const publicV2FixtureRoot = "../../../test/fixtures/current-v2-public"

type publicV2Manifest struct {
	Schema       int               `json:"schema_version"`
	Block        domain.BlockRef   `json:"block"`
	Pool         config.PoolConfig `json:"pool"`
	Expected     domain.Pool       `json:"expected_pool"`
	Endpoints    []string          `json:"endpoints"`
	Count        int               `json:"record_count"`
	RawBytes     int               `json:"raw_bytes"`
	File         string            `json:"bundle_file"`
	BundleSHA    string            `json:"bundle_sha256"`
	BundleBytes  int               `json:"bundle_bytes"`
	DecodedSHA   string            `json:"decoded_sha256"`
	DecodedBytes int               `json:"decoded_bytes"`
	Records      []struct {
		Name     string `json:"file"`
		SHA      string `json:"sha256"`
		Bytes    int    `json:"bytes"`
		Provider string `json:"provider_id"`
	} `json:"records"`
}
type publicV2Read struct {
	Method string
	Params []json.RawMessage
	Body   json.RawMessage
}

func publicV2Digest(raw []byte) string {
	hash := sha256.Sum256(raw)
	return hex.EncodeToString(hash[:])
}
func publicV2RequestKey(method string, params []json.RawMessage) string {
	var value any
	raw, _ := json.Marshal(params)
	_ = json.Unmarshal(raw, &value)
	raw, _ = json.Marshal(value)
	return method + ":" + string(raw)
}
func publicV2Fixture(t *testing.T) (publicV2Manifest, map[string][]publicV2Read) {
	t.Helper()
	var m publicV2Manifest
	raw, e := os.ReadFile(filepath.Join(publicV2FixtureRoot, "manifest.json"))
	if e != nil {
		t.Fatal(e)
	}
	if e = json.Unmarshal(raw, &m); e != nil {
		t.Fatal(e)
	}
	if m.Schema != 1 || m.Count != 32 || len(m.Records) != m.Count || m.File != "responses.json.gz" || m.BundleBytes > 32<<20 || m.DecodedBytes > 32<<20 {
		t.Fatal("fixed fixture shape")
	}
	compressed, e := os.ReadFile(filepath.Join(publicV2FixtureRoot, m.File))
	if e != nil {
		t.Fatal(e)
	}
	if len(compressed) != m.BundleBytes || publicV2Digest(compressed) != m.BundleSHA {
		t.Fatal("fixture compressed bytes changed")
	}
	z, e := gzip.NewReader(bytes.NewReader(compressed))
	if e != nil {
		t.Fatal(e)
	}
	defer z.Close()
	decoded, e := io.ReadAll(io.LimitReader(z, (32<<20)+1))
	if e != nil || len(decoded) != m.DecodedBytes || publicV2Digest(decoded) != m.DecodedSHA {
		t.Fatal("fixture decompression budget/hash", e)
	}
	var bundle struct {
		Schema  int `json:"schema_version"`
		Records []struct {
			Name string `json:"name"`
			Data []byte `json:"data"`
		}
	}
	if e = json.Unmarshal(decoded, &bundle); e != nil || bundle.Schema != 1 || len(bundle.Records) != m.Count {
		t.Fatal("fixture member inventory", e)
	}
	expected := map[string]int{}
	for i, row := range m.Records {
		if _, exists := expected[row.Name]; exists {
			t.Fatal("duplicate member")
		}
		expected[row.Name] = i
	}
	output := map[string][]publicV2Read{}
	total := 0
	for _, row := range bundle.Records {
		index, ok := expected[row.Name]
		if !ok || !regexp.MustCompile(`^[0-9a-f]{64}\.raw$`).MatchString(row.Name) {
			t.Fatal("unexpected fixed member", row.Name)
		}
		entry := m.Records[index]
		delete(expected, row.Name)
		if len(row.Data) != entry.Bytes || publicV2Digest(row.Data) != entry.SHA {
			t.Fatal("fixed raw bytes changed", row.Name)
		}
		total += len(row.Data)
		var envelope struct {
			Digest  string
			Payload json.RawMessage
		}
		if e = json.Unmarshal(row.Data, &envelope); e != nil || publicV2Digest(envelope.Payload) != envelope.Digest {
			t.Fatal("envelope hash", e)
		}
		var record struct {
			Ref struct {
				ID, Digest string
				Owner      struct {
					Chain            string
					Height           uint64
					Hash, ProviderID string
				}
				Bytes int
			}
			Data       []byte
			CapturedAt time.Time
		}
		if e = json.Unmarshal(envelope.Payload, &record); e != nil {
			t.Fatal(e)
		}
		owner, _ := json.Marshal(record.Ref.Owner)
		if record.Ref.ID+".raw" != row.Name || record.Ref.ID != publicV2Digest(owner) || record.Ref.Owner.Chain != "evm:1" || record.Ref.Owner.Height != 0 || record.Ref.Owner.ProviderID != entry.Provider || record.Ref.Owner.Hash != record.Ref.Digest || publicV2Digest(record.Data) != record.Ref.Digest || len(record.Data) != record.Ref.Bytes || record.CapturedAt.IsZero() {
			t.Fatal("raw source identity")
		}
		var observation struct {
			Kind     string `json:"source_kind"`
			Method   string
			Params   []json.RawMessage
			Response []byte
		}
		if e = json.Unmarshal(record.Data, &observation); e != nil || observation.Kind != "raw-json-rpc-response" {
			t.Fatal("source envelope", e)
		}
		var response struct {
			JSONRPC string `json:"jsonrpc"`
			ID      int
			Result  json.RawMessage
			Error   json.RawMessage
		}
		if e = json.Unmarshal(observation.Response, &response); e != nil || response.ID != 1 || response.JSONRPC != "2.0" || len(response.Result) == 0 || len(response.Error) > 0 {
			t.Fatal("retained successful RPC", e)
		}
		output[entry.Provider] = append(output[entry.Provider], publicV2Read{observation.Method, observation.Params, observation.Response})
	}
	if len(expected) != 0 || total != m.RawBytes || len(output) != 2 {
		t.Fatal("fixed member coverage")
	}
	return m, output
}
func publicV2Adapter(t *testing.T, fault string) (*Adapter, publicV2Manifest, *atomic.Int64) {
	t.Helper()
	m, records := publicV2Fixture(t)
	var mutations atomic.Int64
	urls := []string{}
	for _, endpoint := range m.Endpoints {
		id := "rpc-" + publicV2Digest([]byte(endpoint))[:24]
		rows := records[id]
		if len(rows) == 0 {
			t.Fatal("missing provider capture")
		}
		index := map[string]json.RawMessage{}
		for _, row := range rows {
			key := publicV2RequestKey(row.Method, row.Params)
			if old, exists := index[key]; exists {
				var a, b any
				json.Unmarshal(old, &a)
				json.Unmarshal(row.Body, &b)
				if !reflect.DeepEqual(a, b) {
					t.Fatal("ambiguous retained RPC")
				}
			} else {
				index[key] = row.Body
			}
		}
		server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			var q struct {
				ID     int
				Method string
				Params []json.RawMessage
			}
			if e := json.NewDecoder(io.LimitReader(r.Body, 65537)).Decode(&q); e != nil {
				t.Error(e)
				return
			}
			body, ok := index[publicV2RequestKey(q.Method, q.Params)]
			if !ok {
				t.Error("unrecorded source request", q.Method, stringMustJSON(q.Params))
				http.Error(w, "unrecorded", 400)
				return
			}
			var value map[string]json.RawMessage
			json.Unmarshal(body, &value)
			changed := false
			if fault == "proof-node" && q.Method == "eth_getProof" {
				var result map[string]json.RawMessage
				json.Unmarshal(value["result"], &result)
				result["accountProof"] = json.RawMessage(`["0xc0"]`)
				value["result"], _ = json.Marshal(result)
				changed = true
			}
			if fault == "header-root" && q.Method == "eth_getBlockByNumber" && string(q.Params[0]) != `"0x0"` {
				var result map[string]json.RawMessage
				json.Unmarshal(value["result"], &result)
				result["stateRoot"], _ = json.Marshal("0x" + strings.Repeat("11", 32))
				value["result"], _ = json.Marshal(result)
				changed = true
			}
			if changed {
				body, _ = json.Marshal(value)
				mutations.Add(1)
			}
			w.Header().Set("Content-Type", "application/json")
			w.Write(body)
		}))
		t.Cleanup(server.Close)
		urls = append(urls, server.URL)
	}
	chain := config.Chain{Key: "evm:1", Genesis: "0xd4e56740f876aef8c010b86a40d5f56745a118d0906a34e69aec8c0db1cb8fa3", Pools: []config.PoolConfig{m.Pool}, Confirmation: "finalized", BlockIntegrity: ChainNativeCommitmentsV2, ReceiptsMode: "bulk-auto", NativeProof: &config.NativeProofConfig{MaxBytes: 64 << 20, MaxHeaders: 64, MaxWindowBlocks: 16, MaxTransactions: 4096, MaxLookahead: 32}}
	return &Adapter{Chain: chain, Client: rpc.New(urls, 60*time.Second, 16<<20)}, m, &mutations
}
func stringMustJSON(value any) string { raw, _ := json.Marshal(value); return string(raw) }
func TestV2CompleteRealPublicFixtureNormalReadAuthenticatesBodyStateAndTokenBalances(t *testing.T) {
	for attempt := 0; attempt < 2; attempt++ {
		t.Run(fmt.Sprint(attempt), func(t *testing.T) {
			a, m, _ := publicV2Adapter(t, "")
			ctx := context.Background()
			if e := a.Client.VerifyEVM(ctx, a.Chain.Key, a.Chain.Genesis); e != nil {
				t.Fatal(e)
			}
			ref, pools, e := a.ReadState(ctx, "finalized", false)
			if e != nil || len(pools) != 1 || ref != m.Block || !reflect.DeepEqual(pools[0], m.Expected) {
				t.Fatal("actual public bank changed", e, ref, len(pools))
			}
		})
	}
}
func TestV2CompleteRealPublicFixtureRejectsOriginalProofAndHeaderMutation(t *testing.T) {
	for _, fault := range []string{"proof-node", "header-root"} {
		t.Run(fault, func(t *testing.T) {
			a, _, mutations := publicV2Adapter(t, fault)
			_, pools, e := a.ReadState(context.Background(), "finalized", false)
			if e == nil || len(pools) != 0 || mutations.Load() == 0 {
				t.Fatal("real public rejection not exercised", fault, e, pools, mutations.Load())
			}
		})
	}
}
