package checkpoint

import (
	"bytes"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"google.golang.org/protobuf/proto"
	"io"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"xone/chain-parser-go/internal/domain"
	"xone/chain-parser-go/internal/journal"
	pb "xone/common/protoc/golang/dexpb"
)

const BootstrapCodec = "dex.v1.OwnerBootstrap/protobuf"
const MaxBootstrapBytes = 64 << 20
const maxManifestBytes = 65536

type ManifestCursor struct {
	Topic      string `json:"topic"`
	Partition  int32  `json:"partition"`
	NextOffset string `json:"next_offset"`
}
type BootstrapManifest struct {
	Schema    uint32             `json:"schema_version"`
	Codec     string             `json:"codec"`
	Reference domain.SnapshotRef `json:"reference"`
	Cursor    ManifestCursor     `json:"cursor"`
	Payload   string             `json:"payload"`
	SHA256    string             `json:"sha256"`
	Size      uint64             `json:"size"`
}
type Publication struct {
	ManifestPath   string            `json:"manifest_path"`
	ManifestSHA256 string            `json:"manifest_sha256"`
	Manifest       BootstrapManifest `json:"manifest"`
}

func rawSHA(raw []byte) string { sum := sha256.Sum256(raw); return hex.EncodeToString(sum[:]) }
func digest(v string) bool {
	if len(v) != 64 || v == strings.Repeat("0", 64) {
		return false
	}
	for _, c := range v {
		if !(c >= '0' && c <= '9' || c >= 'a' && c <= 'f') {
			return false
		}
	}
	return true
}
func (m BootstrapManifest) cursor() (journal.Cursor, error) {
	if m.Cursor.Partition != 0 || m.Cursor.Topic == "" || len(m.Cursor.Topic) > 249 || m.Cursor.Topic == "." || m.Cursor.Topic == ".." {
		return journal.Cursor{}, errors.New("CHECKPOINT_CURSOR")
	}
	for _, c := range m.Cursor.Topic {
		if !(c >= 'a' && c <= 'z' || c >= 'A' && c <= 'Z' || c >= '0' && c <= '9' || c == '.' || c == '_' || c == '-') {
			return journal.Cursor{}, errors.New("CHECKPOINT_CURSOR")
		}
	}
	offset, e := strconv.ParseInt(m.Cursor.NextOffset, 10, 64)
	if e != nil || offset < 1 || strconv.FormatInt(offset, 10) != m.Cursor.NextOffset {
		return journal.Cursor{}, errors.New("CHECKPOINT_CURSOR")
	}
	return journal.Cursor{Topic: m.Cursor.Topic, Partition: 0, Offset: offset}, nil
}
func syncDirectory(path string) error {
	f, e := os.Open(path)
	if e != nil {
		return e
	}
	defer f.Close()
	return f.Sync()
}

// The configured root and its ancestors are operator controlled; resolve an
// existing ancestor once, then make all newly created directory entries durable.
func durableDirectory(path string) (string, error) {
	absolute, e := filepath.Abs(path)
	if e != nil {
		return "", e
	}
	ancestor := absolute
	var suffix []string
	for {
		info, err := os.Stat(ancestor)
		if err == nil {
			if !info.IsDir() {
				return "", errors.New("CHECKPOINT_DIRECTORY")
			}
			resolved, err := filepath.EvalSymlinks(ancestor)
			if err != nil {
				return "", err
			}
			for i := len(suffix) - 1; i >= 0; i-- {
				resolved = filepath.Join(resolved, suffix[i])
			}
			absolute = resolved
			break
		} else if !errors.Is(err, os.ErrNotExist) {
			return "", err
		}
		suffix = append(suffix, filepath.Base(ancestor))
		parent := filepath.Dir(ancestor)
		if parent == ancestor {
			return "", errors.New("CHECKPOINT_DIRECTORY")
		}
		ancestor = parent
	}
	var missing []string
	for current := absolute; ; current = filepath.Dir(current) {
		if _, err := os.Stat(current); err == nil {
			break
		} else if !errors.Is(err, os.ErrNotExist) {
			return "", err
		}
		missing = append(missing, current)
	}
	if e = os.MkdirAll(absolute, 0700); e != nil {
		return "", e
	}
	for i := len(missing) - 1; i >= 0; i-- {
		if e = syncDirectory(filepath.Dir(missing[i])); e != nil {
			return "", e
		}
	}
	return absolute, syncDirectory(absolute)
}
func readRegular(root *os.Root, name string, limit int64) ([]byte, error) {
	before, e := root.Lstat(name)
	if e != nil {
		return nil, e
	}
	if !before.Mode().IsRegular() || before.Size() > limit {
		return nil, errors.New("CHECKPOINT_FILE_LIMIT")
	}
	f, e := root.Open(name)
	if e != nil {
		return nil, e
	}
	defer f.Close()
	actual, e := f.Stat()
	if e != nil {
		return nil, e
	}
	if !actual.Mode().IsRegular() || !os.SameFile(before, actual) || actual.Size() > limit {
		return nil, errors.New("CHECKPOINT_FILE_IDENTITY")
	}
	raw, e := io.ReadAll(io.LimitReader(f, limit+1))
	if e != nil {
		return nil, e
	}
	if int64(len(raw)) > limit {
		return nil, errors.New("CHECKPOINT_FILE_LIMIT")
	}
	return raw, nil
}

