package service

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"net/http"
	"net/http/httptest"
	"testing"
	"time"

	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/domain"
	"xone/chain-parser-go/internal/ingest"
	"xone/chain-parser-go/internal/journal"
	"xone/chain-parser-go/internal/reducer"
	"xone/chain-parser-go/internal/rpc"
)

func TestRawMaintenanceEnabledRequiresDeclaredResponsibility(t *testing.T) {
	s, m, key, _ := maintenanceFixture(t)
	_ = m
	before, _, e := s.rawSources[key].archive.Usage()
	if e != nil {
		t.Fatal(e)
	}
	var result string
	e = s.adapters[key].Client.Call(context.Background(), "eth_chainId", []any{}, &result)
	if e == nil {
		t.Fatal("maintenance capture silently acquired unowned pins")
	}
	after, _, e := s.rawSources[key].archive.Usage()
	if e != nil || after != before {
		t.Fatal("unowned capture wrote data", before, after, e)
	}
}
func TestRawMaintenanceObservationTicketReleasesOnlyArchivedCompletedInput(t *testing.T) {
	s, m, key, _ := maintenanceFixture(t)
	var id string
	if e := m.Observe(context.Background(), key, RawObserveIdentity, func(ctx context.Context) error { return s.adapters[key].Client.Call(ctx, "eth_chainId", []any{}, &id) }); e != nil {
		t.Fatal(e)
	}
	drainRawMaintenance(t, m, func(records []ingest.RawRecordInfo) bool {
		for _, r := range records {
			if r.Raw.Owner.ProviderID == RawObservationTicketVersion && !r.Archived {
				return false
			}
		}
		return true
	})
	records, e := s.rawSources[key].archive.RawRecords()
	if e != nil {
		t.Fatal(e)
	}
	completed := 0
	for _, r := range records {
		if r.Raw.Owner.ProviderID == RawObservationTicketVersion {
			completed++
			if !r.Archived {
				t.Fatal("ticket not archived", r)
			}
			manifest, e := s.rawSources[key].archive.ArchivedManifest(r.Raw)
			if e != nil || manifest.Metadata.DecoderVersion != RawObservationVersion {
				t.Fatal(manifest, e)
			}
		}
	}
	if completed == 0 {
		t.Fatal("no durable observation receipt")
	}
	// The same response can be observed again after its previous local raw was GC'd.
	if e := m.Observe(context.Background(), key, RawObserveIdentity, func(ctx context.Context) error { return s.adapters[key].Client.Call(ctx, "eth_chainId", []any{}, &id) }); e != nil {
		t.Fatal("repeated response failed after GC", e)
	}
}
func TestRawMaintenanceFailedOrCanceledObservationKeepsPins(t *testing.T) {
	for _, mode := range []string{"error", "cancel"} {
		t.Run(mode, func(t *testing.T) {
			s, m, key, _ := maintenanceFixture(t)
			ctx, cancel := context.WithCancel(context.Background())
			defer cancel()
			var failedRaw ingest.RawRef
			e := m.Observe(ctx, key, RawObserveHeader, func(ctx context.Context) error {
				var id string
				if e := s.adapters[key].Client.Call(ctx, "eth_chainId", []any{}, &id); e != nil {
					return e
				}
				g := ctx.Value(rawCaptureKey{}).(*rawCapture)
				g.mu.Lock()
				failedRaw = g.Pins[0].Raw
				g.mu.Unlock()
				if mode == "cancel" {
					cancel()
					return nil
				}
				return errors.New("decoder did not complete")
			})
			if e == nil {
				t.Fatal("unfinished observation acknowledged")
			}
			records, e := s.rawSources[key].archive.RawRecords()
			if e != nil {
				t.Fatal(e)
			}
			held := 0
			for _, r := range records {
				if r.ActiveReferences > 0 {
					held++
				}
			}
			if held == 0 {
				t.Fatal("unfinished observation released pins")
			}
			for i := 0; i < 2; i++ {
				if e = m.Tick(context.Background()); e != nil {
					t.Fatal(e)
				}
			}
			after, e := s.rawSources[key].archive.RawRecords()
			if e != nil {
				t.Fatal(e)
			}
			found := false
			for _, retained := range after {
				if retained.Raw == failedRaw {
					found = true
					if retained.Reclaimed || retained.ActiveReferences == 0 {
						t.Fatal("unfinished read discharged during maintenance", mode, retained)
					}
				}
			}
			if !found {
				t.Fatal("failed raw disappeared", mode)
			}
		})
	}
}
func TestRawMaintenanceObservationCannotDischargeBaselineGroup(t *testing.T) {
	s, m, key, _ := maintenanceFixture(t)
	ctx, finish, e := m.BeginBaselineCapture(context.Background(), key)
	if e != nil {
		t.Fatal(e)
	}
	defer finish()
	if e = m.Observe(ctx, key, RawObserveIdentity, func(ctx context.Context) error {
		var id string
		return s.adapters[key].Client.Call(ctx, "eth_chainId", []any{}, &id)
	}); e != nil {
		t.Fatal(e)
	}
	for i := 0; i < 2; i++ {
		if e = m.Tick(context.Background()); e != nil {
			t.Fatal(e)
		}
	}
	group := ctx.Value(rawCaptureKey{}).(*rawCapture)
	group.mu.Lock()
	pins := append([]RawPin(nil), group.Pins...)
	group.mu.Unlock()
	if len(pins) == 0 {
		t.Fatal("baseline not captured")
	}
	records, e := s.rawSources[key].archive.RawRecords()
	if e != nil {
		t.Fatal(e)
	}
	for _, pin := range pins {
		found := false
		for _, r := range records {
			if r.Raw == pin.Raw {
				found = true
				if r.Reclaimed || r.ActiveReferences == 0 {
					t.Fatal("observation released baseline input", r)
				}
			}
		}
		if !found {
			t.Fatal("lost baseline raw")
		}
	}
}

