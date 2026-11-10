// Package ingest provides bounded staging and durable raw evidence. It imports
// no chain adapter; callers supply fetch, decode, validation and atomic commit.
//
// R02.3 service integration contract:
//
//   - OpenArchive once per source partition; retain the exclusive handle for the
//     service lifetime. Inspect PendingReplays on startup. Reinvoke Replay with
//     the recorded Spec: publishing/unknown can ONLY Confirm, never republish.
//     A started attempt interrupted before preparing output becomes terminal
//     Retry; create an explicit higher Attempt only after a terminal Retry/Failed.
//   - Use PutPinned for raw evidence referenced by a live consumer, or PinRaw
//     before reading existing evidence. Synchronous RPC response recorders must
//     propagate storage errors and keep stable provider/request ownership. Never
//     ReleaseRaw before the owner's durable commit/checkpoint/plan completion.
//     Existing Pipeline/response-recorder consumption wiring belongs to service.
//   - Feed AdvanceReplayFence from the authoritative lease. ReplaySpec binds
//     explicit IncidentID, original RawRef ownership/digest, decoder artifact version, namespace,
//     writer fence and generation. The owner callback must CAS ALL identities,
//     atomically publish the complete result with its durable cursor and dedupe
//     by AttemptID. ReceiptFor only formats an identity; it is not commit proof.
//     Confirm must return durable owner evidence. CommitNotCommitted additionally
//     means that prior in-flight work is settled/fenced and cannot commit later.
//     A missing/unknown receipt keeps isolation, including cancellation/timeout.
//   - QuarantineIncident returns the stable raw+scope FailureIncident; existing
//     Quarantine callers obtain it through CurrentIncident(DeadLetterID(entry)).
//     Carry IncidentID explicitly in ReplaySpec and the owner receipt. Pending
//     commits lock raw ownership/digest+namespace across diagnostic/scope aliases.
//     A new observation after online Resolved opens a durable higher cycle even
//     when every diagnostic field is identical. Never refresh the IncidentID on
//     an old pending Spec: confirm its exact original identity first. Within a
//     cycle, only the identical Spec can reuse a cached success, after fencing.
//     FailureIncidents and ReplayAudit retain all raw+scope cycles/attempts.
//   - Supply only history callbacks for history work. History cannot call online
//     commit/confirm capabilities or resolve online quarantine. Decoder semantics
//     and chain/canonical correctness remain the service/adapter's responsibility.
//   - After supplying trusted decoder/canonical provenance, call ArchiveRaw with
//     a configured ArchiveSink. Release completed pins and run RunRetention from
//     a service-owned goroutine; propagate errors/TargetMet=false to health and
//     backpressure. Retention never archives automatically with invented evidence.
//     On uncertain storage/verification failure, stop this handle and reopen.
//
// Versioned service configuration must provide separate, non-nested raw/sink
// directories; ArchiveLimits (MaxBytes, MaxRecords, MaxRecordBytes plus positive
// ReservedBytes/ReservedRecords sized for audit/GC headroom); ReplayLimits
// (MaxRawBytes, MaxDecodedBytes <= MaxRecordBytes, Timeout); decoder artifact
// identity, online/history namespace and authoritative writer fence/generation;
// RetentionPolicy (KeepFor, TargetBytes below ingestion ceiling, MaxScan,
// MaxReclaims, Timeout) and maintenance interval. One replay actor per Archive
// bounds callback concurrency. Payload limits admit bounded JSON/copy overhead;
// MaxScan bounds per-pass attempts; the index is bounded by ArchiveLimits.
//
// Immutable DLQ/audit/manifests/pins/tombstones count toward local capacity and
// are never silently compacted. Metadata exhaustion fails closed: plan adequate
// headroom and operator-managed archival/rotation instead of promising infinite
// retention on finite storage. Legacy raw without capture time is held. The
// local sink supplies real copy/fsync/digest checks for public-data acceptance;
// remote replication, encryption, WORM and an independent failure domain require
// deployment support. Hash links detect corruption, not a privileged filesystem
// administrator rewriting the entire chain (including its unanchored tail).
// The service must supervise callbacks: Go cannot kill one that ignores ctx.
package ingest

import (
	"bytes"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"sync"
	"syscall"
	"time"
	"unicode/utf8"
)

