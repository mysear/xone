package checkpoint

import (
	"encoding/json"
	"os"
	"path/filepath"
	"testing"
	"xone/chain-parser-go/internal/domain"
)

func TestImmutableCheckpointIntegrity(t *testing.T) {
	b, e := os.ReadFile("../../test/fixtures/evm/baseline.json")
	if e != nil {
		t.Fatal(e)
	}
	var s domain.Snapshot
	json.Unmarshal(b, &s)
	dir := t.TempDir()
	path, e := Write(dir, s)
	if e != nil {
		t.Fatal(e)
	}
	got, e := Read(path, 8<<20)
	if e != nil || got.Reference != s.Reference {
		t.Fatal(e)
	}
	var m Manifest
	raw, _ := os.ReadFile(path)
	json.Unmarshal(raw, &m)
	os.WriteFile(filepath.Join(filepath.Dir(path), m.Payload), []byte("{}"), 0600)
	if _, e = Read(path, 8<<20); e == nil {
		t.Fatal("checksum not enforced")
	}
}
