package service

import (
	"context"
	"encoding/json"
	"errors"
	"time"

	"xone/chain-parser-go/internal/ingest"
)

const RawObservationTicketVersion = "raw-observation-ticket-v1"

type rawObservationTicket struct {
	Kind        string
	Chain       string
	Purpose     string
	GroupID     string
	Group       rawPinGroup
	Outcome     string
	Canonical   bool
	CompletedAt time.Time
}
type rawObservationEntry struct {
	Raw    ingest.RawRef
	Ticket rawObservationTicket
	// This is only a progress cache. On reopen it is reconstructed from durable
	// pin/tombstone state; it never grants release permission without the ticket.
	Pins     []RawPin
	Released map[string]bool
}

func observationTicketPin(t rawObservationTicket) string { return "observation-ticket-" + t.GroupID }
func (m *RawMaintenance) observationPins(ctx context.Context, key string, entry *rawObservationEntry) ([]RawPin, error) {
	t := entry.Ticket
	if t.Kind != RawObservationTicketVersion || t.Chain != key || !observationPurpose(t.Purpose) || t.Outcome != "decoder-completed" || t.Canonical || t.CompletedAt.IsZero() || len(t.GroupID) != 64 || t.Group.Binding != t.GroupID || t.Group.Kind != "raw-pin-group-v1" {
		return nil, ingest.ErrCorrupt
	}
	return m.expandProofPins(ctx, key, rawOwnerProof{Groups: []rawPinGroup{t.Group}})
}
func (m *RawMaintenance) completeObservation(ctx context.Context, key, purpose, id string, pins []RawPin) error {
	root, e := m.savePinGroup(ctx, key, id, 0, pins)
	if e != nil {
		return e
	}
	if ctx.Err() != nil {
		return ctx.Err()
	}
	ticket := rawObservationTicket{Kind: RawObservationTicketVersion, Chain: key, Purpose: purpose, GroupID: id, Group: root, Outcome: "decoder-completed", CompletedAt: time.Now().UTC()}
	payload, e := json.Marshal(ticket)
	if e != nil || int64(len(payload)) > m.cfg.MaxRawBytes {
		return ingest.ErrBudget
	}
	ref, e := m.putEvidencePinned(ctx, key, ingest.Owner{Chain: key, Height: 0, Hash: rawDigest(payload), ProviderID: RawObservationTicketVersion}, payload, observationTicketPin(ticket))
	if e != nil {
		return e
	}
	// The ticket attests ONLY that this local observation/decoder completed. It
	// does not assert RPC correctness, canonicality or owner-state publication.
	// Raw and receipt pins remain held until Tick copies and verifies each object.
	m.mu.Lock()
	m.chains[key].observations[ref.ID] = &rawObservationEntry{Raw: ref, Ticket: ticket, Pins: append(append([]RawPin(nil), pins...), RawPin{Raw: root.Root, Reference: root.Reference}), Released: map[string]bool{}}
	m.mu.Unlock()
	return ctx.Err()
}
func (m *RawMaintenance) restoreObservationTickets(key string, records []ingest.RawRecordInfo) error {
	status := map[string]ingest.RawRecordInfo{}
	for _, r := range records {
		status[r.Raw.ID] = r
	}
	for _, r := range records {
		if r.Raw.Owner.ProviderID != RawObservationTicketVersion {
			continue
		}
		ctx, cancel := context.WithTimeout(context.Background(), m.operationTimeout())
		data, e := m.readEvidence(ctx, key, r.Raw)
		if e != nil {
			cancel()
			return e
		}
		var ticket rawObservationTicket
		if json.Unmarshal(data, &ticket) != nil {
			cancel()
			return ingest.ErrCorrupt
		}
		entry := &rawObservationEntry{Raw: r.Raw, Ticket: ticket, Released: map[string]bool{}}
		pins, e := m.observationPins(ctx, key, entry)
		entry.Pins = append([]RawPin(nil), pins...)
		cancel()
		if e != nil {
			return e
		}
		for _, pin := range append(pins, RawPin{Raw: r.Raw, Reference: observationTicketPin(ticket)}) {
			s, ok := status[pin.Raw.ID]
			if !ok || s.Raw != pin.Raw {
				return ingest.ErrCorrupt
			}
			if s.Reclaimed || s.ActiveReferences == 0 {
				entry.Released[pin.Raw.ID] = true
			}
		}
		m.chains[key].observations[r.Raw.ID] = entry
	}
	return nil
}
func (m *RawMaintenance) observationEntries(key string) []*rawObservationEntry {
	m.mu.Lock()
	defer m.mu.Unlock()
	out := make([]*rawObservationEntry, 0, len(m.chains[key].observations))
	for _, entry := range m.chains[key].observations {
		out = append(out, entry)
	}
	return out
}
func (m *RawMaintenance) observationProof(ctx context.Context, key string, ref ingest.RawRef) (string, bool, error) {
	for _, entry := range m.observationEntries(key) {
		if ctx.Err() != nil {
			return "", false, ctx.Err()
		}
		candidate := entry.Raw == ref
		for _, pin := range entry.Pins {
			if pin.Raw == ref {
				candidate = true
				break
			}
		}
		if !candidate {
			continue
		}
		// Re-read the durable receipt before trusting the progress cache or members.
		data, e := m.readEvidence(ctx, key, entry.Raw)
		if e != nil {
			return "", false, e
		}
		var ticket rawObservationTicket
		if json.Unmarshal(data, &ticket) != nil || ticket != entry.Ticket {
			return "", false, ingest.ErrCorrupt
		}
		pins, e := m.observationPins(ctx, key, entry)
		if e != nil {
			return "", false, e
		}
		matches := entry.Raw == ref
		for _, pin := range pins {
			if pin.Raw == ref {
				matches = true
				break
			}
		}
		if matches {
			return "observation-only-ticket/" + entry.Raw.Digest, true, nil
		}
	}
	return "", false, nil
}

