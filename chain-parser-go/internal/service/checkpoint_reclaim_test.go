package service

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"io"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
	"xone/chain-parser-go/internal/checkpoint"
	"xone/chain-parser-go/internal/config"
)

func serviceReclaimPolicy() config.CheckpointRetention {
	return config.CheckpointRetention{ReclaimEnabled: true, PlanTTLMS: 300000, KeepLatest: 2, MinAgeMS: 3600000, MaxRemove: 2, MaxObjects: 32, MaxEntries: 256, MaxReadBytes: 1 << 20}
}
func TestNormalCheckpointReclaimHTTPRequiresOptInAndAuthThenAppliesAndResumesBoundedJournal(t *testing.T) {
	policy := serviceReclaimPolicy()

	s, objects := checkpointWorkerServiceConfigured(t, 0, context.Background(), func(cfg *config.Snapshot) {
		cfg.Config.API.AuthToken = cfg.Token
		cfg.Config.Journal.CheckpointStore.LocalRetention = &policy
	}, nil)
	server := httptest.NewServer(s.Handler())
	defer server.Close()
	client := &http.Client{Timeout: 5 * time.Second}
	call := func(method, path string, body []byte, auth bool) (int, []byte) {
		t.Helper()
		req, e := http.NewRequest(method, server.URL+path, bytes.NewReader(body))
		if e != nil {
			t.Fatal(e)
		}
		if auth {
			req.Header.Set("Authorization", "Bearer "+s.cfg.Token)
		}
		req.Header.Set("Content-Type", "application/json")
		response, e := client.Do(req)
		if e != nil {
			t.Fatal(e)
		}
		defer response.Body.Close()
		raw, e := io.ReadAll(io.LimitReader(response.Body, 1<<20))
		if e != nil {
			t.Fatal(e)
		}
		return response.StatusCode, raw
	}
	base := "/internal/chains/evm:31337/checkpoint-retention"
	if code, _ := call("POST", base+"/apply", []byte(`{}`), false); code != 401 {
		t.Fatal(code)
	}
	var publications []checkpoint.Publication
	for i := 0; i < 6; i++ {
		workerBootstrap(t, s, 9007199254741101+int64(i*4))
		code, raw := call("POST", "/internal/checkpoints", nil, true)
		if code != 200 {
			t.Fatal(code, string(raw))
		}
		var response struct {
			Public []checkpoint.Publication `json:"public_checkpoints"`
		}
		if e := json.Unmarshal(raw, &response); e != nil || len(response.Public) != 1 {
			t.Fatal(e, string(raw))
		}
		p := response.Public[0]
		old := time.Now().Add(-48 * time.Hour)
		if e := os.Chtimes(p.ManifestPath, old, old); e != nil {
			t.Fatal(e)
		}
		publications = append(publications, p)
	}
	current, e := s.stores["evm:31337"].Bootstrap()
	if e != nil {
		t.Fatal(e)
	}
	preview := func() checkpoint.RetentionPlan {
		t.Helper()
		code, raw := call("GET", "/internal/chains/evm:31337/checkpoint-retention-plan", nil, true)
		var plan checkpoint.RetentionPlan
		if code != 200 || json.Unmarshal(raw, &plan) != nil {
			t.Fatal(code, string(raw))
		}
		return plan
	}
	plan := preview()
	if plan.SelectedObjects != 2 {
		t.Fatal(plan.SelectedObjects)
	}
	encoded, e := json.Marshal(checkpoint.RetentionApplyRequest{PlanID: plan.PlanID, ObservedAtMS: plan.ObservedAtMS})
	if e != nil {
		t.Fatal(e)
	}
	for _, bad := range []string{`{}`, `[]`, `{"plan_id":"short","observed_at_ms":1}`, `{"plan_id":null,"observed_at_ms":1}`, `{"plan_id":"x","observed_at_ms":1,"observed_at_ms":2}`, `{"PLAN_ID":"x","observed_at_ms":1}`, `{"plan_id":"x","observed_at_ms":"bad"}`, string(encoded) + ` {}`, strings.Repeat(" ", 1025)} {
		code, raw := call("POST", base+"/apply", []byte(bad), true)
		if code != 400 {
			t.Fatal("malformed request accepted", bad, code, string(raw))
		}
	}
	code, raw := call("POST", base+"/apply", encoded, true)
	var applied checkpoint.ReclaimResult
	if code != 200 || json.Unmarshal(raw, &applied) != nil || !applied.Applied || applied.CompletedObjects != 2 || applied.TotalObjects != 2 || applied.Idempotent {
		t.Fatal(code, string(raw))
	}
	code, raw = call("POST", base+"/apply", encoded, true)
	var repeated checkpoint.ReclaimResult
	if code != 200 || json.Unmarshal(raw, &repeated) != nil || !repeated.Idempotent || repeated.TotalObjects != 2 {
		t.Fatal(code, string(raw))
	}
	// Stop a public-library operation through its normal source guard immediately
	// after its durable quarantine rename, then finish via the normal HTTP resume.
	plan2 := preview()
	dir := filepath.Join(s.cfg.Config.State.Path, "checkpoints", "evm-31337")
	c := s.cfg.Config.Journal.CheckpointStore
	remote, e := checkpoint.NewRemote(*c, s.cfg.Config.Journal.SourceAuthTokens[c.BaseURL], s.cfg.Config.Journal.MaxBytes, 2*time.Second)
	if e != nil {
		t.Fatal(e)
	}
	defer remote.Close()
	interrupted := errors.New("CONTROLLED_SOURCE_STOP_AFTER_QUARANTINE")
	pending, e := remote.ApplyRetention(context.Background(), dir, current, policy, checkpoint.RetentionApplyRequest{PlanID: plan2.PlanID, ObservedAtMS: plan2.ObservedAtMS}, time.Now(), func() error {
		dirs, _ := filepath.Glob(filepath.Join(dir, "retirements", plan2.PlanID, "*"))
		if len(dirs) > 0 {
			return interrupted
		}
		return nil
	})
	if !errors.Is(e, interrupted) || pending.Applied || pending.Status != "PREPARED" {
		t.Fatal(pending, e)
	}
	objects.mu.Lock()
	requestsBefore := objects.requests
	objects.mu.Unlock()
	code, raw = call("POST", "/internal/checkpoints", nil, true)
	if code != 503 || !strings.Contains(string(raw), "CHECKPOINT_RECLAIM_PENDING") {
		t.Fatal("pending quarantine bypassed publication capacity", code, string(raw))
	}
	objects.mu.Lock()
	if objects.requests != requestsBefore {
		t.Fatal("publication contacted remote despite pending retirement")
	}
	objects.mu.Unlock()

	encoded, _ = json.Marshal(checkpoint.RetentionResumeRequest{OperationID: plan2.PlanID})
	code, raw = call("POST", base+"/resume", encoded, true)
	var resumed checkpoint.ReclaimResult
	if code != 200 || json.Unmarshal(raw, &resumed) != nil || !resumed.Applied || !resumed.Resumed || resumed.TotalObjects != 4 {
		t.Fatal(code, string(raw))
	}
	code, raw = call("GET", base+"/status", nil, true)
	var status checkpoint.ReclaimResult
	if code != 200 || json.Unmarshal(raw, &status) != nil || !status.Applied || status.TotalObjects != 4 {
		t.Fatal(code, string(raw))
	}
	after, e := s.stores["evm:31337"].Bootstrap()
	if e != nil || after.Snapshot.Reference != current.Snapshot.Reference || after.Cursor != current.Cursor {
		t.Fatal("cache reclamation changed online owner state", e)
	}
	for _, p := range publications[4:] {
		if _, e = checkpoint.ReadBootstrap(p.ManifestPath, p.ManifestSHA256, 8<<20); e != nil {
			t.Fatal("latest backups were removed", e)
		}
	}
	if out := os.Getenv("XONE_CHECKPOINT_RECLAIM_OUTPUT"); out != "" {
		if e = os.MkdirAll(out, 0700); e != nil {
			t.Fatal(e)
		}
		raw, e = json.MarshalIndent(map[string]any{"first_plan": plan, "first_result": applied, "duplicate_result": repeated, "second_pending": pending, "http_resume": resumed, "http_status": status, "owner_state_unchanged": true, "latest_backups_still_decode": true, "verification_scope": "NORMAL_GO_OWNER_AND_SEPARATE_OBJECT_HTTP_EXPLICIT_RECLAIM_AND_PENDING_JOURNAL_RESUME"}, "", "  ")
		if e != nil {
			t.Fatal(e)
		}
		if e = os.WriteFile(filepath.Join(out, "normal-go-reclaim-http.json"), raw, 0600); e != nil {
			t.Fatal(e)
		}
	}
	s.Close()
	code, _ = call("POST", base+"/resume", encoded, true)
	if code != 503 {
		t.Fatal("closed owner resumed mutation", code)
	}
}
func TestNormalCheckpointReclaimPreviewConfigurationCannotSilentlyEnableDeletion(t *testing.T) {
	policy := serviceReclaimPolicy()
	policy.ReclaimEnabled = false
	policy.PlanTTLMS = 0
	s, _ := checkpointWorkerServiceConfigured(t, 0, context.Background(), func(cfg *config.Snapshot) { cfg.Config.Journal.CheckpointStore.LocalRetention = &policy }, nil)
	request := httptest.NewRequest("POST", "/internal/chains/evm:31337/checkpoint-retention/apply", strings.NewReader(`{"plan_id":"`+strings.Repeat("a", 64)+`","observed_at_ms":1}`))
	request.Header.Set("Authorization", "Bearer "+s.cfg.Token)
	response := httptest.NewRecorder()
	s.Handler().ServeHTTP(response, request)
	if response.Code != 503 || !strings.Contains(response.Body.String(), "CHECKPOINT_RECLAIM_DISABLED") {
		t.Fatal(response.Code, response.Body.String())
	}
	if _, e := os.Stat(filepath.Join(s.cfg.Config.State.Path, "checkpoints")); !errors.Is(e, os.ErrNotExist) {
		t.Fatal("disabled mutator wrote storage", e)
	}
}
