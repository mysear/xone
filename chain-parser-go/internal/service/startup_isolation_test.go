package service

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"net/http/httptest"
	"strings"
	"sync"
	"sync/atomic"
	"testing"
	"time"

	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/rpc"
)

type startupRPC struct {
	broken        atomic.Bool
	identityReads atomic.Int64
	stateReads    atomic.Int64
	onIdentity    func()
}

// These are ordinary RPC adapters, not SnapshotSource callbacks: startup and
// recovery exercise the same identity and state-read gates as the Go owner.
func startupRPCChain(t *testing.T, base config.Chain, id uint64, fault string) (config.Chain, *startupRPC) {
	t.Helper()
	chain := base
	chain.Key = fmt.Sprintf("evm:%d", id)
	chain.Genesis = fmt.Sprintf("0x%064x", id)
	chain.Websocket = nil
	chain.ProviderProbe = nil
	chain.DynamicCatalog = nil
	chain.RPCRetry = nil
	token0, token1 := "0x"+strings.Repeat("1", 40), "0x"+strings.Repeat("2", 40)
	chain.Pools = []config.PoolConfig{{ID: "0x" + strings.Repeat("3", 40), Protocol: "uniswap-v2", Token0: token0, Token1: token1, FeeNumerator: 3, FeeDenominator: 1000}}
	node := &startupRPC{}
	node.broken.Store(fault != "")
	timestamp := time.Now().Unix()
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		var req struct {
			Method string
			Params []json.RawMessage
		}
		if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
			t.Error(err)
			w.WriteHeader(http.StatusBadRequest)
			return
		}
		if req.Method == "eth_chainId" {
			node.identityReads.Add(1)
			if node.onIdentity != nil {
				node.onIdentity()
			}
		}
		if node.broken.Load() && fault == "unavailable" {
			w.WriteHeader(http.StatusServiceUnavailable)
			return
		}
		reply := map[string]any{"jsonrpc": "2.0", "id": 1}
		var result any
		switch req.Method {
		case "eth_chainId":
			chainID := id
			if node.broken.Load() && fault == "wrong-chain" {
				chainID++
			}
			result = fmt.Sprintf("0x%x", chainID)
		case "eth_getBlockByNumber":
			var tag string
			if len(req.Params) == 0 || json.Unmarshal(req.Params[0], &tag) != nil {
				t.Error("missing block tag")
				return
			}
			height := uint64(100)
			hash := fmt.Sprintf("0x%064x", id+1000)
			if tag == "0x0" {
				height, hash = 0, chain.Genesis
			}
			result = map[string]any{"number": fmt.Sprintf("0x%x", height), "hash": hash, "parentHash": fmt.Sprintf("0x%064x", id+999), "timestamp": fmt.Sprintf("0x%x", timestamp), "transactions": []string{}}
		case "eth_call":
			node.stateReads.Add(1)
			if node.broken.Load() && fault == "pool-state" {
				reply["error"] = map[string]any{"code": -32000, "message": "state unavailable"}
				break
			}
			var call struct{ Data string }
			if len(req.Params) == 0 || json.Unmarshal(req.Params[0], &call) != nil {
				t.Error("missing call")
				return
			}
			switch call.Data {
			case "0x0dfe1681":
				result = "0x" + strings.Repeat("0", 24) + token0[2:]
			case "0xd21220a7":
				result = "0x" + strings.Repeat("0", 24) + token1[2:]
			case "0x0902f1ac":
				result = fmt.Sprintf("0x%064x%064x%064x", 1000000, 1000000, 0)
			default:
				t.Errorf("unexpected state selector %q", call.Data)
				reply["error"] = map[string]any{"code": -32601, "message": "unexpected state selector"}
			}
		default:
			t.Errorf("unexpected RPC method %q", req.Method)
			reply["error"] = map[string]any{"code": -32601, "message": "unexpected RPC method"}
		}
		if _, failed := reply["error"]; !failed {
			reply["result"] = result
		}
		if err := json.NewEncoder(w).Encode(reply); err != nil {
			t.Error(err)
		}
	}))
	t.Cleanup(server.Close)
	chain.RPCURLs = []string{server.URL}
	return chain, node
}

func startupIsolationConfig(t *testing.T) *config.Snapshot {
	t.Helper()
	cfg, err := config.Load("../../config/config-test.yaml", "chain-parser-go")
	if err != nil {
		t.Fatal(err)
	}
	cfg.Config.State.Path = t.TempDir()
	return cfg
}

func assertStartupBaselineHTTP(t *testing.T, s *Service, key string, want int) {
	t.Helper()
	req := httptest.NewRequest(http.MethodGet, "/internal/chains/"+key+"/baseline", nil)
	if s.cfg.Token != "" {
		req.Header.Set("Authorization", "Bearer "+s.cfg.Token)
	}
	response := httptest.NewRecorder()
	s.Handler().ServeHTTP(response, req)
	if response.Code != want {
		t.Fatalf("%s baseline HTTP=%d want=%d: %s", key, response.Code, want, response.Body.String())
	}
}