// A hard link publishes without ever replacing a preexisting object. A retry
// accepts only the exact original bytes; it cannot repair or overwrite corruption.
func immutableFile(directory, name string, raw []byte) error {
	f, e := os.CreateTemp(directory, ".checkpoint-staging-")
	if e != nil {
		return e
	}
	temp := f.Name()
	defer os.Remove(temp)
	if e = f.Chmod(0600); e != nil {
		f.Close()
		return e
	}
	if _, e = f.Write(raw); e != nil {
		f.Close()
		return e
	}
	if e = f.Sync(); e != nil {
		f.Close()
		return e
	}
	if e = f.Close(); e != nil {
		return e
	}
	if e = os.Link(temp, filepath.Join(directory, name)); e != nil {
		if !errors.Is(e, os.ErrExist) {
			return e
		}
		root, err := os.OpenRoot(directory)
		if err != nil {
			return err
		}
		defer root.Close()
		existing, err := readRegular(root, name, int64(len(raw)))
		if err != nil || !bytes.Equal(existing, raw) {
			return errors.New("CHECKPOINT_IMMUTABLE_CONFLICT")
		}
	}
	return syncDirectory(directory)
}
func EncodeBootstrap(boot journal.Bootstrap, maxBytes int) ([]byte, error) {
	if maxBytes < 1 || maxBytes > MaxBootstrapBytes {
		return nil, errors.New("CHECKPOINT_SIZE_LIMIT")
	}
	if e := boot.Snapshot.Validate(boot.Snapshot.Reference.OwnerService, boot.Snapshot.Reference.OwnerAssignmentVersion); e != nil {
		return nil, e
	}
	raw, e := journal.EncodeBootstrap(boot)
	if e != nil {
		return nil, e
	}
	if len(raw) > maxBytes {
		return nil, errors.New("CHECKPOINT_SIZE_LIMIT")
	}
	return raw, nil
}

// WriteBootstrap publishes payload before manifest, preserving the snapshot and
// next committed log position from one reducer Bootstrap observation. The optional
// guard is run around manifest publication; no mutable "latest" pointer is written.
func WriteBootstrap(dir string, boot journal.Bootstrap, maxBytes int, guard func() error) (Publication, error) {
	raw, e := EncodeBootstrap(boot, maxBytes)
	if e != nil {
		return Publication{}, e
	}
	m := BootstrapManifest{Schema: 2, Codec: BootstrapCodec, Reference: boot.Snapshot.Reference,
		Cursor:  ManifestCursor{Topic: boot.Cursor.Topic, Partition: boot.Cursor.Partition, NextOffset: strconv.FormatInt(boot.Cursor.Offset, 10)},
		Payload: "bootstrap.pb", SHA256: rawSHA(raw), Size: uint64(len(raw))}
	if _, e = m.cursor(); e != nil {
		return Publication{}, e
	}
	directory, e := durableDirectory(dir)
	if e != nil {
		return Publication{}, e
	}
	for _, name := range []string{"public-v2", m.SHA256} {
		child := filepath.Join(directory, name)
		if e = os.Mkdir(child, 0700); e != nil && !errors.Is(e, os.ErrExist) {
			return Publication{}, e
		}
		info, err := os.Lstat(child)
		if err != nil || !info.IsDir() || info.Mode()&os.ModeSymlink != 0 {
			return Publication{}, errors.New("CHECKPOINT_DIRECTORY")
		}
		if e = syncDirectory(directory); e != nil {
			return Publication{}, e
		}
		directory = child
	}
	if e = immutableFile(directory, m.Payload, raw); e != nil {
		return Publication{}, e
	}
	if guard != nil {
		if e = guard(); e != nil {
			return Publication{}, e
		}
	}
	manifest, e := json.Marshal(m)
	if e != nil {
		return Publication{}, e
	}
	if len(manifest) > maxManifestBytes {
		return Publication{}, errors.New("CHECKPOINT_MANIFEST_LIMIT")
	}
	if e = immutableFile(directory, "manifest.json", manifest); e != nil {
		return Publication{}, e
	}
	if guard != nil {
		if e = guard(); e != nil {
			return Publication{}, e
		}
	}
	return Publication{ManifestPath: filepath.Join(directory, "manifest.json"), ManifestSHA256: rawSHA(manifest), Manifest: m}, nil
}

