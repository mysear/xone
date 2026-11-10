package service

import (
	"context"
	"encoding/json"
	"testing"
	"xone/chain-parser-go/internal/ingest"
)

func TestRawMaintenanceLegacyRecorderDoesNotCreateOrphanPins(t *testing.T) {
	a, e := ingest.OpenArchive(t.TempDir(), ingest.ArchiveLimits{MaxBytes: 1 << 20, MaxRecords: 10, MaxRecordBytes: 4096})
	if e != nil {
		t.Fatal(e)
	}
	defer a.Close()
	recorder := newRawRecorder("evm:1", a)
	for i := 0; i < 3; i++ {
		if e := recorder.capture(context.Background(), "public", "eth_chainId", json.RawMessage(`[]`), []byte(`{"jsonrpc":"2.0","id":1,"result":"0x1"}`)); e != nil {
			t.Fatal(e)
		}
	}
	records, e := a.RawRecords()
	if e != nil || len(records) != 1 {
		t.Fatal(records, e)
	}
	if records[0].ActiveReferences != 0 {
		t.Fatal("legacy acquired pins with no consumer", records[0])
	}
}
