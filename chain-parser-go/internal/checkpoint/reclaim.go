package checkpoint

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"time"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/domain"
	"xone/chain-parser-go/internal/journal"
)

type RetentionApplyRequest struct {
	PlanID       string `json:"plan_id"`
	ObservedAtMS int64  `json:"observed_at_ms"`
}
type RetentionResumeRequest struct {
	OperationID string `json:"operation_id"`
}
type reclaimFault func(string) error

// ApplyRetention requires a current-authority guard and the caller's single
// publication gate for this private namespace. It never deletes remote objects.
func (r *Remote) ApplyRetention(ctx context.Context, dir string, boot journal.Bootstrap, policy config.CheckpointRetention, request RetentionApplyRequest, now time.Time, guard func() error) (ReclaimResult, error) {
	if guard == nil {
		return ReclaimResult{}, errors.New("CHECKPOINT_RECLAIM_AUTHORITY_REQUIRED")
	}
	return r.applyRetention(ctx, dir, boot, policy, request, now, guard, nil)
}
func (r *Remote) ResumeRetention(ctx context.Context, dir string, boot journal.Bootstrap, policy config.CheckpointRetention, request RetentionResumeRequest, now time.Time, guard func() error) (ReclaimResult, error) {
	if guard == nil {
		return ReclaimResult{}, errors.New("CHECKPOINT_RECLAIM_AUTHORITY_REQUIRED")
	}
	return r.resumeRetention(ctx, dir, boot, policy, request, now, guard, nil)
}
func reclaimGuard(ctx context.Context, guard func() error) func() error {
	return func() error {
		if ctx.Err() != nil {
			return ctx.Err()
		}
		if guard != nil {
			return guard()
		}
		return nil
	}
}
func invokeFault(fault reclaimFault, event string) error {
	if fault != nil {
		return fault(event)
	}
	return nil
}
func reclaimEnabled(policy config.CheckpointRetention) error {
	if e := policy.Validate(); e != nil {
		return e
	}
	if !policy.ReclaimEnabled {
		return errors.New("CHECKPOINT_RECLAIM_DISABLED")
	}
	return nil
}
func (r *Remote) applyRetention(parent context.Context, dir string, boot journal.Bootstrap, policy config.CheckpointRetention, request RetentionApplyRequest, now time.Time, guard func() error, fault reclaimFault) (ReclaimResult, error) {
	ctx, cancel := context.WithTimeout(parent, r.timeout)
	defer cancel()
	check := reclaimGuard(ctx, guard)
	if e := reclaimEnabled(policy); e != nil {
		return ReclaimResult{}, e
	}
	if e := check(); e != nil {
		return ReclaimResult{}, e
	}
	if !digest(request.PlanID) {
		return ReclaimResult{}, errors.New("CHECKPOINT_RECLAIM_REQUEST")
	}
	previous, read, e := r.loadReclaim(dir, policy.MaxReadBytes)
	if e != nil {
		return ReclaimResult{}, e
	}
	if previous != nil {
		if previous.Status != "COMPLETE" && previous.StoreSHA256 != rawSHA([]byte(r.base)) {
			return previous.result(false), errors.New("CHECKPOINT_RECLAIM_STORE_CHANGED")
		}
		if e = r.reclaimAuthority(previous, boot); e != nil {
			return previous.result(false), e
		}
		if previous.OperationID == request.PlanID && previous.Status == "COMPLETE" {
			result := previous.result(false)
			if e = r.confirmReclaimReceipt(dir, previous, policy.MaxReadBytes-read); e != nil {
				return result, e
			}
			result.Idempotent = true
			result.ReceiptConfirmed = true
			result.ReadBytes = 2 * read
			return result, check()
		}
		if previous.Status != "COMPLETE" {
			return previous.result(false), errors.New("CHECKPOINT_RECLAIM_PENDING")
		}
	}
	if request.ObservedAtMS < 0 || request.ObservedAtMS > now.UnixMilli() || uint64(now.UnixMilli()-request.ObservedAtMS) > policy.PlanTTLMS {
		return ReclaimResult{}, errors.New("CHECKPOINT_RECLAIM_PLAN_EXPIRED")
	}
	plan, e := r.planRetention(dir, boot, policy, time.UnixMilli(request.ObservedAtMS), check, read, 1)
	if e != nil {
		return ReclaimResult{}, e
	}
	if plan.PlanID != request.PlanID {
		return ReclaimResult{}, errors.New("CHECKPOINT_RECLAIM_PLAN_CHANGED")
	}
	budget := &reclaimBudget{policy: policy, guard: check, read: read + plan.ReadBytes, entries: 1 + plan.ScannedEntries, verified: reclaimVerified(plan)}
	state := &reclaimJournal{BaseURL: r.base, Schema: 1, StoreSHA256: rawSHA([]byte(r.base)), OperationID: plan.PlanID, NamespaceSHA256: plan.NamespaceSHA256, Reference: boot.Snapshot.Reference, Cursor: plan.Cursor, Policy: policy, StartedAtMS: now.UnixMilli(), Status: "PREPARED", Objects: []reclaimObject{}}
	if previous != nil {
		result := previous.result(false)
		state.PriorObjects = result.TotalObjects
		state.PriorBytes = result.TotalBytes
	}
	namespace, e := openReclaimNamespace(dir)
	if e != nil {
		return ReclaimResult{}, e
	}
	defer namespace.Close()
	files := 0
	for _, row := range plan.Objects {
		if !row.Selected {
			continue
		}
		object, e := r.captureReclaimObject(namespace, row, budget)
		if e != nil {
			return ReclaimResult{}, e
		}
		files += len(object.Files)
		if files > maxReclaimFiles {
			return ReclaimResult{}, errors.New("CHECKPOINT_RECLAIM_FILE_LIMIT")
		}
		if e = r.verifyRemoteReclaim(ctx, &object, budget); e != nil {
			return ReclaimResult{}, e
		}
		state.Objects = append(state.Objects, object)
	}
	if e = check(); e != nil {
		return ReclaimResult{}, e
	}
	root, ledger, e := openReclaimLedger(dir)
	if e != nil {
		return ReclaimResult{}, e
	}
	defer root.Close()
	defer ledger.Close()
	if previous != nil {
		if e = cleanupCompletedJob(ledger, previous, budget); e != nil {
			return ReclaimResult{}, e
		}
	}
	if e = cleanReclaimWorkspace(ledger, previous, state.OperationID, budget); e != nil {
		return ReclaimResult{}, e
	}
	if e = safeDirectory(ledger, state.OperationID); e != nil {
		return ReclaimResult{}, e
	}
	if e = r.saveReclaim(ledger, state); e != nil {
		return state.result(false), e
	}
	if e = invokeFault(fault, "intent_persisted"); e != nil {
		return state.result(false), e
	}
	return r.executeReclaim(ctx, root, ledger, namespace, boot, state, budget, now, false, fault)
}
func (r *Remote) resumeRetention(parent context.Context, dir string, boot journal.Bootstrap, policy config.CheckpointRetention, request RetentionResumeRequest, now time.Time, guard func() error, fault reclaimFault) (ReclaimResult, error) {
	ctx, cancel := context.WithTimeout(parent, r.timeout)
	defer cancel()
	check := reclaimGuard(ctx, guard)
	if e := reclaimEnabled(policy); e != nil {
		return ReclaimResult{}, e
	}
	if e := check(); e != nil {
		return ReclaimResult{}, e
	}
	if !digest(request.OperationID) {
		return ReclaimResult{}, errors.New("CHECKPOINT_RECLAIM_REQUEST")
	}
	state, read, e := r.loadReclaim(dir, policy.MaxReadBytes)
	if e != nil {
		return ReclaimResult{}, e
	}
	if state == nil || state.OperationID != request.OperationID {
		return ReclaimResult{}, errors.New("CHECKPOINT_RECLAIM_OPERATION_NOT_FOUND")
	}
	if state.StoreSHA256 != rawSHA([]byte(r.base)) {
		return state.result(true), errors.New("CHECKPOINT_RECLAIM_STORE_CHANGED")
	}
	if e = r.reclaimAuthority(state, boot); e != nil {
		return state.result(true), e
	}
	if state.Status == "COMPLETE" {
		result := state.result(true)
		if e = r.confirmReclaimReceipt(dir, state, policy.MaxReadBytes-read); e != nil {
			return result, e
		}
		result.ReceiptConfirmed = true
		result.Idempotent = true
		result.ReadBytes = 2 * read
		return result, check()
	}
	plan, e := r.planRetention(dir, boot, policy, now, check, read, 1)
	if e != nil {
		return state.result(true), e
	}
	budget := &reclaimBudget{policy: policy, guard: check, read: read + plan.ReadBytes, entries: 1 + plan.ScannedEntries, verified: reclaimVerified(plan)}
	namespace, e := openReclaimNamespace(dir)
	if e != nil {
		return state.result(true), e
	}
	defer namespace.Close()
	root, ledger, e := openReclaimLedger(dir)
	if e != nil {
		return state.result(true), e
	}
	defer root.Close()
	defer ledger.Close()
	if e = cleanReclaimWorkspace(ledger, state, state.OperationID, budget); e != nil {
		return state.result(true), e
	}
	return r.executeReclaim(ctx, root, ledger, namespace, boot, state, budget, now, true, fault)
}
func (r *Remote) reclaimAuthority(state *reclaimJournal, boot journal.Bootstrap) error {
	a, b := state.Reference, boot.Snapshot.Reference
	if a.Block.ChainKey != b.Block.ChainKey || a.OwnerService != b.OwnerService || a.OwnerAssignmentVersion > b.OwnerAssignmentVersion || a.WriterEpoch > b.WriterEpoch || a.CanonicalEpoch > b.CanonicalEpoch || a.JournalSeq > b.JournalSeq {
		return errors.New("CHECKPOINT_RECLAIM_AUTHORITY")
	}
	if _, e := EncodeBootstrap(boot, r.maxBytes); e != nil {
		return e
	}
	return nil
}
func openReclaimNamespace(dir string) (*os.Root, error) {
	path := filepath.Join(dir, "public-v2")
	info, e := os.Lstat(path)
	if e != nil || !info.IsDir() || info.Mode()&os.ModeSymlink != 0 {
		return nil, errors.New("CHECKPOINT_RECLAIM_DIRECTORY")
	}
	root, e := os.OpenRoot(path)
	if e != nil {
		return nil, errors.New("CHECKPOINT_RECLAIM_DIRECTORY")
	}
	return root, nil
}
func openReclaimLedger(dir string) (*os.Root, *os.Root, error) {
	root, e := os.OpenRoot(dir)
	if e != nil {
		return nil, nil, errors.New("CHECKPOINT_RECLAIM_DIRECTORY")
	}
	if e = safeDirectory(root, "retirements"); e != nil {
		root.Close()
		return nil, nil, e
	}
	ledger, e := root.OpenRoot("retirements")
	if e != nil {
		root.Close()
		return nil, nil, errors.New("CHECKPOINT_RECLAIM_DIRECTORY")
	}
	return root, ledger, nil
}
func (r *Remote) captureReclaimObject(namespace *os.Root, row RetentionObject, budget *reclaimBudget) (reclaimObject, error) {
	var object reclaimObject
	if !digest(row.ObjectSHA256) || row.Reason != "RETENTION_ELIGIBLE" || row.ContentCoverage != "FULL_DECODE_AND_STORE_CERTIFICATE" || !row.Selected {
		return object, errors.New("CHECKPOINT_RECLAIM_SELECTION")
	}
	info, e := namespace.Lstat(row.ObjectSHA256)
	if e != nil || !info.IsDir() || info.Mode()&os.ModeSymlink != 0 {
		return object, errors.New("CHECKPOINT_RECLAIM_ENTRY")
	}
	root, e := namespace.OpenRoot(row.ObjectSHA256)
	if e != nil {
		return object, errors.New("CHECKPOINT_RECLAIM_DIRECTORY")
	}
	defer root.Close()
	files, e := budget.list(root)
	if e != nil {
		return object, e
	}
	manifest, e := budget.file(root, "manifest.json", maxManifestBytes)
	if e != nil {
		return object, e
	}
	m, e := ParseBootstrapManifest(manifest, row.ManifestSHA256, r.maxBytes)
	if e != nil || m.SHA256 != row.ObjectSHA256 {
		return object, errors.New("CHECKPOINT_RECLAIM_MANIFEST")
	}
	object.Manifest = m
	object.ManifestSHA256 = row.ManifestSHA256
	object.Phase = "PREPARED"
	var payload []byte
	var certificate []byte
	for _, name := range sortedReclaimFiles(files) {
		if !reclaimName(name) || !files[name].Mode().IsRegular() {
			return object, errors.New("CHECKPOINT_RECLAIM_ENTRY")
		}
		maximum := uint64(maxManifestBytes)
		if name == "bootstrap.pb" {
			maximum = m.Size
		}
		raw, e := budget.file(root, name, maximum)
		if e != nil {
			return object, e
		}
		object.Files = append(object.Files, reclaimFile{Name: name, Size: uint64(len(raw)), SHA256: rawSHA(raw)})
		object.Bytes += uint64(len(raw))
		if name == "bootstrap.pb" {
			payload = raw
		}
		if name == r.completionName() {
			certificate = raw
		}
	}
	if _, e = DecodeBootstrap(manifest, payload, row.ManifestSHA256, r.maxBytes); e != nil {
		return object, e
	}
	expected, e := json.Marshal(r.publication(Publication{ManifestSHA256: row.ManifestSHA256}, m))
	if e != nil || !bytes.Equal(expected, certificate) || object.Bytes != row.Bytes || uint64(len(object.Files)) != row.Files {
		return object, errors.New("CHECKPOINT_RECLAIM_CERTIFICATE")
	}
	return object, nil
}
func (r *Remote) verifyRemoteReclaim(ctx context.Context, object *reclaimObject, budget *reclaimBudget) error {
	raw, e := json.Marshal(object.Manifest)
	if e != nil {
		return errors.New("CHECKPOINT_RECLAIM_MANIFEST")
	}
	if _, e = ParseBootstrapManifest(raw, object.ManifestSHA256, r.maxBytes); e != nil {
		return e
	}
	manifestURL, payloadURL, e := r.objects(object.Manifest.SHA256)
	if e != nil {
		return e
	}
	for _, request := range []struct {
		url  string
		size uint64
		sha  string
	}{{manifestURL, uint64(len(raw)), object.ManifestSHA256}, {payloadURL, object.Manifest.Size, object.Manifest.SHA256}} {
		if e = budget.check(); e != nil {
			return e
		}
		if budget.read > budget.policy.MaxReadBytes || request.size > budget.policy.MaxReadBytes-budget.read {
			return errors.New("CHECKPOINT_RECLAIM_READ_BUDGET")
		}
		budget.read += request.size
		actual, ok, e := r.get(ctx, request.url, int64(request.size))
		if e != nil {
			return e
		}
		if !ok || uint64(len(actual)) != request.size || rawSHA(actual) != request.sha {
			return errors.New("CHECKPOINT_RECLAIM_REMOTE_BACKUP")
		}
	}
	return budget.check()
}
func cleanupCompletedJob(ledger *os.Root, state *reclaimJournal, budget *reclaimBudget) error {
	info, e := ledger.Lstat(state.OperationID)
	if errors.Is(e, os.ErrNotExist) {
		return nil
	}
	if e != nil || !info.IsDir() || info.Mode()&os.ModeSymlink != 0 {
		return errors.New("CHECKPOINT_RECLAIM_DIRECTORY")
	}
	root, e := ledger.OpenRoot(state.OperationID)
	if e != nil {
		return errors.New("CHECKPOINT_RECLAIM_DIRECTORY")
	}
	files, e := budget.list(root)
	root.Close()
	if e != nil {
		return e
	}
	if len(files) != 0 {
		return errors.New("CHECKPOINT_RECLAIM_COMPLETED_CONFLICT")
	}
	if e = budget.check(); e != nil {
		return e
	}
	if e = ledger.Remove(state.OperationID); e != nil {
		return errors.New("CHECKPOINT_RECLAIM_DIRECTORY")
	}
	return syncRoot(ledger)
}

