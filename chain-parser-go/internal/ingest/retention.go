package ingest

import (
	"context"
	"errors"
	"os"
	"path/filepath"
	"sort"
	"time"
)

var (
	ErrRetentionBlocked = errors.New("INGEST_RETENTION_BLOCKED")
	ErrRawReclaimed     = errors.New("INGEST_RAW_RECLAIMED")
)

type ArchiveMetadata struct {
	Namespace      ReplayNamespace
	DecoderVersion string
	CanonicalProof string
}

// ArchiveManifest binds source, decoder, canonical evidence and content. Sink
// Store/Verify must persist and compare BOTH this manifest and the raw bytes.
type ArchiveManifest struct {
	Version       int
	Raw           RawRef
	Metadata      ArchiveMetadata
	CapturedAt    time.Time
	ContentDigest string
}

func (m ArchiveManifest) ObjectKey() string   { return m.Raw.ID + "-" + m.ContentDigest + ".blob" }
func (m ArchiveManifest) ManifestKey() string { return jsonID(m) + ".manifest" }
func (m ArchiveManifest) valid() bool {
	return m.Version == 1 && validOwner(m.Raw.Owner) && m.Raw.ID == jsonID(m.Raw.Owner) && validDigest(m.ContentDigest) && m.ContentDigest == m.Raw.Digest && m.Raw.Bytes >= 0 && !m.CapturedAt.IsZero() && validNamespace(m.Metadata.Namespace) && validLabel(m.Metadata.DecoderVersion, 512) && validLabel(m.Metadata.CanonicalProof, 2048)
}

// ArchiveSink implementations must honor ctx and verify manifest AND full
// content digest. Object existence, upload success and ETag alone are not proof.
type ArchiveSink interface {
	Store(context.Context, ArchiveManifest, []byte) error
	Verify(context.Context, ArchiveManifest) error
}
type rawPin struct {
	Raw       RawRef
	Reference string
	Active    bool
}

func (p rawPin) key() string {
	return jsonID(struct {
		Raw       RawRef
		Reference string
	}{p.Raw, p.Reference})
}

// PinRaw persists checkpoint/reorg/response-recorder/replay-plan ownership.
// Reference names cannot be reused after release; use an immutable plan ID.
func (a *Archive) PinRaw(ref RawRef, reference string) error {
	a.mu.Lock()
	defer a.mu.Unlock()
	if err := a.usable(); err != nil {
		return err
	}
	p := rawPin{ref, reference, true}
	if old, ok := a.pins[p.key()]; ok && old == p {
		return nil
	}
	return a.appendEvent(archiveEvent{Kind: "pin", Pin: &p})
}
func (a *Archive) ReleaseRaw(ref RawRef, reference string) error {
	a.mu.Lock()
	defer a.mu.Unlock()
	if err := a.usable(); err != nil {
		return err
	}
	p := rawPin{ref, reference, false}
	if old, ok := a.pins[p.key()]; ok && old == p {
		return nil
	}
	return a.appendEvent(archiveEvent{Kind: "release", Pin: &p})
}
func (a *Archive) blocked(ref RawRef) bool {
	if a.active[ref.ID] > 0 || a.quarantined(ref) {
		return true
	}
	for _, p := range a.pins {
		if p.Raw == ref && p.Active {
			return true
		}
	}
	for _, r := range a.replays {
		if r.Spec.Raw == ref && r.Phase != "finished" {
			return true
		}
	}
	return false
}
func (a *Archive) poison(err error) error {
	if err != nil {
		a.mu.Lock()
		a.failed = true
		a.mu.Unlock()
	}
	return err
}
func (a *Archive) ArchiveRaw(ctx context.Context, ref RawRef, meta ArchiveMetadata, sink ArchiveSink) (ArchiveManifest, error) {
	if ctx.Err() != nil {
		return ArchiveManifest{}, ctx.Err()
	}
	if sink == nil {
		return ArchiveManifest{}, ErrOwnership
	}
	a.mu.Lock()
	if err := a.usable(); err != nil {
		a.mu.Unlock()
		return ArchiveManifest{}, err
	}
	data, err := a.read(ref)
	if err != nil {
		a.mu.Unlock()
		return ArchiveManifest{}, err
	}
	m := ArchiveManifest{Version: 1, Raw: ref, Metadata: meta, CapturedAt: a.captured[ref.ID], ContentDigest: ref.Digest}
	// Legacy raw with unknown capture time is held, never assigned an invented age.
	if !m.valid() {
		a.mu.Unlock()
		return ArchiveManifest{}, ErrOwnership
	}
	old, exists := a.manifests[ref.ID]
	if exists && old != m {
		a.mu.Unlock()
		return ArchiveManifest{}, ErrOwnership
	}
	a.active[ref.ID]++
	a.mu.Unlock()
	defer func() { a.mu.Lock(); a.active[ref.ID]--; a.mu.Unlock() }()
	if !exists {
		if err := sink.Store(ctx, m, data); err != nil {
			return m, a.poison(errors.Join(err, ctx.Err()))
		}
	}
	if err := sink.Verify(ctx, m); err != nil {
		return m, a.poison(errors.Join(err, ctx.Err()))
	}
	if ctx.Err() != nil {
		return m, a.poison(ctx.Err())
	}
	a.mu.Lock()
	defer a.mu.Unlock()
	if err := a.usable(); err != nil {
		return m, err
	}
	if old, ok := a.manifests[ref.ID]; ok {
		if old != m {
			return m, ErrOwnership
		}
		return m, nil
	}
	if err := a.appendEvent(archiveEvent{Kind: "archive", Manifest: &m}); err != nil {
		return m, err
	}
	return m, nil
}