var (
	ErrBudget    = errors.New("INGEST_BUDGET_EXCEEDED")
	ErrCorrupt   = errors.New("INGEST_ARCHIVE_CORRUPT")
	ErrOwnership = errors.New("INGEST_RAW_OWNERSHIP_CONFLICT")
	ErrClosed    = errors.New("INGEST_ARCHIVE_CLOSED")
)

type ArchiveLimits struct {
	MaxBytes       int64
	MaxRecords     int
	MaxRecordBytes int64
	// Reserve headroom for audit/tombstones when ingestion reaches capacity.
	// This is a hard ingestion margin, not an unlimited metadata allowance.
	ReservedBytes   int64
	ReservedRecords int
}

// Owner distinguishes identical payloads acquired for different block/provider
// identities. Identical owner + different payload is rejected, never overwritten.
type Owner struct {
	Chain      string
	Height     uint64
	Hash       string
	ProviderID string
}
type RawRef struct {
	ID     string
	Digest string
	Owner  Owner
	Bytes  int64
}
type rawRecord struct {
	Ref        RawRef
	Data       []byte
	CapturedAt time.Time
}
type DeadLetter struct {
	Raw     RawRef
	Stage   string
	Code    string
	Scope   string
	Message string
}
type envelope struct {
	Digest  string
	Payload json.RawMessage
}

// Archive is an exclusively locked local directory. MaxBytes includes encoded
// raw, DLQ and immutable event records; MaxRecords counts all files. All writes
// sync a temporary file, link without replacement, then sync the directory.
// Uncertain I/O poisons this handle until reopen, including link-success with
// directory-sync failure. Pending files are cleaned on reopen.
type Archive struct {
	mu                      sync.Mutex
	directory               string
	limits                  ArchiveLimits
	lock                    *os.File
	raw                     map[string]RawRef
	dlq                     map[string]DeadLetter
	used                    int64
	closed                  bool
	failed                  bool
	syncDir                 func(string) error
	records                 int
	events                  []archiveEvent
	replays                 map[string]ReplayRecord
	authorities             map[string]ReplayAuthority
	manifests               map[string]ArchiveManifest
	reclaimed               map[string]RawRef
	captured                map[string]time.Time
	pins                    map[string]rawPin
	active                  map[string]int
	replaying               map[string]bool
	retentionCursor         string
	incidents               map[string]FailureIncident
	currentIncidents        map[string]FailureIncident
	failureBarriers         map[string]failureBarrier
	failureUncertain        bool
	failureRecoveryRequired bool
	failureSessionArmed     bool
	restoring               bool
}

func (a *Archive) Directory() string     { return a.directory }
func (a *Archive) Limits() ArchiveLimits { return a.limits }
func digest(b []byte) string             { h := sha256.Sum256(b); return hex.EncodeToString(h[:]) }
func jsonID(v any) string                { b, _ := json.Marshal(v); return digest(b) }
func validOwner(o Owner) bool {
	return utf8.ValidString(o.Chain) && utf8.ValidString(o.Hash) && utf8.ValidString(o.ProviderID) && o.Chain != "" && o.Hash != "" && o.ProviderID != "" && len(o.Chain) <= 512 && len(o.Hash) <= 512 && len(o.ProviderID) <= 512
}
func directorySync(path string) error {
	f, err := os.Open(path)
	if err != nil {
		return err
	}
	defer f.Close()
	return f.Sync()
}

