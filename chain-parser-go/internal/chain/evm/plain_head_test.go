package evm

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"net/http/httptest"
	"testing"
	"time"

	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/rpc"
)

func TestPlainHeadFallbackRequiresFreshAndConsistentProvider(t *testing.T) {
	now := time.Now().Unix()
	genesis := fmt.Sprintf("0x%064x", 1)
	block := func(height uint64, timestamp int64) RawBlock {
		return RawBlock{
			Number: fmt.Sprintf("0x%x", height), Timestamp: fmt.Sprintf("0x%x", timestamp),
			Hash: fmt.Sprintf("0x%064x", height), ParentHash: fmt.Sprintf("0x%064x", height-1),
			Transactions: []string{},
		}
	}
	provider := func(chainID string, head, anchor RawBlock) *httptest.Server {
		return httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			var request struct {
				Method string
				Params []json.RawMessage
			}
			if err := json.NewDecoder(r.Body).Decode(&request); err != nil {
				t.Error(err)
				return
			}
			var result any
			switch request.Method {
			case "eth_chainId":
				result = chainID
			case "eth_getBlockByNumber":
				switch string(request.Params[0]) {
				case `"latest"`:
					result = head
				case `"0x0"`:
					result = map[string]string{"hash": genesis}
				case `"0x64"`:
					result = anchor
				default:
					t.Errorf("unexpected block lookup %s", request.Params[0])
				}
			default:
				t.Errorf("unexpected method %s", request.Method)
			}
			if err := json.NewEncoder(w).Encode(map[string]any{"jsonrpc": "2.0", "id": 1, "result": result}); err != nil {
				t.Error(err)
			}
		}))
	}
	stale := block(100, now-120)
	fresh := block(101, now)
	wrong := stale
	wrong.Hash = fmt.Sprintf("0x%064x", 999)
	wrongClock := stale
	wrongClock.Timestamp = fmt.Sprintf("0x%x", now)
	primary := provider("0x7a69", stale, stale)
	defer primary.Close()
	for _, tc := range []struct {
		name, chainID string
		head, anchor  RawBlock
		wantError     string
	}{
		{"qualified", "0x7a69", fresh, stale, ""},
		{"stale", "0x7a69", block(101, now-120), stale, "BASELINE_TOO_OLD"},
		{"future", "0x7a69", block(101, now+60), stale, "BASELINE_FUTURE_TIMESTAMP"},
		{"wrong-chain", "0x1", fresh, stale, "BASELINE_TOO_OLD"},
		{"different-anchor", "0x7a69", fresh, wrong, "HEAD_PROVIDER_DISAGREEMENT"},
		{"same-height-different-clock", "0x7a69", wrongClock, stale, "HEAD_PROVIDER_DISAGREEMENT"},
	} {
		t.Run(tc.name, func(t *testing.T) {
			peer := provider(tc.chainID, tc.head, tc.anchor)
			defer peer.Close()
			adapter := &Adapter{
				Client: rpc.New([]string{primary.URL, peer.URL}, time.Second, 1<<20),
				Chain:  config.Chain{Key: "evm:31337", Genesis: genesis}, OnlineMaxAgeMS: 60_000,
			}
			got, err := adapter.Header(context.Background(), "latest")
			if tc.wantError == "" {
				if err != nil || got.Number != 101 || got.Hash != fresh.Hash {
					t.Fatalf("qualified peer head = %v, error = %v", got, err)
				}
			} else if err == nil || err.Error() != tc.wantError {
				t.Fatalf("head = %v, error = %v, want %s", got, err, tc.wantError)
			}
			pinned, err := adapter.Header(context.Background(), "0x64")
			if err != nil || pinned.Number != 100 || pinned.TimestampMS != uint64(now-120)*1000 {
				t.Fatalf("fixed block changed: %v, error = %v", pinned, err)
			}
		})
	}
}