// ReclaimRaw verifies external manifest/content, then rechecks all references.
// A durable tombstone precedes unlink. Reopen finishes interrupted unlink; any
// uncertain I/O poisons the handle. Original DLQ/audit metadata stays immutable.
func (a *Archive) ReclaimRaw(ctx context.Context, ref RawRef, sink ArchiveSink) error {
	if ctx.Err() != nil {
		return ctx.Err()
	}
	if sink == nil {
		return ErrRetentionBlocked
	}
	a.mu.Lock()
	if err := a.usable(); err != nil {
		a.mu.Unlock()
		return err
	}
	if a.raw[ref.ID] != ref {
		a.mu.Unlock()
		return ErrOwnership
	}
	if _, ok := a.reclaimed[ref.ID]; ok {
		a.mu.Unlock()
		return nil
	}
	if err := a.verifyRawJournal(ref); err != nil {
		a.mu.Unlock()
		return err
	}
	m, ok := a.manifests[ref.ID]
	if !ok || a.blocked(ref) {
		a.mu.Unlock()
		return ErrRetentionBlocked
	}
	a.active[ref.ID]++
	a.mu.Unlock()
	err := sink.Verify(ctx, m)
	a.mu.Lock()
	a.active[ref.ID]--
	defer a.mu.Unlock()
	if err != nil || ctx.Err() != nil {
		a.failed = true
		return errors.Join(err, ctx.Err())
	}
	if err := a.usable(); err != nil {
		return err
	}
	if err := a.verifyRawJournal(ref); err != nil {
		return err
	}
	if a.blocked(ref) {
		return ErrRetentionBlocked
	}
	if _, ok := a.reclaimed[ref.ID]; ok {
		return nil
	}
	if _, err := a.read(ref); err != nil {
		return err
	}
	path := filepath.Join(a.directory, ref.ID+".raw")
	info, err := os.Stat(path)
	if err != nil {
		a.failed = true
		return err
	}
	if err := a.appendEvent(archiveEvent{Kind: "reclaim", Reclaimed: &ref}); err != nil {
		return err
	}
	a.failed = true
	if err := os.Remove(path); err != nil {
		return err
	}
	if err := a.syncDir(a.directory); err != nil {
		return err
	}
	a.used -= info.Size()
	a.records--
	a.failed = false
	return nil
}

// Above TargetBytes, age is relaxed but verification/DLQ/reference gates remain.
// Zero KeepFor explicitly permits immediate GC. Audit/manifest/pin/tombstone
// metadata counts against capacity and is never deleted.
type RetentionPolicy struct {
	KeepFor     time.Duration
	TargetBytes int64
	MaxScan     int
	MaxReclaims int
	Timeout     time.Duration
}
type RetentionReport struct {
	Scanned   int
	Reclaimed int
	Blocked   int
	UsedBytes int64
	TargetMet bool
}

func (a *Archive) Usage() (int64, int, error) {
	a.mu.Lock()
	defer a.mu.Unlock()
	if a.failureRecoveryRequired {
		return 0, 0, ErrFailureRecoveryRequired
	}
	if err := a.usable(); err != nil {
		return 0, 0, err
	}
	return a.used, a.records, nil
}

