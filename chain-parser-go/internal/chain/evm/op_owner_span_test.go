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

func TestOPOwnerSpanMembership(t *testing.T) {
	const fixtures = "op-mainnet-batches/"
	var observation OPChannelObservation
	var submission OPChannelSubmission
	readOriginFixture(t, fixtures+"batch-26453870.json.gz", &submission.Evidence)
	readOriginFixture(t, fixtures+"authority-26453870.json.gz", &submission.Authority)
	observation.Submissions = []OPChannelSubmission{submission}
	readOriginFixture(t, fixtures+"l2-159774755-159774778.json.gz", &observation.Blocks)
	var beacon json.RawMessage
	readOriginFixture(t, fixtures+"beacon-26453871.json.gz", &beacon)
	dir := t.TempDir()
	beaconFile := filepath.Join(dir, "beacon.json")
	if err := os.WriteFile(beaconFile, beacon, 0600); err != nil {
		t.Fatal(err)
	}
	raw, err := json.Marshal(observation)
	if err != nil {
		t.Fatal(err)
	}
	if err = os.WriteFile(filepath.Join(dir, "channel.json"), raw, 0600); err != nil {
		t.Fatal(err)
	}
	var headers []json.RawMessage
	readOriginFixture(t, fixtures+"origins-26453871-26453857.json.gz", &headers)
	byNumber := map[string]json.RawMessage{}
	for _, h := range headers {
		var b RawBlock
		if err := json.Unmarshal(h, &b); err != nil {
			t.Fatal(err)
		}
		byNumber[b.Number] = h
	}
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
			if tag == "0x0" {
				g, _ := knownBodyGenesis("evm:1")
				result = map[string]string{"hash": g}
			} else {
				result = byNumber[tag]
			}
		default:
			t.Error(req.Method)
		}
		json.NewEncoder(w).Encode(map[string]any{"id": 1, "jsonrpc": "2.0", "result": result})
	}))
	defer server.Close()
	policy := config.OPOriginConfig{BeaconFinalityConfig: config.BeaconFinalityConfig{CheckpointRoot: "0x48176a8eb6476fee86b5540b0764721cb93572a1d83234db95dbedcc845907c7", MaxCheckpointAgeMS: 16 * 8192 * 12000}, L1RPCURL: server.URL, BatchProofFile: "channel.json"}
	a := Adapter{Client: rpc.New([]string{server.URL}, 10*time.Second, 4<<20), Chain: config.Chain{Key: "evm:10", OPOrigin: &policy}, OPOriginProofFile: beaconFile, OPOriginMaxAgeMS: 3600000}
	ctx := context.Background()
	body, err := VerifyChainBlockJSON(ctx, "evm:10", observation.Blocks[12], 32<<20)
	if err != nil {
		t.Fatal(err)
	}
	b := body.Block
	b.NativeCoverage = &VerifiedReceiptCoverage{}
	b.NativeBodyProof = &body
	limits := nativeProofTestLimits()
	limits.MaxHeaders = 32
	clock := func() time.Time { return time.Unix(1606824023+12*(15693696+128), 0) }
	for _, tc := range []struct{ name, want string }{{"matching", ""}, {"wrong hash", "OP_SPAN_OWNER_BLOCK_MISMATCH"}, {"budget", "OP_BATCH_OBSERVATION_FILE"}} {
		t.Run(tc.name, func(t *testing.T) {
			block := b
			block.nativeSession = &nativeReadSession{ctx: ctx, limits: limits, remaining: 32 << 20}
			if tc.name == "wrong hash" {
				block.Hash = block.ParentHash
			}
			if tc.name == "budget" {
				block.nativeSession.remaining = len(raw) - 1
			}
			check, err := a.verifyOnlineOPOrigin(ctx, block, clock)
			if tc.want == "" {
				if err != nil {
					t.Fatal(err)
				}
				if err = check(); err != nil {
					t.Fatal(err)
				}
				if block.nativeSession.remaining >= 32<<20-len(raw) {
					t.Fatal("unaccounted proof")
				}
			} else if err == nil || err.Error() != tc.want || check != nil || !block.nativeSession.rejected {
				t.Fatal(err)
			}
		})
	}
}