// ParseBootstrapManifest validates the pin and bounds before any payload I/O.
func ParseBootstrapManifest(manifest []byte, expectedManifest string, maxBytes int) (BootstrapManifest, error) {
	if maxBytes < 1 || maxBytes > MaxBootstrapBytes || !digest(expectedManifest) || len(manifest) > maxManifestBytes {
		return BootstrapManifest{}, errors.New("CHECKPOINT_SIZE_OR_PIN")
	}
	if rawSHA(manifest) != expectedManifest {
		return BootstrapManifest{}, errors.New("CHECKPOINT_MANIFEST_CHECKSUM")
	}
	var m BootstrapManifest
	decoder := json.NewDecoder(bytes.NewReader(manifest))
	decoder.DisallowUnknownFields()
	if decoder.Decode(&m) != nil {
		return BootstrapManifest{}, errors.New("CHECKPOINT_MANIFEST_INVALID")
	}
	canonical, e := json.Marshal(m)
	if e != nil || !bytes.Equal(canonical, manifest) || m.Schema != 2 || m.Codec != BootstrapCodec || m.Payload != "bootstrap.pb" || !digest(m.SHA256) || m.Size == 0 || m.Size > uint64(maxBytes) {
		return BootstrapManifest{}, errors.New("CHECKPOINT_MANIFEST_INVALID")
	}
	_, e = m.cursor()
	if e != nil {
		return BootstrapManifest{}, e
	}
	return m, nil
}

// DecodeBootstrap checks integrity only; callers enforce owner authority and freshness.
func DecodeBootstrap(manifest, payload []byte, expectedManifest string, maxBytes int) (journal.Bootstrap, error) {
	var boot journal.Bootstrap
	if maxBytes < 1 || maxBytes > MaxBootstrapBytes || !digest(expectedManifest) || len(manifest) > maxManifestBytes || len(payload) > maxBytes {
		return boot, errors.New("CHECKPOINT_SIZE_OR_PIN")
	}
	m, e := ParseBootstrapManifest(manifest, expectedManifest, maxBytes)
	if e != nil {
		return boot, e
	}
	cursor, e := m.cursor()
	if e != nil {
		return boot, e
	}
	if uint64(len(payload)) != m.Size || rawSHA(payload) != m.SHA256 {
		return boot, errors.New("CHECKPOINT_PAYLOAD_CHECKSUM")
	}
	var message pb.OwnerBootstrap
	if proto.Unmarshal(payload, &message) != nil || message.Cursor == nil {
		return boot, errors.New("CHECKPOINT_PROTOBUF")
	}
	snapshot, e := domain.SnapshotFromProto(message.Snapshot)
	if e != nil {
		return boot, e
	}
	if snapshot.Reference != m.Reference || message.Cursor.Topic != cursor.Topic || message.Cursor.Partition != cursor.Partition || message.Cursor.NextOffset != cursor.Offset {
		return boot, errors.New("CHECKPOINT_IDENTITY")
	}
	boot = journal.Bootstrap{Snapshot: snapshot, Cursor: cursor}
	again, e := EncodeBootstrap(boot, maxBytes)
	if e != nil || !bytes.Equal(again, payload) {
		return journal.Bootstrap{}, errors.New("CHECKPOINT_PROTOBUF_NONCANONICAL")
	}
	return boot, nil
}
func ReadBootstrap(path, expectedManifest string, maxBytes int) (journal.Bootstrap, error) {
	if maxBytes < 1 || maxBytes > MaxBootstrapBytes {
		return journal.Bootstrap{}, errors.New("CHECKPOINT_SIZE_LIMIT")
	}
	if filepath.Base(path) != "manifest.json" {
		return journal.Bootstrap{}, errors.New("CHECKPOINT_MANIFEST_PATH")
	}
	root, e := os.OpenRoot(filepath.Dir(path))
	if e != nil {
		return journal.Bootstrap{}, e
	}
	defer root.Close()
	manifest, e := readRegular(root, "manifest.json", maxManifestBytes)
	if e != nil {
		return journal.Bootstrap{}, e
	}
	m, e := ParseBootstrapManifest(manifest, expectedManifest, maxBytes)
	if e != nil {
		return journal.Bootstrap{}, e
	}
	payload, e := readRegular(root, "bootstrap.pb", int64(m.Size))
	if e != nil {
		return journal.Bootstrap{}, e
	}
	return DecodeBootstrap(manifest, payload, expectedManifest, maxBytes)
}
