package checkpoint

import (
	"bytes"
	"encoding/json"
	"errors"
	"io"
	"os"
	"path/filepath"
	"sort"
	"strconv"
	"strings"
	"time"
	"unicode/utf8"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/domain"
	"xone/chain-parser-go/internal/journal"
)

type RetentionObject struct {
	ContentCoverage string              `json:"content_coverage"`
	ObjectSHA256    string              `json:"object_sha256"`
	ManifestSHA256  string              `json:"manifest_sha256,omitempty"`
	Reference       *domain.SnapshotRef `json:"reference,omitempty"`
	Bytes           uint64              `json:"bytes"`
	Files           uint64              `json:"files"`
	Reason          string              `json:"reason"`
	Selected        bool                `json:"selected"`
}
type RetentionPlan struct {
	NamespaceDigestBasis string                     `json:"namespace_digest_basis"`
	Schema               uint32                     `json:"schema_version"`
	PlanID               string                     `json:"plan_id"`
	NamespaceSHA256      string                     `json:"namespace_sha256"`
	StoreSHA256          string                     `json:"store_sha256"`
	Reference            domain.SnapshotRef         `json:"reference"`
	Cursor               ManifestCursor             `json:"cursor"`
	Policy               config.CheckpointRetention `json:"policy"`
	ObservedAtMS         int64                      `json:"observed_at_ms"`
	ScannedEntries       uint64                     `json:"scanned_entries"`
	ReadBytes            uint64                     `json:"read_bytes"`
	SelectedBytes        uint64                     `json:"selected_bytes"`
	SelectedObjects      uint64                     `json:"selected_objects"`
	Objects              []RetentionObject          `json:"objects"`
	Applied              bool                       `json:"applied"`
}
type inventoryEntry struct {
	Path       string `json:"path"`
	Kind       string `json:"kind"`
	Size       int64  `json:"size"`
	ModifiedNS int64  `json:"modified_ns"`
	SHA256     string `json:"sha256,omitempty"`
}
type retentionObject struct {
	name             string
	files            map[string]os.FileInfo
	manifestModified time.Time
	raw              map[string][]byte
	row              RetentionObject
}

