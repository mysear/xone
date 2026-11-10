package ingest

import (
	"encoding/json"
	"errors"
	"os"
	"path/filepath"
	"sort"
	"time"
)

// archiveEvent is an append-only, hash-linked record. The original .dlq files
// are never rewritten. Hashes detect accidental edits; filesystem administrators
// remain trusted (use an independently protected archive for adversarial audit).
type archiveEvent struct {
	Sequence  uint64
	Previous  string
	Kind      string
	Replay    *ReplayRecord    `json:",omitempty"`
	Authority *ReplayAuthority `json:",omitempty"`
	Manifest  *ArchiveManifest `json:",omitempty"`
	Pin       *rawPin          `json:",omitempty"`
	Incident  *FailureIncident `json:",omitempty"`
	Reclaimed *RawRef          `json:",omitempty"`
}

func (a *Archive) initJournal() {
	a.replays = map[string]ReplayRecord{}
	a.authorities = map[string]ReplayAuthority{}
	a.manifests = map[string]ArchiveManifest{}
	a.reclaimed = map[string]RawRef{}
	a.captured = map[string]time.Time{}
	a.pins = map[string]rawPin{}
	a.active = map[string]int{}
	a.replaying = map[string]bool{}
	a.incidents = map[string]FailureIncident{}
	a.currentIncidents = map[string]FailureIncident{}
	a.failureBarriers = map[string]failureBarrier{}
}
func (a *Archive) appendEvent(e archiveEvent) error {
	// Own every pointer/slice: callers continue mutating their working state.
	encoded, err := json.Marshal(e)
	if err != nil {
		return err
	}
	var owned archiveEvent
	if err := json.Unmarshal(encoded, &owned); err != nil {
		return err
	}
	e = owned
	e.Sequence = uint64(len(a.events)) + 1
	if len(a.events) > 0 {
		e.Previous = jsonID(a.events[len(a.events)-1])
	}
	if err := a.validateEvent(e); err != nil {
		return err
	}
	if err := a.write(jsonID(e)+".event", e); err != nil {
		return err
	}
	a.applyEvent(e)
	a.events = append(a.events, e)
	return nil
}
func (a *Archive) validateEvent(e archiveEvent) error {
	fields := 0
	for _, ok := range []bool{e.Replay != nil, e.Authority != nil, e.Manifest != nil, e.Pin != nil, e.Reclaimed != nil, e.Incident != nil} {
		if ok {
			fields++
		}
	}
	if fields != 1 {
		return ErrCorrupt
	}
	switch e.Kind {
	case "incident":
		if e.Incident == nil {
			return ErrCorrupt
		}
		return a.validateIncident(*e.Incident)
	case "replay":
		if e.Replay == nil {
			return ErrCorrupt
		}
		return a.validateReplay(*e.Replay)
	case "authority":
		if e.Authority == nil || !validAuthority(*e.Authority) {
			return ErrOwnership
		}
		old, ok := a.authorities[e.Authority.key()]
		if ok && (e.Authority.Fence < old.Fence || (e.Authority.Fence == old.Fence && *e.Authority != old)) {
			return ErrReplayFence
		}
	case "archive":
		if e.Manifest == nil || !e.Manifest.valid() || a.raw[e.Manifest.Raw.ID] != e.Manifest.Raw {
			return ErrCorrupt
		}
		if old, ok := a.manifests[e.Manifest.Raw.ID]; ok && old != *e.Manifest {
			return ErrOwnership
		}
	case "pin", "release":
		if e.Pin == nil || !validLabel(e.Pin.Reference, 512) || a.raw[e.Pin.Raw.ID] != e.Pin.Raw {
			return ErrOwnership
		}
		if _, ok := a.reclaimed[e.Pin.Raw.ID]; ok {
			return ErrRawReclaimed
		}
		old, ok := a.pins[e.Pin.key()]
		if e.Kind == "pin" {
			if ok {
				return ErrOwnership
			}
			if !e.Pin.Active {
				return ErrCorrupt
			}
		} else if !ok || !old.Active || e.Pin.Active {
			return ErrOwnership
		}
	case "reclaim":
		if e.Reclaimed == nil || a.raw[e.Reclaimed.ID] != *e.Reclaimed {
			return ErrOwnership
		}
		if _, ok := a.reclaimed[e.Reclaimed.ID]; ok {
			return ErrCorrupt
		}
		if _, ok := a.manifests[e.Reclaimed.ID]; !ok || a.blocked(*e.Reclaimed) {
			return ErrRetentionBlocked
		}
	default:
		return ErrCorrupt
	}
	return nil
}
func (a *Archive) applyEvent(e archiveEvent) {
	switch e.Kind {
	case "incident":
		a.incidents[e.Incident.ID] = *e.Incident
		a.currentIncidents[e.Incident.key()] = *e.Incident
	case "replay":
		a.replays[e.Replay.AttemptID] = *e.Replay
	case "authority":
		a.authorities[e.Authority.key()] = *e.Authority
	case "archive":
		a.manifests[e.Manifest.Raw.ID] = *e.Manifest
	case "pin", "release":
		a.pins[e.Pin.key()] = *e.Pin
	case "reclaim":
		a.reclaimed[e.Reclaimed.ID] = *e.Reclaimed
	}
}
func (a *Archive) restoreJournal(events []archiveEvent) error {
	sort.Slice(events, func(i, j int) bool { return events[i].Sequence < events[j].Sequence })
	a.seedLegacyIncidents(events)
	// Tombstones supply the immutable raw identity when its payload was removed.
	for _, e := range events {
		if e.Kind == "reclaim" && e.Reclaimed != nil {
			ref := *e.Reclaimed
			if !validOwner(ref.Owner) || ref.ID != jsonID(ref.Owner) || !validDigest(ref.Digest) || ref.Bytes < 0 {
				return ErrCorrupt
			}
			if old, ok := a.raw[ref.ID]; ok && old != ref {
				return ErrCorrupt
			}
			a.raw[ref.ID] = ref
		}
	}
	for _, e := range events {
		previous := ""
		if len(a.events) > 0 {
			previous = jsonID(a.events[len(a.events)-1])
		}
		if e.Sequence != uint64(len(a.events))+1 || e.Previous != previous {
			return ErrCorrupt
		}
		if err := a.validateEvent(e); err != nil {
			return errors.Join(ErrCorrupt, err)
		}
		a.applyEvent(e)
		a.events = append(a.events, e)
	}
	// Finish unlink after a crash between durable tombstone and directory sync.
	for id := range a.reclaimed {
		path := filepath.Join(a.directory, id+".raw")
		info, err := os.Stat(path)
		if errors.Is(err, os.ErrNotExist) {
			continue
		}
		if err != nil {
			return err
		}
		if err := os.Remove(path); err != nil {
			return err
		}
		a.used -= info.Size()
		a.records--
	}
	for _, m := range a.manifests {
		a.captured[m.Raw.ID] = m.CapturedAt
	}
	return nil
}

