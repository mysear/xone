package checkpoint

import (
	"bytes"
	"crypto/rand"
	"encoding/hex"
	"encoding/json"
	"errors"
	"io"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/domain"
)

const maxReclaimJournalBytes = 1 << 20
const maxReclaimFiles = 4096

type reclaimFile struct {
	Name   string `json:"name"`
	Size   uint64 `json:"size"`
	SHA256 string `json:"sha256"`
}
type reclaimObject struct {
	Manifest       BootstrapManifest `json:"manifest"`
	ManifestSHA256 string            `json:"manifest_sha256"`
	Files          []reclaimFile     `json:"files"`
	Bytes          uint64            `json:"bytes"`
	Phase          string            `json:"phase"`
}
type reclaimJournal struct {
	BaseURL         string                     `json:"base_url"`
	Schema          uint32                     `json:"schema_version"`
	StateSHA256     string                     `json:"state_sha256"`
	StoreSHA256     string                     `json:"store_sha256"`
	OperationID     string                     `json:"operation_id"`
	NamespaceSHA256 string                     `json:"namespace_sha256"`
	Reference       domain.SnapshotRef         `json:"reference"`
	Cursor          ManifestCursor             `json:"cursor"`
	Policy          config.CheckpointRetention `json:"policy"`
	StartedAtMS     int64                      `json:"started_at_ms"`
	CompletedAtMS   int64                      `json:"completed_at_ms"`
	Status          string                     `json:"status"`
	PriorObjects    uint64                     `json:"prior_objects"`
	PriorBytes      uint64                     `json:"prior_bytes"`
	Objects         []reclaimObject            `json:"objects"`
}
type ReclaimResult struct {
	ReceiptConfirmed bool   `json:"receipt_confirmed"`
	Idempotent       bool   `json:"idempotent"`
	OperationID      string `json:"operation_id"`
	Status           string `json:"status"`
	Applied          bool   `json:"applied"`
	Resumed          bool   `json:"resumed"`
	CompletedObjects uint64 `json:"completed_objects"`
	CompletedBytes   uint64 `json:"completed_bytes"`
	TotalObjects     uint64 `json:"total_objects"`
	TotalBytes       uint64 `json:"total_bytes"`
	ReadBytes        uint64 `json:"budgeted_read_bytes"`
	ScannedEntries   uint64 `json:"budgeted_entries"`
}