func OpenArchive(path string, limits ArchiveLimits) (*Archive, error) {
	if limits.MaxBytes <= 0 || limits.MaxRecords <= 0 || limits.MaxRecordBytes <= 0 || limits.MaxRecordBytes > 1<<30 || limits.ReservedBytes < 0 || limits.ReservedBytes >= limits.MaxBytes || limits.ReservedRecords < 0 || limits.ReservedRecords >= limits.MaxRecords {
		return nil, ErrBudget
	}
	// Remember all newly created ancestors. Syncing only the final directory's
	// parent would leave higher mkdir entries vulnerable to a host crash.
	absolute, err := filepath.Abs(path)
	if err != nil {
		return nil, err
	}
	var created []string
	for current := absolute; ; current = filepath.Dir(current) {
		if _, statErr := os.Stat(current); statErr == nil {
			break
		} else if !errors.Is(statErr, os.ErrNotExist) {
			return nil, statErr
		}
		created = append(created, current)
	}
	if err := os.MkdirAll(path, 0700); err != nil {
		return nil, err
	}
	for i := len(created) - 1; i >= 0; i-- {
		if err := directorySync(filepath.Dir(created[i])); err != nil {
			return nil, err
		}
	}
	info, err := os.Lstat(path)
	if err != nil || !info.IsDir() {
		return nil, ErrCorrupt
	}
	lockPath := filepath.Join(path, ".lock")
	if info, err := os.Lstat(lockPath); err == nil && !info.Mode().IsRegular() {
		return nil, ErrCorrupt
	}
	lock, err := os.OpenFile(lockPath, os.O_CREATE|os.O_RDWR, 0600)
	if err != nil {
		return nil, err
	}
	if err = syscall.Flock(int(lock.Fd()), syscall.LOCK_EX|syscall.LOCK_NB); err != nil {
		lock.Close()
		return nil, errors.New("INGEST_ARCHIVE_LOCKED")
	}
	a := &Archive{directory: path, limits: limits, lock: lock, raw: map[string]RawRef{}, dlq: map[string]DeadLetter{}, syncDir: directorySync}
	a.initJournal()
	a.restoring = true
	if err = a.restore(); err != nil {
		a.Close()
		return nil, err
	}
	a.restoring = false
	if err = a.armFailureSession(); err != nil {
		a.Close()
		return nil, err
	}
	// Persist creation of the directory entry itself as well as its contents.
	if err = directorySync(filepath.Dir(filepath.Clean(path))); err != nil {
		a.Close()
		return nil, err
	}
	return a, nil
}
func (a *Archive) restore() error {
	entries, err := os.ReadDir(a.directory)
	if err != nil {
		return err
	}
	var pending []string
	var events []archiveEvent
	for _, e := range entries {
		name := e.Name()
		info, err := e.Info()
		if err != nil {
			return err
		}
		if !info.Mode().IsRegular() {
			return ErrCorrupt
		}
		if name == ".lock" {
			continue
		}
		if name == failureSessionFile {
			if info.Size() != 0 {
				return ErrCorrupt
			}
			a.failureRecoveryRequired = true
			continue
		}
		if strings.HasPrefix(name, failureBarrierPrefix) {
			if info.Size() != 0 {
				return ErrCorrupt
			}
			barrier, err := parseFailureBarrier(name)
			if err != nil {
				return err
			}
			a.failureBarriers[name] = barrier
			continue
		}
		if strings.HasPrefix(name, ".pending-") {
			pending = append(pending, name)
			continue
		}
		ext := filepath.Ext(name)
		id := strings.TrimSuffix(name, ext)
		if (ext != ".raw" && ext != ".dlq" && ext != ".event") || len(id) != 64 {
			return ErrCorrupt
		}
		if _, err := hex.DecodeString(id); err != nil {
			return ErrCorrupt
		}
		a.used += info.Size()
		a.records++
		if ext == ".raw" {
			var r rawRecord
			if err := a.load(name, &r); err != nil {
				return err
			}
			if !validRaw(r) || r.Ref.ID != id || r.Ref.Bytes > a.limits.MaxRecordBytes {
				return ErrCorrupt
			}
			a.raw[id] = r.Ref
			a.captured[id] = r.CapturedAt
		} else if ext == ".event" {
			var event archiveEvent
			if err := a.load(name, &event); err != nil {
				return err
			}
			if jsonID(event) != id {
				return ErrCorrupt
			}
			events = append(events, event)
		} else {
			var d DeadLetter
			if err := a.load(name, &d); err != nil {
				return err
			}
			if !validDeadLetter(d) || jsonID(d) != id {
				return ErrCorrupt
			}
			a.dlq[id] = d
		}
	}
	if err := a.restoreJournal(events); err != nil {
		return err
	}
	for _, entry := range a.dlq {
		if ref, ok := a.raw[entry.Raw.ID]; !ok || ref != entry.Raw {
			return ErrCorrupt
		}
	}
	if err := a.restoreFailureBarriers(); err != nil {
		return err
	}
	for _, name := range pending {
		if err := os.Remove(filepath.Join(a.directory, name)); err != nil {
			return err
		}
	}
	if a.used > a.limits.MaxBytes || a.records > a.limits.MaxRecords {
		return ErrBudget
	}
	return a.syncDir(a.directory)
}
func validRaw(r rawRecord) bool {
	return validOwner(r.Ref.Owner) && r.Ref.ID == jsonID(r.Ref.Owner) && r.Ref.Digest == digest(r.Data) && r.Ref.Bytes == int64(len(r.Data))
}
func validDeadLetter(d DeadLetter) bool {
	return utf8.ValidString(d.Stage) && utf8.ValidString(d.Code) && utf8.ValidString(d.Scope) && utf8.ValidString(d.Message) && d.Stage != "" && d.Code != "" && len(d.Stage) <= 128 && len(d.Code) <= 128 && len(d.Scope) <= 512 && len(d.Message) <= 2048
}
func (a *Archive) usable() error {
	if a.closed {
		return ErrClosed
	}
	if a.failed {
		return errors.New("INGEST_ARCHIVE_REOPEN_REQUIRED")
	}
	return nil
}
func (a *Archive) load(name string, out any) (err error) {
	defer func() {
		if err != nil {
			a.failed = true
		}
	}()
	f, err := openRegular(filepath.Join(a.directory, name))
	if err != nil {
		return err
	}
	defer f.Close()
	info, err := f.Stat()
	if err != nil {
		return err
	}
	if !info.Mode().IsRegular() || info.Size() > a.limits.MaxRecordBytes*2+(256<<10) {
		return ErrCorrupt
	}
	encoded, err := io.ReadAll(io.LimitReader(f, a.limits.MaxRecordBytes*2+(256<<10)+1))
	if err != nil {
		return err
	}
	var env envelope
	if json.Unmarshal(encoded, &env) != nil || env.Digest != digest(env.Payload) {
		return ErrCorrupt
	}
	decoder := json.NewDecoder(bytes.NewReader(env.Payload))
	decoder.DisallowUnknownFields()
	if decoder.Decode(out) != nil {
		return ErrCorrupt
	}
	return nil
}
func (a *Archive) write(name string, value any) error {
	payload, err := json.Marshal(value)
	if err != nil {
		return err
	}
	encoded, err := json.Marshal(envelope{Digest: digest(payload), Payload: payload})
	if err != nil {
		return err
	}
	if int64(len(encoded)) > a.limits.MaxRecordBytes*2+(256<<10) {
		return ErrBudget
	}
	maxBytes, maxRecords := a.limits.MaxBytes, a.limits.MaxRecords
	if strings.HasSuffix(name, ".raw") || strings.HasSuffix(name, ".dlq") {
		maxBytes -= a.limits.ReservedBytes
		maxRecords -= a.limits.ReservedRecords
	}
	if int64(len(encoded)) > maxBytes-a.used || a.records >= maxRecords {
		return ErrBudget
	}
	if _, err := os.Lstat(filepath.Join(a.directory, name)); !errors.Is(err, os.ErrNotExist) {
		a.failed = true
		return ErrCorrupt
	}
	a.failed = true
	temp, err := os.CreateTemp(a.directory, ".pending-")
	if err != nil {
		return err
	}
	tempPath := temp.Name()
	defer os.Remove(tempPath)
	// Any I/O failure requires a reopen; capacity rejection above is unambiguous.
	a.failed = true
	if _, err = temp.Write(encoded); err != nil {
		temp.Close()
		return err
	}
	if err = temp.Sync(); err != nil {
		temp.Close()
		return err
	}
	if err = temp.Close(); err != nil {
		return err
	}
	if err = os.Link(tempPath, filepath.Join(a.directory, name)); err != nil {
		return err
	}
	if err = os.Remove(tempPath); err != nil {
		return err
	}
	if err = a.syncDir(a.directory); err != nil {
		return err
	}
	a.records++
	a.used += int64(len(encoded))
	a.failed = false
	return nil
}
func (a *Archive) Put(owner Owner, data []byte) (RawRef, error) {
	a.mu.Lock()
	defer a.mu.Unlock()
	return a.put(owner, data)
}
func (a *Archive) put(owner Owner, data []byte) (RawRef, error) {
	if err := a.usable(); err != nil {
		return RawRef{}, err
	}
	if !validOwner(owner) {
		return RawRef{}, ErrOwnership
	}
	if int64(len(data)) > a.limits.MaxRecordBytes {
		return RawRef{}, ErrBudget
	}
	ref := RawRef{ID: jsonID(owner), Digest: digest(data), Owner: owner, Bytes: int64(len(data))}
	if old, exists := a.raw[ref.ID]; exists {
		if old != ref {
			return RawRef{}, ErrOwnership
		}
		if _, reclaimed := a.reclaimed[ref.ID]; reclaimed {
			return old, nil
		}
		if _, err := a.read(ref); err != nil {
			return RawRef{}, err
		}
		return old, nil
	}
	capturedAt := time.Now().UTC()
	if err := a.write(ref.ID+".raw", rawRecord{Ref: ref, Data: data, CapturedAt: capturedAt}); err != nil {
		return RawRef{}, err
	}
	a.raw[ref.ID] = ref
	a.captured[ref.ID] = capturedAt
	return ref, nil
}
func (a *Archive) read(ref RawRef) ([]byte, error) {
	old, ok := a.raw[ref.ID]
	if !ok || old != ref {
		return nil, ErrOwnership
	}
	if _, reclaimed := a.reclaimed[ref.ID]; reclaimed {
		return nil, ErrRawReclaimed
	}
	var r rawRecord
	if err := a.load(ref.ID+".raw", &r); err != nil {
		a.failed = true
		return nil, err
	}
	if !validRaw(r) || r.Ref != ref {
		a.failed = true
		return nil, ErrCorrupt
	}
	return r.Data, nil
}

