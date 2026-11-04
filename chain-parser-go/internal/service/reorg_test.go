package service

import (
	"context"
	"fmt"
	"strconv"
	"strings"
	"testing"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/domain"
)

type forkFixture struct {
	blocks   map[uint64]domain.Snapshot
	head     uint64
	disagree bool
	reads    []uint64
}

func (f *forkFixture) ReadState(_ context.Context, tag string, complete bool) (domain.BlockRef, []domain.Pool, error) {
	n := f.head
	if strings.HasPrefix(tag, "0x") {
		n, _ = strconv.ParseUint(tag[2:], 16, 64)
	}
	s, ok := f.blocks[n]
	if !ok {
		return domain.BlockRef{}, nil, fmt.Errorf("MISSING_FIXTURE_BLOCK")
	}
	if complete {
		f.reads = append(f.reads, n)
	}
	return s.Reference.Block, s.Pools, nil
}
func (f *forkFixture) ConfirmBlock(_ context.Context, _ domain.BlockRef) error {
	if f.disagree {
		return fmt.Errorf("FORK_PROVIDER_DISAGREEMENT")
	}
	return nil
}
func forkService(t *testing.T) (*Service, config.Chain, domain.Snapshot, *forkFixture) {
	t.Helper()
	cfg, e := config.Load("../../config/config-test.yaml", "chain-parser-go")
	if e != nil {
		t.Fatal(e)
	}
	cfg.Config.State.Path = t.TempDir()
	s, e := NewWithSource(cfg, testSnapshotSource(t))
	if e != nil {
		t.Fatal(e)
	}
	t.Cleanup(s.Close)
	if e = s.Start(context.Background()); e != nil {
		t.Fatal(e)
	}
	ch := cfg.Config.Chains[0]
	ch.ReorgWindow = 4
	base, _ := s.stores[ch.Key].Current()
	tip := base
	for n := uint64(101); n <= 102; n++ {
		next := tip
		next.Reference.JournalSeq++
		next.Reference.Block.Number = n
		next.Reference.Block.ParentHash = tip.Reference.Block.Hash
		next.Reference.Block.Hash = fmt.Sprintf("0x%064x", n+1000)
		if e = s.stores[ch.Key].Commit(next); e != nil {
			t.Fatal(e)
		}
		tip = next
	}
	f := &forkFixture{blocks: map[uint64]domain.Snapshot{100: base}, head: 103}
	parent := base.Reference.Block.Hash
	for n := uint64(101); n <= 103; n++ {
		next := base
		next.Reference.Block.Number = n
		next.Reference.Block.ParentHash = parent
		next.Reference.Block.Hash = fmt.Sprintf("0x%064x", n+2000)
		next.Pools = append([]domain.Pool(nil), base.Pools...)
		next.Pools[0].Reserve0 = domain.Amount(fmt.Sprint(1000000 + n))
		f.blocks[n] = next
		parent = next.Reference.Block.Hash
	}
	return s, ch, tip, f
}
func TestBoundedForkPublishesOnlyBeginAndCompleteBranch(t *testing.T) {
	s, ch, tip, f := forkService(t)
	if e := s.recoverFork(context.Background(), ch, tip, f); e != nil {
		t.Fatal(e)
	}
	got, ready := s.stores[ch.Key].Current()
	if !ready || got.Reference.Block.Hash != fmt.Sprintf("0x%064x", 2103) || got.Reference.CanonicalEpoch != tip.Reference.CanonicalEpoch+1 || got.Reference.JournalSeq != tip.Reference.JournalSeq+3 || got.Reference.OnlineGeneration != tip.Reference.OnlineGeneration {
		t.Fatal(got.Reference, ready)
	}
	if fmt.Sprint(f.reads) != "[101 102 103]" {
		t.Fatal("new branch not read in order", f.reads)
	}
	records, e := s.stores[ch.Key].Records(tip.Reference.JournalSeq, 10)
	if e != nil || len(records) != 3 || records[0].Kind != "ONLINE_SCOPE_REVOKED" || records[1].Kind != "REORG_BEGIN" || records[2].Kind != "REORG_COMMIT" {
		t.Fatal(records, e)
	}
	parent, e := s.stores[ch.Key].UndoParent(got)
	if e != nil || parent.Reference.Block.Hash != fmt.Sprintf("0x%064x", 2102) {
		t.Fatal("replacement undo lost", e)
	}
}
func TestDeepOrUnconfirmedForkCannotPublishReady(t *testing.T) {
	for _, mode := range []string{"deep", "disagree", "gap"} {
		t.Run(mode, func(t *testing.T) {
			s, ch, tip, f := forkService(t)
			switch mode {
			case "deep":
				ch.ReorgWindow = 1
			case "disagree":
				f.disagree = true
			case "gap":
				b := f.blocks[102]
				b.Reference.Block.ParentHash = "wrong"
				f.blocks[102] = b
			}
			if e := s.recoverFork(context.Background(), ch, tip, f); e == nil {
				t.Fatal("invalid branch accepted")
			}
			if mode == "disagree" && s.stores[ch.Key].DurableRef().CanonicalEpoch != tip.Reference.CanonicalEpoch {
				t.Fatal("unconfirmed fork advanced canonical epoch")
			}
			if _, ready := s.stores[ch.Key].Current(); ready {
				t.Fatal("failed reorg made ready")
			}
			if _, e := s.stores[ch.Key].Bootstrap(); e == nil {
				t.Fatal("failed reorg bootstrap")
			}
		})
	}
}

func (f *forkFixture) ReadComplete(ctx context.Context, tag string) (domain.BlockRef, []domain.Pool, []domain.SwapFact, error) {
	b, p, e := f.ReadState(ctx, tag, true)
	return b, p, []domain.SwapFact{}, e
}
