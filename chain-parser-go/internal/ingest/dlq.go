package ingest

import (
	"context"
	"encoding/hex"
	"errors"
	"fmt"
	"sort"
	"time"
	"unicode/utf8"
)

var (
	ErrCommitUnknown     = errors.New("INGEST_REPLAY_COMMIT_UNKNOWN")
	ErrReplayFence       = errors.New("INGEST_REPLAY_FENCED")
	ErrReplayBusy        = errors.New("INGEST_REPLAY_ACTIVE_OR_PENDING")
	ErrReplayFinal       = errors.New("INGEST_REPLAY_ATTEMPT_FINAL")
	ErrReplayInterrupted = errors.New("INGEST_REPLAY_INTERRUPTED")
)

type ReplayNamespace string

const (
	NamespaceOnline  ReplayNamespace = "online"
	NamespaceHistory ReplayNamespace = "history"
)

type ReplayStatus string

const (
	Resolved ReplayStatus = "Resolved"
	Retry    ReplayStatus = "Retry"
	Failed   ReplayStatus = "Failed"
)

// ReplayAuthority must come from the service's authoritative lease/generation.
// Local monotonic fencing supplements, but cannot replace, owner-side atomic CAS.
type ReplayAuthority struct {
	Namespace  ReplayNamespace
	Chain      string
	WriterID   string
	Fence      uint64
	Generation string
}

func (r ReplayAuthority) key() string {
	return jsonID(struct {
		Namespace ReplayNamespace
		Chain     string
	}{r.Namespace, r.Chain})
}
func validLabel(s string, max int) bool { return s != "" && len(s) <= max && utf8.ValidString(s) }
func validDigest(s string) bool {
	b, err := hex.DecodeString(s)
	return err == nil && len(b) == 32 && hex.EncodeToString(b) == s
}
func validNamespace(n ReplayNamespace) bool { return n == NamespaceOnline || n == NamespaceHistory }
func validAuthority(r ReplayAuthority) bool {
	return validNamespace(r.Namespace) && validLabel(r.Chain, 512) && validLabel(r.WriterID, 512) && r.Fence > 0 && validLabel(r.Generation, 512)
}

type ReplaySpec struct {
	DeadLetterID   string
	IncidentID     string `json:",omitempty"` // explicit immutable raw+scope failure cycle
	Raw            RawRef // includes complete source ownership and digest; never inferred
	DecoderVersion string // artifact/build identity supplied by the service
	Namespace      ReplayNamespace
	WriterID       string
	Fence          uint64
	Generation     string
	Attempt        uint64 // per incident+namespace; increment after terminal Failed/Retry, never for unknown commit
}

func (s ReplaySpec) authority() ReplayAuthority {
	return ReplayAuthority{s.Namespace, s.Raw.Owner.Chain, s.WriterID, s.Fence, s.Generation}
}
func DeadLetterID(d DeadLetter) string { return jsonID(d) }
func (s ReplaySpec) ID() string        { return jsonID(s) }

// A whole-raw owner publication is shared by every diagnostic/scope alias in
// this namespace. Scope belongs to failure identity, never to the commit lock.
func sameReplayDomain(a, b ReplaySpec) bool { return a.Raw == b.Raw && a.Namespace == b.Namespace }

type ReplayLimits struct {
	MaxRawBytes     int64
	MaxDecodedBytes int64
	Timeout         time.Duration
}
type ReplayIntent struct {
	AttemptID    string
	Spec         ReplaySpec
	OutputDigest string
	OutputBytes  int64
}
type CommitState string

const (
	CommitConfirmed    CommitState = "confirmed"
	CommitNotCommitted CommitState = "not_committed"
	CommitUnknown      CommitState = "unknown"
)

// Receipt must be obtained from an authoritative, durable owner transaction or
// query. Echoing an intent is not proof; ReceiptFor is only an identity helper.
type CommitReceipt struct {
	IncidentID     string `json:",omitempty"`
	AttemptID      string
	Namespace      ReplayNamespace
	Raw            RawRef
	DecoderVersion string
	WriterID       string
	Fence          uint64
	Generation     string
	OutputDigest   string
	State          CommitState
	Proof          string
}