// These identifiers must be supplied by the main thread's same-batch scope API;
// creating a record from an already-visible Current snapshot is not acceptable.
type baselineRecordsInstaller interface {
	InstallBaselineScopeRecordsCAS(domain.Snapshot, []byte, string, json.RawMessage, string, map[string]json.RawMessage, func() error) error
}

func installCapturedBaseline(t *testing.T, s *Service, m *RawMaintenance, key string, ctx context.Context) {
	t.Helper()
	snapshot, ok := s.stores[key].Current()
	if !ok {
		t.Fatal("baseline")
	}
	snapshot.Reference.OnlineGeneration++
	snapshot.Reference.WriterEpoch++
	snapshot.Reference.JournalSeq++
	job := "raw-captured-baseline"
	fence := json.RawMessage(fmt.Sprintf(`{"generation":%d,"writer_epoch":%d}`, snapshot.Reference.OnlineGeneration, snapshot.Reference.WriterEpoch))
	unit := journal.Unit{Kind: "ONLINE_BASELINE_COMMIT", Snapshot: snapshot}
	records, e := m.BaselineOwnerRecords(ctx, unit, job, fence)
	if e != nil {
		t.Fatal(e)
	}
	if e = m.AcknowledgeOwnerRecords(unit, records); e == nil {
		t.Fatal("source-read success treated as owner commit")
	}
	install, ok := any(s.stores[key]).(baselineRecordsInstaller)
	if !ok {
		t.Fatal("main scope+owner-records CAS hook required")
	}
	previous, e := s.stores[key].ScopeState()
	if e != nil {
		t.Fatal(e)
	}
	if e = install.InstallBaselineScopeRecordsCAS(snapshot, []byte(`{"captured_baseline":true}`), job, fence, reducer.ScopeLedgerDigest(previous), records, nil); e != nil {
		t.Fatal(e)
	}
	if e = m.AcknowledgeOwnerRecords(unit, records); e != nil {
		t.Fatal(e)
	}
}
func TestRawMaintenanceIndependentBackgroundProbeDoesNotAccumulatePins(t *testing.T) {
	s, m, key, _ := maintenanceFixture(t, func(c *config.RawMaintenanceConfig) { c.IntervalMS = 10 })
	// New -> Start has already started the real periodic worker. No manual Tick
	// drives this test; the completed observation must hand off all its own pins.
	for round := 0; round < 3; round++ {
		if e := m.Observe(context.Background(), key, RawObserveHeader, func(ctx context.Context) error { _, e := s.adapters[key].Header(ctx, "0x64"); return e }); e != nil {
			t.Fatal(e)
		}
		// This drain requires several bounded Tick passes. Bound the absence of
		// durable progress by one unchanged operation timeout, rather than requiring
		// the whole admitted backlog to finish within a single pass's timeout.
		progressDeadline := time.NewTimer(m.operationTimeout())
		poll := time.NewTicker(10 * time.Millisecond)
		lastProgress := -1
		for {
			records, e := s.rawSources[key].archive.RawRecords()
			if e != nil {
				t.Fatal(e)
			}
			complete, progress := true, 0
			for _, r := range records {
				if r.ActiveReferences > 0 {
					complete = false
				}
				if r.Archived {
					progress++
				}
				if r.Reclaimed {
					progress++
				}
			}
			if progress > lastProgress {
				lastProgress = progress
				if !progressDeadline.Stop() {
					select {
					case <-progressDeadline.C:
					default:
					}
				}
				progressDeadline.Reset(m.operationTimeout())
			}
			if complete {
				break
			}
			select {
			case <-progressDeadline.C:
				t.Fatal("periodic worker made no durable progress within its operation budget", round, progress, m.Status())
			case <-poll.C:
			}
		}
		poll.Stop()
		progressDeadline.Stop()
	}
}

