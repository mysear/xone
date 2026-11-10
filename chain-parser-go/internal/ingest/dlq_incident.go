package ingest

import (
	"errors"
	"sort"
	"time"
)

var ErrFailureIncident = errors.New("INGEST_FAILURE_INCIDENT_MISMATCH")

// FailureIncident is a durable failure cycle for exact raw ownership/digest and
// scope. Message, Code and Stage remain immutable diagnostic evidence, not cycle
// or commit identities. Resolution is specific to this ID and its namespace.
// Cycle zero is reserved for pre-incident archives; it is never inferred as a
// substitute for a new-format ReplaySpec with a missing IncidentID.
type FailureIncident struct {
	ID       string
	Raw      RawRef
	Scope    string
	Cycle    uint64
	OpenedBy string // opening DeadLetterID; legacy cycle zero uses a deterministic retained diagnostic
	At       time.Time
}

func failureKey(ref RawRef, scope string) string {
	return jsonID(struct {
		Raw   RawRef
		Scope string
	}{ref, scope})
}
func failureID(ref RawRef, scope string, cycle uint64) string {
	return jsonID(struct {
		Raw   RawRef
		Scope string
		Cycle uint64
	}{ref, scope, cycle})
}
func (i FailureIncident) key() string { return failureKey(i.Raw, i.Scope) }
func (a *Archive) specIncident(s ReplaySpec) (FailureIncident, bool) {
	d, ok := a.dlq[s.DeadLetterID]
	if !ok || d.Raw != s.Raw {
		return FailureIncident{}, false
	}
	id := s.IncidentID
	// Preserve exact legacy attempt/receipt hashes for confirmation. New records
	// explicitly use CurrentIncident.ID; empty never means "whichever is latest".
	if id == "" {
		id = failureID(d.Raw, d.Scope, 0)
	}
	i, ok := a.incidents[id]
	return i, ok && i.Raw == s.Raw && i.Scope == d.Scope
}
func (a *Archive) incidentResolved(id string, namespace ReplayNamespace) bool {
	resolved := false
	for _, r := range a.replays {
		if r.Spec.Namespace != namespace {
			continue
		}
		i, ok := a.specIncident(r.Spec)
		if !ok || i.ID != id {
			continue
		}
		if r.Phase != "finished" {
			return false
		}
		if r.Status == Resolved {
			resolved = true
		}
	}
	return resolved
}
func (a *Archive) validateIncident(i FailureIncident) error {
	d, ok := a.dlq[i.OpenedBy]
	if !ok || d.Raw != i.Raw || d.Scope != i.Scope || i.ID != failureID(i.Raw, i.Scope, i.Cycle) || i.Cycle == 0 || i.At.IsZero() {
		return ErrFailureIncident
	}
	if _, exists := a.incidents[i.ID]; exists {
		return ErrFailureIncident
	}
	current, exists := a.currentIncidents[i.key()]
	if !exists {
		if i.Cycle != 1 {
			return ErrFailureIncident
		}
		return nil
	}
	if current.Cycle == ^uint64(0) || i.Cycle != current.Cycle+1 || !a.incidentResolved(current.ID, NamespaceOnline) {
		return ErrFailureIncident
	}
	return nil
}

// seedLegacyIncidents creates a deterministic cycle zero only where a legacy
// record actually needs it, or where a durable .dlq predates its first incident
// append (including a crash in that window). Modern cycles are replayed from
// immutable incident events, never assigned to whichever diagnostic sorts first.
func (a *Archive) seedLegacyIncidents(events []archiveEvent) {
	explicit := map[string]bool{}
	legacy := map[string]bool{}
	for _, e := range events {
		if e.Incident != nil && e.Incident.Cycle == 1 {
			explicit[e.Incident.key()] = true
		}
		if e.Replay != nil {
			r := e.Replay
			d, ok := a.dlq[r.Spec.DeadLetterID]
			if ok && (r.Spec.IncidentID == "" || r.Spec.IncidentID == failureID(d.Raw, d.Scope, 0)) {
				legacy[failureKey(d.Raw, d.Scope)] = true
			}
		}
	}
	ids := make([]string, 0, len(a.dlq))
	for id := range a.dlq {
		ids = append(ids, id)
	}
	sort.Strings(ids)
	for _, id := range ids {
		d := a.dlq[id]
		key := failureKey(d.Raw, d.Scope)
		if explicit[key] && !legacy[key] {
			continue
		}
		if _, ok := a.currentIncidents[key]; ok {
			continue
		}
		i := FailureIncident{ID: failureID(d.Raw, d.Scope, 0), Raw: d.Raw, Scope: d.Scope, Cycle: 0, OpenedBy: id}
		a.incidents[i.ID] = i
		a.currentIncidents[key] = i
	}
}