func ReceiptFor(i ReplayIntent, state CommitState, proof string) CommitReceipt {
	s := i.Spec
	return CommitReceipt{IncidentID: s.IncidentID, AttemptID: i.AttemptID, Namespace: s.Namespace, Raw: s.Raw, DecoderVersion: s.DecoderVersion, WriterID: s.WriterID, Fence: s.Fence, Generation: s.Generation, OutputDigest: i.OutputDigest, State: state, Proof: proof}
}
func validReceipt(i ReplayIntent, r CommitReceipt) bool {
	return (r.State == CommitConfirmed || r.State == CommitNotCommitted || r.State == CommitUnknown) && validLabel(r.Proof, 2048) && r == ReceiptFor(i, r.State, r.Proof)
}

// Each Archive admits one replay actor; concurrent Replay returns ErrReplayBusy.
// Callbacks execute synchronously under the derived deadline and supplied byte
// bounds. Decode must bound allocation while decoding. Commit must atomically
// validate incident/namespace/source/digest/fence/generation and idempotently commit the
// WHOLE result keyed by AttemptID, returning an authoritative durable receipt.
// Confirm must fence/settle in-flight work before declaring not_committed; an
// instantaneous "not yet visible" read is CommitUnknown. A history replay never
// invokes online callbacks and rejects an online capability in its callback set.
// Callbacks must honor ctx; Go cannot forcibly stop an uncooperative callback.
type ReplayCallbacks struct {
	Decode         func(context.Context, ReplaySpec, []byte, int64) ([]byte, error)
	CommitOnline   func(context.Context, ReplayIntent, []byte) (CommitReceipt, error)
	ConfirmOnline  func(context.Context, ReplayIntent) (CommitReceipt, error)
	CommitHistory  func(context.Context, ReplayIntent, []byte) (CommitReceipt, error)
	ConfirmHistory func(context.Context, ReplayIntent) (CommitReceipt, error)
}

// Every transition is appended as a separate immutable event, including Retry,
// Failed and Resolved. Decoded bytes are retained on the prepared event only.
type ReplayRecord struct {
	AttemptID string
	Spec      ReplaySpec
	Intent    ReplayIntent
	Phase     string
	Status    ReplayStatus
	Message   string
	At        time.Time
	Decoded   []byte         `json:",omitempty"`
	Receipt   *CommitReceipt `json:",omitempty"`
}