// Read retains the StageError forensic-read contract after a failure-observation
// poison. This exception authorizes no enumeration/Usage, mutation, resolution
// or GC; all of those still reject the poisoned handle.
func (a *Archive) Read(ref RawRef) ([]byte, error) {
	a.mu.Lock()
	defer a.mu.Unlock()
	if err := a.usable(); err != nil {
		if a.closed || !a.failed || (len(a.failureBarriers) == 0 && !a.failureUncertain) {
			return nil, err
		}
	}
	return a.read(ref)
}

// Quarantine observes a failure, joining an unresolved incident or opening a
// new cycle after resolution. Use QuarantineIncident to obtain its replay ID.
func (a *Archive) Quarantine(entry DeadLetter) error {
	_, err := a.QuarantineIncident(entry)
	return err
}
func (a *Archive) quarantine(entry DeadLetter) error {
	if err := a.usable(); err != nil {
		return err
	}
	if !validDeadLetter(entry) {
		return errors.New("INGEST_DLQ_INVALID")
	}
	if _, err := a.read(entry.Raw); err != nil {
		return err
	}
	id := jsonID(entry)
	if _, exists := a.dlq[id]; exists {
		var old DeadLetter
		if err := a.load(id+".dlq", &old); err != nil {
			return err
		}
		if old != entry {
			return ErrCorrupt
		}
		return nil
	}
	if err := a.write(id+".dlq", entry); err != nil {
		return err
	}
	a.dlq[id] = entry
	return nil
}
func (a *Archive) DeadLetters() ([]DeadLetter, error) {
	a.mu.Lock()
	defer a.mu.Unlock()
	if err := a.usable(); err != nil {
		return nil, err
	}
	ids := make([]string, 0, len(a.dlq))
	for id := range a.dlq {
		ids = append(ids, id)
	}
	sort.Strings(ids)
	result := make([]DeadLetter, 0, len(ids))
	for _, id := range ids {
		var entry DeadLetter
		if err := a.load(id+".dlq", &entry); err != nil {
			return nil, err
		}
		if entry != a.dlq[id] {
			a.failed = true
			return nil, ErrCorrupt
		}
		if _, reclaimed := a.reclaimed[entry.Raw.ID]; !reclaimed {
			if _, err := a.read(entry.Raw); err != nil {
				return nil, err
			}
		}
		result = append(result, entry)
	}
	return result, nil
}
func (a *Archive) Close() error {
	a.mu.Lock()
	defer a.mu.Unlock()
	if a.closed {
		return nil
	}
	a.closed = true
	safetyErr := a.closeFailureSession()
	unlock := syscall.Flock(int(a.lock.Fd()), syscall.LOCK_UN)
	closeErr := a.lock.Close()
	if unlock != nil {
		return fmt.Errorf("INGEST_ARCHIVE_UNLOCK: %w", unlock)
	}
	return errors.Join(closeErr, safetyErr)
}