// Keep the controlled HTTP source path real: malformed envelopes must not get an
// observation completion ticket, even though their bytes reached the recorder.
func TestRawMaintenanceInvalidHTTPObservationDoesNotComplete(t *testing.T) {
	s, m, key, _ := maintenanceFixture(t)
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) { w.Write([]byte(`not-json`)) }))
	defer server.Close()
	client := rpc.New([]string{server.URL}, time.Second, 4096)
	client.SetRecorder(s.rawSources[key].capture)
	if e := m.Observe(context.Background(), key, RawObserveIdentity, func(ctx context.Context) error { var v string; return client.Call(ctx, "eth_chainId", []any{}, &v) }); e == nil {
		t.Fatal("invalid decoder completed")
	}
}

func TestRawMaintenanceActualProviderRefreshContinuousSmallSink(t *testing.T) {
	s, m, key, _ := maintenanceFixtureWithChain(t, func(chain *config.Chain, source *rawMaintenanceRPCState) {
		chain.ProviderProbe = &config.ProviderProbeConfig{HistoricalHeight: 100, HistoricalHash: source.BaseHash, Account: chain.Pools[0].ID, TimeoutMS: 5000, IntervalMS: 60000, MaxBackoffMS: 60000, MaxAgeMS: 120000, MaxProviders: 4}
	}, func(c *config.RawMaintenanceConfig) {
		c.SinkMaxBytes = 4 << 20
		c.SinkMaxRecords = 1200
		c.TimeoutMS = 5000
		c.MaxScan = 32
		c.MaxArchive = 4
		c.MaxReclaims = 4
		c.MaxPinsPerCapture = 512
	})
	p := s.providerRuntime[key]
	if p == nil {
		t.Fatal("New did not consume provider_probe configuration")
	}
	var e error
	for round := 0; round < 4; round++ {
		requestedAt := time.Now()
		p.mu.Lock()
		p.next = time.Time{}
		p.mu.Unlock()
		// Start also owns a background Run goroutine. An already-running probe
		// retains responsibility until its ticket completes; actor-busy is not a
		// decoder failure and is retried within the unchanged operation budget.
		probeCtx, cancel := context.WithTimeout(context.Background(), m.operationTimeout())
		for {
			e = p.Refresh(probeCtx)
			if !errors.Is(e, ingest.ErrFactoryBusy) {
				break
			}
			select {
			case <-probeCtx.Done():
				e = probeCtx.Err()
			case <-time.After(time.Millisecond):
				continue
			}
			break
		}
		cancel()
		if errors.Is(e, ErrProviderBackoff) {
			// The normal Run actor may win the due probe. Accept its actual fresh
			// results, not a pre-existing snapshot, then drain its completion ticket.
			observed := p.Snapshot()
			completed := len(observed.Providers) > 0
			for _, provider := range observed.Providers {
				completed = completed && providerQualified(provider) && !provider.CheckedAt.Before(requestedAt)
			}
			if completed {
				e = nil
			}
		}
		if e != nil {
			t.Fatal("real probe did not complete", round, e)
		}
		drainRawMaintenance(t, m, func(records []ingest.RawRecordInfo) bool {
			for _, r := range records {
				if r.ActiveReferences > 0 {
					return false
				}
			}
			return true
		})
	}
	records, e := s.rawSources[key].archive.RawRecords()
	if e != nil {
		t.Fatal(e)
	}
	held := 0
	for _, r := range records {
		if r.ActiveReferences > 0 {
			held++
		}
	}
	if held != 0 {
		t.Fatal("completed New/Start/probes left unconsumed pins", held)
	}
	status, e := m.chains[key].sink.Status()
	if e != nil || status.Failed || status.Records >= 1200 {
		t.Fatal(status, e)
	}
}

