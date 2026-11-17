package checkpoint

import (
	"encoding/hex"
	"encoding/json"
	"errors"
	"math"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"sync"
	"testing"
	"xone/chain-parser-go/internal/domain"
	"xone/chain-parser-go/internal/journal"
)

func bootstrapFixture(t *testing.T) journal.Bootstrap {
	t.Helper()
	raw, e := os.ReadFile("../../test/fixtures/evm/baseline.json")
	if e != nil {
		t.Fatal(e)
	}
	var snapshot domain.Snapshot
	if e = json.Unmarshal(raw, &snapshot); e != nil {
		t.Fatal(e)
	}
	return journal.Bootstrap{Snapshot: snapshot, Cursor: journal.Cursor{Topic: "canonical.evm-31337.v1", Partition: 0, Offset: 9007199254741009}}
}
func TestPublicBootstrapCheckpointBindsWideCursorAndIsImmutable(t *testing.T) {
	boot := bootstrapFixture(t)
	publication, e := WriteBootstrap(t.TempDir(), boot, 8<<20, nil)
	if e != nil {
		t.Fatal(e)
	}
	got, e := ReadBootstrap(publication.ManifestPath, publication.ManifestSHA256, 8<<20)
	if e != nil {
		t.Fatal(e)
	}
	if got.Cursor != boot.Cursor || got.Snapshot.Reference != boot.Snapshot.Reference {
		t.Fatal("checkpoint lost state/cursor identity")
	}
	if publication.Manifest.Cursor.NextOffset != strconv.FormatInt(boot.Cursor.Offset, 10) {
		t.Fatal("wide offset was narrowed")
	}
	info, e := os.Stat(publication.ManifestPath)
	if e != nil {
		t.Fatal(e)
	}
	root := filepath.Dir(filepath.Dir(filepath.Dir(publication.ManifestPath)))
	again, e := WriteBootstrap(root, boot, 8<<20, nil)
	if e != nil || again.ManifestPath != publication.ManifestPath || again.ManifestSHA256 != publication.ManifestSHA256 {
		t.Fatal(e, again)
	}
	after, _ := os.Stat(again.ManifestPath)
	if !os.SameFile(info, after) {
		t.Fatal("immutable manifest was replaced")
	}
	payload := filepath.Join(filepath.Dir(publication.ManifestPath), "bootstrap.pb")
	if e = os.WriteFile(payload, []byte("corrupt"), 0600); e != nil {
		t.Fatal(e)
	}
	if _, e = WriteBootstrap(root, boot, 8<<20, nil); e == nil {
		t.Fatal("immutable corruption silently overwritten")
	}
	raw, _ := os.ReadFile(payload)
	if string(raw) != "corrupt" {
		t.Fatal("existing object changed")
	}
}
func TestPublicBootstrapHalfPublicationHasNoManifestAndCanResume(t *testing.T) {
	boot := bootstrapFixture(t)
	dir := t.TempDir()
	blocked := errors.New("OWNER_REVOKED")
	if _, e := WriteBootstrap(dir, boot, 8<<20, func() error { return blocked }); !errors.Is(e, blocked) {
		t.Fatal(e)
	}
	raw, e := EncodeBootstrap(boot, 8<<20)
	if e != nil {
		t.Fatal(e)
	}
	directory := filepath.Join(dir, "public-v2", rawSHA(raw))
	if _, e = os.Stat(filepath.Join(directory, "bootstrap.pb")); e != nil {
		t.Fatal(e)
	}
	if _, e = os.Stat(filepath.Join(directory, "manifest.json")); !errors.Is(e, os.ErrNotExist) {
		t.Fatal("partial checkpoint advertised", e)
	}
	before, _ := os.Stat(filepath.Join(directory, "bootstrap.pb"))
	publication, e := WriteBootstrap(dir, boot, 8<<20, nil)
	if e != nil {
		t.Fatal(e)
	}
	after, _ := os.Stat(filepath.Join(directory, "bootstrap.pb"))
	if !os.SameFile(before, after) {
		t.Fatal("payload was replaced on resume")
	}
	if _, e = ReadBootstrap(publication.ManifestPath, publication.ManifestSHA256, 8<<20); e != nil {
		t.Fatal(e)
	}
}
func TestPublicBootstrapCorruptionIdentityCanonicalityAndLimitsFailClosed(t *testing.T) {
	boot := bootstrapFixture(t)
	publication, e := WriteBootstrap(t.TempDir(), boot, 8<<20, nil)
	if e != nil {
		t.Fatal(e)
	}
	original, _ := os.ReadFile(publication.ManifestPath)
	payload, _ := os.ReadFile(filepath.Join(filepath.Dir(publication.ManifestPath), "bootstrap.pb"))
	for mode := 0; mode < 12; mode++ {
		m := publication.Manifest
		data := append([]byte(nil), payload...)
		var manifest []byte
		switch mode {
		case 0:
			data[0] ^= 1
		case 1:
			m.Cursor.NextOffset = "01"
		case 2:
			m.Cursor.Partition = 1
		case 3:
			m.Cursor.Topic = "../escape"
		case 4:
			m.Payload = "../escape"
		case 5:
			m.Reference.JournalSeq++
		case 6:
			m.Cursor.NextOffset = strconv.FormatInt(boot.Cursor.Offset+1, 10)
		case 7:
			m.Codec = "unknown"
		case 8:
			manifest = append(append([]byte(nil), original...), '\n')
		case 9:
			manifest = append(append([]byte(nil), original[:len(original)-1]...), []byte(",\"size\":"+strconv.Itoa(len(payload))+"}")...)
		case 10:
			data = append(data, 0x98, 0x01, 0x01)
			m.SHA256 = rawSHA(data)
			m.Size = uint64(len(data)) // reserved unknown protobuf field19
		case 11:
			m.SHA256 = strings.Repeat("0", 64)
		}
		if manifest == nil {
			manifest, _ = json.Marshal(m)
		}
		if _, e = DecodeBootstrap(manifest, data, rawSHA(manifest), 8<<20); e == nil {
			t.Fatal("invalid archive accepted", mode)
		}
	}
	for _, limit := range []int{0, -1, MaxBootstrapBytes + 1, math.MaxInt} {
		if _, e = ReadBootstrap(publication.ManifestPath, publication.ManifestSHA256, limit); e == nil {
			t.Fatal("invalid limit", limit)
		}
	}
	if _, e = DecodeBootstrap(original, payload, publication.ManifestSHA256, len(payload)-1); e == nil {
		t.Fatal("payload budget ignored")
	}
	if _, e = ReadBootstrap(publication.ManifestPath, strings.Repeat("a", 64), 8<<20); e == nil {
		t.Fatal("unpinned manifest accepted")
	}
}
func TestPublicBootstrapRejectsSymlinkObjectsAndConcurrentPublicationIsIdempotent(t *testing.T) {
	boot := bootstrapFixture(t)
	dir := t.TempDir()
	var wg sync.WaitGroup
	errorsCh := make(chan error, 8)
	ids := make(chan string, 8)
	for i := 0; i < 8; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			p, e := WriteBootstrap(dir, boot, 8<<20, nil)
			if e != nil {
				errorsCh <- e
			} else {
				ids <- p.ManifestSHA256
			}
		}()
	}
	wg.Wait()
	close(errorsCh)
	close(ids)
	for e := range errorsCh {
		t.Fatal(e)
	}
	var digest string
	for id := range ids {
		if digest != "" && digest != id {
			t.Fatal("concurrent checkpoint identity changed")
		}
		digest = id
	}
	p, e := WriteBootstrap(dir, boot, 8<<20, nil)
	if e != nil {
		t.Fatal(e)
	}
	payload := filepath.Join(filepath.Dir(p.ManifestPath), "bootstrap.pb")
	bytes, _ := os.ReadFile(payload)
	outside := filepath.Join(t.TempDir(), "payload")
	if e = os.WriteFile(outside, bytes, 0600); e != nil {
		t.Fatal(e)
	}
	if e = os.Remove(payload); e != nil {
		t.Fatal(e)
	}
	if e = os.Symlink(outside, payload); e != nil {
		t.Fatal(e)
	}
	if _, e = ReadBootstrap(p.ManifestPath, p.ManifestSHA256, 8<<20); e == nil {
		t.Fatal("symlink object accepted")
	}
}
func TestPublicBootstrapExportsIndependentLanguageVectors(t *testing.T) {
	directory := os.Getenv("XONE_CHECKPOINT_VECTORS_OUTPUT")
	if directory == "" {
		directory = t.TempDir()
	}
	if e := os.MkdirAll(directory, 0700); e != nil {
		t.Fatal(e)
	}
	var vectors []map[string]any
	for i := 0; i < 3; i++ {
		boot := bootstrapFixture(t)
		if i == 1 {
			boot.Cursor.Offset = math.MaxInt64 - 1
			boot.Snapshot.Reference.JournalSeq = uint64(1)<<63 + 7
		}
		if i == 2 {
			boot.Snapshot.Pools[0].Reserve0 = domain.Amount("1606938044258990275541962092341162602522202993782792835301376")
		}
		p, e := WriteBootstrap(filepath.Join(directory, "objects"), boot, 8<<20, nil)
		if e != nil {
			t.Fatal(e)
		}
		manifest, _ := os.ReadFile(p.ManifestPath)
		payload, _ := os.ReadFile(filepath.Join(filepath.Dir(p.ManifestPath), "bootstrap.pb"))
		vectors = append(vectors, map[string]any{"manifest": string(manifest), "payload_hex": hex.EncodeToString(payload),
			"manifest_sha256": p.ManifestSHA256, "reference": boot.Snapshot.Reference, "cursor": boot.Cursor})
	}
	bytes, e := json.MarshalIndent(map[string]any{"schema_version": 1, "vectors": vectors}, "", "  ")
	if e != nil {
		t.Fatal(e)
	}
	if e = os.WriteFile(filepath.Join(directory, "vectors.json"), bytes, 0600); e != nil {
		t.Fatal(e)
	}
}

