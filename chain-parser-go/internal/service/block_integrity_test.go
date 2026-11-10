package service

import (
	"context"
	"encoding/json"
	"fmt"
	"math/big"
	"net/http"
	"net/http/httptest"
	"strings"
	"sync/atomic"
	"testing"
	"time"
	"xone/chain-parser-go/internal/config"

	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/core/types"
)

func TestNormalEthereumServiceIntegrityFailureRevokesScopeAndRetainsRawDLQ(t *testing.T) {
	cfg, err := config.Load("../../config/config-test.yaml", "chain-parser-go")
	if err != nil {
		t.Fatal(err)
	}
	cfg.Config.State.Path = t.TempDir()
	cfg.Config.Journal.RawEvidence = &config.RawEvidenceConfig{MaxBytes: 128 << 20, MaxRecords: 10000, MaxRecordBytes: int64(cfg.Config.Journal.MaxBytes)*4 + 16384}
	var tamper atomic.Bool
	makeHeader := func(number int64, parent common.Hash) *types.Header {
		return &types.Header{Number: big.NewInt(number), ParentHash: parent, UncleHash: types.EmptyUncleHash, Root: common.HexToHash("0x1"), TxHash: types.EmptyTxsHash, ReceiptHash: types.EmptyReceiptsHash, Difficulty: big.NewInt(0), GasLimit: 30000000, Time: uint64(time.Now().Unix()), Extra: []byte{}}
	}
	genesis := makeHeader(0, common.Hash{})
	baseline := makeHeader(100, common.HexToHash("0x99"))
	next := makeHeader(101, baseline.Hash())
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		var q struct {
			Method string
			Params []json.RawMessage
		}
		if json.NewDecoder(r.Body).Decode(&q) != nil {
			t.Error("invalid fixture request")
			return
		}
		var result any
		switch q.Method {
		case "eth_chainId":
			result = "0x1"
		case "eth_getBlockByNumber":
			var tag string
			_ = json.Unmarshal(q.Params[0], &tag)
			header := baseline
			if tag == "0x0" {
				header = genesis
			} else if tamper.Load() && tag != "0x64" {
				header = next
			}
			raw, _ := json.Marshal(header)
			var fields map[string]any
			_ = json.Unmarshal(raw, &fields)
			fields["transactions"] = []any{}
			if tamper.Load() && header == next {
				fields["timestamp"] = fmt.Sprintf("0x%x", next.Time+1)
			}
			result = fields
		case "eth_call":
			var call struct{ Data string }
			_ = json.Unmarshal(q.Params[0], &call)
			switch call.Data {
			case "0x0dfe1681":
				result = "0x" + strings.Repeat("0", 24) + strings.Repeat("1", 40)
			case "0xd21220a7":
				result = "0x" + strings.Repeat("0", 24) + strings.Repeat("2", 40)
			default:
				result = fmt.Sprintf("0x%064x%064x%064x", 1000000, 1000000, 0)
			}
		default:
			t.Errorf("unexpected fixture method %s", q.Method)
		}
		_ = json.NewEncoder(w).Encode(map[string]any{"jsonrpc": "2.0", "id": 1, "result": result})
	}))
	defer server.Close()
	chain := &cfg.Config.Chains[0]
	chain.Key = "evm:1"
	chain.Genesis = genesis.Hash().Hex()
	chain.RPCURLs = []string{server.URL}
	chain.BlockIntegrity = "ethereum-header-transactions-v1"
	chain.Confirmation = "finalized"
	chain.Pools = []config.PoolConfig{{ID: "0x" + strings.Repeat("3", 40), Protocol: "uniswap-v2", Token0: "0x" + strings.Repeat("1", 40), Token1: "0x" + strings.Repeat("2", 40), FeeNumerator: 3, FeeDenominator: 1000}}
	s, err := New(cfg)
	if err != nil {
		t.Fatal(err)
	}
	defer s.Close()
	if err = s.Start(context.Background()); err != nil {
		t.Fatal(err)
	}
	before, ready := s.stores[chain.Key].Current()
	if !ready {
		t.Fatal("controlled baseline not ready")
	}
	response := httptest.NewRecorder()
	s.Handler().ServeHTTP(response, httptest.NewRequest("GET", "/internal/chains/evm:1/source-evidence", nil))
	var metadata map[string]any
	_ = json.Unmarshal(response.Body.Bytes(), &metadata)
	if metadata["block_integrity_profile"] != chain.BlockIntegrity || metadata["consensus_verified"] != false {
		t.Errorf("missing policy/trust metadata: %s", response.Body.String())
	}
	tamper.Store(true)
	s.Tick(context.Background())
	if _, ready = s.stores[chain.Key].Current(); ready {
		t.Fatal("tampered online block left scope ready")
	}
	if s.stores[chain.Key].DurableRef().Block.Hash != before.Reference.Block.Hash {
		t.Fatal("tampered block became committed")
	}
	letters, err := s.rawSources[chain.Key].archive.DeadLetters()
	if err != nil || len(letters) != 1 {
		t.Fatal("missing durable quarantine", err, len(letters))
	}
}