// releaseObservationRaw is called only AFTER the exact raw's manifest/content
// was copied and verified. It cannot release another consumer's reference.
// The group root follows all data members, and the ticket follows the group root.
func (m *RawMaintenance) releaseObservationRaw(ctx context.Context, key string, ref ingest.RawRef) (bool, error) {
	for _, entry := range m.observationEntries(key) {
		candidate := entry.Raw == ref
		for _, pin := range entry.Pins {
			if pin.Raw == ref {
				candidate = true
				break
			}
		}
		if !candidate {
			continue
		}
		pins, e := m.observationPins(ctx, key, entry)
		if e != nil {
			return false, e
		}
		var target *RawPin
		for i := range pins {
			if pins[i].Raw == ref {
				copy := pins[i]
				target = &copy
				break
			}
		}
		ticket := entry.Raw == ref
		if target == nil && !ticket {
			continue
		}
		m.mu.Lock()
		already := entry.Released[ref.ID]
		ready := true
		if ticket {
			ready = entry.Released[entry.Ticket.Group.Root.ID]
		} else if ref == entry.Ticket.Group.Root {
			for _, p := range pins {
				if p.Raw != ref && !entry.Released[p.Raw.ID] {
					ready = false
					break
				}
			}
		}
		m.mu.Unlock()
		if !ready {
			return false, nil
		}
		if already {
			return true, nil
		}
		if ticket {
			target = &RawPin{Raw: ref, Reference: observationTicketPin(entry.Ticket)}
		}
		if ctx.Err() != nil {
			return false, ctx.Err()
		}
		if target == nil {
			return false, errors.New("RAW_OBSERVATION_PIN")
		}
		if e = m.chains[key].recorder.archive.ReleaseRaw(target.Raw, target.Reference); e != nil {
			return false, e
		}
		m.mu.Lock()
		entry.Released[ref.ID] = true
		m.mu.Unlock()
		return true, nil
	}
	return false, nil
}