// Every pending object's last copy and all current candidate references are
// reconsidered after restart. A stricter policy can block, never widen, a resume.
func (r *Remote) protectReclaim(namespace *os.Root, boot journal.Bootstrap, state *reclaimJournal, budget *reclaimBudget) (map[string]bool, error) {
	payload, e := EncodeBootstrap(boot, r.maxBytes)
	if e != nil {
		return nil, e
	}
	protected := map[string]bool{rawSHA(payload): true}
	top, e := budget.list(namespace)
	if e != nil {
		return nil, e
	}
	references := map[string]domain.SnapshotRef{}
	for name, meta := range top {
		if meta.IsDir() {
			if !digest(name) {
				return nil, errors.New("CHECKPOINT_RECLAIM_ENTRY")
			}
			root, e := namespace.OpenRoot(name)
			if e != nil {
				return nil, errors.New("CHECKPOINT_RECLAIM_DIRECTORY")
			}
			if _, e = root.Lstat("manifest.json"); errors.Is(e, os.ErrNotExist) {
				root.Close()
				continue
			}
			raw, e := budget.file(root, "manifest.json", maxManifestBytes)
			root.Close()
			if e != nil {
				return nil, e
			}
			m, e := ParseBootstrapManifest(raw, rawSHA(raw), r.maxBytes)
			if e != nil || m.SHA256 != name {
				return nil, errors.New("CHECKPOINT_RECLAIM_MANIFEST")
			}
			if verified, ok := budget.verified[name]; ok && verified == m.Reference {
				references[name] = m.Reference
			}
		} else if strings.HasPrefix(name, "candidate-") {
			raw, e := budget.file(namespace, name, maxManifestBytes)
			if e != nil {
				return nil, e
			}
			var p RemotePublication
			if json.Unmarshal(raw, &p) != nil {
				return nil, errors.New("CHECKPOINT_RECLAIM_CANDIDATE")
			}
			canonical, _ := json.Marshal(p)
			encoded, _ := json.Marshal(p.Reference)
			if !bytes.Equal(raw, canonical) {
				return nil, errors.New("CHECKPOINT_RECLAIM_CANDIDATE")
			}
			if _, e = ParseBootstrapManifest(encoded, p.ManifestSHA256, r.maxBytes); e != nil {
				return nil, e
			}
			protected[p.Reference.SHA256] = true
		}
	}
	for _, object := range state.Objects {
		if object.Phase != "DONE" {
			references[object.Manifest.SHA256] = object.Manifest.Reference
		}
	}
	names := make([]string, 0, len(references))
	for name, ref := range references {
		head := boot.Snapshot.Reference
		if ref.OwnerAssignmentVersion > head.OwnerAssignmentVersion || ref.WriterEpoch > head.WriterEpoch || ref.CanonicalEpoch > head.CanonicalEpoch || ref.JournalSeq > head.JournalSeq {
			protected[name] = true
			continue
		}
		names = append(names, name)
	}
	sort.Slice(names, func(i, j int) bool {
		a, b := references[names[i]], references[names[j]]
		if a.JournalSeq != b.JournalSeq {
			return a.JournalSeq > b.JournalSeq
		}
		return names[i] < names[j]
	})
	for i, name := range names {
		if uint64(i) < budget.policy.KeepLatest {
			protected[name] = true
		}
	}
	return protected, budget.check()
}

