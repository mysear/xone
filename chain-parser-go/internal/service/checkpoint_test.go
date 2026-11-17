package service

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"
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
}
