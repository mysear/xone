package testfixture

import (
	"compress/gzip"
	"encoding/json"
	"io"
	"os"
	"testing"
)

// ReadGzipJSON loads a local, bounded test fixture.
func ReadGzipJSON(t testing.TB, path string, dest any) {
	t.Helper()
	source, err := os.Open(path)
	if err != nil {
		t.Fatal(err)
	}
	defer source.Close()
	compressed, err := gzip.NewReader(source)
	if err != nil {
		t.Fatal(err)
	}
	defer compressed.Close()
	const maxFixtureBytes = 16 << 20
	raw, err := io.ReadAll(io.LimitReader(compressed, maxFixtureBytes+1))
	if err != nil {
		t.Fatal(err)
	}
	if len(raw) > maxFixtureBytes {
		t.Fatalf("test fixture too large: %s", path)
	}
	if err := json.Unmarshal(raw, dest); err != nil {
		t.Fatalf("invalid test fixture %s: %v", path, err)
	}
}
