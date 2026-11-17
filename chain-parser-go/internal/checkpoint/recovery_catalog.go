package checkpoint

import (
	"bytes"
	"encoding/json"
	"errors"
	"os"
	"path/filepath"
	"strconv"
	"xone/chain-parser-go/internal/domain"
	"xone/chain-parser-go/internal/journal"
)

// RecoveryCatalog attests an immutable checkpoint and a complete committed head.
// The mutable local candidate is only a lookup hint, never a readiness authority.
type RecoveryCatalog struct {
	Schema        uint32             `json:"schema_version"`
	Checkpoint    RemotePublication  `json:"checkpoint"`
	HeadReference domain.SnapshotRef `json:"head_reference"`
	HeadCursor    ManifestCursor     `json:"head_cursor"`
}

func (r *Remote) candidateName() string { return "candidate-" + rawSHA([]byte(r.base)) + ".json" }
func (r *Remote) recordCandidate(objectDir string, p RemotePublication) error {
	directory := filepath.Dir(objectDir)
	path := filepath.Join(directory, r.candidateName())
	if info, e := os.Lstat(path); e == nil && !info.Mode().IsRegular() {
		return errors.New("CHECKPOINT_CANDIDATE_FILE")
	} else if e != nil && !errors.Is(e, os.ErrNotExist) {
		return e
	}
	raw, e := json.Marshal(p)
	if e != nil {
		return e
	}
	return atomic(path, raw)
}
func sameEra(a, b domain.SnapshotRef) bool {
	return a.Block.ChainKey == b.Block.ChainKey && a.SchemaVersion == b.SchemaVersion && a.OwnerService == b.OwnerService && a.OwnerAssignmentVersion == b.OwnerAssignmentVersion && a.WriterEpoch == b.WriterEpoch && a.CanonicalEpoch == b.CanonicalEpoch && a.OnlineGeneration == b.OnlineGeneration && a.ScopeHash == b.ScopeHash && a.AdapterSetHash == b.AdapterSetHash
}
func (r *Remote) RecoveryCatalog(dir string, head domain.SnapshotRef, cursor journal.Cursor, maxTail int) (RecoveryCatalog, error) {
	fail := func(e error) (RecoveryCatalog, error) { return RecoveryCatalog{}, e }
	if maxTail < 1 || maxTail > 1000000 {
		return fail(errors.New("CHECKPOINT_TAIL_LIMIT"))
	}
	encoded, e := domain.RefToProto(head)
	if e != nil {
		return fail(e)
	}
	normalized, e := domain.RefFromProto(encoded)
	if e != nil || normalized != head {
		return fail(errors.New("CHECKPOINT_HEAD_REFERENCE"))
	}
	hc := ManifestCursor{Topic: cursor.Topic, Partition: cursor.Partition, NextOffset: strconv.FormatInt(cursor.Offset, 10)}
	if _, e := (BootstrapManifest{Cursor: hc}).cursor(); e != nil {
		return fail(e)
	}
	root, e := os.OpenRoot(dir)
	if e != nil {
		return fail(e)
	}
	defer root.Close()
	namespace, e := root.OpenRoot("public-v2")
	if e != nil {
		return fail(e)
	}
	defer namespace.Close()
	hint, e := readRegular(namespace, r.candidateName(), maxManifestBytes)
	if e != nil {
		return fail(e)
	}
	var p RemotePublication
	if json.Unmarshal(hint, &p) != nil {
		return fail(errors.New("CHECKPOINT_CANDIDATE_INVALID"))
	}
	canonical, e := json.Marshal(p)
	if e != nil || !bytes.Equal(canonical, hint) {
		return fail(errors.New("CHECKPOINT_CANDIDATE_INVALID"))
	}
	manifest, e := json.Marshal(p.Reference)
	if e != nil {
		return fail(e)
	}
	m, e := ParseBootstrapManifest(manifest, p.ManifestSHA256, r.maxBytes)
	if e != nil {
		return fail(e)
	}
	expectedURL, _, e := r.objects(m.SHA256)
	if e != nil || p.ManifestURL != expectedURL {
		return fail(errors.New("CHECKPOINT_CANDIDATE_LOCATION"))
	}
	oldCursor, e := m.cursor()
	if e != nil {
		return fail(e)
	}
	if !sameEra(m.Reference, head) || m.Reference.JournalSeq > head.JournalSeq || m.Reference.Block.Number > head.Block.Number || oldCursor.Topic != cursor.Topic || oldCursor.Partition != cursor.Partition || oldCursor.Offset > cursor.Offset || cursor.Offset-oldCursor.Offset > int64(maxTail) {
		return fail(errors.New("CHECKPOINT_CANDIDATE_REANCHOR"))
	}
	if m.Reference.JournalSeq < head.JournalSeq && (oldCursor.Offset >= cursor.Offset || m.Reference.Block.Number >= head.Block.Number || head.JournalSeq-m.Reference.JournalSeq > uint64(cursor.Offset-oldCursor.Offset)) {
		return fail(errors.New("CHECKPOINT_CANDIDATE_HEAD_CONFLICT"))
	}
	if m.Reference.JournalSeq == head.JournalSeq && (m.Reference != head || oldCursor != cursor) {
		return fail(errors.New("CHECKPOINT_CANDIDATE_HEAD_CONFLICT"))
	}
	object, e := namespace.OpenRoot(m.SHA256)
	if e != nil {
		return fail(e)
	}
	defer object.Close()
	for _, v := range []struct {
		name     string
		expected []byte
	}{{"manifest.json", manifest}, {r.completionName(), canonical}} {
		raw, e := readRegular(object, v.name, maxManifestBytes)
		if e != nil || !bytes.Equal(raw, v.expected) {
			return fail(errors.New("CHECKPOINT_CANDIDATE_CONTENT"))
		}
	}
	payload, e := readRegular(object, "bootstrap.pb", int64(m.Size))
	if e != nil {
		return fail(e)
	}
	if _, e = DecodeBootstrap(manifest, payload, p.ManifestSHA256, r.maxBytes); e != nil {
		return fail(e)
	}
	return RecoveryCatalog{Schema: 1, Checkpoint: p, HeadReference: head, HeadCursor: hc}, nil
}
