package service

import (
	"bytes"
	"context"
	"encoding/json"
	"net/http/httptest"
	"testing"
	"time"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/domain"
)

func TestEnsurePoolScopeUsesCurrentCompleteGenerationAndRejectsInvalidScopes(t *testing.T) {
	cfg, e := config.Load("../../config/config-test.yaml", "chain-parser-go")
	if e != nil {
		t.Fatal(e)
	}
	cfg.Config.State.Path = t.TempDir()
	cfg.Token = "unit-test-explicit-api-token-12345"
	base := testSnapshotSource(t)
	source := func(ctx context.Context, ch config.Chain) (domain.Snapshot, error) {
		s, e := base(ctx, ch)
		s.Reference.Block.TimestampMS = uint64(time.Now().UnixMilli())
		return s, e
	}
	s, e := NewWithSource(cfg, source)
	if e != nil {
		t.Fatal(e)
	}
	defer s.Close()
	if e = s.Start(context.Background()); e != nil {
		t.Fatal(e)
	}
	current, ok := s.stores[cfg.Config.Chains[0].Key].Current()
	if !ok {
		t.Fatal("missing baseline")
	}
	id := current.Pools[0].PoolID
	// Inject the fixture catalog after config validation; production catalogs come from validated config.
	cfg.Config.Chains[0].Pools = []config.PoolConfig{{ID: id}}
	request := func(body string, token bool) int {
		req := httptest.NewRequest("POST", "/internal/chains/"+cfg.Config.Chains[0].Key+"/ensure-pool-scope", bytes.NewBufferString(body))
		if token {
			req.Header.Set("Authorization", "Bearer "+cfg.Token)
		}
		w := httptest.NewRecorder()
		s.Handler().ServeHTTP(w, req)
		return w.Code
	}
	body := func(ids []string, generation uint64) string {
		b, _ := json.Marshal(map[string]any{"pool_ids": ids, "expected_online_generation": generation})
		return string(b)
	}
	good := body([]string{id}, current.Reference.OnlineGeneration)
	if c := request(good, false); c != 401 {
		t.Fatal("unauthorized", c)
	}
	for i := 0; i < 2; i++ {
		if c := request(good, true); c != 200 {
			t.Fatal("current scope", c)
		}
	}
	for _, v := range []struct {
		body string
		code int
	}{{body([]string{"0x0000000000000000000000000000000000000001"}, current.Reference.OnlineGeneration), 404}, {body([]string{id, id}, current.Reference.OnlineGeneration), 400}, {body([]string{}, current.Reference.OnlineGeneration), 400}, {body([]string{id}, current.Reference.OnlineGeneration+1), 409}, {`{"pool_ids":[],"unknown":true}`, 400}, {good + `{}`, 400}} {
		if c := request(v.body, true); c != v.code {
			t.Fatalf("body %s expected %d got%d", v.body, v.code, c)
		}
	}
}
