package evm

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"os"
	"strings"
	"sync/atomic"
	"testing"
	"time"

	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/rpc"
)

func TestBeaconAncestorMainnet(t *testing.T) {
	data, err := os.ReadFile("../../../test/fixtures/source/testdata/ethereum-mainnet/uniswap-v2-26355865-evidence.json")
	if err != nil {
		t.Fatal(err)
	}
	var capture struct {
		Block map[string]json.RawMessage
		Hash  string `json:"expected_block_hash"`
	}
	if err = json.Unmarshal(data, &capture); err != nil {
		t.Fatal(err)
	}
	// Reproduce the standard RPC hashes-only representation of the recorded block.
	var txs []struct{ Hash string }
	if err = json.Unmarshal(capture.Block["transactions"], &txs); err != nil {
		t.Fatal(err)
	}
	hashes := make([]string, len(txs))
	for i, tx := range txs {
		hashes[i] = tx.Hash
	}
	capture.Block["transactions"], _ = json.Marshal(hashes)
	for _, tc := range []struct {
		name, tag, want string
		requests        int32
	}{
		{"valid", "0x1922899", "", 1}, {"tampered-parent", "0x1922899", "BLOCK_INTEGRITY_HEADER_HASH", 1}, {"height", "0x1922898", "BLOCK_INTEGRITY_HEIGHT", 1}, {"moving-tag", "finalized", "INVALID_HEX_QUANTITY", 0}, {"alias", "0x01922899", "INVALID_HEX_QUANTITY", 0}, {"cancel", "0x1922899", "context canceled", 0}, {"policy-disabled", "0x1922899", "BEACON_ANCESTRY_PROFILE", 0},
	} {
		t.Run(tc.name, func(t *testing.T) {
			var count atomic.Int32
			server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				count.Add(1)
				var q struct {
					Method string
					Params []json.RawMessage
				}
				if err := json.NewDecoder(r.Body).Decode(&q); err != nil || q.Method != "eth_getBlockByNumber" || len(q.Params) != 2 || string(q.Params[1]) != "false" || string(q.Params[0]) != `"`+tc.tag+`"` {
					t.Errorf("unexpected request: %+v (%v)", q, err)
					return
				}
				fields := make(map[string]json.RawMessage, len(capture.Block))
				for k, v := range capture.Block {
					fields[k] = v
				}
				if tc.name == "tampered-parent" {
					fields["parentHash"], _ = json.Marshal("0x" + strings.Repeat("0", 64))
				}
				json.NewEncoder(w).Encode(map[string]any{"jsonrpc": "2.0", "id": 1, "result": fields})
			}))
			defer server.Close()
			chain := config.Chain{Key: "evm:1", BlockIntegrity: ChainNativeCommitmentsV2, BeaconFinality: &config.BeaconFinalityConfig{}}
			if tc.name == "policy-disabled" {
				chain.BeaconFinality = nil
			}
			reader := BeaconAncestorReader{Adapter: &Adapter{Chain: chain, Client: rpc.New([]string{server.URL}, time.Second, 1<<20)}}
			ctx, cancel := context.WithCancel(context.Background())
			defer cancel()
			if tc.name == "cancel" {
				cancel()
			}
			ref, err := reader.Header(ctx, tc.tag)
			if tc.want == "" {
				if err != nil || ref.Hash != capture.Hash || ref.Number != 26355865 || ref.ChainKey != "evm:1" {
					t.Fatalf("ref=%+v err=%v", ref, err)
				}
			} else if err == nil || err.Error() != tc.want {
				t.Fatalf("got %v, want %s", err, tc.want)
			}
			if count.Load() != tc.requests {
				t.Fatalf("RPC count %d", count.Load())
			}
		})
	}
}