func (state *reclaimJournal) result(resumed bool) ReclaimResult {
	r := ReclaimResult{OperationID: state.OperationID, Status: state.Status, Applied: state.Status == "COMPLETE", Resumed: resumed, TotalObjects: state.PriorObjects, TotalBytes: state.PriorBytes}
	for _, object := range state.Objects {
		if object.Phase == "DONE" {
			r.CompletedObjects++
			r.CompletedBytes += object.Bytes
		}
	}
	r.TotalObjects += r.CompletedObjects
	r.TotalBytes += r.CompletedBytes
	return r
}
func reclaimName(name string) bool {
	return name == "manifest.json" || name == "bootstrap.pb" || strings.HasPrefix(name, "remote-") && strings.HasSuffix(name, ".json") && digest(strings.TrimSuffix(strings.TrimPrefix(name, "remote-"), ".json"))
}
func validateReclaimState(state *reclaimJournal) error {
	fail := errors.New("CHECKPOINT_RECLAIM_JOURNAL_INVALID")
	if (&config.CheckpointStore{BaseURL: state.BaseURL, AllowPlaintext: true}).Validate() != nil {
		return fail
	}
	if state.StartedAtMS < 0 || state.Status == "PREPARED" && state.CompletedAtMS != 0 || state.Status == "COMPLETE" && state.CompletedAtMS < state.StartedAtMS {
		return fail
	}
	if state.Schema != 1 || !digest(state.StateSHA256) || state.StoreSHA256 != rawSHA([]byte(state.BaseURL)) || !digest(state.OperationID) || !digest(state.NamespaceSHA256) || state.Policy.Validate() != nil || !state.Policy.ReclaimEnabled || len(state.Objects) > 64 || state.Status != "PREPARED" && state.Status != "COMPLETE" {
		return fail
	}
	pin := state.StateSHA256
	copy := *state
	copy.StateSHA256 = ""
	raw, e := json.Marshal(copy)
	if e != nil || rawSHA(raw) != pin {
		return fail
	}
	ref, e := domain.RefToProto(state.Reference)
	if e != nil {
		return fail
	}
	normalized, e := domain.RefFromProto(ref)
	if e != nil || normalized != state.Reference {
		return fail
	}
	if _, e = (BootstrapManifest{Cursor: state.Cursor}).cursor(); e != nil {
		return fail
	}
	seen := map[string]bool{}
	files := 0
	var totalBytes uint64
	for _, object := range state.Objects {
		m := object.Manifest
		raw, e := json.Marshal(m)
		if e != nil {
			return fail
		}
		if _, e = ParseBootstrapManifest(raw, object.ManifestSHA256, MaxBootstrapBytes); e != nil {
			return fail
		}
		if seen[m.SHA256] || m.Reference.Block.ChainKey != state.Reference.Block.ChainKey || m.Reference.OwnerService != state.Reference.OwnerService || m.Reference.JournalSeq > state.Reference.JournalSeq || object.Phase != "PREPARED" && object.Phase != "QUARANTINED" && object.Phase != "DONE" {
			return fail
		}
		seen[m.SHA256] = true
		if state.Status == "COMPLETE" && object.Phase != "DONE" {
			return fail
		}
		previous := ""
		names := map[string]reclaimFile{}
		var bytes uint64
		for _, file := range object.Files {
			files++
			maximum := uint64(maxManifestBytes)
			if file.Name == "bootstrap.pb" {
				maximum = uint64(MaxBootstrapBytes)
			}
			if files > maxReclaimFiles || !reclaimName(file.Name) || file.Name <= previous || !digest(file.SHA256) || file.Size > maximum || ^uint64(0)-bytes < file.Size {
				return fail
			}
			previous = file.Name
			names[file.Name] = file
			bytes += file.Size
		}
		manifest, ok := names["manifest.json"]
		if !ok || manifest.SHA256 != object.ManifestSHA256 || manifest.Size != uint64(len(raw)) {
			return fail
		}
		payload, ok := names["bootstrap.pb"]
		if !ok || payload.SHA256 != m.SHA256 || payload.Size != m.Size {
			return fail
		}
		certificate, e := json.Marshal(RemotePublication{ManifestURL: state.BaseURL + "/public-v2/" + m.SHA256 + "/manifest.json", ManifestSHA256: object.ManifestSHA256, Reference: m})
		if e != nil {
			return fail
		}
		cert, ok := names["remote-"+state.StoreSHA256+".json"]
		if !ok || cert.SHA256 != rawSHA(certificate) || cert.Size != uint64(len(certificate)) || bytes != object.Bytes || ^uint64(0)-totalBytes < bytes {
			return fail
		}
		totalBytes += bytes
	}
	if ^uint64(0)-state.PriorBytes < totalBytes || ^uint64(0)-state.PriorObjects < uint64(len(state.Objects)) {
		return fail
	}
	return nil
}