type RawRecordInfo struct {
	Raw              RawRef
	CapturedAt       time.Time
	Reclaimed        bool
	Archived         bool
	Quarantined      bool
	ActiveReferences int
}

// RawRecords includes tombstones: ownership/digest remain enumerable after GC.
func (a *Archive) RawRecords() ([]RawRecordInfo, error) {
	a.mu.Lock()
	defer a.mu.Unlock()
	if err := a.usable(); err != nil {
		return nil, err
	}
	ids := make([]string, 0, len(a.raw))
	for id := range a.raw {
		ids = append(ids, id)
	}
	sort.Strings(ids)
	out := make([]RawRecordInfo, 0, len(ids))
	for _, id := range ids {
		ref := a.raw[id]
		_, reclaimed := a.reclaimed[id]
		_, archived := a.manifests[id]
		if !reclaimed {
			if _, err := a.read(ref); err != nil {
				return nil, err
			}
		}
		count := a.active[id]
		for _, p := range a.pins {
			if p.Raw == ref && p.Active {
				count++
			}
		}
		out = append(out, RawRecordInfo{Raw: ref, CapturedAt: a.captured[id], Reclaimed: reclaimed, Archived: archived, Quarantined: a.quarantined(ref), ActiveReferences: count})
	}
	return out, nil
}

// Verify the durable evidence on which a destructive retention decision rests,
// including releases and owner receipts. Cached resolved state alone is not
// sufficient after an on-disk corruption.
func (a *Archive) verifyRawJournal(ref RawRef) error {
	for id, d := range a.dlq {
		if d.Raw == ref {
			var disk DeadLetter
			if err := a.load(id+".dlq", &disk); err != nil {
				return err
			}
			if disk != d {
				a.failed = true
				return ErrCorrupt
			}
		}
	}
	for _, e := range a.events {
		relevant := (e.Replay != nil && e.Replay.Spec.Raw == ref) || (e.Manifest != nil && e.Manifest.Raw == ref) || (e.Pin != nil && e.Pin.Raw == ref) || (e.Reclaimed != nil && *e.Reclaimed == ref) || (e.Incident != nil && e.Incident.Raw == ref)
		if !relevant {
			continue
		}
		var disk archiveEvent
		if err := a.load(jsonID(e)+".event", &disk); err != nil {
			return err
		}
		if jsonID(disk) != jsonID(e) {
			a.failed = true
			return ErrCorrupt
		}
	}
	return nil
}
