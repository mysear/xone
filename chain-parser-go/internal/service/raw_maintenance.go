package service

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"path/filepath"
	"sort"
	"strings"
	"sync"
	"time"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/ingest"
)

const RawStagedDecoderVersion = "staged-state/public-adapter-v1"
const RawObservationVersion = "raw-observation-only/1"

type rawChainMaintenance struct {
	recorder     *rawRecorder
	sink         *ingest.BoundedLocalArchiveSink
	acquisitions []stagedAcquisition
	cursor       string
	ledgers      map[string]rawOwnerLedgerEntry
	observations map[string]*rawObservationEntry
}
type RawMaintenance struct {
	s                     *Service
	cfg                   config.RawMaintenanceConfig
	mu                    sync.Mutex
	chains                map[string]*rawChainMaintenance
	cancel                context.CancelFunc
	stepCancel            context.CancelFunc
	wg                    sync.WaitGroup
	started, closed, busy bool
	lastError             string
	afterOwnerCommit      func() error
	captureCancels        map[*rawCapture]context.CancelFunc
}

// Main hooks: Service.rawMaintenance *RawMaintenance; construct after rawSources,
// Start after first owner scope, Tick outside syncMu, Close before archives and
// reducer stores, and mount Handler. Config's optional Journal.RawMaintenance
// field and Validate hook are declared in config/raw_maintenance.go.
func NewRawMaintenance(s *Service, c *config.RawMaintenanceConfig) (*RawMaintenance, error) {
	if c == nil {
		return nil, nil
	}
	if s == nil || s.cfg == nil {
		return nil, errors.New("RAW_SERVICE_REQUIRED")
	}
	if e := c.Validate(s.cfg.Config.Journal.RawEvidence); e != nil {
		return nil, e
	}
	if len(s.cfg.Token) < 16 {
		return nil, errors.New("RAW_MAINTENANCE_AUTH_REQUIRED")
	}
	m := &RawMaintenance{s: s, cfg: *c, chains: map[string]*rawChainMaintenance{}, captureCancels: map[*rawCapture]context.CancelFunc{}}
	for key, recorder := range s.rawSources {
		if s.adapters[key] == nil {
			m.Close()
			return nil, errors.New("RAW_NORMAL_ADAPTER_REQUIRED")
		}
		path := filepath.Join(s.cfg.Resolve(c.SinkPath), strings.ReplaceAll(key, ":", "-"))
		root, _ := filepath.Abs(recorder.archive.Directory())
		dest, _ := filepath.Abs(path)
		if dest == root || strings.HasPrefix(dest, root+string(filepath.Separator)) || strings.HasPrefix(root, dest+string(filepath.Separator)) {
			m.Close()
			return nil, errors.New("RAW_SINK_OVERLAP")
		}
		sink, e := ingest.OpenBoundedLocalArchiveSink(path, ingest.LocalArchiveLimits{MaxBytes: c.SinkMaxBytes, MaxRecords: c.SinkMaxRecords})
		if e != nil {
			m.Close()
			return nil, e
		}
		state := &rawChainMaintenance{recorder: recorder, sink: sink, ledgers: map[string]rawOwnerLedgerEntry{}, observations: map[string]*rawObservationEntry{}}
		m.chains[key] = state
		recorder.mu.Lock()
		recorder.maintenance = m
		recorder.mu.Unlock()
		records, e := recorder.archive.RawRecords()
		if e != nil {
			m.Close()
			return nil, e
		}
		if e := m.restoreObservationTickets(key, records); e != nil {
			m.Close()
			return nil, e
		}
		for _, r := range records {
			if r.Raw.Owner.ProviderID != rawOwnerLedgerVersion {
				continue
			}
			ctx, cancel := context.WithTimeout(context.Background(), time.Duration(c.TimeoutMS)*time.Millisecond)
			data, e := m.readEvidence(ctx, key, r.Raw)
			cancel()
			if e != nil {
				m.Close()
				return nil, e
			}
			var ledger rawOwnerLedger
			if json.Unmarshal(data, &ledger) != nil || ledger.Kind != rawOwnerLedgerVersion {
				m.Close()
				return nil, ingest.ErrCorrupt
			}
			if _, e = decodeOwnerProof(ledger.Record, key); e != nil {
				m.Close()
				return nil, e
			}
			if old, ok := state.ledgers[ledger.Record.Key]; ok {
				a, _ := json.Marshal(old.Ledger.Record)
				b, _ := json.Marshal(ledger.Record)
				if string(a) != string(b) {
					m.Close()
					return nil, ingest.ErrCorrupt
				}
			}
			state.ledgers[ledger.Record.Key] = rawOwnerLedgerEntry{Raw: r.Raw, Ledger: ledger}
		}
		for _, r := range records {
			if r.Reclaimed || r.Raw.Owner.ProviderID != "staged-acquisition-v1" {
				continue
			}
			if r.Raw.Bytes > c.MaxRawBytes {
				m.Close()
				return nil, ingest.ErrBudget
			}
			raw, e := recorder.archive.Read(r.Raw)
			if e != nil {
				m.Close()
				return nil, e
			}
			var a stagedAcquisition
			if json.Unmarshal(raw, &a) != nil || a.Kind != "staged-acquisition-v1" || a.Raw.Owner.Chain != key || a.Base.Block.ChainKey != key || len(a.Group) != 64 || len(a.Pins) > c.MaxPinsPerCapture+1 {
				m.Close()
				return nil, errors.New("RAW_ACQUISITION_CORRUPT")
			}
			a.Proof = r.Raw
			state.acquisitions = append(state.acquisitions, a)
		}
	}
	if len(m.chains) == 0 {
		m.Close()
		return nil, errors.New("RAW_EVIDENCE_REQUIRED")
	}
	return m, nil
}
func (m *RawMaintenance) Start(ctx context.Context) error {
	if m == nil {
		return nil
	}
	m.mu.Lock()
	defer m.mu.Unlock()
	if m.closed {
		return ingest.ErrClosed
	}
	if m.started {
		return nil
	}
	if ctx.Err() != nil {
		return ctx.Err()
	}
	ctx, m.cancel = context.WithCancel(ctx)
	m.started = true
	m.wg.Add(1)
	go func() {
		defer m.wg.Done()
		tick := time.NewTicker(time.Duration(m.cfg.IntervalMS) * time.Millisecond)
		defer tick.Stop()
		for {
			select {
			case <-ctx.Done():
				return
			case <-tick.C:
				_ = m.Tick(ctx)
			}
		}
	}()
	return nil
}
func (m *RawMaintenance) enter(ctx context.Context) (context.Context, func(), error) {
	m.mu.Lock()
	defer m.mu.Unlock()
	if m.closed {
		return nil, nil, ingest.ErrClosed
	}
	if m.busy {
		return nil, nil, ingest.ErrReplayBusy
	}
	if ctx.Err() != nil {
		return nil, nil, ctx.Err()
	}
	ctx, cancel := context.WithCancel(ctx)
	m.stepCancel = cancel
	m.busy = true
	m.wg.Add(1)
	return ctx, func() {
		cancel()
		m.mu.Lock()
		m.busy = false
		m.stepCancel = nil
		m.mu.Unlock()
		m.wg.Done()
	}, nil
}
func (m *RawMaintenance) Tick(ctx context.Context) (err error) {
	if m == nil {
		return nil
	}
	ctx, done, e := m.enter(ctx)
	if e != nil {
		return e
	}
	defer done()
	ctx, cancel := context.WithTimeout(ctx, time.Duration(m.cfg.TimeoutMS)*time.Millisecond)
	defer cancel()
	defer func() {
		m.mu.Lock()
		if err != nil {
			m.lastError = err.Error()
		} else {
			m.lastError = ""
		}
		m.mu.Unlock()
	}()
	for key, state := range m.chains {
		if state.recorder.archive.FailureRecoveryRequired() {
			return ingest.ErrFailureRecoveryRequired
		}
		records, e := state.recorder.archive.RawRecords()
		if e != nil {
			return e
		}
		sort.Slice(records, func(i, j int) bool { return records[i].Raw.ID < records[j].Raw.ID })
		split := sort.Search(len(records), func(i int) bool { return records[i].Raw.ID > state.cursor })
		records = append(records[split:], records[:split]...)
		archived, scanned, reclaimed := 0, 0, 0
		for _, r := range records {
			if scanned >= m.cfg.MaxScan {
				break
			}
			if ctx.Err() != nil {
				return ctx.Err()
			}
			scanned++
			state.cursor = r.Raw.ID
			if r.Reclaimed {
				continue
			}
			if r.Raw.Bytes > m.cfg.MaxRawBytes {
				return ingest.ErrBudget
			}
			proof, proved, e := m.proofForRaw(ctx, key, r.Raw)
			if e != nil {
				return e
			}
			staged := r.Raw.Owner.ProviderID == "derived-state-from-recorded-rpc"
			if !r.Archived && archived < m.cfg.MaxArchive {
				if staged && !proved {
					continue
				}
				meta := ingest.ArchiveMetadata{Namespace: ingest.NamespaceHistory, DecoderVersion: RawObservationVersion, CanonicalProof: "raw-observation-only; no canonical or whole-scope claim"}
				if staged {
					meta = ingest.ArchiveMetadata{Namespace: ingest.NamespaceOnline, DecoderVersion: m.cfg.DecoderVersion, CanonicalProof: "owner-bound staged input; canonical output is separate; " + proof}
				}
				if _, e = state.recorder.archive.ArchiveRaw(ctx, r.Raw, meta, state.sink); e != nil {
					return e
				}
				archived++
				r.Archived = true
			}
			if proved && r.Archived && r.Raw.Owner.ProviderID == rawOwnerLedgerVersion {
				m.mu.Lock()
				var matched *rawOwnerLedgerEntry
				for _, entry := range state.ledgers {
					if entry.Raw == r.Raw {
						copy := entry
						matched = &copy
						break
					}
				}
				m.mu.Unlock()
				if matched != nil && matched.Retired {
					if e = state.recorder.archive.ReleaseRaw(r.Raw, "owner-ledger-"+rawDigest([]byte(matched.Ledger.Record.Key))); e != nil {
						return e
					}
					r.ActiveReferences = 0
				}
			}
			if proved && r.Archived && !r.Quarantined {
				released, e := m.releaseObservationRaw(ctx, key, r.Raw)
				if e != nil {
					return e
				}
				if released && r.ActiveReferences > 0 {
					r.ActiveReferences--
				}
			}
			if !proved || !r.Archived || r.Quarantined || r.ActiveReferences > 0 || reclaimed >= m.cfg.MaxReclaims {
				continue
			}
			used, _, e := state.recorder.archive.Usage()
			if e != nil {
				return e
			}
			if used <= m.cfg.TargetBytes && time.Since(r.CapturedAt) < time.Duration(m.cfg.KeepMS)*time.Millisecond {
				continue
			}
			if e = state.recorder.archive.ReclaimRaw(ctx, r.Raw, state.sink); errors.Is(e, ingest.ErrRetentionBlocked) {
				continue
			} else if e != nil {
				return e
			}
			reclaimed++
		}
	}
	return nil
}
func (m *RawMaintenance) Close() error {
	if m == nil {
		return nil
	}
	m.mu.Lock()
	if m.closed {
		m.mu.Unlock()
		return nil
	}
	m.closed = true
	for _, cancel := range m.captureCancels {
		cancel()
	}
	if m.stepCancel != nil {
		m.stepCancel()
	}
	if m.cancel != nil {
		m.cancel()
	}
	m.mu.Unlock()
	m.wg.Wait()
	var errs []error
	for _, c := range m.chains {
		errs = append(errs, c.sink.Close())
	}
	return errors.Join(errs...)
}
func (m *RawMaintenance) proofForRaw(ctx context.Context, key string, ref ingest.RawRef) (string, bool, error) {
	if proof, ok, e := m.observationProof(ctx, key, ref); e != nil || ok {
		return proof, ok, e
	}
	if proof, ok, e := m.ledgerProofForRaw(ctx, key, ref); e != nil || ok {
		return proof, ok, e
	}
	m.mu.Lock()
	acqs := append([]stagedAcquisition(nil), m.chains[key].acquisitions...)
	m.mu.Unlock()
	for _, a := range acqs {
		for _, pin := range acquisitionPins(a) {
			if pin.Raw != ref {
				continue
			}
			record, e := m.s.stores[key].OwnerRecord("online-replay/" + a.Group)
			if e == nil {
				var p rawOwnerProof
				if json.Unmarshal(record.Payload, &p) != nil || p.Kind != "raw-capture-owner-v1" || p.CommitID != record.CommitID || p.Reference != record.Reference || p.Raw != a.Raw || record.Kind != "BLOCK_COMMIT" || p.Reference.Block.Hash != a.Raw.Owner.Hash || p.Reference.Block.Number != a.Raw.Owner.Height || p.Reference.WriterEpoch != a.Base.WriterEpoch || p.Reference.OnlineGeneration != a.Base.OnlineGeneration || p.Reference.ScopeHash != a.Base.ScopeHash || p.Reference.AdapterSetHash != a.Base.AdapterSetHash {
					return "", false, errors.New("RAW_OWNER_PROOF_CORRUPT")
				}
				return "owner-record/" + record.Key + "/" + record.CommitID, true, nil
			}
		}
	}
	letters, e := m.chains[key].recorder.archive.DeadLetters()
	if e != nil {
		return "", false, e
	}
	for _, d := range letters {
		audit, e := m.chains[key].recorder.archive.ReplayAudit(ingest.DeadLetterID(d))
		if e != nil {
			return "", false, e
		}
		for _, r := range audit {
			if r.Status != ingest.Resolved {
				continue
			}
			record, e := m.s.stores[key].OwnerRecord(string(r.Spec.Namespace) + "-replay/" + r.AttemptID)
			if e != nil {
				continue
			}
			var p rawOwnerProof
			if json.Unmarshal(record.Payload, &p) != nil {
				return "", false, errors.New("RAW_OWNER_PROOF_CORRUPT")
			}
			if p.Raw == ref {
				receipt, e := m.confirmOwner(key, r.Intent)
				if e != nil {
					return "", false, e
				}
				return receipt.Proof, true, nil
			}
			for _, pin := range p.Pins {
				if pin.Raw == ref {
					receipt, e := m.confirmOwner(key, r.Intent)
					if e != nil {
						return "", false, e
					}
					return receipt.Proof, true, nil
				}
			}
		}
	}
	return "", false, nil
}
func (m *RawMaintenance) Status() map[string]any {
	m.mu.Lock()
	defer m.mu.Unlock()
	out := map[string]any{"started": m.started, "closed": m.closed, "error": m.lastError}
	for key, c := range m.chains {
		status, e := c.sink.Status()
		pending := 0
		for _, entry := range c.ledgers {
			if !entry.Retired {
				pending++
			}
		}
		out[key] = map[string]any{"sink": status, "owner_proofs_pending_retire": pending, "error": fmt.Sprint(e), "failure_recovery_required": c.recorder.archive.FailureRecoveryRequired()}
	}
	return out
}