func rootedAtomic(root *os.Root, name string, raw []byte) error {
	if len(raw) > maxReclaimJournalBytes {
		return errors.New("CHECKPOINT_RECLAIM_JOURNAL_LIMIT")
	}
	if info, e := root.Lstat(name); e == nil && !info.Mode().IsRegular() {
		return errors.New("CHECKPOINT_RECLAIM_JOURNAL_FILE")
	} else if e != nil && !errors.Is(e, os.ErrNotExist) {
		return errors.New("CHECKPOINT_RECLAIM_JOURNAL_FILE")
	}
	id := make([]byte, 16)
	if _, e := rand.Read(id); e != nil {
		return errors.New("CHECKPOINT_RECLAIM_JOURNAL_WRITE")
	}
	temporary := ".reclaim-staging-" + hex.EncodeToString(id)
	file, e := root.OpenFile(temporary, os.O_WRONLY|os.O_CREATE|os.O_EXCL, 0600)
	if e != nil {
		return errors.New("CHECKPOINT_RECLAIM_JOURNAL_WRITE")
	}
	defer root.Remove(temporary)
	if _, e = file.Write(raw); e != nil {
		file.Close()
		return errors.New("CHECKPOINT_RECLAIM_JOURNAL_WRITE")
	}
	if e = file.Sync(); e != nil {
		file.Close()
		return errors.New("CHECKPOINT_RECLAIM_JOURNAL_SYNC")
	}
	if e = file.Close(); e != nil {
		return errors.New("CHECKPOINT_RECLAIM_JOURNAL_WRITE")
	}
	if e = root.Rename(temporary, name); e != nil {
		return errors.New("CHECKPOINT_RECLAIM_JOURNAL_RENAME")
	}
	return syncRoot(root)
}
func syncRoot(root *os.Root) error {
	file, e := root.Open(".")
	if e != nil {
		return errors.New("CHECKPOINT_RECLAIM_DIRECTORY")
	}
	defer file.Close()
	if e = file.Sync(); e != nil {
		return errors.New("CHECKPOINT_RECLAIM_DIRECTORY_SYNC")
	}
	return nil
}
func safeDirectory(root *os.Root, name string) error {
	if e := root.Mkdir(name, 0700); e != nil && !errors.Is(e, os.ErrExist) {
		return errors.New("CHECKPOINT_RECLAIM_DIRECTORY")
	}
	info, e := root.Lstat(name)
	if e != nil || !info.IsDir() || info.Mode()&os.ModeSymlink != 0 {
		return errors.New("CHECKPOINT_RECLAIM_DIRECTORY")
	}
	return syncRoot(root)
}
func (r *Remote) loadReclaim(dir string, limit uint64) (*reclaimJournal, uint64, error) {
	return loadReclaimJournal(dir, limit)
}
func loadReclaimJournal(dir string, limit uint64) (*reclaimJournal, uint64, error) {
	path := filepath.Join(dir, "retirements")
	info, e := os.Lstat(path)
	if errors.Is(e, os.ErrNotExist) {
		return nil, 0, nil
	}
	if e != nil || !info.IsDir() || info.Mode()&os.ModeSymlink != 0 {
		return nil, 0, errors.New("CHECKPOINT_RECLAIM_DIRECTORY")
	}
	root, e := os.OpenRoot(path)
	if e != nil {
		return nil, 0, errors.New("CHECKPOINT_RECLAIM_DIRECTORY")
	}
	defer root.Close()
	info, e = root.Lstat("state.json")
	if errors.Is(e, os.ErrNotExist) {
		return nil, 0, nil
	}
	if e != nil || !info.Mode().IsRegular() || info.Size() < 0 || uint64(info.Size()) > limit || info.Size() > maxReclaimJournalBytes {
		return nil, 0, errors.New("CHECKPOINT_RECLAIM_JOURNAL_LIMIT")
	}
	raw, e := readRegular(root, "state.json", int64(info.Size()))
	if e != nil {
		return nil, 0, errors.New("CHECKPOINT_RECLAIM_JOURNAL_FILE")
	}
	var state reclaimJournal
	decoder := json.NewDecoder(bytes.NewReader(raw))
	decoder.DisallowUnknownFields()
	if decoder.Decode(&state) != nil {
		return nil, 0, errors.New("CHECKPOINT_RECLAIM_JOURNAL_INVALID")
	}
	canonical, e := json.Marshal(state)
	if e != nil || !bytes.Equal(raw, canonical) {
		return nil, 0, errors.New("CHECKPOINT_RECLAIM_JOURNAL_INVALID")
	}
	if e = validateReclaimState(&state); e != nil {
		return nil, 0, e
	}
	return &state, uint64(len(raw)), nil
}
func (r *Remote) saveReclaim(root *os.Root, state *reclaimJournal) error {
	state.StateSHA256 = ""
	raw, e := json.Marshal(state)
	if e != nil {
		return errors.New("CHECKPOINT_RECLAIM_JOURNAL_ENCODING")
	}
	state.StateSHA256 = rawSHA(raw)
	if e = validateReclaimState(state); e != nil {
		return e
	}
	raw, e = json.Marshal(state)
	if e != nil {
		return errors.New("CHECKPOINT_RECLAIM_JOURNAL_ENCODING")
	}
	return rootedAtomic(root, "state.json", raw)
}

