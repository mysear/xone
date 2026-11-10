package service

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"net/http/httptest"
	"os"
	"sort"
	"strings"
	"testing"
	"time"
	"xone/chain-parser-go/internal/chain/evm"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/domain"
	"xone/chain-parser-go/internal/rpc"
)

func TestLargeGapWeightedAdmissionFailureStillPublishesRevocation(t *testing.T) {
	cfg, e := config.Load("../../config/config-test.yaml", "chain-parser-go")
	if e != nil {
		t.Fatal(e)
	}
	cfg.Config.State.Path = t.TempDir()
	cfg.Config.Startup.MaxTail = 2
	ch := cfg.Config.Chains[0]
	raw, e := os.ReadFile("../../test/contracts/weighted-state.json")
	if e != nil {
		t.Fatal(e)
	}
	var data struct {
		Configs []config.PoolConfig `json:"configs"`
		Pools   []domain.Pool       `json:"pools"`
	}
	if e = json.Unmarshal(raw, &data); e != nil {
		t.Fatal(e)
	}
	ch.Pools = data.Configs
	ch.Genesis = "0x" + strings.Repeat("9", 64)
	cfg.Config.Chains[0] = ch
	base := testSnapshotSource(t)
	source := func(ctx context.Context, chain config.Chain) (domain.Snapshot, error) {
		snapshot, err := base(ctx, chain)
		if err != nil {
			return snapshot, err
		}
		snapshot.Pools = data.Pools
		snapshot.Reference.ScopeHash, err = domain.ScopeHash(snapshot.Pools)
		if err != nil {
			return snapshot, err
		}
		seen := map[string]bool{}
		versions := []string{}
		for _, pool := range snapshot.Pools {
			if !seen[pool.AdapterVersion] {
				seen[pool.AdapterVersion] = true
				versions = append(versions, pool.AdapterVersion)
			}
		}
		sort.Strings(versions)
		snapshot.Reference.AdapterSetHash, err = domain.Hash(versions)
		return snapshot, err
	}
	s, e := NewWithSource(cfg, source)
	if e != nil {
		t.Fatal(e)
	}
	defer s.Close()
	if e = s.Start(context.Background()); e != nil {
		t.Fatal(e)
	}
	s.source = nil
	original, ready := s.stores[ch.Key].Current()
	if !ready {
		t.Fatal("missing initial state")
	}
	head := original.Reference.Block.Number + 10
	ch.Genesis = "0x" + strings.Repeat("9", 64)
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		var q struct {
			ID     json.RawMessage   `json:"id"`
			Method string            `json:"method"`
			Params []json.RawMessage `json:"params"`
		}
		json.NewDecoder(r.Body).Decode(&q)
		var out any
		switch q.Method {
		case "eth_chainId":
			out = "0x7a69"
		case "eth_getBlockByNumber":
			var tag string
			json.Unmarshal(q.Params[0], &tag)
			n := head
			hash := "0x" + strings.Repeat("a", 64)
			if tag == "0x0" {
				n = 0
				hash = ch.Genesis
			}
			out = map[string]any{"number": fmt.Sprintf("0x%x", n), "hash": hash, "parentHash": "0x" + strings.Repeat("b", 64), "timestamp": fmt.Sprintf("0x%x", time.Now().Unix()), "transactions": []string{}, "receiptsRoot": "0x56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421"}
		case "eth_getCode":
			out = "0x6002"
		default:
			t.Error("unexpected RPC", q.Method)
		}
		json.NewEncoder(w).Encode(map[string]any{"jsonrpc": "2.0", "id": q.ID, "result": out})
	}))
	defer server.Close()
	s.adapters[ch.Key] = &evm.Adapter{Client: rpc.New([]string{server.URL}, time.Second, 1<<20), Chain: ch}
	if e = s.syncChain(context.Background(), ch); e == nil || e.Error() != "STABLE_CODE_HASH_MISMATCH" {
		t.Fatal("expected weighted source rejection", e)
	}
	if _, ready = s.stores[ch.Key].Current(); ready {
		t.Fatal("failed reanchor remained ready")
	}
	records, e := s.stores[ch.Key].Records(original.Reference.JournalSeq, 10)
	if e != nil || len(records) != 1 || records[0].Kind != "ONLINE_SCOPE_REVOKED" {
		t.Fatal("old remote scope not revoked", records, e)
	}
}
