package evm

import (
	"context"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"github.com/ethereum/go-ethereum/core/types"
	"net/http"
	"net/http/httptest"
	"os"
	"strings"
	"sync/atomic"
	"testing"
	"time"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/provenance"
	"xone/chain-parser-go/internal/rpc"
)

func TestOnlineEthereumBlockVerifiesRetainedRealHeaderAndTransactionRoot(t *testing.T) {
	raw, err := os.ReadFile("../../../test/fixtures/source/testdata/ethereum-mainnet/uniswap-v2-26355865-evidence.json")
	if err != nil {
		t.Fatal(err)
	}
	var evidence provenance.Evidence
	if err = json.Unmarshal(raw, &evidence); err != nil {
		t.Fatal(err)
	}
	var fields map[string]json.RawMessage
	if err = json.Unmarshal(evidence.Block, &fields); err != nil {
		t.Fatal(err)
	}
	hashes := make([]string, len(evidence.RawTransactions))
	fullTransactions := make([]map[string]any, len(hashes))
	transactions := map[string]string{}
	for i, encoded := range evidence.RawTransactions {
		decoded, e := hex.DecodeString(encoded[2:])
		if e != nil {
			t.Fatal(e)
		}
		var tx types.Transaction
		if e = tx.UnmarshalBinary(decoded); e != nil {
			t.Fatal(e)
		}
		hashes[i] = tx.Hash().Hex()
		transactions[hashes[i]] = encoded
		txJSON, _ := json.Marshal(&tx)
		_ = json.Unmarshal(txJSON, &fullTransactions[i])
		fullTransactions[i]["blockHash"] = evidence.ExpectedBlockHash
		fullTransactions[i]["blockNumber"] = json.RawMessage(fields["number"])
		fullTransactions[i]["transactionIndex"] = fmt.Sprintf("0x%x", i)
	}
	fields["transactions"], _ = json.Marshal(fullTransactions)
	fields["hash"], _ = json.Marshal(evidence.ExpectedBlockHash)
	good, _ := json.Marshal(fields)
	var number string
	_ = json.Unmarshal(fields["number"], &number)
	for _, change := range []string{"none", "header", "drop-transaction", "raw-body", "unsupported-chain", "unknown-profile", "response-budget", "cancelled", "transaction-position", "transaction-chain", "duplicate-transaction", "wrong-height", "no-full-body"} {
		t.Run(change, func(t *testing.T) {
			var changed map[string]json.RawMessage
			_ = json.Unmarshal(good, &changed)
			switch change {
			case "header":
				changed["timestamp"] = json.RawMessage(`"0x1"`)
			case "drop-transaction":
				changed["transactions"], _ = json.Marshal(fullTransactions[:len(hashes)-1])
			}
			block, _ := json.Marshal(changed)
			if change == "raw-body" || change == "transaction-position" || change == "transaction-chain" || change == "duplicate-transaction" {
				copied := make([]map[string]any, len(fullTransactions))
				rawCopy, _ := json.Marshal(fullTransactions)
				_ = json.Unmarshal(rawCopy, &copied)
				switch change {
				case "raw-body":
					copied[0]["value"] = "0x1234"
				case "transaction-position":
					copied[0]["transactionIndex"] = "0x1"
				case "transaction-chain":
					copied[0]["chainId"] = "0x2105"
				case "duplicate-transaction":
					copied[1] = copied[0]
				}
				changed["transactions"], _ = json.Marshal(copied)
				block, _ = json.Marshal(changed)
			}
			if change == "no-full-body" {
				changed["transactions"], _ = json.Marshal(hashes)
				block, _ = json.Marshal(changed)
			}
			var bodyReads atomic.Int64
			server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				var request struct {
					Method string            `json:"method"`
					Params []json.RawMessage `json:"params"`
				}
				if e := json.NewDecoder(r.Body).Decode(&request); e != nil {
					t.Error(e)
					return
				}
				var result any
				switch request.Method {
				case "eth_getBlockByNumber":
					var full bool
					_ = json.Unmarshal(request.Params[1], &full)
					if !full {
						t.Error("integrity policy must use standard full-block request")
					}
					bodyReads.Add(1)
					result = json.RawMessage(block)
				case "eth_getRawTransactionByHash":
					bodyReads.Add(1)
					var hash string
					_ = json.Unmarshal(request.Params[0], &hash)
					result = transactions[hash]
					if change == "raw-body" {
						result = transactions[hashes[1]]
					}
				default:
					t.Errorf("unexpected method %s", request.Method)
				}
				_ = json.NewEncoder(w).Encode(map[string]any{"jsonrpc": "2.0", "id": 1, "result": result})
			}))
			defer server.Close()
			var chain config.Chain
			// Decode through the real config shape. Old code silently ignores the new
			// policy and therefore wrongly accepts these tampering cases.
			encoded := `{"key":"evm:1","block_integrity":"ethereum-header-transactions-v1"}`
			if change == "unsupported-chain" {
				encoded = strings.ReplaceAll(encoded, "evm:1", "evm:8453")
			}
			if change == "unknown-profile" {
				encoded = strings.ReplaceAll(encoded, "ethereum-header-transactions-v1", "unknown")
			}
			_ = json.Unmarshal([]byte(encoded), &chain)
			budget := 8 << 20
			if change == "response-budget" {
				budget = len(block) + 512
			}
			adapter := Adapter{Client: rpc.New([]string{server.URL}, 5*time.Second, budget), Chain: chain}
			ctx, cancel := context.WithCancel(context.Background())
			defer cancel()
			if change == "cancelled" {
				cancel()
			}
			requestedNumber := number
			if change == "wrong-height" {
				requestedNumber = "0x1"
			}
			observed, e := adapter.Block(ctx, requestedNumber)
			if change == "none" {
				if e != nil {
					t.Fatal(e)
				}
				if observed.Hash != evidence.ExpectedBlockHash || bodyReads.Load() != 1 {
					t.Fatalf("body not verified: hash=%s reads=%d", observed.Hash, bodyReads.Load())
				}
			} else if e == nil {
				t.Fatal("unverified block accepted: " + change)
			}
			if (change == "unsupported-chain" || change == "unknown-profile" || change == "cancelled") && bodyReads.Load() != 0 {
				t.Fatal("rejected header/profile must not fetch body")
			}
		})
	}
}