// Each pass obeys the configured scan/archive/reclaim limits. The bound derives
// from admitted evidence plus the two metadata dependencies per observation;
// filesystem timing or concurrent goroutines do not determine the assertion.
func drainRawMaintenance(t *testing.T, m *RawMaintenance, condition func([]ingest.RawRecordInfo) bool) {
	t.Helper()
	maxPasses := 16
	for _, state := range m.chains {
		records, e := state.recorder.archive.RawRecords()
		if e != nil {
			t.Fatal(e)
		}
		maxPasses += 4 * len(records)
	}
	for pass := 0; pass < maxPasses; pass++ {
		before := map[string][2]int{}
		for key, state := range m.chains {
			rows, e := state.recorder.archive.RawRecords()
			if e != nil {
				t.Fatal(e)
			}
			var counts [2]int
			for _, row := range rows {
				if row.Archived {
					counts[0]++
				}
				if row.Reclaimed {
					counts[1]++
				}
			}
			before[key] = counts
		}
		e := m.Tick(context.Background())
		if e != nil {
			if errors.Is(e, ingest.ErrReplayBusy) {
				time.Sleep(time.Millisecond)
				continue
			}
			t.Fatal("bounded maintenance pass", pass, e)
		}
		var records []ingest.RawRecordInfo
		for key, state := range m.chains {
			part, e := state.recorder.archive.RawRecords()
			if e != nil {
				t.Fatal(e)
			}
			var counts [2]int
			for _, row := range part {
				if row.Archived {
					counts[0]++
				}
				if row.Reclaimed {
					counts[1]++
				}
			}
			previous := before[key]
			if counts[0]-previous[0] > m.cfg.MaxArchive || counts[1]-previous[1] > m.cfg.MaxReclaims {
				t.Fatal("maintenance exceeded per-pass work budget", key, previous, counts)
			}
			records = append(records, part...)
		}
		if condition(records) {
			return
		}
		if pass == 15 {
			held := map[string]int{}
			for _, r := range records {
				if r.ActiveReferences > 0 {
					held[r.Raw.Owner.ProviderID]++
				}
			}
			t.Log("bounded retention progress", "pass", pass, "records", len(records), "held", held)
		}
	}
	var held []string
	for key, state := range m.chains {
		records, e := state.recorder.archive.RawRecords()
		if e != nil {
			t.Fatal(e)
		}
		for _, r := range records {
			if r.ActiveReferences > 0 {
				held = append(held, fmt.Sprintf("%s/%s/%s refs=%d archived=%v quarantined=%v", key, r.Raw.Owner.ProviderID, r.Raw.ID, r.ActiveReferences, r.Archived, r.Quarantined))
			}
		}
	}
	t.Fatal("maintenance made no completion within admitted work bound", held)
}