// QuarantineIncident returns the exact incident to bind to ReplaySpec. While
// online-unresolved, observations/diagnostic aliases join the current cycle.
// After online resolution, even a byte-identical observation opens a new cycle.
// Any error must stop the caller; diagnostic bytes may already be durable.
func (a *Archive) QuarantineIncident(entry DeadLetter) (FailureIncident, error) {
	a.mu.Lock()
	defer a.mu.Unlock()
	if err := a.usable(); err != nil {
		return FailureIncident{}, err
	}
	if !validDeadLetter(entry) {
		return FailureIncident{}, errors.New("INGEST_DLQ_INVALID")
	}
	if _, err := a.read(entry.Raw); err != nil {
		return FailureIncident{}, err
	}
	key := failureKey(entry.Raw, entry.Scope)
	current, exists := a.currentIncidents[key]
	resolved := exists && a.incidentResolved(current.ID, NamespaceOnline)
	cycle := uint64(1)
	if exists {
		cycle = current.Cycle
		if resolved {
			if cycle == ^uint64(0) {
				a.failed = true
				a.failureUncertain = true
				return FailureIncident{}, ErrBudget
			}
			cycle++
		}
	}
	barrier := failureBarrier{RawID: entry.Raw.ID, Key: key, Cycle: cycle}
	// Isolation precedes BOTH diagnostic and incident writes and bypasses their
	// ordinary capacity budget. A later quota rejection must poison this handle.
	if err := a.beginFailureBarrier(barrier); err != nil {
		return FailureIncident{}, err
	}
	if err := a.quarantine(entry); err != nil {
		a.failed = true
		return FailureIncident{}, err
	}
	if err := a.verifyRawJournal(entry.Raw); err != nil {
		a.failed = true
		return FailureIncident{}, err
	}
	if exists && !resolved {
		if err := a.finishFailureBarrier(barrier); err != nil {
			return FailureIncident{}, err
		}
		return current, nil
	}
	i := FailureIncident{ID: failureID(entry.Raw, entry.Scope, cycle), Raw: entry.Raw, Scope: entry.Scope, Cycle: cycle, OpenedBy: DeadLetterID(entry), At: time.Now().UTC()}
	if err := a.appendEvent(archiveEvent{Kind: "incident", Incident: &i}); err != nil {
		a.failed = true
		return FailureIncident{}, err
	}
	if err := a.finishFailureBarrier(barrier); err != nil {
		return FailureIncident{}, err
	}
	return i, nil
}

// CurrentIncident obtains the current raw+scope cycle for existing Quarantine
// callers (including Pipeline). Carry its ID explicitly through ReplaySpec;
// never change an old pending Spec to the newly observed incident.
func (a *Archive) CurrentIncident(dlqID string) (FailureIncident, error) {
	a.mu.Lock()
	defer a.mu.Unlock()
	if err := a.usable(); err != nil {
		return FailureIncident{}, err
	}
	d, ok := a.dlq[dlqID]
	if !ok {
		return FailureIncident{}, ErrOwnership
	}
	if a.failureHeld(d.Raw) {
		return FailureIncident{}, ErrFailureRecoveryRequired
	}
	if err := a.verifyRawJournal(d.Raw); err != nil {
		return FailureIncident{}, err
	}
	i, ok := a.currentIncidents[failureKey(d.Raw, d.Scope)]
	if !ok {
		return FailureIncident{}, ErrFailureIncident
	}
	return i, nil
}

// FailureIncidents preserves historical cycles without turning their resolution
// into current authorization. Use ReplayAudit for the immutable attempt history.
func (a *Archive) FailureIncidents(dlqID string) ([]FailureIncident, error) {
	a.mu.Lock()
	defer a.mu.Unlock()
	if err := a.usable(); err != nil {
		return nil, err
	}
	d, ok := a.dlq[dlqID]
	if !ok {
		return nil, ErrOwnership
	}
	if err := a.verifyRawJournal(d.Raw); err != nil {
		return nil, err
	}
	var out []FailureIncident
	for _, i := range a.incidents {
		if i.key() == failureKey(d.Raw, d.Scope) {
			out = append(out, i)
		}
	}
	sort.Slice(out, func(i, j int) bool { return out[i].Cycle < out[j].Cycle })
	return out, nil
}
