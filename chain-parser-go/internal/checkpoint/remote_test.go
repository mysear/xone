package checkpoint

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
	"sync"
	"testing"
	"time"
	"xone/chain-parser-go/internal/config"
)

type objectFixture struct {
	mu           sync.Mutex
	objects      map[string][]byte
	requests     []string
	failManifest bool
	corruptRead  bool
	hook         func(http.ResponseWriter, *http.Request) bool
}

func objectStore(t *testing.T) (*objectFixture, *httptest.Server) {
	t.Helper()
	f := &objectFixture{objects: map[string][]byte{}}
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Header.Get("Authorization") != "Bearer checkpoint-test-secret" {
			w.WriteHeader(401)
			return
		}
		f.mu.Lock()
		defer f.mu.Unlock()
		f.requests = append(f.requests, r.Method+" "+r.URL.Path)
		if f.hook != nil && f.hook(w, r) {
			return
		}
		if r.Method == http.MethodPut {
			if r.Header.Get("If-None-Match") != "*" {
				t.Error("missing conditional write")
				w.WriteHeader(400)
				return
			}
			if f.failManifest && strings.HasSuffix(r.URL.Path, "manifest.json") {
				w.WriteHeader(503)
				return
			}
			if _, ok := f.objects[r.URL.Path]; ok {
				w.WriteHeader(412)
				return
			}
			raw, e := io.ReadAll(io.LimitReader(r.Body, MaxBootstrapBytes+1))
			if e != nil {
				w.WriteHeader(400)
				return
			}
			f.objects[r.URL.Path] = raw
			w.WriteHeader(201)
			return
		}
		if r.Method != http.MethodGet {
			w.WriteHeader(405)
			return
		}
		raw, ok := f.objects[r.URL.Path]
		if !ok {
			w.WriteHeader(404)
			return
		}
		if f.corruptRead {
			raw = append([]byte(nil), raw...)
			raw[0] ^= 1
		}
		w.Write(raw)
	}))
	t.Cleanup(server.Close)
	return f, server
}
func remoteFixture(t *testing.T, url string, timeout time.Duration) *Remote {
	t.Helper()
	r, e := NewRemote(config.CheckpointStore{BaseURL: url, AllowPlaintext: true}, "checkpoint-test-secret", 8<<20, timeout)
	if e != nil {
		t.Fatal(e)
	}
	t.Cleanup(r.Close)
	return r
}
func TestRemoteCheckpointPublishReadbackRoundtripAndConcurrentRetry(t *testing.T) {
	f, server := objectStore(t)
	remote := remoteFixture(t, server.URL+"/bucket", 5*time.Second)
	boot := bootstrapFixture(t)
	publication, e := WriteBootstrap(t.TempDir(), boot, 8<<20, nil)
	if e != nil {
		t.Fatal(e)
	}
	result, e := remote.Publish(context.Background(), publication, nil)
	if e != nil {
		t.Fatal(e)
	}
	if result.ManifestSHA256 != publication.ManifestSHA256 || !strings.HasSuffix(result.ManifestURL, "/manifest.json") {
		t.Fatal(result)
	}
	got, e := remote.Fetch(context.Background(), publication.Manifest.SHA256, publication.ManifestSHA256)
	if e != nil || got.Cursor != boot.Cursor || got.Snapshot.Reference != boot.Snapshot.Reference {
		t.Fatal(got.Cursor, e)
	}
	f.mu.Lock()
	requests := append([]string(nil), f.requests...)
	f.mu.Unlock()
	payloadPut, manifestPut := -1, -1
	for i, v := range requests {
		if strings.HasPrefix(v, "PUT ") {
			if strings.HasSuffix(v, "bootstrap.pb") {
				payloadPut = i
			} else {
				manifestPut = i
			}
		}
	}
	if payloadPut < 0 || manifestPut <= payloadPut+1 || !strings.HasPrefix(requests[payloadPut+1], "GET ") {
		t.Fatal("manifest preceded readback", requests)
	}
	var wg sync.WaitGroup
	for i := 0; i < 8; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			if _, e := remote.Publish(context.Background(), publication, nil); e != nil {
				t.Error(e)
			}
		}()
	}
	wg.Wait()
	f.mu.Lock()
	defer f.mu.Unlock()
	for _, v := range f.requests[len(requests):] {
		if strings.HasPrefix(v, "PUT ") {
			t.Fatal("immutable retry overwrote an object")
		}
	}
}
func TestRemoteCheckpointHalfUploadRetryConflictAndRevocation(t *testing.T) {
	f, server := objectStore(t)
	remote := remoteFixture(t, server.URL, 5*time.Second)
	p, e := WriteBootstrap(t.TempDir(), bootstrapFixture(t), 8<<20, nil)
	if e != nil {
		t.Fatal(e)
	}
	f.failManifest = true
	if result, e := remote.Publish(context.Background(), p, nil); e == nil || result.ManifestURL != "" {
		t.Fatal("half upload qualified", e)
	}
	f.mu.Lock()
	if len(f.objects) != 1 {
		t.Fatal(f.objects)
	}
	f.failManifest = false
	f.mu.Unlock()
	if _, e = remote.Publish(context.Background(), p, nil); e != nil {
		t.Fatal(e)
	}
	f.mu.Lock()
	before := len(f.requests)
	payloadKey := "/public-v2/" + p.Manifest.SHA256 + "/bootstrap.pb"
	f.objects[payloadKey] = []byte("conflict")
	f.mu.Unlock()
	if _, e = remote.Publish(context.Background(), p, nil); e == nil {
		t.Fatal("remote conflict overwritten")
	}
	f.mu.Lock()
	for _, v := range f.requests[before:] {
		if strings.HasPrefix(v, "PUT ") {
			t.Fatal(v)
		}
	}
	f.mu.Unlock()
	calls := 0
	denied := errors.New("SOURCE_REVOKED")
	// A post-manifest guard can revoke qualification even when both objects already exist.
	f.mu.Lock()
	root, _ := os.OpenRoot(filepath.Dir(p.ManifestPath))
	raw, _ := readRegular(root, "bootstrap.pb", MaxBootstrapBytes)
	root.Close()
	f.objects[payloadKey] = raw
	f.mu.Unlock()
	result, e := remote.Publish(context.Background(), p, func() error {
		calls++
		if calls == 3 {
			return denied
		}
		return nil
	})
	if !errors.Is(e, denied) || result.ManifestURL != "" {
		t.Fatal(calls, result, e)
	}
}
func TestRemoteCheckpointManifestAuthenticationPrecedesPayloadDownload(t *testing.T) {
	f, server := objectStore(t)
	remote := remoteFixture(t, server.URL, 5*time.Second)
	p, e := WriteBootstrap(t.TempDir(), bootstrapFixture(t), 8<<20, nil)
	if e != nil {
		t.Fatal(e)
	}
	if _, e = remote.Publish(context.Background(), p, nil); e != nil {
		t.Fatal(e)
	}
	f.mu.Lock()
	before := len(f.requests)
	f.mu.Unlock()
	if _, e = remote.Fetch(context.Background(), p.Manifest.SHA256, strings.Repeat("a", 64)); e == nil || e.Error() != "CHECKPOINT_MANIFEST_CHECKSUM" {
		t.Fatal(e)
	}
	f.mu.Lock()
	if len(f.requests) != before+1 || !strings.HasSuffix(f.requests[before], "manifest.json") {
		t.Fatal(f.requests[before:])
	}
	f.mu.Unlock()
	// A correctly pinned but oversized manifest cannot trigger a payload request.
	f.mu.Lock()
	key := "/public-v2/" + p.Manifest.SHA256 + "/manifest.json"
	var m BootstrapManifest
	json.Unmarshal(f.objects[key], &m)
	m.Size = MaxBootstrapBytes + 1
	changed, _ := json.Marshal(m)
	f.objects[key] = changed
	before = len(f.requests)
	f.mu.Unlock()
	if _, e = remote.Fetch(context.Background(), p.Manifest.SHA256, rawSHA(changed)); e == nil {
		t.Fatal("oversize accepted")
	}
	f.mu.Lock()
	defer f.mu.Unlock()
	if len(f.requests) != before+1 {
		t.Fatal(f.requests[before:])
	}
}
func TestRemoteCheckpointReadbackCorruptionRedirectAndStreamLimitsFailClosed(t *testing.T) {
	for _, mode := range []string{"corruption", "redirect", "declared", "stream", "encoded", "missing-payload", "wrong-pin-content"} {
		t.Run(mode, func(t *testing.T) {
			f, server := objectStore(t)
			remote := remoteFixture(t, server.URL, 5*time.Second)
			p, e := WriteBootstrap(t.TempDir(), bootstrapFixture(t), 8<<20, nil)
			if e != nil {
				t.Fatal(e)
			}
			if _, e = remote.Publish(context.Background(), p, nil); e != nil {
				t.Fatal(e)
			}
			f.mu.Lock()
			f.hook = func(w http.ResponseWriter, r *http.Request) bool {
				if !strings.HasSuffix(r.URL.Path, "bootstrap.pb") {
					return false
				}
				switch mode {
				case "corruption":
					w.Write(bytes.Repeat([]byte{1}, int(p.Manifest.Size)))
				case "redirect":
					w.Header().Set("Location", server.URL+"/unconfigured-target")
					w.WriteHeader(302)
				case "declared":
					w.Header().Set("Content-Length", "999999999")
					w.WriteHeader(200)
				case "stream":
					w.WriteHeader(200)
					w.(http.Flusher).Flush()
					w.Write(make([]byte, int(p.Manifest.Size)+1))
				case "encoded":
					w.Header().Set("Content-Encoding", "gzip")
					w.Write([]byte("fake"))
				case "missing-payload":
					w.WriteHeader(404)
				case "wrong-pin-content":
					w.Write(f.objects[r.URL.Path])
					return false
				}
				return true
			}
			f.mu.Unlock()
			pin := p.ManifestSHA256
			if mode == "wrong-pin-content" {
				pin = strings.Repeat("b", 64)
			}
			if _, e = remote.Fetch(context.Background(), p.Manifest.SHA256, pin); e == nil {
				t.Fatal("invalid remote object accepted")
			}
			f.mu.Lock()
			defer f.mu.Unlock()
			for _, r := range f.requests {
				if strings.HasSuffix(r, "unconfigured-target") {
					t.Fatal("redirect followed")
				}
			}
		})
	}
}
func TestRemoteCheckpointCancellationDeadlineAndLocalCorruptionPreventQualification(t *testing.T) {
	f, server := objectStore(t)
	remote := remoteFixture(t, server.URL, 80*time.Millisecond)
	p, e := WriteBootstrap(t.TempDir(), bootstrapFixture(t), 8<<20, nil)
	if e != nil {
		t.Fatal(e)
	}
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	if _, e = remote.Publish(ctx, p, nil); e == nil {
		t.Fatal("cancelled upload qualified")
	}
	f.mu.Lock()
	if len(f.requests) != 0 {
		t.Fatal("cancelled request reached server")
	}
	f.hook = func(w http.ResponseWriter, r *http.Request) bool { <-r.Context().Done(); return true }
	f.mu.Unlock()
	started := time.Now()
	if _, e = remote.Publish(context.Background(), p, nil); e == nil {
		t.Fatal("deadline ignored")
	}
	if time.Since(started) > 2*time.Second {
		t.Fatal("unbounded deadline")
	}
	f.mu.Lock()
	f.hook = nil
	before := len(f.requests)
	f.mu.Unlock()
	if e = os.WriteFile(filepath.Join(filepath.Dir(p.ManifestPath), "bootstrap.pb"), []byte("local-corrupt"), 0600); e != nil {
		t.Fatal(e)
	}
	if _, e = remote.Publish(context.Background(), p, nil); e == nil {
		t.Fatal("corrupt local state uploaded")
	}
	f.mu.Lock()
	defer f.mu.Unlock()
	if len(f.requests) != before {
		t.Fatal("local validation followed network")
	}
}