// PlanRetention is read-only. It binds observed file bytes and current owner state;
// an executor must recheck both rather than treating this plan as deletion authority.
func (r *Remote) PlanRetention(dir string, boot journal.Bootstrap, policy config.CheckpointRetention, observed time.Time, guard func() error) (RetentionPlan, error) {
	return r.planRetention(dir, boot, policy, observed, guard, 0, 0)
}
func (r *Remote) planRetention(dir string, boot journal.Bootstrap, policy config.CheckpointRetention, observed time.Time, guard func() error, spentRead, spentEntries uint64) (RetentionPlan, error) {
	var plan RetentionPlan
	fail := func(e error) (RetentionPlan, error) { return RetentionPlan{}, e }
	if e := policy.Validate(); e != nil {
		return fail(e)
	}
	if spentRead > policy.MaxReadBytes || spentEntries > policy.MaxEntries {
		return fail(errors.New("CHECKPOINT_RETENTION_BUDGET"))
	}
	availableRead := policy.MaxReadBytes - spentRead
	availableEntries := policy.MaxEntries - spentEntries
	check := func() error {
		if guard != nil {
			return guard()
		}
		return nil
	}
	if e := check(); e != nil {
		return fail(e)
	}
	payload, e := EncodeBootstrap(boot, r.maxBytes)
	if e != nil {
		return fail(e)
	}
	currentSHA := rawSHA(payload)
	protected := map[string]string{currentSHA: "CURRENT_STATE"}
	path := filepath.Join(dir, "public-v2")
	info, e := os.Lstat(path)
	if e != nil {
		return fail(e)
	}
	if !info.IsDir() || info.Mode()&os.ModeSymlink != 0 {
		return fail(errors.New("CHECKPOINT_RETENTION_LAYOUT"))
	}
	root, e := os.OpenRoot(path)
	if e != nil {
		return fail(e)
	}
	defer root.Close()
	var inventory []inventoryEntry
	entries := uint64(0)
	readBytes := uint64(0)
	read := func(root *os.Root, name string, size int64, maximum int64) ([]byte, error) {
		if e := check(); e != nil {
			return nil, e
		}
		if size < 0 || size > maximum || uint64(size) > availableRead-readBytes {
			return nil, errors.New("CHECKPOINT_RETENTION_READ_BUDGET")
		}
		raw, e := readRegular(root, name, maximum)
		if e != nil {
			return nil, e
		}
		if uint64(len(raw)) > availableRead-readBytes {
			return nil, errors.New("CHECKPOINT_RETENTION_READ_BUDGET")
		}
		readBytes += uint64(len(raw))
		return raw, nil
	}
	list := func(root *os.Root, prefix string) (map[string]os.FileInfo, error) {
		f, e := root.Open(".")
		if e != nil {
			return nil, e
		}
		defer f.Close()
		result := map[string]os.FileInfo{}
		for {
			next, e := f.ReadDir(1)
			if e != nil && e != io.EOF {
				return nil, e
			}
			if len(next) == 0 {
				return result, nil
			}
			if e = check(); e != nil {
				return nil, e
			}
			entries++
			if entries > availableEntries {
				return nil, errors.New("CHECKPOINT_RETENTION_SCAN_BUDGET")
			}
			name := next[0].Name()
			if !utf8.ValidString(name) {
				return nil, errors.New("CHECKPOINT_RETENTION_LAYOUT")
			}
			meta, e := root.Lstat(name)
			if e != nil {
				return nil, e
			}
			if meta.Mode()&os.ModeSymlink != 0 || !meta.Mode().IsRegular() && !meta.IsDir() {
				return nil, errors.New("CHECKPOINT_RETENTION_LAYOUT")
			}
			ns := meta.ModTime().UnixNano()
			if !time.Unix(0, ns).Equal(meta.ModTime()) {
				return nil, errors.New("CHECKPOINT_RETENTION_CLOCK")
			}
			kind := "file"
			if meta.IsDir() {
				kind = "directory"
			}
			inventory = append(inventory, inventoryEntry{Path: prefix + name, Kind: kind, Size: meta.Size(), ModifiedNS: ns})
			result[name] = meta
		}
	}
	top, e := list(root, "")
	if e != nil {
		return fail(e)
	}
	var objects []*retentionObject
	for name, meta := range top {
		if meta.IsDir() {
			if !digest(name) {
				return fail(errors.New("CHECKPOINT_RETENTION_LAYOUT"))
			}
			if uint64(len(objects)) >= policy.MaxObjects {
				return fail(errors.New("CHECKPOINT_RETENTION_OBJECT_BUDGET"))
			}
			child, e := root.OpenRoot(name)
			if e != nil {
				return fail(e)
			}
			files, e := list(child, name+"/")
			child.Close()
			if e != nil {
				return fail(e)
			}
			object := &retentionObject{name: name, files: files, raw: map[string][]byte{}, row: RetentionObject{ObjectSHA256: name, Reason: "INCOMPLETE", ContentCoverage: "METADATA_ONLY"}}
			for _, entry := range files {
				if !entry.Mode().IsRegular() {
					return fail(errors.New("CHECKPOINT_RETENTION_LAYOUT"))
				}
				object.row.Files++
				if entry.Size() < 0 || ^uint64(0)-object.row.Bytes < uint64(entry.Size()) {
					return fail(errors.New("CHECKPOINT_RETENTION_SIZE_OVERFLOW"))
				}
				object.row.Bytes += uint64(entry.Size())
			}
			objects = append(objects, object)
		} else if strings.HasPrefix(name, "candidate-") {
			id := strings.TrimSuffix(strings.TrimPrefix(name, "candidate-"), ".json")
			if name != "candidate-"+id+".json" || !digest(id) {
				return fail(errors.New("CHECKPOINT_RETENTION_CANDIDATE"))
			}
			raw, e := read(root, name, meta.Size(), maxManifestBytes)
			if e != nil {
				return fail(e)
			}
			var p RemotePublication
			if json.Unmarshal(raw, &p) != nil {
				return fail(errors.New("CHECKPOINT_RETENTION_CANDIDATE"))
			}
			canonical, _ := json.Marshal(p)
			encoded, _ := json.Marshal(p.Reference)
			if !bytes.Equal(raw, canonical) {
				return fail(errors.New("CHECKPOINT_RETENTION_CANDIDATE"))
			}
			if _, e = ParseBootstrapManifest(encoded, p.ManifestSHA256, r.maxBytes); e != nil {
				return fail(e)
			}
			if _, exists := protected[p.Reference.SHA256]; !exists {
				protected[p.Reference.SHA256] = "CANDIDATE_REFERENCE"
			}
			for i := range inventory {
				if inventory[i].Path == name {
					inventory[i].SHA256 = rawSHA(raw)
				}
			}
		}
	}
	// Stable order makes evidence independent of filesystem enumeration order.
	sort.Slice(objects, func(i, j int) bool { return objects[i].name < objects[j].name })
	var qualified []*retentionObject
	for _, object := range objects {
		child, e := root.OpenRoot(object.name)
		if e != nil {
			return fail(e)
		}
		known := true
		for name := range object.files {
			if name != "bootstrap.pb" && name != "manifest.json" && !(strings.HasPrefix(name, "remote-") && strings.HasSuffix(name, ".json") && digest(strings.TrimSuffix(strings.TrimPrefix(name, "remote-"), ".json"))) {
				known = false
			}
		}
		if !known {
			object.row.Reason = "UNKNOWN_FILES"
			child.Close()
			continue
		}
		for _, required := range []string{"manifest.json", "bootstrap.pb", r.completionName()} {
			if _, ok := object.files[required]; !ok {
				known = false
			}
		}
		if !known {
			child.Close()
			continue
		}
		manifest, e := read(child, "manifest.json", object.files["manifest.json"].Size(), maxManifestBytes)
		if e != nil {
			child.Close()
			return fail(e)
		}
		pin := rawSHA(manifest)
		m, e := ParseBootstrapManifest(manifest, pin, r.maxBytes)
		if e != nil || m.SHA256 != object.name {
			child.Close()
			return fail(errors.New("CHECKPOINT_RETENTION_MANIFEST"))
		}
		if m.Reference.Block.ChainKey != boot.Snapshot.Reference.Block.ChainKey || m.Reference.OwnerService != boot.Snapshot.Reference.OwnerService {
			object.row.Reason = "OTHER_OWNER"
			child.Close()
			continue
		}
		// Hash complete bytes, not metadata-only stubs, within the explicit read budget.
		for name, meta := range object.files {
			maximum := int64(maxManifestBytes)
			if name == "bootstrap.pb" {
				maximum = int64(m.Size)
			}
			raw, e := read(child, name, meta.Size(), maximum)
			if e != nil {
				child.Close()
				return fail(e)
			}
			object.raw[name] = raw
		}
		child.Close()
		if uint64(len(object.raw["bootstrap.pb"])) != m.Size || rawSHA(object.raw["bootstrap.pb"]) != m.SHA256 {
			return fail(errors.New("CHECKPOINT_RETENTION_PAYLOAD"))
		}
		if _, e = DecodeBootstrap(manifest, object.raw["bootstrap.pb"], pin, r.maxBytes); e != nil {
			return fail(e)
		}
		object.row.ContentCoverage = "FULL_DECODE_AND_STORE_CERTIFICATE"
		expected := r.publication(Publication{ManifestSHA256: pin}, m)
		certificate, _ := json.Marshal(expected)
		if !bytes.Equal(certificate, object.raw[r.completionName()]) {
			return fail(errors.New("CHECKPOINT_RETENTION_CERTIFICATE"))
		}
		object.manifestModified = object.files["manifest.json"].ModTime()
		object.row.ManifestSHA256 = pin
		ref := m.Reference
		object.row.Reference = &ref
		object.row.Reason = "AGE_PROTECTED"
		qualified = append(qualified, object)
		for i := range inventory {
			if raw, ok := object.raw[strings.TrimPrefix(inventory[i].Path, object.name+"/")]; ok && strings.HasPrefix(inventory[i].Path, object.name+"/") {
				inventory[i].SHA256 = rawSHA(raw)
			}
		}
	}
	sort.Slice(qualified, func(i, j int) bool {
		a, b := qualified[i].row.Reference, qualified[j].row.Reference
		if a.JournalSeq != b.JournalSeq {
			return a.JournalSeq > b.JournalSeq
		}
		return qualified[i].name < qualified[j].name
	})

	rank := uint64(0)
	for _, object := range qualified {
		ref := object.row.Reference
		head := boot.Snapshot.Reference
		if ref.WriterEpoch > head.WriterEpoch || ref.CanonicalEpoch > head.CanonicalEpoch || ref.OwnerAssignmentVersion > head.OwnerAssignmentVersion || ref.JournalSeq > head.JournalSeq {
			object.row.Reason = "FUTURE_IDENTITY"
			continue
		}
		latest := rank < policy.KeepLatest
		rank++
		if reason, ok := protected[object.name]; ok {
			object.row.Reason = reason
			continue
		}
		if latest {
			object.row.Reason = "LATEST_BACKUP"
			continue
		}
		cutoff := observed.Add(-time.Duration(policy.MinAgeMS) * time.Millisecond)
		if cutoff.UnixMilli() < 0 || ref.Block.TimestampMS > uint64(cutoff.UnixMilli()) || object.manifestModified.After(cutoff) {
			continue
		}
		object.row.Reason = "RETENTION_ELIGIBLE"
	}

	// Oldest eligible objects are selected first, up to the configured finite batch.
	for i := len(qualified) - 1; i >= 0; i-- {
		object := qualified[i]
		if object.row.Reason == "RETENTION_ELIGIBLE" && plan.SelectedObjects < policy.MaxRemove {
			object.row.Selected = true
			plan.SelectedObjects++
			plan.SelectedBytes += object.row.Bytes
		}
	}
	plan.NamespaceDigestBasis = "FILE_METADATA_AND_VERIFIED_KNOWN_OBJECT_BYTES"
	plan.Schema = 1
	plan.NamespaceSHA256 = ""
	plan.StoreSHA256 = rawSHA([]byte(r.base))
	plan.Reference = boot.Snapshot.Reference
	plan.Cursor = ManifestCursor{Topic: boot.Cursor.Topic, Partition: boot.Cursor.Partition, NextOffset: strconv.FormatInt(boot.Cursor.Offset, 10)}
	plan.Policy = policy
	plan.ObservedAtMS = observed.UnixMilli()
	plan.ScannedEntries = entries
	plan.ReadBytes = readBytes
	plan.Objects = []RetentionObject{}
	for _, object := range objects {
		plan.Objects = append(plan.Objects, object.row)
	}
	sort.Slice(inventory, func(i, j int) bool { return inventory[i].Path < inventory[j].Path })
	raw, e := json.Marshal(inventory)
	if e != nil {
		return fail(e)
	}
	plan.NamespaceSHA256 = rawSHA(raw)
	raw, e = json.Marshal(plan)
	if e != nil {
		return fail(e)
	}
	plan.PlanID = rawSHA(raw)
	if e = check(); e != nil {
		return fail(e)
	}
	return plan, nil
}
