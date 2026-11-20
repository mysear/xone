package evm

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"testing"
	"time"

	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/rpc"
)

func TestOPOnlineOriginRequiresFreshMatchingL1Headers(t *testing.T) {
	var beacon json.RawMessage
	readOriginFixture(t, "beacon-fork-handover/electra-fulu.json.gz", &beacon)
	path := filepath.Join(t.TempDir(), "proof.json")
	if err := os.WriteFile(path, beacon, 0600); err != nil {
		t.Fatal(err)
	}
	var evidence OPOriginEvidence
	readOriginFixture(t, "op-mainnet-withdrawals/origin-144644465.json.gz", &evidence)
	ctx := context.Background()
	body, err := VerifyChainBlockJSON(ctx, "evm:10", evidence.Block, 32<<20)
	if err != nil {
		t.Fatal(err)
	}
	for _, wrong := range []bool{false, true} {
		server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			var req struct {
				Method string
				Params []json.RawMessage
			}
			if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
				t.Error(err)
				return
			}
			var result any
			switch req.Method {
			case "eth_chainId":
				result = "0x1"
			case "eth_getBlockByNumber":
				var tag string
				json.Unmarshal(req.Params[0], &tag)
				switch tag {
				case "0x0":
					genesis, _ := knownBodyGenesis("evm:1")
					result = map[string]string{"hash": genesis}
				case "0x16d55f5":
					result = evidence.Headers[0]
				case "0x16d55f4":
					result = evidence.Headers[1]
					if wrong {
						result = evidence.Headers[0]
					}
				default:
					t.Error(tag)
				}
			default:
				t.Error(req.Method)
			}
			json.NewEncoder(w).Encode(map[string]any{"id": 1, "jsonrpc": "2.0", "result": result})
		}))
		t.Cleanup(server.Close)
		a := Adapter{Client: rpc.New([]string{server.URL}, time.Second, 4<<20), Chain: config.Chain{Key: "evm:10", OPOrigin: &config.OPOriginConfig{BeaconFinalityConfig: config.BeaconFinalityConfig{CheckpointRoot: "0xd203fe26634fb6d972b70fdc29d9684963183549cab12cae1ce60aea9aa2533c", MaxCheckpointAgeMS: 16 * 8192 * 12000}, L1RPCURL: server.URL}}, OPOriginProofFile: path, OPOriginMaxAgeMS: 3600000}
		block := body.Block
		block.NativeBodyProof = &body
		block.NativeCoverage = &VerifiedReceiptCoverage{}
		block.nativeSession = &nativeReadSession{ctx: ctx, limits: nativeProofTestLimits(), remaining: 16 << 20}
		clock := time.Unix(1764887627+1800, 0)
		check, e := a.verifyOnlineOPOrigin(ctx, block, func() time.Time { return clock })
		if wrong {
			if e == nil || !block.nativeSession.rejected {
				t.Fatal("wrong ancestor", e)
			}
			continue
		}
		if e != nil {
			t.Fatal(e)
		}
		if err := check(); err != nil {
			t.Fatal(err)
		}
		clock = clock.Add(time.Hour)
		if err := check(); err == nil {
			t.Fatal("expired signed head admitted")
		}
	}
}