func (r *Remote) executeReclaim(ctx context.Context, root, ledger, namespace *os.Root, boot journal.Bootstrap, state *reclaimJournal, budget *reclaimBudget, now time.Time, resumed bool, fault reclaimFault) (ReclaimResult, error) {
	completionSaved := false
	result := func(e error) (ReclaimResult, error) {
		value := state.result(resumed)
		value.ReceiptConfirmed = completionSaved
		if state.Status == "COMPLETE" && !completionSaved {
			value.Status = "COMPLETION_UNCONFIRMED"
			value.Applied = false
		}
		value.ReadBytes = budget.read
		value.ScannedEntries = budget.entries
		return value, e
	}
	job, e := ledger.OpenRoot(state.OperationID)
	if e != nil {
		return result(errors.New("CHECKPOINT_RECLAIM_DIRECTORY"))
	}
	defer job.Close()
	if e = budget.check(); e != nil {
		return result(e)
	}
	processed := uint64(0)
	for index := range state.Objects {
		object := &state.Objects[index]
		if object.Phase == "DONE" {
			continue
		}
		if processed >= budget.policy.MaxRemove {
			return result(nil)
		}
		processed++
		protected, e := r.protectReclaim(namespace, boot, state, budget)
		if e != nil {
			return result(e)
		}
		if protected[object.Manifest.SHA256] {
			return result(errors.New("CHECKPOINT_RECLAIM_PROTECTED"))
		}
		cutoff := now.Add(-time.Duration(budget.policy.MinAgeMS) * time.Millisecond).UnixMilli()
		if cutoff < 0 || object.Manifest.Reference.Block.TimestampMS > uint64(cutoff) {
			return result(errors.New("CHECKPOINT_RECLAIM_AGE"))
		}
		source, sourceExists, e := reclaimDirectoryExists(namespace, object.Manifest.SHA256)
		if e != nil {
			return result(e)
		}
		if source != nil {
			source.Close()
		}
		quarantine, quarantineExists, e := reclaimDirectoryExists(job, object.Manifest.SHA256)
		if e != nil {
			return result(e)
		}
		if quarantine != nil {
			quarantine.Close()
		}
		if sourceExists && quarantineExists {
			return result(errors.New("CHECKPOINT_RECLAIM_CONFLICT"))
		}
		if !sourceExists && !quarantineExists {
			if object.Phase != "QUARANTINED" {
				return result(errors.New("CHECKPOINT_RECLAIM_OBJECT_MISSING"))
			}
			object.Phase = "DONE"
			if e = r.saveReclaim(ledger, state); e != nil {
				return result(e)
			}
			continue
		}
		if e = r.verifyRemoteReclaim(ctx, object, budget); e != nil {
			return result(e)
		}
		if sourceExists {
			if object.Phase != "PREPARED" {
				return result(errors.New("CHECKPOINT_RECLAIM_CONFLICT"))
			}
			source, e := namespace.OpenRoot(object.Manifest.SHA256)
			if e != nil {
				return result(errors.New("CHECKPOINT_RECLAIM_DIRECTORY"))
			}
			if e = verifyReclaimFiles(source, object, budget, false); e != nil {
				source.Close()
				return result(e)
			}
			info, e := source.Stat("manifest.json")
			source.Close()
			if e != nil || info.ModTime().After(time.UnixMilli(cutoff)) {
				return result(errors.New("CHECKPOINT_RECLAIM_AGE"))
			}
			if e = budget.check(); e != nil {
				return result(e)
			}
			if e = invokeFault(fault, "before_rename"); e != nil {
				return result(e)
			}
			from := filepath.Join("public-v2", object.Manifest.SHA256)
			to := filepath.Join("retirements", state.OperationID, object.Manifest.SHA256)
			if e = root.Rename(from, to); e != nil {
				return result(errors.New("CHECKPOINT_RECLAIM_RENAME"))
			}
			if e = syncRoot(namespace); e != nil {
				return result(e)
			}
			if e = syncRoot(job); e != nil {
				return result(e)
			}
			if e = invokeFault(fault, "after_rename"); e != nil {
				return result(e)
			}
		}
		quarantine, e = job.OpenRoot(object.Manifest.SHA256)
		if e != nil {
			return result(errors.New("CHECKPOINT_RECLAIM_DIRECTORY"))
		}
		if e = verifyReclaimFiles(quarantine, object, budget, object.Phase == "QUARANTINED"); e != nil {
			quarantine.Close()
			return result(e)
		}
		object.Phase = "QUARANTINED"
		if e = r.saveReclaim(ledger, state); e != nil {
			quarantine.Close()
			return result(e)
		}
		if e = invokeFault(fault, "quarantine_persisted"); e != nil {
			quarantine.Close()
			return result(e)
		}
		for _, file := range object.Files {
			if e = budget.check(); e != nil {
				quarantine.Close()
				return result(e)
			}
			info, e := quarantine.Lstat(file.Name)
			if errors.Is(e, os.ErrNotExist) {
				continue
			}
			if e != nil || !info.Mode().IsRegular() {
				quarantine.Close()
				return result(errors.New("CHECKPOINT_RECLAIM_ENTRY"))
			}
			if e = quarantine.Remove(file.Name); e != nil {
				quarantine.Close()
				return result(errors.New("CHECKPOINT_RECLAIM_UNLINK"))
			}
			if e = syncRoot(quarantine); e != nil {
				quarantine.Close()
				return result(e)
			}
			if e = invokeFault(fault, "after_remove:"+file.Name); e != nil {
				quarantine.Close()
				return result(e)
			}
		}
		quarantine.Close()
		if e = budget.check(); e != nil {
			return result(e)
		}
		if e = job.Remove(object.Manifest.SHA256); e != nil {
			return result(errors.New("CHECKPOINT_RECLAIM_DIRECTORY"))
		}
		if e = syncRoot(job); e != nil {
			return result(e)
		}
		if e = invokeFault(fault, "directory_removed"); e != nil {
			return result(e)
		}
		object.Phase = "DONE"
		if e = r.saveReclaim(ledger, state); e != nil {
			return result(e)
		}
		delete(budget.verified, object.Manifest.SHA256)
		if e = invokeFault(fault, "object_committed"); e != nil {
			return result(e)
		}
	}
	state.Status = "COMPLETE"
	state.CompletedAtMS = time.Now().UnixMilli()
	if state.CompletedAtMS < state.StartedAtMS {
		state.CompletedAtMS = state.StartedAtMS
	}
	if e = invokeFault(fault, "before_complete_commit"); e != nil {
		return result(e)
	}
	if e = r.saveReclaim(ledger, state); e != nil {
		return result(e)
	}
	completionSaved = true
	if e = invokeFault(fault, "complete_persisted"); e != nil {
		return result(e)
	}
	if e = cleanupCompletedJob(ledger, state, budget); e != nil {
		return result(e)
	}
	return result(budget.check())
}
func reclaimDirectoryExists(root *os.Root, name string) (*os.Root, bool, error) {
	info, e := root.Lstat(name)
	if errors.Is(e, os.ErrNotExist) {
		return nil, false, nil
	}
	if e != nil || !info.IsDir() || info.Mode()&os.ModeSymlink != 0 {
		return nil, false, errors.New("CHECKPOINT_RECLAIM_DIRECTORY")
	}
	child, e := root.OpenRoot(name)
	if e != nil {
		return nil, false, errors.New("CHECKPOINT_RECLAIM_DIRECTORY")
	}
	return child, true, nil
}
func verifyReclaimFiles(root *os.Root, object *reclaimObject, budget *reclaimBudget, missingAllowed bool) error {
	actual, e := budget.list(root)
	if e != nil {
		return e
	}
	expected := map[string]reclaimFile{}
	for _, file := range object.Files {
		expected[file.Name] = file
	}
	if !missingAllowed && len(actual) != len(expected) {
		return errors.New("CHECKPOINT_RECLAIM_FILE_SET")
	}
	for name, info := range actual {
		file, ok := expected[name]
		if !ok || !info.Mode().IsRegular() || info.Size() < 0 || uint64(info.Size()) != file.Size {
			return errors.New("CHECKPOINT_RECLAIM_FILE_SET")
		}
		raw, e := budget.file(root, name, file.Size)
		if e != nil {
			return e
		}
		if rawSHA(raw) != file.SHA256 {
			return errors.New("CHECKPOINT_RECLAIM_FILE_CHANGED")
		}
	}
	return budget.check()
}

func reclaimVerified(plan RetentionPlan) map[string]domain.SnapshotRef {
	result := map[string]domain.SnapshotRef{}
	for _, row := range plan.Objects {
		if row.ContentCoverage == "FULL_DECODE_AND_STORE_CERTIFICATE" && row.Reference != nil {
			result[row.ObjectSHA256] = *row.Reference
		}
	}
	return result
}
