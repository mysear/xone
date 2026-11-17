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

func TestNormalCheckpointEndpointPublishesBoundPublicBootstrapWithoutInventingCursor(t *testing.T) {
	cfg, e := config.Load("../../config/config-test.yaml", "chain-parser-go")
	if e != nil {
		t.Fatal(e)
	}
	cfg.Config.State.Path = t.TempDir()
	cfg.Token = "checkpoint-authorization-test-20261117"
	service, e := NewWithSource(cfg, testSnapshotSource(t))
	if e != nil {
		t.Fatal(e)
	}
	defer service.Close()
	if e = service.Start(context.Background()); e != nil {
		t.Fatal(e)
	}
	request := func(auth bool) *httptest.ResponseRecorder {
		req := httptest.NewRequest("POST", "/internal/checkpoints", nil)
		if auth {
			req.Header.Set("Authorization", "Bearer "+cfg.Token)
		}
		w := httptest.NewRecorder()
		service.Handler().ServeHTTP(w, req)
		return w
	}
	if w := request(false); w.Code != http.StatusUnauthorized {
		t.Fatal(w.Code)
	}
	w := request(true)
	if w.Code != http.StatusOK {
		t.Fatal(w.Code, w.Body.String())
	}
	var initial struct {
		Public []checkpoint.Publication `json:"public_checkpoints"`
	}
	if e = json.Unmarshal(w.Body.Bytes(), &initial); e != nil {
		t.Fatal(e)
	}
	if len(initial.Public) != 0 {
		t.Fatal("HTTP development state fabricated a Kafka cursor")
	}
	store := service.stores["evm:31337"]
	current, ok := store.Current()
	if !ok {
		t.Fatal("baseline missing")
	}
	current.Reference.OnlineGeneration++
	current.Reference.JournalSeq++
	if e = store.Prepare(journal.Unit{Kind: "ONLINE_BASELINE_COMMIT", Snapshot: current}); e != nil {
		t.Fatal(e)
	}
	cursor := journal.Cursor{Topic: "test.checkpoint", Partition: 0, Offset: 9007199254741011}
	if e = store.ApplyPrepared(cursor); e != nil {
		t.Fatal(e)
	}
	w = request(true)
	if w.Code != http.StatusOK {
		t.Fatal(w.Code, w.Body.String())
	}
	var response struct {
		Manifests []string                 `json:"manifests"`
		Public    []checkpoint.Publication `json:"public_checkpoints"`
	}
	if e = json.Unmarshal(w.Body.Bytes(), &response); e != nil {
		t.Fatal(e)
	}
	if len(response.Manifests) != 1 || len(response.Public) != 1 {
		t.Fatal(response)
	}
	publication := response.Public[0]
	restored, e := checkpoint.ReadBootstrap(publication.ManifestPath, publication.ManifestSHA256, cfg.Config.Journal.MaxBytes)
	if e != nil || restored.Cursor != cursor || restored.Snapshot.Reference != current.Reference {
		t.Fatal("normal endpoint lost atomic state/cursor", e)
	}
	again := request(true)
	if again.Code != http.StatusOK {
		t.Fatal(again.Code, again.Body.String())
	}
	// This is the normal authenticated service handler, with a separately listening object API.
	var mu sync.Mutex
	objects := map[string][]byte{}
	unavailable := false
	objectAPI := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		mu.Lock()
		defer mu.Unlock()
		if r.Header.Get("Authorization") != "Bearer object-api-secret-20261117" {
			w.WriteHeader(401)
			return
		}
		if unavailable {
			w.WriteHeader(503)
			return
		}
		switch r.Method {
		case "GET":
			raw, ok := objects[r.URL.Path]
			if !ok {
				w.WriteHeader(404)
				return
			}
			w.Write(raw)
		case "PUT":
			if r.Header.Get("If-None-Match") != "*" {
				w.WriteHeader(400)
				return
			}
			if _, ok := objects[r.URL.Path]; ok {
				w.WriteHeader(412)
				return
			}
			raw, e := io.ReadAll(r.Body)
			if e != nil {
				w.WriteHeader(400)
				return
			}
			objects[r.URL.Path] = raw
			w.WriteHeader(201)
		default:
			w.WriteHeader(405)
		}
	}))
	defer objectAPI.Close()
	remoteConfig := &config.CheckpointStore{BaseURL: objectAPI.URL + "/checkpoint-bucket", AllowPlaintext: true}
	cfg.Config.Journal.CheckpointStore = remoteConfig
	cfg.Config.Journal.SourceAuthTokens = map[string]string{remoteConfig.BaseURL: "object-api-secret-20261117"}
	w = request(true)
	if w.Code != 200 {
		t.Fatal(w.Code, w.Body.String())
	}
	var remoteResponse struct {
		Remote []checkpoint.RemotePublication `json:"remote_checkpoints"`
	}
	if e = json.Unmarshal(w.Body.Bytes(), &remoteResponse); e != nil || len(remoteResponse.Remote) != 1 {
		t.Fatal(w.Body.String(), e)
	}
	catalog := func(auth bool) *httptest.ResponseRecorder {
		req := httptest.NewRequest("GET", "/internal/chains/evm:31337/checkpoint-catalog", nil)
		if auth {
			req.Header.Set("Authorization", "Bearer "+cfg.Token)
		}
		w := httptest.NewRecorder()
		service.Handler().ServeHTTP(w, req)
		return w
	}
	if denied := catalog(false); denied.Code != 401 {
		t.Fatal(denied.Code)
	}
	if available := catalog(true); available.Code != 200 {
		t.Fatal(available.Code, available.Body.String())
	}
	remotePublication := remoteResponse.Remote[0]
	if !strings.HasPrefix(remotePublication.ManifestURL, remoteConfig.BaseURL+"/public-v2/") || remotePublication.ManifestSHA256 != publication.ManifestSHA256 {
		t.Fatal(remotePublication)
	}
	remote, e := checkpoint.NewRemote(*remoteConfig, cfg.Config.Journal.SourceAuthTokens[remoteConfig.BaseURL], cfg.Config.Journal.MaxBytes, 5*time.Second)
	if e != nil {
		t.Fatal(e)
	}
	defer remote.Close()
	fetched, e := remote.Fetch(context.Background(), publication.Manifest.SHA256, publication.ManifestSHA256)
	if e != nil || fetched.Cursor != cursor || fetched.Snapshot.Reference != current.Reference {
		t.Fatal("normal service remote state/cursor mismatch", e)
	}
	mu.Lock()
	if len(objects) != 2 {
		t.Fatal(objects)
	}
	unavailable = true
	mu.Unlock()
	if failed := request(true); failed.Code != 503 || !strings.Contains(failed.Body.String(), "CHECKPOINT_REMOTE_PUBLICATION_FAILED") {
		t.Fatal(failed.Code, failed.Body.String())
	}

	// A successful upload of an old seq never qualifies a newer live snapshot.
	next := current
	next.Reference.JournalSeq++
	next.Reference.Block.Number++
	next.Reference.Block.ParentHash = current.Reference.Block.Hash
	next.Reference.Block.Hash = "0x" + strings.Repeat("3", 64)
	next.Reference.Block.TimestampMS++
	if e = store.Prepare(journal.Unit{Kind: "BLOCK_COMMIT", Snapshot: next}); e != nil {
		t.Fatal(e)
	}
	cursor.Offset++
	if e = store.ApplyPrepared(cursor); e != nil {
		t.Fatal(e)
	}
	if stale := catalog(true); stale.Code != 503 {
		t.Fatal("stale checkpoint catalog remained available", stale.Code, stale.Body.String())
	}

}