// ReclaimStatus is read-only, including when a previous operation is unfinished.
func (r *Remote) ReclaimStatus(dir string) (ReclaimResult, error) {
	state, _, e := r.loadReclaim(dir, maxReclaimJournalBytes)
	if e != nil {
		return ReclaimResult{}, e
	}
	if state == nil {
		return ReclaimResult{Status: "IDLE"}, nil
	}
	return state.result(false), nil
}

type reclaimBudget struct {
	verified      map[string]domain.SnapshotRef
	policy        config.CheckpointRetention
	read, entries uint64
	guard         func() error
}

func (b *reclaimBudget) check() error {
	if b.guard != nil {
		return b.guard()
	}
	return nil
}
func (b *reclaimBudget) list(root *os.Root) (map[string]os.FileInfo, error) {
	f, e := root.Open(".")
	if e != nil {
		return nil, errors.New("CHECKPOINT_RECLAIM_DIRECTORY")
	}
	defer f.Close()
	result := map[string]os.FileInfo{}
	for {
		values, e := f.ReadDir(1)
		if e != nil && e != io.EOF {
			return nil, errors.New("CHECKPOINT_RECLAIM_DIRECTORY")
		}
		if len(values) == 0 {
			return result, nil
		}
		if e = b.check(); e != nil {
			return nil, e
		}
		b.entries++
		if b.entries > b.policy.MaxEntries {
			return nil, errors.New("CHECKPOINT_RECLAIM_SCAN_BUDGET")
		}
		name := values[0].Name()
		info, e := root.Lstat(name)
		if e != nil || info.Mode()&os.ModeSymlink != 0 {
			return nil, errors.New("CHECKPOINT_RECLAIM_ENTRY")
		}
		result[name] = info
	}
}
func (b *reclaimBudget) file(root *os.Root, name string, maximum uint64) ([]byte, error) {
	if e := b.check(); e != nil {
		return nil, e
	}
	info, e := root.Lstat(name)
	if e != nil || !info.Mode().IsRegular() || info.Size() < 0 || uint64(info.Size()) > maximum || b.read > b.policy.MaxReadBytes || uint64(info.Size()) > b.policy.MaxReadBytes-b.read {
		return nil, errors.New("CHECKPOINT_RECLAIM_READ_BUDGET")
	}
	raw, e := readRegular(root, name, int64(info.Size()))
	if e != nil {
		return nil, errors.New("CHECKPOINT_RECLAIM_FILE")
	}
	if uint64(len(raw)) > b.policy.MaxReadBytes-b.read {
		return nil, errors.New("CHECKPOINT_RECLAIM_READ_BUDGET")
	}
	b.read += uint64(len(raw))
	return raw, nil
}
func sortedReclaimFiles(files map[string]os.FileInfo) []string {
	names := make([]string, 0, len(files))
	for name := range files {
		names = append(names, name)
	}
	sort.Strings(names)
	return names
}