func (a *Archive) validateReplay(r ReplayRecord) error {
	s := r.Spec
	d, ok := a.dlq[s.DeadLetterID]
	if !ok || d.Raw != s.Raw || !validAuthority(s.authority()) || !validLabel(s.DecoderVersion, 512) || r.AttemptID != s.ID() || r.Intent.AttemptID != r.AttemptID || r.Intent.Spec != s || r.At.IsZero() || len(r.Message) > 2048 || !utf8.ValidString(r.Message) {
		return ErrCorrupt
	}
	incident, bound := a.specIncident(s)
	if !bound {
		return ErrFailureIncident
	}
	old, exists := a.replays[r.AttemptID]
	if exists && (old.Spec != s || old.Phase == "finished") {
		return ErrCorrupt
	}
	if exists && old.Intent.OutputDigest != "" && (old.Intent != r.Intent) {
		return ErrCorrupt
	}
	switch r.Phase {
	case "started":
		if a.currentIncidents[incident.key()].ID != incident.ID {
			return ErrFailureIncident
		}
		for id, pending := range a.replays {
			if s.IncidentID != "" && id != r.AttemptID && sameReplayDomain(pending.Spec, s) && pending.Phase != "finished" {
				return ErrReplayBusy
			}
		}
		if exists || r.Status != Retry || r.Intent.OutputDigest != "" {
			return ErrCorrupt
		}
	case "prepared":
		if !exists || old.Phase != "started" || r.Status != Retry || digest(r.Decoded) != r.Intent.OutputDigest || int64(len(r.Decoded)) != r.Intent.OutputBytes {
			return ErrCorrupt
		}
	case "publishing":
		if a.currentIncidents[incident.key()].ID != incident.ID {
			return ErrFailureIncident
		}
		if !exists || old.Phase != "prepared" || r.Status != Retry {
			return ErrCorrupt
		}
	case "unknown":
		if !exists || (old.Phase != "publishing" && old.Phase != "unknown") || r.Status != Retry {
			return ErrCorrupt
		}
	case "finished":
		if !exists || (r.Status != Resolved && r.Status != Retry && r.Status != Failed) {
			return ErrCorrupt
		}
		if r.Status == Resolved {
			if (old.Phase != "publishing" && old.Phase != "unknown") || r.Receipt == nil || !validReceipt(r.Intent, *r.Receipt) || r.Receipt.State != CommitConfirmed {
				return ErrCorrupt
			}
		} else if old.Phase == "publishing" || old.Phase == "unknown" {
			if r.Receipt == nil || !validReceipt(r.Intent, *r.Receipt) || r.Receipt.State != CommitNotCommitted {
				return ErrCorrupt
			}
		}
	default:
		return ErrCorrupt
	}
	if r.Phase != "started" && r.Phase != "finished" && (!validDigest(r.Intent.OutputDigest) || r.Intent.OutputBytes < 0) {
		return ErrCorrupt
	}
	if r.Phase != "prepared" && len(r.Decoded) > 0 {
		return ErrCorrupt
	}
	return nil
}
func (a *Archive) AdvanceReplayFence(r ReplayAuthority) error {
	a.mu.Lock()
	defer a.mu.Unlock()
	if err := a.usable(); err != nil {
		return err
	}
	if old, ok := a.authorities[r.key()]; ok && old == r {
		return nil
	}
	return a.appendEvent(archiveEvent{Kind: "authority", Authority: &r})
}
func (a *Archive) quarantined(ref RawRef) bool {
	if a.failureHeld(ref) {
		return true
	}
	for _, d := range a.dlq {
		if d.Raw != ref {
			continue
		}
		i, ok := a.currentIncidents[failureKey(ref, d.Scope)]
		if !ok || !a.incidentResolved(i.ID, NamespaceOnline) {
			return true
		}
	}
	return false
}

// History success records a repair but carries no authority to clear online DLQ.
func (a *Archive) IsQuarantined(ref RawRef) (bool, error) {
	a.mu.Lock()
	defer a.mu.Unlock()
	if err := a.usable(); err != nil {
		return false, err
	}
	if a.raw[ref.ID] != ref {
		return false, ErrOwnership
	}
	return a.quarantined(ref), nil
}
func cloneReplay(r ReplayRecord) ReplayRecord {
	r.Decoded = append([]byte(nil), r.Decoded...)
	if r.Receipt != nil {
		c := *r.Receipt
		r.Receipt = &c
	}
	return r
}

// ReplayAudit selects the complete immutable raw+scope history addressed by a
// diagnostic, including its aliases and all failure cycles. A historical record
// is evidence, not authorization to resolve the current incident.
func (a *Archive) ReplayAudit(dlqID string) ([]ReplayRecord, error) {
	a.mu.Lock()
	defer a.mu.Unlock()
	if err := a.usable(); err != nil {
		return nil, err
	}
	if _, ok := a.dlq[dlqID]; !ok {
		return nil, ErrOwnership
	}
	var out []ReplayRecord
	if err := a.verifyReplayAudit(dlqID); err != nil {
		return nil, err
	}
	for _, e := range a.events {
		if e.Replay != nil {
			d := a.dlq[dlqID]
			i, ok := a.specIncident(e.Replay.Spec)
			if !ok || i.key() != failureKey(d.Raw, d.Scope) {
				continue
			}
			out = append(out, cloneReplay(*e.Replay))
		}
	}

	return out, nil
}
func (a *Archive) verifyReplayAudit(dlqID string) error {
	d, ok := a.dlq[dlqID]
	if !ok {
		return ErrOwnership
	}
	return a.verifyRawJournal(d.Raw)
}

