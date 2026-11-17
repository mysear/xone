package service

import (
	"context"
	"encoding/json"
	"io"
	"net/http"
	"net/http/httptest"
	"strings"
	"sync"
	"testing"
	"time"
	"xone/chain-parser-go/internal/checkpoint"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/journal"
)

type checkpointObjects struct {
	mu                                     sync.Mutex
	objects                                map[string][]byte
	fail, hold                             bool
	entered                                chan struct{}
	active, maxActive, cancelled, requests int
}

func checkpointWorkerService(t *testing.T, interval uint64) (*Service, *checkpointObjects) {
	return checkpointWorkerServiceContext(t, interval, context.Background())
}
func checkpointWorkerServiceContext(t *testing.T, interval uint64, parent context.Context) (*Service, *checkpointObjects) {
	t.Helper()
	objects := &checkpointObjects{objects: map[string][]byte{}, entered: make(chan struct{}, 32)}
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Header.Get("Authorization") != "Bearer worker-object-token-20261117" {
			w.WriteHeader(401)
			return
		}
		objects.mu.Lock()
		objects.active++
		objects.requests++
		if objects.active > objects.maxActive {
			objects.maxActive = objects.active
		}
		hold, fail := objects.hold, objects.fail
		objects.mu.Unlock()
		defer func() { objects.mu.Lock(); objects.active--; objects.mu.Unlock() }()
		if hold {
			select {
			case objects.entered <- struct{}{}:
			default:
			}
			<-r.Context().Done()
			objects.mu.Lock()
			objects.cancelled++
			objects.mu.Unlock()
			return
		}
		if fail {
			w.WriteHeader(503)
			return
		}
		objects.mu.Lock()
		defer objects.mu.Unlock()
		switch r.Method {
		case "GET":
			raw, ok := objects.objects[r.URL.Path]
			if !ok {
				w.WriteHeader(404)
				return
			}
			w.Write(raw)
		case "PUT":
			if r.Header.Get("If-None-Match") != "*" {
				t.Error("nonconditional upload")
				w.WriteHeader(400)
				return
			}
			if _, ok := objects.objects[r.URL.Path]; ok {
				w.WriteHeader(412)
				return
			}
			raw, e := io.ReadAll(r.Body)
			if e != nil {
				w.WriteHeader(400)
				return
			}
			objects.objects[r.URL.Path] = raw
			w.WriteHeader(201)
		default:
			w.WriteHeader(405)
		}
	}))
	t.Cleanup(server.Close)
	cfg, e := config.Load("../../config/config-test.yaml", "chain-parser-go")
	if e != nil {
		t.Fatal(e)
	}
	cfg.Config.State.Path = t.TempDir()
	cfg.Config.Journal.CheckpointStore = &config.CheckpointStore{BaseURL: server.URL, AllowPlaintext: true, PublishIntervalMS: interval}
	cfg.Config.Journal.SourceAuthTokens = map[string]string{server.URL: "worker-object-token-20261117"}
	cfg.Token = "worker-internal-api-token-20261117"
	s, e := NewWithSource(cfg, testSnapshotSource(t))
	if e != nil {
		t.Fatal(e)
	}
	t.Cleanup(s.Close)
	if e = s.Start(parent); e != nil {
		t.Fatal(e)
	}
	return s, objects
}
func workerWait(t *testing.T, condition func() bool) {
	t.Helper()
	deadline := time.NewTimer(4 * time.Second)
	defer deadline.Stop()
	ticker := time.NewTicker(10 * time.Millisecond)
	defer ticker.Stop()
	for {
		if condition() {
			return
		}
		select {
		case <-deadline.C:
			t.Fatal("checkpoint worker condition timed out")
		case <-ticker.C:
		}
	}
}
func workerBootstrap(t *testing.T, s *Service, offset int64) journal.Bootstrap {
	t.Helper()
	store := s.stores["evm:31337"]
	current, ok := store.Current()
	if !ok {
		t.Fatal("baseline absent")
	}
	current.Reference.OnlineGeneration++
	current.Reference.JournalSeq++
	if e := store.Prepare(journal.Unit{Kind: "ONLINE_BASELINE_COMMIT", Snapshot: current}); e != nil {
		t.Fatal(e)
	}
	cursor := journal.Cursor{Topic: "test.worker-checkpoint", Partition: 0, Offset: offset}
	if e := store.ApplyPrepared(cursor); e != nil {
		t.Fatal(e)
	}
	return journal.Bootstrap{Snapshot: current, Cursor: cursor}
}
func workerRequest(s *Service, ctx context.Context, method, path string) *httptest.ResponseRecorder {
	request := httptest.NewRequest(method, path, nil).WithContext(ctx)
	request.Header.Set("Authorization", "Bearer "+s.cfg.Token)
	response := httptest.NewRecorder()
	s.Handler().ServeHTTP(response, request)
	return response
}
func TestCheckpointWorkerPublishesWithoutManualPOSTAndRecoversFailedUpload(t *testing.T) {
	s, objects := checkpointWorkerService(t, 100)
	workerWait(t, func() bool { return s.checkpointCycles.Load() > 0 })
	objects.mu.Lock()
	if objects.requests != 0 {
		t.Fatal("worker fabricated a cursor")
	}
	objects.fail = true
	objects.mu.Unlock()
	boot := workerBootstrap(t, s, 9007199254741013)
	workerWait(t, func() bool { return s.checkpointErrors.Load() > 0 })
	if w := workerRequest(s, context.Background(), "GET", "/internal/chains/evm:31337/checkpoint-catalog"); w.Code != 503 {
		t.Fatal(w.Code, w.Body.String())
	}
	objects.mu.Lock()
	objects.fail = false
	objects.mu.Unlock()
	var publication checkpoint.RemotePublication
	workerWait(t, func() bool {
		w := workerRequest(s, context.Background(), "GET", "/internal/chains/evm:31337/checkpoint-catalog")
		return w.Code == 200 && json.Unmarshal(w.Body.Bytes(), &publication) == nil
	})
	if publication.Reference.Reference != boot.Snapshot.Reference || publication.Reference.Cursor.NextOffset != "9007199254741013" {
		t.Fatal(publication)
	}
	current, e := s.stores["evm:31337"].Bootstrap()
	if e != nil || current.Cursor != boot.Cursor || current.Snapshot.Reference != boot.Snapshot.Reference {
		t.Fatal("checkpoint worker changed owner state or committed cursor", e)
	}
	previous := publication.ManifestSHA256
	boot = workerBootstrap(t, s, 9007199254741021)
	workerWait(t, func() bool {
		w := workerRequest(s, context.Background(), "GET", "/internal/chains/evm:31337/checkpoint-catalog")
		return w.Code == 200 && json.Unmarshal(w.Body.Bytes(), &publication) == nil && publication.ManifestSHA256 != previous
	})
	if publication.Reference.Reference != boot.Snapshot.Reference {
		t.Fatal("worker did not publish updated generation")
	}
	status := workerRequest(s, context.Background(), "GET", "/internal/checkpoints/status")
	if status.Code != 200 || !strings.Contains(status.Body.String(), `"enabled":true`) {
		t.Fatal(status.Code, status.Body.String())
	}
	s.Close()
	if s.checkpointActive.Load() {
		t.Fatal("worker survived Close")
	}
	objects.mu.Lock()
	before := objects.requests
	max := objects.maxActive
	objects.mu.Unlock()
	time.Sleep(150 * time.Millisecond)
	objects.mu.Lock()
	defer objects.mu.Unlock()
	if objects.requests != before || max != 1 {
		t.Fatal("worker queued cycles or parallelized uploads", before, objects.requests, max)
	}
}
func TestCheckpointWorkerGateCancellationAndCloseInterruptInflightUpload(t *testing.T) {
	s, objects := checkpointWorkerService(t, 100)
	objects.mu.Lock()
	objects.hold = true
	objects.mu.Unlock()
	workerBootstrap(t, s, 9007199254741017)
	select {
	case <-objects.entered:
	case <-time.After(4 * time.Second):
		t.Fatal("worker did not reach HTTP store")
	}
	// Wait over two intervals: a busy cycle cannot queue another HTTP request.
	time.Sleep(220 * time.Millisecond)
	objects.mu.Lock()
	if objects.requests != 1 || objects.maxActive != 1 {
		t.Fatal(objects.requests, objects.maxActive)
	}
	objects.mu.Unlock()
	ctx, cancel := context.WithTimeout(context.Background(), 40*time.Millisecond)
	defer cancel()
	started := time.Now()
	w := workerRequest(s, ctx, "POST", "/internal/checkpoints")
	if w.Code != 503 || !strings.Contains(w.Body.String(), "CHECKPOINT_CANCELLED") || time.Since(started) > time.Second {
		t.Fatal(w.Code, w.Body.String())
	}
	closed := make(chan struct{})
	go func() { s.Close(); close(closed) }()
	select {
	case <-closed:
	case <-time.After(time.Second):
		t.Fatal("Close did not cancel inflight worker")
	}
	workerWait(t, func() bool {
		objects.mu.Lock()
		defer objects.mu.Unlock()
		return objects.cancelled == 1 && objects.active == 0
	})
	if s.checkpointActive.Load() {
		t.Fatal("worker not joined")
	}
	if w = workerRequest(s, context.Background(), "GET", "/internal/chains/evm:31337/checkpoint-catalog"); w.Code != 503 {
		t.Fatal("closed owner advertised a checkpoint")
	}
}
func TestCheckpointCloseAlsoCancelsManualUploadAndDisabledWorkerStaysIdle(t *testing.T) {
	s, objects := checkpointWorkerService(t, 0)
	workerBootstrap(t, s, 9007199254741023)
	if s.checkpointActive.Load() {
		t.Fatal("manual mode started a worker")
	}
	objects.mu.Lock()
	objects.hold = true
	objects.mu.Unlock()
	response := make(chan *httptest.ResponseRecorder, 1)
	go func() { response <- workerRequest(s, context.Background(), "POST", "/internal/checkpoints") }()
	select {
	case <-objects.entered:
	case <-time.After(4 * time.Second):
		t.Fatal("manual upload not started")
	}
	closed := make(chan struct{})
	go func() { s.Close(); close(closed) }()
	select {
	case <-closed:
	case <-time.After(time.Second):
		t.Fatal("Close did not cancel manual publication")
	}
	select {
	case w := <-response:
		if w.Code != 503 {
			t.Fatal(w.Code)
		}
	case <-time.After(time.Second):
		t.Fatal("manual publication survived Close")
	}
	workerWait(t, func() bool {
		objects.mu.Lock()
		defer objects.mu.Unlock()
		return objects.cancelled == 1 && objects.active == 0
	})
	if w := workerRequest(s, context.Background(), "POST", "/internal/checkpoints"); w.Code != 503 {
		t.Fatal("closed service accepted new publication")
	}
}

func TestCheckpointWorkerParentCancellationLeavesManualPublicationAvailable(t *testing.T) {
	parent, cancel := context.WithCancel(context.Background())
	defer cancel()
	s, objects := checkpointWorkerServiceContext(t, 100, parent)
	workerWait(t, func() bool { return s.checkpointCycles.Load() > 0 })
	cancel()
	workerWait(t, func() bool { return !s.checkpointActive.Load() })
	before := s.checkpointCycles.Load()
	workerBootstrap(t, s, 9007199254741027)
	time.Sleep(150 * time.Millisecond)
	objects.mu.Lock()
	requests := objects.requests
	objects.mu.Unlock()
	if requests != 0 || s.checkpointCycles.Load() != before {
		t.Fatal("cancelled parent left queued work", requests)
	}
	if w := workerRequest(s, context.Background(), "POST", "/internal/checkpoints"); w.Code != 200 {
		t.Fatal("worker cancellation poisoned manual publication", w.Code, w.Body.String())
	}
}