func cleanReclaimWorkspace(ledger *os.Root, current *reclaimJournal, nextID string, budget *reclaimBudget) error {
	entries, e := budget.list(ledger)
	if e != nil {
		return e
	}
	for _, name := range sortedReclaimFiles(entries) {
		info := entries[name]
		if name == "state.json" {
			if !info.Mode().IsRegular() || info.Size() > maxReclaimJournalBytes {
				return errors.New("CHECKPOINT_RECLAIM_JOURNAL_FILE")
			}
			continue
		}
		if digest(name) && info.IsDir() {
			if current != nil && name == current.OperationID && current.Status != "COMPLETE" {
				continue
			}
			child, e := ledger.OpenRoot(name)
			if e != nil {
				return errors.New("CHECKPOINT_RECLAIM_DIRECTORY")
			}
			files, e := budget.list(child)
			child.Close()
			if e != nil {
				return e
			}
			if len(files) != 0 {
				return errors.New("CHECKPOINT_RECLAIM_ORPHAN_CONFLICT")
			}
			if name == nextID {
				continue
			}
			if e = budget.check(); e != nil {
				return e
			}
			if e = ledger.Remove(name); e != nil {
				return errors.New("CHECKPOINT_RECLAIM_DIRECTORY")
			}
			if e = syncRoot(ledger); e != nil {
				return e
			}
			continue
		}
		if strings.HasPrefix(name, ".reclaim-staging-") && len(name) == len(".reclaim-staging-")+32 && info.Mode().IsRegular() && info.Size() <= maxReclaimJournalBytes {
			id := strings.TrimPrefix(name, ".reclaim-staging-")
			decoded, decodeError := hex.DecodeString(id)
			if decodeError != nil || hex.EncodeToString(decoded) != id {
				return errors.New("CHECKPOINT_RECLAIM_ORPHAN_CONFLICT")
			}
			if e = budget.check(); e != nil {
				return e
			}
			if e = ledger.Remove(name); e != nil {
				return errors.New("CHECKPOINT_RECLAIM_JOURNAL_FILE")
			}
			if e = syncRoot(ledger); e != nil {
				return e
			}
			continue
		}
		return errors.New("CHECKPOINT_RECLAIM_ORPHAN_CONFLICT")
	}
	return budget.check()
}

func (r *Remote) confirmReclaimReceipt(dir string, state *reclaimJournal, available uint64) error {
	path := filepath.Join(dir, "retirements")
	root, e := os.OpenRoot(path)
	if e != nil {
		return errors.New("CHECKPOINT_RECLAIM_DIRECTORY")
	}
	defer root.Close()
	expected, e := json.Marshal(state)
	if e != nil || uint64(len(expected)) > available {
		return errors.New("CHECKPOINT_RECLAIM_READ_BUDGET")
	}
	raw, e := readRegular(root, "state.json", int64(len(expected)))
	if e != nil {
		return errors.New("CHECKPOINT_RECLAIM_JOURNAL_FILE")
	}
	if !bytes.Equal(raw, expected) {
		return errors.New("CHECKPOINT_RECLAIM_JOURNAL_CHANGED")
	}
	file, e := root.Open("state.json")
	if e != nil {
		return errors.New("CHECKPOINT_RECLAIM_JOURNAL_FILE")
	}
	defer file.Close()
	if e = file.Sync(); e != nil {
		return errors.New("CHECKPOINT_RECLAIM_JOURNAL_SYNC")
	}
	return syncRoot(root)
}

// HasPendingReclaim also gates later public checkpoint publication. Quarantined
// bytes are not free capacity until the one active local operation is finished.
func HasPendingReclaim(dir string) (bool, error) {
	state, _, e := loadReclaimJournal(dir, maxReclaimJournalBytes)
	if e != nil {
		return false, e
	}
	return state != nil && state.Status != "COMPLETE", nil
}