func TestPublicBootstrapNeverWritesThroughSymlinkNamespaceOrObjectDirectory(t *testing.T) {
	boot := bootstrapFixture(t)
	raw, e := EncodeBootstrap(boot, 8<<20)
	if e != nil {
		t.Fatal(e)
	}
	for _, namespace := range []bool{true, false} {
		root := t.TempDir()
		outside := t.TempDir()
		link := filepath.Join(root, "public-v2")
		if !namespace {
			if e = os.Mkdir(link, 0700); e != nil {
				t.Fatal(e)
			}
			link = filepath.Join(link, rawSHA(raw))
		}
		if e = os.Symlink(outside, link); e != nil {
			t.Fatal(e)
		}
		if _, e = WriteBootstrap(root, boot, 8<<20, nil); e == nil {
			t.Fatal("symlink publication directory accepted")
		}
		files, e := os.ReadDir(outside)
		if e != nil || len(files) != 0 {
			t.Fatal("wrote outside checkpoint root", e, len(files))
		}
	}
}

func TestPublicBootstrapChecksManifestPinBeforePayloadIO(t *testing.T) {
	publication, err := WriteBootstrap(t.TempDir(), bootstrapFixture(t), 8<<20, nil)
	if err != nil {
		t.Fatal(err)
	}
	payload := filepath.Join(filepath.Dir(publication.ManifestPath), "bootstrap.pb")
	if err = os.Remove(payload); err != nil {
		t.Fatal(err)
	}
	if _, err = ReadBootstrap(publication.ManifestPath, strings.Repeat("a", 64), 8<<20); err == nil || err.Error() != "CHECKPOINT_MANIFEST_CHECKSUM" {
		t.Fatal("payload I/O preceded manifest authentication", err)
	}
	if _, err = ReadBootstrap(publication.ManifestPath, publication.ManifestSHA256, 8<<20); !errors.Is(err, os.ErrNotExist) {
		t.Fatal(err)
	}
}
func TestPublicBootstrapPostPublicationRevocationReturnsNoQualification(t *testing.T) {
	root := t.TempDir()
	calls := 0
	denied := errors.New("OWNER_REVOKED_AFTER_PUBLICATION")
	publication, err := WriteBootstrap(root, bootstrapFixture(t), 8<<20, func() error {
		calls++
		if calls == 2 {
			return denied
		}
		return nil
	})
	if !errors.Is(err, denied) || calls != 2 || publication.ManifestPath != "" {
		t.Fatal("revoked publication qualified", publication, err, calls)
	}
	// Physical immutable objects may survive revocation; a digest never grants authority.
	files, err := filepath.Glob(filepath.Join(root, "public-v2", "*", "manifest.json"))
	if err != nil || len(files) != 1 {
		t.Fatal(files, err)
	}
}