func boundedMessage(err error) string {
	if err == nil {
		return ""
	}
	s := []rune(err.Error())
	out := ""
	for _, r := range s {
		if len(out)+len(string(r)) > 2048 {
			break
		}
		out += string(r)
	}
	return out
}
func (a *Archive) saveReplay(r ReplayRecord) (ReplayRecord, error) {
	a.mu.Lock()
	defer a.mu.Unlock()
	if err := a.usable(); err != nil {
		return r, err
	}
	r.At = time.Now().UTC()
	if err := a.appendEvent(archiveEvent{Kind: "replay", Replay: &r}); err != nil {
		return r, err
	}
	return cloneReplay(r), nil
}
func (a *Archive) Replay(ctx context.Context, s ReplaySpec, l ReplayLimits, cb ReplayCallbacks) (ReplayRecord, error) {
	if ctx.Err() != nil {
		return ReplayRecord{}, ctx.Err()
	}
	if l.Timeout <= 0 || l.MaxRawBytes <= 0 || l.MaxDecodedBytes <= 0 {
		return ReplayRecord{}, ErrBudget
	}
	commit, confirm := cb.CommitOnline, cb.ConfirmOnline
	if s.Namespace == NamespaceHistory {
		if cb.CommitOnline != nil || cb.ConfirmOnline != nil {
			return ReplayRecord{}, ErrOwnership
		}
		commit, confirm = cb.CommitHistory, cb.ConfirmHistory
	} else if s.Namespace != NamespaceOnline || cb.CommitHistory != nil || cb.ConfirmHistory != nil {
		return ReplayRecord{}, ErrOwnership
	}
	ctx, cancel := context.WithTimeout(ctx, l.Timeout)
	defer cancel()
	a.mu.Lock()
	if err := a.usable(); err != nil {
		a.mu.Unlock()
		return ReplayRecord{}, err
	}
	d, ok := a.dlq[s.DeadLetterID]
	if !ok || d.Raw != s.Raw || !validAuthority(s.authority()) || !validLabel(s.DecoderVersion, 512) {
		a.mu.Unlock()
		return ReplayRecord{}, ErrOwnership
	}
	// Verify every alias/cycle whose durable state controls this raw commit.
	if err := a.verifyRawJournal(s.Raw); err != nil {
		a.mu.Unlock()
		return ReplayRecord{}, err
	}
	id := s.ID()
	old, exists := a.replays[id]
	if len(a.replaying) > 0 {
		a.mu.Unlock()
		return ReplayRecord{}, ErrReplayBusy
	}
	recovering := exists && (old.Phase == "publishing" || old.Phase == "unknown")
	if a.failureHeld(s.Raw) && !recovering {
		a.mu.Unlock()
		return ReplayRecord{}, ErrFailureRecoveryRequired
	}
	blockedExisting := false
	// Diagnostic IDs, cycle changes and distinct scopes cannot bypass a pending
	// whole-raw publication. Reconcile the exact old Spec returned by PendingReplays.
	for otherID, pending := range a.replays {
		if otherID != id && sameReplayDomain(pending.Spec, s) && pending.Phase != "finished" {
			if recovering {
				continue
			} // Only authoritative confirmation of exact old intent.
			if exists && (old.Phase == "started" || old.Phase == "prepared") {
				blockedExisting = true
				continue
			}
			a.mu.Unlock()
			return ReplayRecord{}, ErrReplayBusy
		}
	}
	incident, bound := a.specIncident(s)
	if !bound || (s.IncidentID == "" && !exists) {
		a.mu.Unlock()
		return ReplayRecord{}, ErrFailureIncident
	}
	currentIncident, hasIncident := a.currentIncidents[incident.key()]
	staleIncident := !hasIncident || currentIncident.ID != incident.ID

	if staleIncident && (!exists || old.Phase == "finished") {
		a.mu.Unlock()
		return ReplayRecord{}, ErrFailureIncident
	}
	authority := s.authority()
	current, has := a.authorities[authority.key()]
	// Historical unknown commits can only confirm their original owner identity.
	// Every other request, including exact cached success, must pass current fencing.
	if !recovering && !staleIncident && !blockedExisting && has && (authority.Fence < current.Fence || (authority.Fence == current.Fence && authority != current)) {
		a.mu.Unlock()
		return ReplayRecord{}, ErrReplayFence
	}
	if exists && old.Phase == "finished" {
		a.mu.Unlock()
		if old.Status == Resolved {
			return cloneReplay(old), nil
		}
		return cloneReplay(old), ErrReplayFinal
	}
	if !exists {
		for otherID, r := range a.replays {
			if otherID == id || r.Spec.Namespace != s.Namespace {
				continue
			}
			otherIncident, ok := a.specIncident(r.Spec)
			if !ok || otherIncident.ID != incident.ID {
				continue
			}
			// A success is reusable ONLY for the exact original Spec. A new failure must
			// first be observed with Quarantine[Incident], creating its own cycle.
			if r.Status == Resolved || s.Attempt <= r.Spec.Attempt {
				a.mu.Unlock()
				return ReplayRecord{}, ErrReplayFinal
			}
		}
	}
	if s.Raw.Bytes > l.MaxRawBytes || l.MaxDecodedBytes > a.limits.MaxRecordBytes || (exists && old.Phase == "prepared" && old.Intent.OutputBytes > l.MaxDecodedBytes) {
		a.mu.Unlock()
		return ReplayRecord{}, ErrBudget
	}
	if !exists && (cb.Decode == nil || commit == nil) {
		a.mu.Unlock()
		return ReplayRecord{}, ErrOwnership
	}
	if recovering && confirm == nil {
		a.mu.Unlock()
		return cloneReplay(old), ErrCommitUnknown
	}
	if !recovering && !staleIncident && !blockedExisting && (!has || authority != current) {
		if err := a.appendEvent(archiveEvent{Kind: "authority", Authority: &authority}); err != nil {
			a.mu.Unlock()
			return ReplayRecord{}, err
		}
	}
	a.replaying[s.DeadLetterID] = true
	a.active[s.Raw.ID]++
	a.mu.Unlock()
	defer func() { a.mu.Lock(); delete(a.replaying, s.DeadLetterID); a.active[s.Raw.ID]--; a.mu.Unlock() }()
	r := old
	finish := func(status ReplayStatus, cause error, receipt *CommitReceipt) (ReplayRecord, error) {
		previous := cloneReplay(r)
		r.Phase = "finished"
		r.Status = status
		r.Message = boundedMessage(cause)
		r.Decoded = nil
		r.Receipt = receipt
		saved, err := a.saveReplay(r)
		if err != nil {
			if previous.Phase == "publishing" || previous.Phase == "unknown" {
				previous.Status = Retry
				previous.Phase = "unknown"
				previous.Receipt = nil
				return previous, errors.Join(ErrCommitUnknown, cause, err, ctx.Err())
			}
			return previous, errors.Join(cause, err, ctx.Err())
		}
		return saved, errors.Join(cause, ctx.Err())
	}
	unknown := func(cause error) (ReplayRecord, error) {
		cause = errors.Join(ErrCommitUnknown, cause, ctx.Err())
		r.Phase = "unknown"
		r.Status = Retry
		r.Message = boundedMessage(cause)
		r.Decoded = nil
		r.Receipt = nil
		saved, err := a.saveReplay(r)
		return saved, errors.Join(cause, err)
	}
	if recovering {
		receipt, err := confirm(ctx, r.Intent)
		if err != nil || ctx.Err() != nil || !validReceipt(r.Intent, receipt) || receipt.State == CommitUnknown {
			return unknown(err)
		}
		if receipt.State == CommitConfirmed {
			return finish(Resolved, nil, &receipt)
		}
		return finish(Retry, ErrReplayInterrupted, &receipt)
	}
	if blockedExisting {
		return finish(Retry, ErrReplayBusy, nil)
	}
	if staleIncident {
		return finish(Retry, ErrFailureIncident, nil)
	}
	if exists && r.Phase == "started" {
		return finish(Retry, ErrReplayInterrupted, nil)
	}
	if !exists {
		r = ReplayRecord{AttemptID: id, Spec: s, Intent: ReplayIntent{AttemptID: id, Spec: s}, Phase: "started", Status: Retry}
		var err error
		r, err = a.saveReplay(r)
		if err != nil {
			return r, err
		}
		raw, err := a.Read(s.Raw)
		if err != nil {
			return finish(Failed, err, nil)
		}
		output, err := cb.Decode(ctx, s, raw, l.MaxDecodedBytes)
		if ctx.Err() != nil {
			return finish(Retry, errors.Join(err, ctx.Err()), nil)
		}
		if err != nil {
			return finish(Failed, err, nil)
		}
		if int64(len(output)) > l.MaxDecodedBytes {
			return finish(Failed, ErrBudget, nil)
		}
		r.Intent.OutputDigest = digest(output)
		r.Intent.OutputBytes = int64(len(output))
		r.Decoded = append([]byte(nil), output...)
		r.Phase = "prepared"
		r, err = a.saveReplay(r)
		if err != nil {
			return r, err
		}
	}
	if commit == nil {
		return r, ErrOwnership
	}
	if ctx.Err() != nil {
		return finish(Retry, ctx.Err(), nil)
	}
	// Recheck fencing and durably record intent in the same lock before invoking
	// owner code. The owner CAS is still mandatory against cross-process takeover.
	a.mu.Lock()
	if err := a.usable(); err != nil {
		a.mu.Unlock()
		return r, err
	}
	if a.currentIncidents[incident.key()].ID != incident.ID {
		a.mu.Unlock()
		return finish(Retry, ErrFailureIncident, nil)
	}
	if a.authorities[authority.key()] != authority {
		a.mu.Unlock()
		return finish(Retry, ErrReplayFence, nil)
	}
	output := append([]byte(nil), r.Decoded...)
	r.Decoded = nil
	r.Phase = "publishing"
	r.At = time.Now().UTC()
	err := a.appendEvent(archiveEvent{Kind: "replay", Replay: &r})
	a.mu.Unlock()
	if err != nil {
		return r, err
	}
	receipt, err := commit(ctx, r.Intent, output)
	if err != nil || ctx.Err() != nil || !validReceipt(r.Intent, receipt) || receipt.State == CommitUnknown {
		return unknown(err)
	}
	if receipt.State == CommitConfirmed {
		return finish(Resolved, nil, &receipt)
	}
	return finish(Retry, fmt.Errorf("owner rejected replay: %s", receipt.Proof), &receipt)
}

// PendingReplays enumerates exact durable intents to reconcile after restart.
// Never synthesize a new Attempt for a publishing/unknown record. Reinvoke
// Replay with its recorded Spec and an authoritative Confirm callback.
func (a *Archive) PendingReplays() ([]ReplayRecord, error) {
	a.mu.Lock()
	defer a.mu.Unlock()
	if err := a.usable(); err != nil {
		return nil, err
	}
	ids := make([]string, 0, len(a.replays))
	for id, r := range a.replays {
		if r.Phase != "finished" {
			ids = append(ids, id)
		}
	}
	sort.Strings(ids)
	var out []ReplayRecord
	verified := map[string]bool{}
	for _, id := range ids {
		r := a.replays[id]
		if !verified[r.Spec.DeadLetterID] {
			if err := a.verifyReplayAudit(r.Spec.DeadLetterID); err != nil {
				return nil, err
			}
			verified[r.Spec.DeadLetterID] = true
		}
		out = append(out, cloneReplay(r))
	}
	return out, nil
}