func TestStartupChainFailureIsolationAndRPCRecovery(t *testing.T) {
	for _, fault := range []string{"wrong-chain", "unavailable", "pool-state"} {
		for _, badFirst := range []bool{true, false} {
			t.Run(fmt.Sprintf("%s/bad-first=%v", fault, badFirst), func(t *testing.T) {
				cfg := startupIsolationConfig(t)
				bad, badNode := startupRPCChain(t, cfg.Config.Chains[0], 31337, fault)
				good, _ := startupRPCChain(t, cfg.Config.Chains[0], 31338, "")
				cfg.Config.Chains = []config.Chain{bad, good}
				if !badFirst {
					cfg.Config.Chains = []config.Chain{good, bad}
				}
				s, err := New(cfg)
				if err != nil {
					t.Fatal(err)
				}
				t.Cleanup(s.Close)
				if err = s.Start(context.Background()); err != nil {
					t.Fatal("independent healthy chain failed to start", err)
				}
				assertStartupBaselineHTTP(t, s, bad.Key, http.StatusServiceUnavailable)
				assertStartupBaselineHTTP(t, s, good.Key, http.StatusOK)
				before, ready := s.stores[good.Key].Current()
				if !ready {
					t.Fatal("healthy chain lacks baseline")
				}
				reads := badNode.identityReads.Load()
				s.Tick(context.Background())
				if badNode.identityReads.Load() <= reads {
					t.Fatal("failed startup identity was not retried by Tick")
				}
				assertStartupBaselineHTTP(t, s, bad.Key, http.StatusServiceUnavailable)
				if fault != "pool-state" && badNode.stateReads.Load() != 0 {
					t.Fatal("unverified source reached pool state collection")
				}
				reads = badNode.identityReads.Load()
				badNode.broken.Store(false)
				s.Tick(context.Background())
				if badNode.identityReads.Load() <= reads {
					t.Fatal("repaired source published without an identity retry")
				}
				assertStartupBaselineHTTP(t, s, bad.Key, http.StatusOK)
				assertStartupBaselineHTTP(t, s, good.Key, http.StatusOK)
				after, ready := s.stores[good.Key].Current()
				if !ready || after.Reference != before.Reference {
					t.Fatal("another chain's recovery changed the healthy generation", before.Reference, after.Reference)
				}
				recovered, ready := s.stores[bad.Key].Current()
				if !ready || recovered.Reference.OnlineGeneration != 1 || recovered.Reference.Block.ChainKey != bad.Key || badNode.stateReads.Load() == 0 {
					t.Fatal("repaired source did not publish its own initial baseline", recovered.Reference)
				}
			})
		}
	}
}

func TestStartupAllChainFailuresReturnFirstError(t *testing.T) {
	for _, wrongFirst := range []bool{true, false} {
		t.Run(fmt.Sprintf("wrong-first=%v", wrongFirst), func(t *testing.T) {
			cfg := startupIsolationConfig(t)
			wrong, wrongNode := startupRPCChain(t, cfg.Config.Chains[0], 31337, "wrong-chain")
			down, downNode := startupRPCChain(t, cfg.Config.Chains[0], 31338, "unavailable")
			cfg.Config.Chains = []config.Chain{wrong, down}
			want := rpc.IdentityMismatch
			if !wrongFirst {
				cfg.Config.Chains = []config.Chain{down, wrong}
				want = rpc.Transient
			}
			s, err := New(cfg)
			if err != nil {
				t.Fatal(err)
			}
			t.Cleanup(s.Close)
			err = s.Start(context.Background())
			if err == nil || rpc.Classify(err) != want || s.started.Load() {
				t.Fatal("all failed startup did not return the first source failure", err, want)
			}
			if wrongNode.identityReads.Load() == 0 || downNode.identityReads.Load() == 0 {
				t.Fatal("startup stopped before attempting all configured chains")
			}
			assertStartupBaselineHTTP(t, s, wrong.Key, http.StatusServiceUnavailable)
			assertStartupBaselineHTTP(t, s, down.Key, http.StatusServiceUnavailable)
		})
	}
}

func TestStartupCancellationRevokesEarlierHealthyChain(t *testing.T) {
	cfg := startupIsolationConfig(t)
	good, _ := startupRPCChain(t, cfg.Config.Chains[0], 31337, "")
	second, secondNode := startupRPCChain(t, cfg.Config.Chains[0], 31338, "")
	cfg.Config.Chains = []config.Chain{good, second}
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	release := make(chan struct{})
	var releaseOnce sync.Once
	defer releaseOnce.Do(func() { close(release) })
	secondNode.onIdentity = func() { <-release; cancel() }
	s, err := New(cfg)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(s.Close)
	started := make(chan error, 1)
	go func() { started <- s.Start(ctx) }()
	deadline := time.After(2 * time.Second)
	for {
		if _, ready := s.stores[good.Key].Current(); ready {
			break
		}
		select {
		case <-deadline:
			t.Fatal("healthy chain did not commit before the other identity completed")
		case <-time.After(time.Millisecond):
		}
	}
	releaseOnce.Do(func() { close(release) })
	if err = <-started; err != nil && err != context.Canceled {
		t.Fatal("unexpected startup result", err)
	}
	<-ctx.Done()
	drained := make(chan struct{})
	go func() { s.actorWG.Wait(); close(drained) }()
	select {
	case <-drained:
	case <-time.After(2 * time.Second):
		t.Fatal("canceled startup lifetime did not drain its registered chains")
	}
	if s.started.Load() {
		t.Fatal("canceled source lifetime left startup ready")
	}
	if s.stores[good.Key].DurableRef().OnlineGeneration != 1 {
		t.Fatal("test did not cancel after the first chain committed")
	}
	assertStartupBaselineHTTP(t, s, good.Key, http.StatusServiceUnavailable)
	assertStartupBaselineHTTP(t, s, second.Key, http.StatusServiceUnavailable)
}