func TestCurrentRustCheckpointVectorsDecodeIndependentlyInGo(t *testing.T) {
	directory := os.Getenv("XONE_RUST_CHECKPOINT_VECTORS_OUTPUT")
	if directory == "" {
		t.Skip("requires current validation run Rust checkpoint exports")
	}
	raw, e := os.ReadFile(filepath.Join(directory, "vectors.json"))
	if e != nil {
		t.Fatal(e)
	}
	var document struct {
		Vectors []struct {
			Manifest   string             `json:"manifest"`
			PayloadHex string             `json:"payload_hex"`
			Pin        string             `json:"manifest_sha256"`
			Reference  domain.SnapshotRef `json:"reference"`
			Cursor     journal.Cursor     `json:"cursor"`
		} `json:"vectors"`
	}
	if e = json.Unmarshal(raw, &document); e != nil {
		t.Fatal(e)
	}
	if len(document.Vectors) != 3 {
		t.Fatal("incomplete Rust vectors")
	}
	for i, v := range document.Vectors {
		payload, e := hex.DecodeString(v.PayloadHex)
		if e != nil {
			t.Fatal(e)
		}
		boot, e := DecodeBootstrap([]byte(v.Manifest), payload, v.Pin, 8<<20)
		if e != nil {
			t.Fatal(i, e)
		}
		if boot.Snapshot.Reference != v.Reference || boot.Cursor != v.Cursor {
			t.Fatal("Rust/Go state cursor mismatch", i)
		}
		if i == 1 && (boot.Cursor.Offset != math.MaxInt64-1 || boot.Snapshot.Reference.JournalSeq != uint64(1)<<63+7) {
			t.Fatal("wide integer narrowed")
		}
		if i == 2 && boot.Snapshot.Pools[0].Reserve0 != domain.Amount("1606938044258990275541962092341162602522202993782792835301376") {
			t.Fatal("amount narrowed")
		}
	}
}
