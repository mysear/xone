package evm

import (
	"bytes"
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"

	"github.com/ethereum/go-ethereum/crypto/kzg4844"
	"xone/chain-parser-go/internal/provenance"
	"xone/chain-parser-go/internal/rpc"
)

func TestCollectOPSubmissionChecksEveryHeight(t *testing.T) {
	const dir = "op-mainnet-batches/"
	var beacon provenance.OnlineBeaconProof
	var evidence OPBatchEvidence
	var authority OPBatchSenderProof
	var head json.RawMessage
	readOriginFixture(t, dir+"beacon-26453871.json.gz", &beacon)
	readOriginFixture(t, dir+"batch-26453870.json.gz", &evidence)
	readOriginFixture(t, dir+"authority-26453870.json.gz", &authority)
	readOriginFixture(t, dir+"search-head-26453871.json.gz", &head)
	proof, err := provenance.VerifyExecutionFinality("0x48176a8eb6476fee86b5540b0764721cb93572a1d83234db95dbedcc845907c7", beacon.Bootstrap, beacon.Finality, beacon.Updates)
	if err != nil {
		t.Fatal(err)
	}
	for _, tc := range []struct {
		name           string
		window, budget int
		want           string
	}{{"parent", 2, 32 << 20, ""}, {"limited window", 1, 32 << 20, "OP_BATCH_SEARCH_EXHAUSTED"}, {"wrong parent", 2, 32 << 20, "OP_BATCH_ANCESTRY_LINK"}, {"limited bytes", 2, 1, "OP_BATCH_SEARCH_BUDGET"}} {
		t.Run(tc.name, func(t *testing.T) {
			server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				var req struct {
					Method string
					Params []json.RawMessage
				}
				if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
					t.Error(err)
					return
				}
				var first string
				json.Unmarshal(req.Params[0], &first)
				var result json.RawMessage
				switch req.Method {
				case "eth_getBlockByNumber":
					if first == "0x193a76f" && string(req.Params[1]) == "false" {
						result = evidence.Descendants[0]
					} else if first == "0x193a76f" || tc.name == "wrong parent" {
						result = head
					} else {
						result = evidence.Block
					}
				case "eth_getProof":
					if first == opConfigProxy {
						result = authority.Proxy
					} else if strings.EqualFold(first, "0xade27248542c0f3e89b49e4eb76155e267f14377") {
						result = authority.Implementation
					} else {
						t.Errorf("unexpected account %s", first)
					}
				default:
					t.Error(req.Method)
				}
				json.NewEncoder(w).Encode(map[string]any{"id": 1, "jsonrpc": "2.0", "result": result})
			}))
			defer server.Close()
			client := rpc.New([]string{server.URL}, 5*time.Second, 8<<20)
			defer client.HTTP.CloseIdleConnections()
			calls := 0
			blobsAt := func(_ context.Context, slot uint64) ([]*kzg4844.Blob, error) {
				calls++
				if slot != 15693695 {
					t.Fatal(slot)
				}
				return evidence.Blobs, nil
			}
			items, err := collectOPSubmission(context.Background(), proof, client, client, tc.window, tc.budget, blobsAt)
			if tc.want != "" {
				if err == nil || err.Error() != tc.want || items != nil || calls != 0 {
					t.Fatal(err, calls)
				}
				return
			}
			if err != nil || len(items) != 1 || items[0].Evidence.TxHash != evidence.TxHash || len(items[0].Evidence.Descendants) != 1 || calls != 1 {
				t.Fatal(err, calls)
			}
			// This allowance covers the two execution responses and is one byte
			// short of the authority responses. Proof reads must exhaust it
			// before any blob request, even before sender/frame filtering.
			proofBudget := -1
			for _, raw := range []json.RawMessage{evidence.Descendants[0], evidence.Block, authority.Proxy, authority.Implementation} {
				// json.Encoder compacts the server's RawMessage result, so use
				// the wire size rather than retained fixture whitespace.
				var compact bytes.Buffer
				if err := json.Compact(&compact, raw); err != nil {
					t.Fatal(err)
				}
				proofBudget += compact.Len()
			}
			beforeProofBudget := calls
			if got, err := collectOPForward(context.Background(), proof, client, client, tc.window, proofBudget, opScanCursor{26453870, 0}, blobsAt); err == nil || err.Error() != "OP_BATCH_SEARCH_BUDGET" || got != nil || calls != beforeProofBudget {
				t.Fatal("authority bytes not charged before blob retrieval", err, calls-beforeProofBudget)
			}
			for _, scan := range []struct {
				cursor opScanCursor
				want   string
			}{
				{opScanCursor{26453870, 0}, ""},
				{opScanCursor{26453870, 1 << 32}, "OP_BATCH_SCAN_CURSOR"},
				{opScanCursor{26453871, 0}, "OP_BATCH_SEARCH_EXHAUSTED"},
				{opScanCursor{26453869, 0}, "OP_BATCH_SEARCH_WINDOW"},
			} {
				got, err := collectOPForward(context.Background(), proof, client, client, tc.window, tc.budget, scan.cursor, blobsAt)
				if scan.want != "" {
					if err == nil || err.Error() != scan.want || got != nil {
						t.Fatal(scan, err)
					}
					continue
				}
				if err != nil || len(got) != 1 || got[0].Evidence.TxHash != evidence.TxHash {
					t.Fatal(scan, err)
				}
				var source struct{ Transactions []struct{ Hash string } }
				if err = json.Unmarshal(evidence.Block, &source); err != nil {
					t.Fatal(err)
				}
				for index, tx := range source.Transactions {
					if tx.Hash == evidence.TxHash {
						_, err = collectOPForward(context.Background(), proof, client, client, tc.window, tc.budget, opScanCursor{26453870, uint64(index) + 1}, blobsAt)
						if err == nil || err.Error() != "OP_BATCH_SEARCH_EXHAUSTED" {
							t.Fatal("resubmitted previous transaction", err)
						}
					}
				}
			}

		})
	}
}
