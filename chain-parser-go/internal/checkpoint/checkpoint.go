package checkpoint

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"io"
	"os"
	"path/filepath"
	"xone/chain-parser-go/internal/domain"
)

type Manifest struct {
	Schema    uint32             `json:"schema_version"`
	Reference domain.SnapshotRef `json:"reference"`
	Payload   string             `json:"payload"`
	SHA256    string             `json:"sha256"`
	Size      uint64             `json:"size"`
}

func atomic(path string, b []byte) error {
	f, e := os.CreateTemp(filepath.Dir(path), ".staging-")
	if e != nil {
		return e
	}
	temp := f.Name()
	defer os.Remove(temp)
	if e = f.Chmod(0600); e != nil {
		f.Close()
		return e
	}
	if _, e = f.Write(b); e != nil {
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
	if e = os.Rename(temp, path); e != nil {
		return e
	}
	d, e := os.Open(filepath.Dir(path))
	if e != nil {
		return e
	}
	defer d.Close()
	return d.Sync()
}
func Write(dir string, s domain.Snapshot) (string, error) {
	if e := s.Validate(s.Reference.OwnerService, s.Reference.OwnerAssignmentVersion); e != nil {
		return "", e
	}
	b, e := json.Marshal(s)
	if e != nil {
		return "", e
	}
	h := sha256.Sum256(b)
	id := hex.EncodeToString(h[:])
	dir = filepath.Join(dir, id)
	if e = os.MkdirAll(dir, 0700); e != nil {
		return "", e
	}
	payload := "state.json"
	if e = atomic(filepath.Join(dir, payload), b); e != nil {
		return "", e
	}
	m := Manifest{1, s.Reference, payload, id, uint64(len(b))}
	b, e = json.Marshal(m)
	if e != nil {
		return "", e
	}
	path := filepath.Join(dir, "manifest.json")
	return path, atomic(path, b)
}
func Read(path string, maxBytes int64) (domain.Snapshot, error) {
	var s domain.Snapshot
	if maxBytes <= 0 {
		return s, errors.New("INVALID_LIMIT")
	}
	manifestFile, e := os.Open(path)
	if e != nil {
		return s, e
	}
	defer manifestFile.Close()
	b, e := io.ReadAll(io.LimitReader(manifestFile, 65537))
	if e != nil {
		return s, e
	}
	if len(b) > 65536 {
		return s, errors.New("MANIFEST_SIZE")
	}
	var m Manifest
	if e = json.Unmarshal(b, &m); e != nil || m.Schema != 1 || m.Size > uint64(maxBytes) || m.Payload != "state.json" {
		return s, errors.New("MANIFEST_INVALID")
	}
	f, e := os.Open(filepath.Join(filepath.Dir(path), m.Payload))
	if e != nil {
		return s, e
	}
	defer f.Close()
	b, e = io.ReadAll(io.LimitReader(f, maxBytes+1))
	if e != nil {
		return s, e
	}
	h := sha256.Sum256(b)
	if uint64(len(b)) != m.Size || hex.EncodeToString(h[:]) != m.SHA256 {
		return s, errors.New("CHECKPOINT_CHECKSUM")
	}
	if e = json.Unmarshal(b, &s); e != nil {
		return s, e
	}
	if s.Reference != m.Reference {
		return s, errors.New("CHECKPOINT_IDENTITY")
	}
	return s, s.Validate(s.Reference.OwnerService, s.Reference.OwnerAssignmentVersion)
}
