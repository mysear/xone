package service

import (
	"context"
	"errors"
	"net/http"
	"net/http/httptest"
	"testing"
	"time"
	"xone/chain-parser-go/internal/chain/evm"
	"xone/chain-parser-go/internal/control"
	"xone/chain-parser-go/internal/domain"
	"xone/chain-parser-go/internal/rpc"
)

type catchupAuthority struct {
	renewals int
	fail     bool
}

func (p *catchupAuthority) Renew(_ context.Context, l control.Lease, _ uint64) (control.Lease, error) {
	if p.fail {
		return l, errors.New("LEASE_EXPIRED_OR_FENCED")
	}
	p.renewals++
	return l, nil
}
func (*catchupAuthority) Check(context.Context, control.Lease, uint64) error { return nil }
func (*catchupAuthority) AllocateBaseline(context.Context, control.Lease, domain.SnapshotRef) (control.Lease, error) {
	panic("unused")
}
func (*catchupAuthority) Committed(context.Context, control.Lease, domain.SnapshotRef) error {
	return nil
}
func (*catchupAuthority) Release(context.Context, control.Lease) error { return nil }

type catchupRead struct {
	*forkFixture
	delay time.Duration
	reads int
}

func (r *catchupRead) ReadComplete(ctx context.Context, _ string) (domain.BlockRef, []domain.Pool, []domain.SwapFact, error) {
	r.reads++
	select {
	case <-time.After(r.delay):
		return r.blocks[100].Reference.Block, r.blocks[100].Pools, nil, nil
	case <-ctx.Done():
		return domain.BlockRef{}, nil, nil, ctx.Err()
	}
}
func TestCatchupReadsRenewAuthorityAcrossLongBatchAndReserveRevocationBudget(t *testing.T) {
	s, ch, _, f := forkService(t)
	cfg := *s.cfg
	cfg.Config.Control.LeaseTTLMS = 200
	cfg.Config.Control.RenewalMS = 10
	cfg.Config.Control.MinCommitMS = 100
	authority := &catchupAuthority{}
	w := &writerSession{pg: authority, cfg: &cfg, store: s.stores[ch.Key], renewed: time.Now()}
	w.store.SetAuthorityDeadline(w.renewed.Add(200 * time.Millisecond))
	reader := &catchupRead{forkFixture: f, delay: 30 * time.Millisecond}
	for i := 0; i < 8; i++ {
		if _, _, _, e := readCompleteWithLease(context.Background(), reader, w, "0x64"); e != nil {
			t.Fatal(e)
		}
		if _, ready := w.store.Current(); !ready {
			t.Fatal("long catchup lost admitted bank before a source error could revoke it")
		}
	}
	if authority.renewals == 0 {
		t.Fatal("no authority renewal during >TTL batch")
	}
	reader.delay = time.Second
	started := time.Now()
	if _, _, _, e := readCompleteWithLease(context.Background(), reader, w, "0x64"); !errors.Is(e, context.DeadlineExceeded) {
		t.Fatal("slow source read must stop with revocation budget remaining", e)
	}
	if time.Since(started) >= 180*time.Millisecond {
		t.Fatal("source read consumed entire writer lease")
	}
	if _, ready := w.store.Current(); !ready {
		t.Fatal("source deadline left no admitted scope for revocation")
	}
}
func TestCatchupLostAuthorityPreventsFurtherSourceRead(t *testing.T) {
	s, ch, _, f := forkService(t)
	authority := &catchupAuthority{fail: true}
	w := &writerSession{pg: authority, cfg: s.cfg, store: s.stores[ch.Key], renewed: time.Time{}}
	r := &catchupRead{forkFixture: f}
	if _, _, _, e := readCompleteWithLease(context.Background(), r, w, "0x64"); e == nil || r.reads != 0 {
		t.Fatal("source read started without renewable authority", e, r.reads)
	}
}

type catchupHeader struct{ calls int }

func (r *catchupHeader) Header(ctx context.Context, _ string) (domain.BlockRef, error) {
	r.calls++
	<-ctx.Done()
	return domain.BlockRef{}, ctx.Err()
}
func TestCatchupForkProbeCannotConsumeReservedRevocationWindow(t *testing.T) {
	s, ch, _, _ := forkService(t)
	cfg := *s.cfg
	cfg.Config.Control.LeaseTTLMS = 200
	cfg.Config.Control.MinCommitMS = 100
	w := &writerSession{cfg: &cfg, store: s.stores[ch.Key], renewed: time.Now().Add(-120 * time.Millisecond)}
	w.store.SetAuthorityDeadline(w.renewed.Add(200 * time.Millisecond))
	probe := &catchupHeader{}
	started := time.Now()
	ctx, cancel := context.WithTimeout(context.Background(), 300*time.Millisecond)
	defer cancel()
	if _, e := readHeaderWithLease(ctx, probe, w, "0x64"); !errors.Is(e, context.DeadlineExceeded) {
		t.Fatal(e)
	}
	if time.Since(started) > 50*time.Millisecond {
		t.Fatal("fork check consumed authority reserved for revocation")
	}
	if _, ready := w.store.Current(); !ready {
		t.Fatal("fork check left no admitted bank to revoke")
	}
}

func TestSlowLiveHeadLeavesAuthorityForScopeRevocation(t *testing.T) {
	s, ch, _, _ := forkService(t)
	s.source = nil
	s.cfg.Config.Control.LeaseTTLMS = 400
	s.cfg.Config.Control.MinCommitMS = 250
	w := &writerSession{cfg: s.cfg, store: s.stores[ch.Key], renewed: time.Now()}
	if s.sessions == nil {
		s.sessions = map[string]*writerSession{}
	}
	s.sessions[ch.Key] = w
	defer delete(s.sessions, ch.Key)
	w.store.SetAuthorityDeadline(w.renewed.Add(400 * time.Millisecond))
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		select {
		case <-r.Context().Done():
			return
		case <-time.After(600 * time.Millisecond):
			w.WriteHeader(503)
		}
	}))
	defer server.Close()
	s.adapters[ch.Key] = &evm.Adapter{Client: rpc.New([]string{server.URL}, time.Second, 1<<20), Chain: ch}
	started := time.Now()
	if e := s.syncChain(context.Background(), ch); e == nil {
		t.Fatal("slow head unexpectedly admitted")
	}
	if time.Since(started) > 300*time.Millisecond {
		t.Fatal("head read consumed lease reserved for scope revocation")
	}
	if _, ready := s.stores[ch.Key].Current(); !ready {
		t.Fatal("head failure lost admitted bank before revocation")
	}
}
