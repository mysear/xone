package service

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"
	"time"
	"xone/chain-parser-go/internal/ingest"
	"xone/chain-parser-go/internal/rpc"
)

func TestNormalRPCResponseIsArchivedBeforeDecodeFailureAndSurvivesReopen(t *testing.T) {
	directory := t.TempDir()
	limits := ingest.ArchiveLimits{MaxBytes: 100000, MaxRecords: 20, MaxRecordBytes: 10000}
	archive, e := ingest.OpenArchive(directory, limits)
	if e != nil {
		t.Fatal(e)
	}
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) { w.Write([]byte(`not-json`)) }))
	defer server.Close()
	client := rpc.New([]string{server.URL}, time.Second, 4096)
	tracker := newRawRecorder("evm:31337", archive)
	client.SetRecorder(tracker.capture)
	var value string
	if e = client.Call(context.Background(), "eth_chainId", []any{}, &value); e == nil {
		t.Fatal("invalid JSON accepted")
	}
	ref, ok := tracker.last()
	if !ok {
		t.Fatal("raw missing")
	}
	raw, e := archive.Read(ref)
	if e != nil {
		t.Fatal(e)
	}
	var observed struct {
		Response []byte `json:"response"`
	}
	if json.Unmarshal(raw, &observed) != nil || string(observed.Response) != "not-json" {
		t.Fatal("response not retained", string(raw))
	}
	if e = tracker.quarantine("RPC_INVALID_ENVELOPE", "chain"); e != nil {
		t.Fatal(e)
	}
	archive.Close()
	archive, e = ingest.OpenArchive(directory, limits)
	if e != nil {
		t.Fatal(e)
	}
	defer archive.Close()
	letters, e := archive.DeadLetters()
	if e != nil || len(letters) != 1 {
		t.Fatal("DLQ not durable", e, len(letters))
	}
}