func TestRemoteCheckpointPutAcknowledgementRequiresActualExactObject(t *testing.T) {
	for _, mode := range []string{"conditional-race", "lost-write", "corrupt-readback"} {
		t.Run(mode, func(t *testing.T) {
			f, server := objectStore(t)
			remote := remoteFixture(t, server.URL, 5*time.Second)
			p, e := WriteBootstrap(t.TempDir(), bootstrapFixture(t), 8<<20, nil)
			if e != nil {
				t.Fatal(e)
			}
			f.mu.Lock()
			f.corruptRead = mode == "corrupt-readback"
			if mode != "corrupt-readback" {
				f.hook = func(w http.ResponseWriter, r *http.Request) bool {
					if r.Method != "PUT" {
						return false
					}
					if r.Header.Get("If-None-Match") != "*" {
						t.Error("missing condition")
					}
					raw, _ := io.ReadAll(r.Body)
					if mode == "conditional-race" {
						f.objects[r.URL.Path] = raw
						w.WriteHeader(412)
					} else {
						w.WriteHeader(201)
					}
					return true
				}
			}
			f.mu.Unlock()
			result, e := remote.Publish(context.Background(), p, nil)
			if mode == "conditional-race" {
				if e != nil || result.ManifestURL == "" {
					t.Fatal(e)
				}
				return
			}
			if e == nil || result.ManifestURL != "" {
				t.Fatal("unverified put acknowledgement qualified", e)
			}
			f.mu.Lock()
			defer f.mu.Unlock()
			for key := range f.objects {
				if strings.HasSuffix(key, "manifest.json") {
					t.Fatal("bad payload advertised")
				}
			}
		})
	}
}

