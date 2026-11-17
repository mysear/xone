package checkpoint

import (
	"encoding/json"
	"errors"
	"io"
	"os"
	"path/filepath"
	"strconv"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/journal"
)

type BudgetUsage struct {
	Bytes   uint64 `json:"bytes"`
	Files   uint64 `json:"files"`
	Entries uint64 `json:"entries"`
}

// Admission is performed under the owner's shared publication gate. The private
// namespace has one writer; this is a logical file quota, not filesystem allocation.
func AdmitBootstrap(dir string, boot journal.Bootstrap, maxBytes int, base string, limit *config.CheckpointBudget, guard func() error) (BudgetUsage, error) {
	if limit == nil {
		return BudgetUsage{}, nil
	}
	if e := limit.Validate(); e != nil {
		return BudgetUsage{}, e
	}
	payload, e := EncodeBootstrap(boot, maxBytes)
	if e != nil {
		return BudgetUsage{}, e
	}
	m := BootstrapManifest{Schema: 2, Codec: BootstrapCodec, Reference: boot.Snapshot.Reference, Cursor: ManifestCursor{Topic: boot.Cursor.Topic, Partition: boot.Cursor.Partition, NextOffset: strconv.FormatInt(boot.Cursor.Offset, 10)}, Payload: "bootstrap.pb", SHA256: rawSHA(payload), Size: uint64(len(payload))}
	raw, e := json.Marshal(m)
	if e != nil {
		return BudgetUsage{}, e
	}
	if _, e = ParseBootstrapManifest(raw, rawSHA(raw), maxBytes); e != nil {
		return BudgetUsage{}, e
	}
	planned := map[string]uint64{m.SHA256 + "/bootstrap.pb": uint64(len(payload)), m.SHA256 + "/manifest.json": uint64(len(raw))}
	if base != "" {
		certificate, e := json.Marshal(RemotePublication{ManifestURL: base + "/public-v2/" + m.SHA256 + "/manifest.json", ManifestSHA256: rawSHA(raw), Reference: m})
		if e != nil {
			return BudgetUsage{}, e
		}
		id := rawSHA([]byte(base))
		planned[m.SHA256+"/remote-"+id+".json"] = uint64(len(certificate))
		planned["candidate-"+id+".json"] = uint64(len(certificate))
	}
	return admitFiles(dir, planned, *limit, guard)
}
func admitFiles(dir string, planned map[string]uint64, limit config.CheckpointBudget, guard func() error) (BudgetUsage, error) {
	var usage BudgetUsage
	check := func() error {
		if guard != nil {
			return guard()
		}
		return nil
	}
	if e := check(); e != nil {
		return usage, e
	}
	files := map[string]uint64{}
	directories := map[string]bool{}
	path := filepath.Join(dir, "public-v2")
	meta, e := os.Lstat(path)
	if e != nil && !errors.Is(e, os.ErrNotExist) {
		return usage, e
	}
	if e == nil {
		if !meta.IsDir() || meta.Mode()&os.ModeSymlink != 0 {
			return usage, errors.New("CHECKPOINT_LOCAL_BUDGET_ENTRY")
		}
		root, e := os.OpenRoot(path)
		if e != nil {
			return usage, e
		}
		defer root.Close()
		var scan func(*os.Root, string, int) error
		scan = func(root *os.Root, prefix string, depth int) error {
			if depth > 4 {
				return errors.New("CHECKPOINT_LOCAL_BUDGET_DEPTH")
			}
			f, e := root.Open(".")
			if e != nil {
				return e
			}
			defer f.Close()
			for {
				entries, e := f.ReadDir(1)
				if e != nil && e != io.EOF {
					return e
				}
				if len(entries) == 0 {
					return nil
				}
				if e = check(); e != nil {
					return e
				}
				usage.Entries++
				if usage.Entries > limit.MaxEntries {
					return errors.New("CHECKPOINT_LOCAL_BUDGET_SCAN")
				}
				entry := entries[0]
				relative := prefix + entry.Name()
				info, e := root.Lstat(entry.Name())
				if e != nil {
					return e
				}
				if info.Mode()&os.ModeSymlink != 0 {
					return errors.New("CHECKPOINT_LOCAL_BUDGET_ENTRY")
				}
				if info.IsDir() {
					directories[relative] = true
					child, e := root.OpenRoot(entry.Name())
					if e != nil {
						return e
					}
					e = scan(child, relative+"/", depth+1)
					child.Close()
					if e != nil {
						return e
					}
				} else if info.Mode().IsRegular() {
					size := uint64(info.Size())
					if size > limit.MaxBytes || usage.Bytes > limit.MaxBytes-size {
						return errors.New("CHECKPOINT_LOCAL_BUDGET_EXCEEDED")
					}
					files[relative] = size
					usage.Bytes += size
					usage.Files++
					if usage.Files > limit.MaxFiles {
						return errors.New("CHECKPOINT_LOCAL_BUDGET_EXCEEDED")
					}
				} else {
					return errors.New("CHECKPOINT_LOCAL_BUDGET_ENTRY")
				}
			}
		}
		if e = scan(root, "", 0); e != nil {
			return usage, e
		}
	}
	for name, size := range planned {
		old, exists := files[name]
		if !exists {
			usage.Files++
			usage.Entries++
			for parent := filepath.ToSlash(filepath.Dir(name)); parent != "."; parent = filepath.ToSlash(filepath.Dir(parent)) {
				if !directories[parent] {
					directories[parent] = true
					usage.Entries++
				}
			}
		}
		if size > old {
			delta := size - old
			if delta > limit.MaxBytes || usage.Bytes > limit.MaxBytes-delta {
				return usage, errors.New("CHECKPOINT_LOCAL_BUDGET_EXCEEDED")
			}
			usage.Bytes += delta
		}
		if usage.Files > limit.MaxFiles || usage.Entries > limit.MaxEntries {
			return usage, errors.New("CHECKPOINT_LOCAL_BUDGET_EXCEEDED")
		}
	}
	return usage, check()
}
