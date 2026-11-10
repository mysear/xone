package evm

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"sync"
	"sync/atomic"
	"testing"
	"time"

	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/rpc"
)

func TestNativeWitnessReprovesCurrentSAEInsteadOfOwnHeader(t *testing.T) {
	target, proof := retainedSAEProof(t)
	expected, err := VerifySAEExecutionProof(context.Background(), target, proof, nativeProofTestLimits())
	if err != nil {
		t.Fatal(err)
	}
	full := retainedFullChainBlock(t, "avalanche-current")
	headers := map[string]json.RawMessage{}
	for _, h := range proof.Headers {
		var b RawBlock
		json.Unmarshal(h, &b)
		headers[b.Number] = h
	}
	var settledReads sync.Map
	var mutateAfterReceipt atomic.Bool
	handler := http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		var req struct {
			Method string
			Params []json.RawMessage
		}
		json.NewDecoder(r.Body).Decode(&req)
		var result any
		switch req.Method {
		case "eth_chainId":
			result = "0xa86a"
		case "eth_getBlockByNumber":
			var tag string
			var fullBody bool
			json.Unmarshal(req.Params[0], &tag)
			json.Unmarshal(req.Params[1], &fullBody)
			if tag == "0x0" {
				result = map[string]any{"hash": proof.GenesisHash}
			} else if tag == target.Number && fullBody {
				result = full
			} else {
				result = headers[tag]
				if _, orphan := settledReads.Load(r.Host); orphan && tag == "0x5f06ad2" {
					var fields map[string]json.RawMessage
					json.Unmarshal(headers[tag], &fields)
					fields["stateRoot"] = json.RawMessage(`"0x1111111111111111111111111111111111111111111111111111111111111111"`)
					result = fields
				}
			}
		case "eth_getBlockReceipts":
			result = proof.Window[0].Receipts
			if mutateAfterReceipt.Load() {
				settledReads.Store(r.Host, true)
			}
		default:
			t.Error(req.Method)
		}
		json.NewEncoder(w).Encode(map[string]any{"jsonrpc": "2.0", "id": 1, "result": result})
	})
	left := httptest.NewServer(handler)
	defer left.Close()
	right := httptest.NewServer(handler)
	defer right.Close()
	a := Adapter{Client: rpc.New([]string{left.URL, right.URL}, 10*time.Second, 16<<20), Chain: config.Chain{Key: "evm:43114", Genesis: proof.GenesisHash, BlockIntegrity: ChainNativeCommitmentsV2}}
	if err := a.VerifyNativeReceiptWitnesses(context.Background(), expected, "bulk-auto", nativeProofTestLimits()); err != nil {
		t.Fatal(err)
	}
	mutateAfterReceipt.Store(true)
	if err := a.VerifyNativeReceiptWitnesses(context.Background(), expected, "bulk-auto", nativeProofTestLimits()); err == nil {
		t.Fatal("witness final check ignored orphaned settling header")
	}
}