func TestRemoteCheckpointCatalogRequiresReadbackMarkerExactLiveStateAndStoreIdentity(t *testing.T) {
	f, server := objectStore(t)
	remote := remoteFixture(t, server.URL+"/bucket", 5*time.Second)
	boot := bootstrapFixture(t)
	dir := t.TempDir()
	p, e := WriteBootstrap(dir, boot, 8<<20, nil)
	if e != nil {
		t.Fatal(e)
	}
	if _, e = remote.Catalog(dir, boot); e == nil {
		t.Fatal("local checkpoint qualified before upload")
	}
	f.mu.Lock()
	f.failManifest = true
	f.mu.Unlock()
	if _, e = remote.Publish(context.Background(), p, nil); e == nil {
		t.Fatal("partial upload qualified")
	}
	if _, e = remote.Catalog(dir, boot); e == nil {
		t.Fatal("half upload entered catalog")
	}
	f.mu.Lock()
	f.failManifest = false
	f.mu.Unlock()
	published, e := remote.Publish(context.Background(), p, nil)
	if e != nil {
		t.Fatal(e)
	}
	got, e := remote.Catalog(dir, boot)
	if e != nil || got.ManifestURL != published.ManifestURL || got.ManifestSHA256 != published.ManifestSHA256 {
		t.Fatal(got, e)
	}
	reopened := remoteFixture(t, server.URL+"/bucket", 5*time.Second)
	if restored, e := reopened.Catalog(dir, boot); e != nil || restored.ManifestSHA256 != published.ManifestSHA256 {
		t.Fatal("durable catalogue marker unavailable after client restart", e)
	}
	for _, mode := range []string{"seq", "cursor", "generation", "owner", "store", "content"} {
		t.Run(mode, func(t *testing.T) {
			changed := bootstrapFixture(t)
			use := remote
			switch mode {
			case "seq":
				changed.Snapshot.Reference.JournalSeq++
			case "cursor":
				changed.Cursor.Offset++
			case "generation":
				changed.Snapshot.Reference.OnlineGeneration++
			case "owner":
				changed.Snapshot.Reference.WriterEpoch++
			case "store":
				use = remoteFixture(t, server.URL+"/other-store", 5*time.Second)
			case "content":
				changed.Snapshot.HistoryGaps = append(changed.Snapshot.HistoryGaps, changed.Snapshot.HistoryGaps...)
				changed.Snapshot.Source = "OTHER_SOURCE"
			}
			if _, e := use.Catalog(dir, changed); e == nil {
				t.Fatal("catalog qualified other live state", mode)
			}
		})
	}
	if e = os.WriteFile(filepath.Join(filepath.Dir(p.ManifestPath), remote.completionName()), []byte("corrupt"), 0600); e != nil {
		t.Fatal(e)
	}
	if _, e = remote.Catalog(dir, boot); e == nil {
		t.Fatal("corrupt completion marker qualified")
	}
}
