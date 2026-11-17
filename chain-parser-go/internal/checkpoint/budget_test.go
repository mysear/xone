package checkpoint

import (
	"context"
	"encoding/hex"
	"encoding/json"
	"errors"
	"os"
	"path/filepath"
	"testing"
	"time"
	"xone/chain-parser-go/internal/config"
)

func TestCheckpointBudgetProjectsExactPublicObjectsAndAllowsIdempotentRetry(t *testing.T) {
	_, server := objectStore(t)
	remote := remoteFixture(t, server.URL, 5*time.Second)
	boot := bootstrapFixture(t)
	directory := t.TempDir()
	output := os.Getenv("XONE_CHECKPOINT_BUDGET_VECTORS_OUTPUT")
	if output != "" {
		directory = filepath.Join(output, "objects")
		if e := os.MkdirAll(directory, 0700); e != nil {
			t.Fatal(e)
		}
	}
	generous := config.CheckpointBudget{MaxBytes: 1 << 20, MaxFiles: 32, MaxEntries: 64}
	expected, e := AdmitBootstrap(directory, boot, 8<<20, server.URL, &generous, nil)
	if e != nil {
		t.Fatal(e)
	}
	if expected.Files != 4 || expected.Entries != 5 {
		t.Fatal(expected)
	}
	exact := config.CheckpointBudget{MaxBytes: expected.Bytes, MaxFiles: expected.Files, MaxEntries: expected.Entries}
	for _, mode := range []string{"bytes", "files", "entries"} {
		limited := exact
		switch mode {
		case "bytes":
			limited.MaxBytes--
		case "files":
			limited.MaxFiles--
		case "entries":
			limited.MaxEntries--
		}
		if _, e = AdmitBootstrap(directory, boot, 8<<20, server.URL, &limited, nil); e == nil {
			t.Fatal("budget did not reject prospective", mode)
		}
	}
	if entries, e := os.ReadDir(directory); e != nil || len(entries) != 0 {
		t.Fatal("admission touched disk", entries, e)
	}
	if _, e = AdmitBootstrap(directory, boot, 8<<20, server.URL, &exact, nil); e != nil {
		t.Fatal(e)
	}
	p, e := WriteBootstrap(directory, boot, 8<<20, nil)
	if e != nil {
		t.Fatal(e)
	}
	if _, e = remote.Publish(context.Background(), p, nil); e != nil {
		t.Fatal(e)
	}
	actual, e := AdmitBootstrap(directory, boot, 8<<20, server.URL, &exact, nil)
	if e != nil || actual != expected {
		t.Fatal("idempotent retry charged twice", actual, expected, e)
	}
	changed := boot
	changed.Snapshot.Reference.JournalSeq++
	changed.Snapshot.Reference.OnlineGeneration++
	changed.Cursor.Offset += 4
	if _, e = AdmitBootstrap(directory, changed, 8<<20, server.URL, &exact, nil); e == nil {
		t.Fatal("new generation bypassed capacity")
	}
	if _, e = remote.Catalog(directory, boot); e != nil {
		t.Fatal("rejection deleted previously qualified state", e)
	}
	if output != "" {
		manifest, e := os.ReadFile(p.ManifestPath)
		if e != nil {
			t.Fatal(e)
		}
		payload, e := os.ReadFile(filepath.Join(filepath.Dir(p.ManifestPath), "bootstrap.pb"))
		if e != nil {
			t.Fatal(e)
		}
		encoded, e := json.MarshalIndent(map[string]any{"manifest": string(manifest), "payload_hex": hex.EncodeToString(payload), "manifest_sha256": p.ManifestSHA256, "base_url": server.URL, "usage": expected, "budget": exact}, "", "  ")
		if e != nil {
			t.Fatal(e)
		}
		if e = os.WriteFile(filepath.Join(output, "budget-vector.json"), encoded, 0600); e != nil {
			t.Fatal(e)
		}
	}
}
func TestCheckpointBudgetBoundsScanningAndRejectsSymlinkAndDepth(t *testing.T) {
	boot := bootstrapFixture(t)
	limit := config.CheckpointBudget{MaxBytes: 1 << 20, MaxFiles: 32, MaxEntries: 32}
	for _, mode := range []string{"symlink", "namespace", "depth", "entries", "bytes", "files"} {
		t.Run(mode, func(t *testing.T) {
			directory := t.TempDir()
			namespace := filepath.Join(directory, "public-v2")
			if e := os.Mkdir(namespace, 0700); e != nil {
				t.Fatal(e)
			}
			c := limit
			switch mode {
			case "namespace":
				os.Remove(namespace)
				if e := os.Symlink(t.TempDir(), namespace); e != nil {
					t.Fatal(e)
				}
			case "symlink":
				if e := os.Symlink(t.TempDir(), filepath.Join(namespace, "link")); e != nil {
					t.Fatal(e)
				}
			case "depth":
				if e := os.MkdirAll(filepath.Join(namespace, "a", "b", "c", "d", "e", "f"), 0700); e != nil {
					t.Fatal(e)
				}
			case "entries":
				c.MaxEntries = 32
				for i := 0; i < 33; i++ {
					if e := os.Mkdir(filepath.Join(namespace, string(rune('a'+i))), 0700); e != nil {
						t.Fatal(e)
					}
				}
			case "bytes":
				c.MaxBytes = 1
				if e := os.WriteFile(filepath.Join(namespace, "unknown"), []byte("full"), 0600); e != nil {
					t.Fatal(e)
				}
			case "files":
				c.MaxFiles = 1
				for _, name := range []string{"one", "two"} {
					if e := os.WriteFile(filepath.Join(namespace, name), nil, 0600); e != nil {
						t.Fatal(e)
					}
				}
			}
			if _, e := AdmitBootstrap(directory, boot, 8<<20, "", &c, nil); e == nil {
				t.Fatal("bad namespace accepted", mode)
			}
		})
	}
	revoked := errors.New("OWNER_REVOKED")
	if _, e := AdmitBootstrap(t.TempDir(), boot, 8<<20, "", &limit, func() error { return revoked }); !errors.Is(e, revoked) {
		t.Fatal(e)
	}
}