func TestRawMaintenanceZeroPoolsNormalNewStartNeedsNoFakeScopeJob(t *testing.T) {
	s, m, key, _ := maintenanceFixtureWithChain(t, func(chain *config.Chain, _ *rawMaintenanceRPCState) { chain.Pools = nil })
	current, ok := s.stores[key].Current()
	if ok {
		t.Fatal("D0 observation fabricated a qualified pool snapshot", current)
	}
	ledger, e := s.stores[key].ScopeState()
	if e != nil {
		t.Fatal(e)
	}
	if ledger != nil {
		t.Fatal("D0 fabricated a qualified pool scope", ledger)
	}
	drainRawMaintenance(t, m, func(records []ingest.RawRecordInfo) bool {
		for _, r := range records {
			if r.ActiveReferences > 0 {
				return false
			}
		}
		return true
	})
}

func TestRawMaintenanceCapturedBaselineRequiresSameBatchScopeProof(t *testing.T) {
	s, m, key, _ := maintenanceFixture(t)
	ctx, finish, e := m.BeginBaselineCapture(context.Background(), key)
	if e != nil {
		t.Fatal(e)
	}
	defer finish()
	if _, _, e = s.adapters[key].ReadState(ctx, "0x64", true); e != nil {
		t.Fatal(e)
	}
	installCapturedBaseline(t, s, m, key, ctx)
	group := ctx.Value(rawCaptureKey{}).(*rawCapture)
	group.mu.Lock()
	pins := append([]RawPin(nil), group.Pins...)
	group.mu.Unlock()
	status, e := s.rawSources[key].archive.RawRecords()
	if e != nil {
		t.Fatal(e)
	}
	for _, pin := range pins {
		for _, r := range status {
			if r.Raw == pin.Raw && r.ActiveReferences != 0 {
				t.Fatal("confirmed scope did not release own raw", r)
			}
		}
	}
}

func TestRawMaintenanceObservationHandoffKeepsOtherOwnerAndUnresolvedDLQ(t *testing.T) {
	for _, mode := range []string{"other-owner", "unresolved-dlq"} {
		t.Run(mode, func(t *testing.T) {
			s, m, key, _ := maintenanceFixture(t)
			var target ingest.RawRef
			e := m.Observe(context.Background(), key, RawObserveHeader, func(ctx context.Context) error {
				if _, e := s.adapters[key].Header(ctx, "0x64"); e != nil {
					return e
				}
				g := ctx.Value(rawCaptureKey{}).(*rawCapture)
				g.mu.Lock()
				target = g.Pins[0].Raw
				g.mu.Unlock()
				if mode == "other-owner" {
					return s.rawSources[key].archive.PinRaw(target, "independent-owner")
				}
				return s.rawSources[key].archive.Quarantine(ingest.DeadLetter{Raw: target, Stage: "source-validation", Code: "ANCHOR_UNCONFIRMED", Scope: "independent-owner", Message: "decoder completion is not owner resolution"})
			})
			if e != nil {
				t.Fatal(e)
			}
			// Visit the entire admitted set, including the target, through bounded passes.
			count := 0
			drainRawMaintenance(t, m, func(records []ingest.RawRecordInfo) bool {
				count++
				for _, r := range records {
					if r.Raw == target {
						return count >= 2 && r.Archived
					}
				}
				return false
			})
			records, e := s.rawSources[key].archive.RawRecords()
			if e != nil {
				t.Fatal(e)
			}
			for _, r := range records {
				if r.Raw == target {
					if r.Reclaimed || r.ActiveReferences != 1 {
						t.Fatal("observation discharged independent responsibility", mode, r)
					}
					if mode == "unresolved-dlq" && !r.Quarantined {
						t.Fatal("observation resolved DLQ", r)
					}
				}
			}
		})
	}
}