// RetainOnce is a bounded maintenance pass. ArchiveRaw is explicit: provenance
// must be supplied BEFORE enabling deletion. Unarchived raws remain blocked.
func (a *Archive) RetainOnce(ctx context.Context, p RetentionPolicy, sink ArchiveSink) (RetentionReport, error) {
	var report RetentionReport
	if p.KeepFor < 0 || p.TargetBytes <= 0 || p.MaxScan <= 0 || p.MaxReclaims <= 0 || p.Timeout <= 0 {
		return report, ErrBudget
	}
	ctx, cancel := context.WithTimeout(ctx, p.Timeout)
	defer cancel()
	a.mu.Lock()
	if err := a.usable(); err != nil {
		a.mu.Unlock()
		return report, err
	}
	candidates := make([]RawRecordInfo, 0, len(a.raw))
	for id, ref := range a.raw {
		if _, gone := a.reclaimed[id]; gone {
			continue
		}
		candidates = append(candidates, RawRecordInfo{Raw: ref, CapturedAt: a.captured[id]})
	}
	a.mu.Unlock()
	sort.Slice(candidates, func(i, j int) bool { return candidates[i].Raw.ID < candidates[j].Raw.ID })
	// Rotate a bounded scan so a permanently blocked oldest record cannot starve
	// later eligible raws. The cursor is advisory; restart safely starts at zero.
	a.mu.Lock()
	cursor := a.retentionCursor
	a.mu.Unlock()
	split := sort.Search(len(candidates), func(i int) bool { return candidates[i].Raw.ID > cursor })
	candidates = append(candidates[split:], candidates[:split]...)
	for _, candidate := range candidates {
		if report.Scanned >= p.MaxScan || report.Reclaimed >= p.MaxReclaims {
			break
		}
		if ctx.Err() != nil {
			return report, ctx.Err()
		}
		report.Scanned++
		a.mu.Lock()
		a.retentionCursor = candidate.Raw.ID
		a.mu.Unlock()
		used, _, err := a.Usage()
		if err != nil {
			return report, err
		}
		if candidate.CapturedAt.IsZero() || (used <= p.TargetBytes && time.Since(candidate.CapturedAt) < p.KeepFor) {
			continue
		}
		err = a.ReclaimRaw(ctx, candidate.Raw, sink)
		if errors.Is(err, ErrRetentionBlocked) {
			report.Blocked++
			continue
		}
		if err != nil {
			return report, err
		}
		report.Reclaimed++
	}
	var err error
	report.UsedBytes, _, err = a.Usage()
	report.TargetMet = report.UsedBytes <= p.TargetBytes
	return report, err
}

// RunRetention owns no background goroutine: service owns its lifecycle/backoff.
// Callback must honor ctx. Errors stop the loop and must reach service health.
func (a *Archive) RunRetention(ctx context.Context, interval time.Duration, p RetentionPolicy, sink ArchiveSink, report func(context.Context, RetentionReport) error) error {
	if interval <= 0 {
		return ErrBudget
	}
	ticker := time.NewTicker(interval)
	defer ticker.Stop()
	for {
		r, err := a.RetainOnce(ctx, p, sink)
		if err != nil {
			return err
		}
		if report != nil {
			if err := report(ctx, r); err != nil {
				return err
			}
		}
		select {
		case <-ctx.Done():
			return ctx.Err()
		case <-ticker.C:
		}
	}
}

// PutPinned makes a raw and its active consumer reference visible under one
// lock. The caller must stop on any error (raw may already be durable). Persist
// commit/checkpoint completion before ReleaseRaw. Concurrent maintenance cannot
// slip between raw acquisition and pin creation. Put is for unreferenced data.
func (a *Archive) PutPinned(owner Owner, data []byte, reference string) (RawRef, error) {
	a.mu.Lock()
	defer a.mu.Unlock()
	if !validLabel(reference, 512) {
		return RawRef{}, ErrOwnership
	}
	ref, err := a.put(owner, data)
	if err != nil {
		return ref, err
	}
	if _, gone := a.reclaimed[ref.ID]; gone {
		return ref, ErrRawReclaimed
	}
	p := rawPin{ref, reference, true}
	if old, ok := a.pins[p.key()]; ok && old == p {
		return ref, nil
	}
	if err := a.appendEvent(archiveEvent{Kind: "pin", Pin: &p}); err != nil {
		return ref, err
	}
	return ref, nil
}

// ArchivedManifest makes external raw recovery discoverable after local GC and
// restart. Verification is still mandatory against the configured trusted sink.
func (a *Archive) ArchivedManifest(ref RawRef) (ArchiveManifest, error) {
	a.mu.Lock()
	defer a.mu.Unlock()
	if err := a.usable(); err != nil {
		return ArchiveManifest{}, err
	}
	m, ok := a.manifests[ref.ID]
	if !ok || m.Raw != ref {
		return ArchiveManifest{}, ErrOwnership
	}
	for _, e := range a.events {
		if e.Manifest != nil && e.Manifest.Raw == ref {
			var disk archiveEvent
			if err := a.load(jsonID(e)+".event", &disk); err != nil {
				return ArchiveManifest{}, err
			}
			if jsonID(disk) != jsonID(e) {
				a.failed = true
				return ArchiveManifest{}, ErrCorrupt
			}
		}
	}
	return m, nil
}
