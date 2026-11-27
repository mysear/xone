package reducer

import (
	"context"
	"encoding/json"
	"errors"
	"github.com/cockroachdb/pebble"
	"os"
	"os/exec"
	"strings"
	"testing"
	"time"
	"xone/chain-parser-go/internal/domain"
	"xone/chain-parser-go/internal/ingest"
)

func historyBatch(height uint64) domain.FactBatch {
	return domain.FactBatch{SchemaVersion: 1, DecoderVersion: "uniswap-v2/1", Blocks: []domain.FactBlock{{Block: domain.BlockRef{ChainKey: "evm:31337", Number: height, Hash: "0x" + strings.Repeat("ab", 32), ParentHash: "0x" + strings.Repeat("ab", 32), TimestampMS: 1000}}}}
}
func historyBaseline(t *testing.T) domain.Snapshot {
	t.Helper()
	block := historyBatch(100).Blocks[0].Block
	pools := []domain.Pool{{
		ChainKey: block.ChainKey, PoolID: "0x" + strings.Repeat("03", 20),
		Token0: "0x" + strings.Repeat("01", 20), Token1: "0x" + strings.Repeat("02", 20),
		Protocol: "uniswap-v2", AdapterVersion: "uniswap-v2/1", TokenBehavior: "standard",
		Reserve0: "1000000", Reserve1: "2000000", FeeNumerator: 3, FeeDenominator: 1000,
	}}
	snapshot, err := domain.NewCurrentStateBaseline(block, pools, "chain-parser-go", 1, block.Number)
	if err != nil {
		t.Fatal(err)
	}
	snapshot.Reference.WriterEpoch = 1
	snapshot.Reference.OnlineGeneration = 1
	snapshot.Reference.JournalSeq = 1
	return snapshot
}
func historyQueue(t *testing.T, s *Store, maxJobs int) *HistoryQueue {
	t.Helper()
	q, e := s.OpenHistoryQueue(HistoryQueueOptions{Chain: "evm:31337", DatasetID: strings.Repeat("de", 32), MaxJobs: maxJobs})
	if e != nil {
		t.Fatal(e)
	}
	return q
}
func TestHistoryQueueReopenRevokesClaimsAndAtomicCompletion(t *testing.T) {
	dir := t.TempDir()
	s, e := Open(dir, "chain-parser-go", 1, 128)
	if e != nil {
		t.Fatal(e)
	}
	q := historyQueue(t, s, 8)
	ctx := context.Background()
	now := time.Now()
	if e = q.Enqueue(ctx, []domain.Gap{{From: 1, To: 1000000, Reason: "HEAD_FIRST_HISTORY_DECOUPLED"}}); e != nil {
		t.Fatal(e)
	}
	c, e := q.Claim(ctx, now)
	if e != nil || c.Height != 1 {
		t.Fatal(c, e)
	}
	if e = s.Close(); e != nil {
		t.Fatal(e)
	}
	s, e = Open(dir, "chain-parser-go", 1, 128)
	if e != nil {
		t.Fatal(e)
	}
	defer s.Close()
	q = historyQueue(t, s, 8)
	if e = q.Complete(ctx, c, historyBatch(1), nil); !errors.Is(e, ErrHistoryClaim) {
		t.Fatalf("stale claim: %v", e)
	}
	c, e = q.Claim(ctx, now)
	if e != nil || c.Height != 1 {
		t.Fatal(c, e)
	}
	if e = q.Complete(ctx, c, historyBatch(1), []HistoryCandidate{{PoolID: "candidate", Protocol: "uniswap-v2"}}); e != nil {
		t.Fatal(e)
	}
	if e = q.Complete(ctx, c, historyBatch(1), nil); !errors.Is(e, ErrHistoryClaim) {
		t.Fatal("duplicate completion", e)
	}
	if e = q.Enqueue(ctx, []domain.Gap{{From: 1, To: 1000000}}); e != nil {
		t.Fatal(e)
	}
	status, e := q.Status(ctx)
	if e != nil || len(status.Jobs) != 1 || status.Jobs[0].Next != 2 || status.FactBlocks != 1 || len(status.Covered) != 1 || status.Covered[0].To != 1 {
		t.Fatal(status, e)
	}
	got, e := q.Facts(ctx, 1)
	if e != nil || got.Blocks[0].Block.Number != 1 {
		t.Fatal(got, e)
	}
	candidates, e := q.Candidates(ctx, 1)
	if e != nil || len(candidates) != 1 {
		t.Fatal(candidates, e)
	}
}
func TestHistoryQueueOverlapCapacityCancelRetryAndFencing(t *testing.T) {
	s, e := Open(t.TempDir(), "chain-parser-go", 1, 128)
	if e != nil {
		t.Fatal(e)
	}
	defer s.Close()
	q := historyQueue(t, s, 2)
	ctx := context.Background()
	now := time.Now()
	if e = q.Enqueue(ctx, []domain.Gap{{From: 10, To: 20}, {From: 15, To: 30}}); e != nil {
		t.Fatal(e)
	}
	before, _ := q.Status(ctx)
	if e = q.Enqueue(ctx, []domain.Gap{{From: 40, To: 50}, {From: 60, To: 70}}); !errors.Is(e, ErrHistoryCapacity) {
		t.Fatal(e)
	}
	after, _ := q.Status(ctx)
	if len(before.Jobs) != len(after.Jobs) {
		t.Fatal("partial enqueue")
	}
	canceled, cancel := context.WithCancel(ctx)
	cancel()
	if e = q.Enqueue(canceled, []domain.Gap{{From: 1, To: 2}}); !errors.Is(e, context.Canceled) {
		t.Fatal(e)
	}
	claim, e := q.Claim(ctx, now)
	if e != nil {
		t.Fatal(e)
	}
	if _, e = q.Claim(ctx, now); !errors.Is(e, ErrHistoryIdle) {
		t.Fatal("parallel claim", e)
	}
	if e = q.Retry(ctx, claim, now.Add(time.Hour), "SOURCE_UNAVAILABLE"); e != nil {
		t.Fatal(e)
	}
	other, e := q.Claim(ctx, now)
	if e != nil || other.Height != 21 {
		t.Fatal(other, e)
	}
	if e = q.Retry(ctx, other, now.Add(time.Hour), "SOURCE_UNAVAILABLE"); e != nil {
		t.Fatal(e)
	}
	if _, e = q.Claim(ctx, now); !errors.Is(e, ErrHistoryIdle) {
		t.Fatal(e)
	}
	q2 := historyQueue(t, s, 2)
	if _, e = q.Claim(ctx, now.Add(2*time.Hour)); !errors.Is(e, ErrHistoryClaim) {
		t.Fatal("old handle", e)
	}
	status, e := q2.Status(ctx)
	if e != nil || status.Jobs[0].Attempts != 1 || status.Jobs[0].LastError != "SOURCE_UNAVAILABLE" {
		t.Fatal(status, e)
	}
}

func TestHistoryQueueRejectsCorruptPersistedJobState(t *testing.T) {
	dir := t.TempDir()
	s, err := Open(dir, "chain-parser-go", 1, 128)
	if err != nil {
		t.Fatal(err)
	}
	q := historyQueue(t, s, 8)
	if err := q.Enqueue(context.Background(), []domain.Gap{{From: 1, To: 2}}); err != nil {
		t.Fatal(err)
	}
	s.Close()
	s, err = Open(dir, "chain-parser-go", 1, 128)
	if err != nil {
		t.Fatal(err)
	}
	defer s.Close()
	corrupt := historyQueueDisk{
		Version: 2, Chain: "evm:31337", Owner: "chain-parser-go", Assignment: 1,
		DatasetID: strings.Repeat("de", 32), Revision: strings.Repeat("ab", 32), OwnerScope: "scope",
		Jobs: []HistoryJob{{From: 1, To: 2, Next: 4, Reason: "corrupt"}},
	}
	raw, err := json.Marshal(corrupt)
	if err != nil {
		t.Fatal(err)
	}
	if err := s.db.Set(historyQueueKey, raw, pebble.Sync); err != nil {
		t.Fatal(err)
	}
	s.Close()
	s, err = Open(dir, "chain-parser-go", 1, 128)
	if err != nil {
		t.Fatal(err)
	}
	defer s.Close()
	_, err = s.OpenHistoryQueue(HistoryQueueOptions{Chain: "evm:31337", DatasetID: strings.Repeat("de", 32), MaxJobs: 8})
	if !errors.Is(err, ErrHistoryState) {
		t.Fatalf("corrupt persisted job accepted: %v", err)
	}
}
func TestHistoryQueueOnlyHistoryAndBoundedFacts(t *testing.T) {
	s, e := Open(t.TempDir(), "chain-parser-go", 1, 128)
	if e != nil {
		t.Fatal(e)
	}
	defer s.Close()
	snap := historyBaseline(t)
	if e = s.InstallBaseline(snap); e != nil {
		t.Fatal(e)
	}
	q, e := s.OpenHistoryQueue(HistoryQueueOptions{Chain: "evm:31337", DatasetID: strings.Repeat("de", 32), MaxBlockBytes: 32})
	if e != nil {
		t.Fatal(e)
	}
	ctx := context.Background()
	if e = q.EnqueueBaseline(ctx, snap); e != nil {
		t.Fatal(e)
	}
	if e = q.Enqueue(ctx, []domain.Gap{{From: 1, To: 2}}); e != nil {
		t.Fatal(e)
	}
	c, e := q.Claim(ctx, time.Now())
	if e != nil {
		t.Fatal(e)
	}
	if e = q.Complete(ctx, c, historyBatch(1), nil); !errors.Is(e, ErrHistoryCapacity) {
		t.Fatal(e)
	}
	got, _ := s.Current()
	if got.Reference != snap.Reference || got.Pools[0].Reserve0 != snap.Pools[0].Reserve0 {
		t.Fatal("history changed online")
	}
	st, _ := q.Status(ctx)
	if st.FactBlocks != 0 || st.Jobs[0].Next != 1 {
		t.Fatal(st)
	}
}

func TestHistoryQueueRecoversBaselineDeclarationWithoutStartupReplay(t *testing.T) {
	dir := t.TempDir()
	s, e := Open(dir, "chain-parser-go", 1, 128)
	if e != nil {
		t.Fatal(e)
	}
	snap := historyBaseline(t)
	snap.HistoryGaps = []domain.Gap{{From: 1, To: snap.Reference.Block.Number - 1, Reason: "HEAD_FIRST_HISTORY_DECOUPLED"}}
	if e = s.InstallBaseline(snap); e != nil {
		t.Fatal(e)
	}
	// Crash window: online baseline was durable before the main thread enqueued.
	if e = s.Close(); e != nil {
		t.Fatal(e)
	}
	s, e = Open(dir, "chain-parser-go", 1, 128)
	if e != nil {
		t.Fatal(e)
	}
	defer s.Close()
	q := historyQueue(t, s, 8)
	st, e := q.Status(context.Background())
	if e != nil || len(st.Gaps) != 2 || st.Gaps[0].From != 1 || st.Gaps[0].To != snap.Reference.Block.Number-1 || st.Gaps[1].From != snap.Reference.Block.Number || st.Gaps[1].Reason != HistoryBaselineFactsGap {
		t.Fatal(st, e)
	}
	if _, ready := s.Current(); ready {
		t.Fatal("history open created online authority")
	}
}
func TestHistoryQueueProcessExitDurability(t *testing.T) {
	if dir := os.Getenv("XONE_HISTORY_CRASH_TEST"); dir != "" {
		s, e := Open(dir, "chain-parser-go", 1, 128)
		if e != nil {
			t.Fatal(e)
		}
		q := historyQueue(t, s, 8)
		ctx := context.Background()
		if e = q.Enqueue(ctx, []domain.Gap{{From: 1, To: 1000000}}); e != nil {
			t.Fatal(e)
		}
		claim, e := q.Claim(ctx, time.Now())
		if e != nil {
			t.Fatal(e)
		}
		if e = q.Complete(ctx, claim, historyBatch(1), nil); e != nil {
			t.Fatal(e)
		}
		if _, e = q.Claim(ctx, time.Now()); e != nil {
			t.Fatal(e)
		}
		os.Exit(0) // Deliberately skip DB and runtime Close after synced writes.
	}
	dir := t.TempDir()
	child := exec.Command(os.Args[0], "-test.run=^TestHistoryQueueProcessExitDurability$")
	child.Env = append(os.Environ(), "XONE_HISTORY_CRASH_TEST="+dir)
	if out, e := child.CombinedOutput(); e != nil {
		t.Fatalf("child: %v %s", e, out)
	}
	s, e := Open(dir, "chain-parser-go", 1, 128)
	if e != nil {
		t.Fatal(e)
	}
	defer s.Close()
	q := historyQueue(t, s, 8)
	st, e := q.Status(context.Background())
	if e != nil || st.FactBlocks != 1 || st.Jobs[0].Next != 2 || st.Jobs[0].Token != "" {
		t.Fatal(st, e)
	}
	c, e := q.Claim(context.Background(), time.Now())
	if e != nil || c.Height != 2 {
		t.Fatal(c, e)
	}
}
func TestHistoryQueueExpiredClaimCannotCompleteOrReleaseNewClaim(t *testing.T) {
	s, e := Open(t.TempDir(), "chain-parser-go", 1, 128)
	if e != nil {
		t.Fatal(e)
	}
	defer s.Close()
	q := historyQueue(t, s, 8)
	ctx := context.Background()
	if e = q.Enqueue(ctx, []domain.Gap{{From: 1, To: 2}}); e != nil {
		t.Fatal(e)
	}
	old, e := q.Claim(ctx, time.Now())
	if e != nil {
		t.Fatal(e)
	}
	fresh, e := q.Claim(ctx, time.Now().Add(time.Hour))
	if e != nil {
		t.Fatal(e)
	}
	if e = q.Complete(ctx, old, historyBatch(1), nil); !errors.Is(e, ErrHistoryClaim) {
		t.Fatal(e)
	}
	if e = q.Retry(ctx, old, time.Now(), "CANCELED"); !errors.Is(e, ErrHistoryClaim) {
		t.Fatal(e)
	}
	if e = q.Complete(ctx, fresh, historyBatch(1), nil); e != nil {
		t.Fatal(e)
	}
}

func TestHistoryQueueRejectsBrokenBaselineAnchorAndAdjacentBlocks(t *testing.T) {
	s, e := Open(t.TempDir(), "chain-parser-go", 1, 128)
	if e != nil {
		t.Fatal(e)
	}
	defer s.Close()
	q := historyQueue(t, s, 8)
	ctx := context.Background()
	snap := historyBaseline(t)
	snap.Reference.Block.Number = 3
	snap.Reference.Block.Hash = historyBatch(3).Blocks[0].Block.Hash
	snap.Reference.Block.ParentHash = "0x" + strings.Repeat("cd", 32)
	snap.HistoryGaps = []domain.Gap{{From: 1, To: 2}}
	if e = s.InstallBaseline(snap); e != nil {
		t.Fatal(e)
	}
	if e = q.EnqueueBaseline(ctx, snap); e != nil {
		t.Fatal(e)
	}
	c, e := q.Claim(ctx, time.Now())
	if e != nil {
		t.Fatal(e)
	}
	if e = q.Complete(ctx, c, historyBatch(1), nil); e != nil {
		t.Fatal(e)
	}
	c, e = q.Claim(ctx, time.Now())
	if e != nil {
		t.Fatal(e)
	}
	if e = q.Complete(ctx, c, historyBatch(2), nil); e == nil {
		t.Fatal("accepted suffix not linking to declared online baseline")
	}
	// Separate interval jobs must still share a valid parent boundary.
	q2 := historyQueue(t, s, 8)
	if e = q2.Enqueue(ctx, []domain.Gap{{From: 3, To: 3}}); e != nil {
		t.Fatal(e)
	}
	// Job 1 remains held on its invalid suffix; persist a retry to select job 3.
	c, e = q2.Claim(ctx, time.Now())
	if e != nil {
		t.Fatal(e)
	}
	if e = q2.Retry(ctx, c, time.Now().Add(time.Hour), "ANCHOR_RETRY"); e != nil {
		t.Fatal(e)
	}
	c, e = q2.Claim(ctx, time.Now())
	if e != nil || c.Height != 3 {
		t.Fatal(c, e)
	}
	if e = q2.Complete(ctx, c, domain.FactBatch{SchemaVersion: 1, DecoderVersion: "uniswap-v2/1", Blocks: []domain.FactBlock{{Block: snap.Reference.Block}}}, nil); e != nil {
		t.Fatal(e)
	}
}
func TestHistoryQueueSuccessfulOldFactsDoNotChangeOnline(t *testing.T) {
	s, e := Open(t.TempDir(), "chain-parser-go", 1, 128)
	if e != nil {
		t.Fatal(e)
	}
	defer s.Close()
	snap := historyBaseline(t)
	if e = s.InstallBaseline(snap); e != nil {
		t.Fatal(e)
	}
	q := historyQueue(t, s, 8)
	ctx := context.Background()
	if e = q.Enqueue(ctx, []domain.Gap{{From: 1, To: 1}}); e != nil {
		t.Fatal(e)
	}
	c, e := q.Claim(ctx, time.Now())
	if e != nil {
		t.Fatal(e)
	}
	if e = q.Complete(ctx, c, historyBatch(1), []HistoryCandidate{{PoolID: "new-history-pool", Protocol: "uniswap-v2"}}); e != nil {
		t.Fatal(e)
	}
	got, ready := s.Current()
	if !ready || got.Reference != snap.Reference || len(got.Pools) != len(snap.Pools) || got.Pools[0].Reserve0 != snap.Pools[0].Reserve0 {
		t.Fatal("old history mutated online state")
	}
}

func TestHistoryQueueExpiredUnreplacedClaimCanPersistRetry(t *testing.T) {
	s, e := Open(t.TempDir(), "chain-parser-go", 1, 128)
	if e != nil {
		t.Fatal(e)
	}
	defer s.Close()
	q := historyQueue(t, s, 8)
	ctx := context.Background()
	if e = q.Enqueue(ctx, []domain.Gap{{From: 1, To: 1}}); e != nil {
		t.Fatal(e)
	}
	c, e := q.Claim(ctx, time.Now().Add(-time.Hour))
	if e != nil {
		t.Fatal(e)
	}
	if e = q.Complete(ctx, c, historyBatch(1), nil); !errors.Is(e, ErrHistoryClaim) {
		t.Fatal(e)
	}
	if e = q.Retry(ctx, c, time.Now().Add(time.Minute), "SOURCE_TIMEOUT"); e != nil {
		t.Fatal("unreplaced expired claim lost its persistent retry", e)
	}
	st, e := q.Status(ctx)
	if e != nil || st.Jobs[0].Attempts != 1 || st.Jobs[0].Token != "" {
		t.Fatal(st, e)
	}
}

func TestHistoryReviewCanonicalRevisionInvalidatesDoneAndClaim(t *testing.T) {
	s, e := Open(t.TempDir(), "chain-parser-go", 1, 128)
	if e != nil {
		t.Fatal(e)
	}
	defer s.Close()
	a := historyBaseline(t)
	a.HistoryGaps = []domain.Gap{{From: 1, To: 2}}
	if e = s.InstallBaseline(a); e != nil {
		t.Fatal(e)
	}
	q := historyQueue(t, s, 8)
	ctx := context.Background()
	first, e := q.Claim(ctx, time.Now())
	if e != nil {
		t.Fatal(e)
	}
	if e = q.Complete(ctx, first, historyBatch(1), nil); e != nil {
		t.Fatal(e)
	}
	old, e := q.Claim(ctx, time.Now())
	if e != nil {
		t.Fatal(e)
	}
	b := a
	b.Reference.CanonicalEpoch++
	b.Reference.OnlineGeneration++
	b.Reference.JournalSeq++
	b.Reference.Block.Hash = "0x" + strings.Repeat("cd", 32)
	if e = s.InstallBaseline(b); e != nil {
		t.Fatal(e)
	}
	if e = q.EnqueueBaseline(ctx, b); e != nil {
		t.Fatal(e)
	}
	t.Run("old-claim", func(t *testing.T) {
		if e = q.Complete(ctx, old, historyBatch(2), nil); e == nil {
			t.Fatal("epoch A claim committed under epoch B")
		}
	})
	t.Run("coverage", func(t *testing.T) {
		st, e := q.Status(ctx)
		if e != nil || len(st.Covered) != 0 || st.FactBlocks != 0 || st.Complete || st.Jobs[0].Next != 1 {
			t.Fatal("old fork counted as current coverage", st, e)
		}
	})
}
func TestHistoryReviewCommitRechecksOwnerCanonicalBeforeEnqueueHook(t *testing.T) {
	s, e := Open(t.TempDir(), "chain-parser-go", 1, 128)
	if e != nil {
		t.Fatal(e)
	}
	defer s.Close()
	a := historyBaseline(t)
	a.HistoryGaps = []domain.Gap{{From: 1, To: 1}}
	if e = s.InstallBaseline(a); e != nil {
		t.Fatal(e)
	}
	q := historyQueue(t, s, 8)
	c, e := q.Claim(context.Background(), time.Now())
	if e != nil {
		t.Fatal(e)
	}
	b := a
	b.Reference.CanonicalEpoch++
	b.Reference.OnlineGeneration++
	b.Reference.JournalSeq++
	b.Reference.Block.Hash = "0x" + strings.Repeat("cd", 32)
	if e = s.InstallBaseline(b); e != nil {
		t.Fatal(e)
	}
	if e = q.Complete(context.Background(), c, historyBatch(1), nil); e == nil {
		t.Fatal("canonical CAS gap accepted old facts before history hook")
	}
}
func TestHistoryReviewActualBatchUnknownIOPoisonsOwner(t *testing.T) {
	s, e := Open(t.TempDir(), "chain-parser-go", 1, 128)
	if e != nil {
		t.Fatal(e)
	}
	defer s.Close()
	q := historyQueue(t, s, 8)
	ctx := context.Background()
	if e = q.Enqueue(ctx, []domain.Gap{{From: 1, To: 1}}); e != nil {
		t.Fatal(e)
	}
	c, e := q.Claim(ctx, time.Now())
	if e != nil {
		t.Fatal(e)
	}
	uncertain := errors.New("history-commit-result-unknown")
	s.mu.Lock()
	s.commitFault = func(b *pebble.Batch) error {
		if e := b.Commit(pebble.Sync); e != nil {
			return e
		}
		return uncertain
	}
	s.mu.Unlock()
	if e = q.Complete(ctx, c, historyBatch(1), nil); !errors.Is(e, uncertain) || !s.NeedsReopen() {
		t.Fatal("history bypassed commitOwnerBatch", e, s.NeedsReopen())
	}
}

func TestHistoryReviewDatasetRebindRevokesClaimAndPreservesOldOwnership(t *testing.T) {
	s, e := Open(t.TempDir(), "chain-parser-go", 1, 128)
	if e != nil {
		t.Fatal(e)
	}
	defer s.Close()
	ctx := context.Background()
	q := historyQueue(t, s, 8)
	if e = q.Enqueue(ctx, []domain.Gap{{From: 1, To: 2}}); e != nil {
		t.Fatal(e)
	}
	c, e := q.Claim(ctx, time.Now())
	if e != nil {
		t.Fatal(e)
	}
	if e = q.Complete(ctx, c, historyBatch(1), nil); e != nil {
		t.Fatal(e)
	}
	old, e := q.Claim(ctx, time.Now())
	if e != nil {
		t.Fatal(e)
	}
	before, _ := q.Status(ctx)
	nextDataset, _ := domain.Hash("pool A+B; factory deployment v2")
	if e = q.BindDataset(ctx, nextDataset, s.DurableRef()); e != nil {
		t.Fatal(e)
	}
	if e = q.Complete(ctx, old, historyBatch(2), nil); !errors.Is(e, ErrHistoryClaim) {
		t.Fatal("old dataset claim accepted", e)
	}
	st, e := q.Status(ctx)
	if e != nil || st.DatasetID == before.DatasetID || st.Complete || len(st.Covered) != 0 || st.Jobs[0].Next != 1 {
		t.Fatal(st, e)
	}
	archived, e := q.FactsAt(ctx, before.DatasetID, before.Revision, 1)
	if e != nil || archived.Blocks[0].Block.Number != 1 {
		t.Fatal("lost old dataset ownership", archived, e)
	}
	if _, e = q.Facts(ctx, 1); !errors.Is(e, pebble.ErrNotFound) {
		t.Fatal("old dataset exposed as current", e)
	}
}

func TestHistoryQueueRepeatedDatasetBindingRetainsOneCopy(t *testing.T) {
	ctx := context.Background()
	dir := t.TempDir()
	s, err := Open(dir, "chain-parser-go", 1, 128)
	if err != nil {
		t.Fatal(err)
	}
	defer func() { s.Close() }()
	q := historyQueue(t, s, 8)
	if err := q.Enqueue(ctx, []domain.Gap{{From: 1, To: 1}}); err != nil {
		t.Fatal(err)
	}
	current, err := q.Status(ctx)
	if err != nil {
		t.Fatal(err)
	}
	other, err := domain.Hash("another immutable history dataset")
	if err != nil {
		t.Fatal(err)
	}
	facts := historyBatch(1)
	candidates := []HistoryCandidate{{PoolID: "pool", Protocol: "uniswap-v2"}}
	claim, err := q.Claim(ctx, time.Now())
	if err != nil {
		t.Fatal(err)
	}
	if err := q.Complete(ctx, claim, facts, candidates); err != nil {
		t.Fatal(err)
	}
	var settledBytes int
	for cycle := 0; cycle < 8; cycle++ {
		for _, dataset := range []string{other, current.DatasetID} {
			if err := q.BindDataset(ctx, dataset, s.DurableRef()); err != nil {
				t.Fatal(err)
			}
		}
		before, err := q.Status(ctx)
		if err != nil {
			t.Fatal(err)
		}
		claim, err := q.Claim(ctx, time.Now())
		if err != nil {
			t.Fatal(err)
		}
		conflictingFacts := historyBatch(1)
		conflictingFacts.Blocks[0].Block.TimestampMS++
		if err := q.Complete(ctx, claim, conflictingFacts, candidates); err == nil || err.Error() != "HISTORY_FACT_CONFLICT" {
			t.Fatalf("conflicting facts: %v", err)
		}
		if err := q.Complete(ctx, claim, facts, []HistoryCandidate{{PoolID: "different", Protocol: "uniswap-v2"}}); err == nil || err.Error() != "HISTORY_CANDIDATE_CONFLICT" {
			t.Fatalf("conflicting candidates: %v", err)
		}
		if err := q.Complete(ctx, claim, facts, candidates); err != nil {
			t.Fatal(err)
		}
		after, err := q.Status(ctx)
		if err != nil || after.Bytes != before.Bytes || after.FactBlocks != 1 || !after.Complete {
			t.Fatalf("replay changed capacity or coverage: before %v, after %v, error %v", before.Bytes, after, err)
		}
		if cycle == 1 {
			settledBytes = after.Bytes
		} else if cycle > 1 && after.Bytes != settledBytes {
			t.Fatalf("dataset switching grew storage from %v to %v", settledBytes, after.Bytes)
		}
	}
	if err := s.Close(); err != nil {
		t.Fatal(err)
	}
	s, err = Open(dir, "chain-parser-go", 1, 128)
	if err != nil {
		t.Fatal(err)
	}
	q = historyQueue(t, s, 8)
	reopened, err := q.Status(ctx)
	if err != nil || reopened.Bytes != settledBytes || !reopened.Complete {
		t.Fatalf("persisted capacity or coverage: %v, error %v", reopened, err)
	}
	stored, err := q.FactsAt(ctx, current.DatasetID, current.Revision, 1)
	if err != nil || stored.Blocks[0].Block != facts.Blocks[0].Block {
		t.Fatalf("committed facts changed after replay: %v, error %v", stored, err)
	}
}

func TestHistoryReviewBaselineHIsIndependentAndNeverSnapshotFactProof(t *testing.T) {
	dir := t.TempDir()
	s, e := Open(dir, "chain-parser-go", 1, 128)
	if e != nil {
		t.Fatal(e)
	}
	snap := historyBaseline(t)
	snap.HistoryGaps = nil
	if e = s.InstallBaseline(snap); e != nil {
		t.Fatal(e)
	}
	if e = s.Close(); e != nil {
		t.Fatal(e)
	}
	s, e = Open(dir, "chain-parser-go", 1, 128)
	if e != nil {
		t.Fatal(e)
	}
	defer s.Close()
	q := historyQueue(t, s, 8)
	st, e := q.Status(context.Background())
	if e != nil || st.Complete || len(st.Gaps) != 1 || st.Gaps[0].From != snap.Reference.Block.Number || st.Gaps[0].To != snap.Reference.Block.Number || st.Gaps[0].Reason != HistoryBaselineFactsGap {
		t.Fatal(st, e)
	}
	if _, e = q.Facts(context.Background(), snap.Reference.Block.Number); !errors.Is(e, pebble.ErrNotFound) {
		t.Fatal("snapshot pretended to collect facts", e)
	}
	c, e := q.Claim(context.Background(), time.Now())
	if e != nil {
		t.Fatal(e)
	}
	f := historyBatch(c.Height)
	f.Blocks[0].Block = snap.Reference.Block
	if e = q.Complete(context.Background(), c, f, nil); e != nil {
		t.Fatal(e)
	}
	st, e = q.Status(context.Background())
	if e != nil || !st.Complete || st.FactBlocks != 1 {
		t.Fatal(st, e)
	}
}

func TestHistoryReviewUnknownCommitRawProofSurvivesOwnerReopen(t *testing.T) {
	dir := t.TempDir()
	s, e := Open(dir, "chain-parser-go", 1, 128)
	if e != nil {
		t.Fatal(e)
	}
	q := historyQueue(t, s, 8)
	ctx := context.Background()
	if e = q.Enqueue(ctx, []domain.Gap{{From: 1, To: 1}}); e != nil {
		t.Fatal(e)
	}
	claim, e := q.Claim(ctx, time.Now())
	if e != nil {
		t.Fatal(e)
	}
	a, e := ingest.OpenArchive(t.TempDir(), ingest.ArchiveLimits{MaxBytes: 1 << 20, MaxRecords: 32, MaxRecordBytes: 1 << 16})
	if e != nil {
		t.Fatal(e)
	}
	defer a.Close()
	ref, e := a.PutPinned(ingest.Owner{Chain: "evm:31337", ProviderID: "public", Hash: "block-proof"}, []byte(`{"result":"raw"}`), "history-step-1")
	if e != nil {
		t.Fatal(e)
	}
	group, _ := domain.Hash("raw-capture-group-1")
	uncertain := errors.New("history-fsync-unknown")
	s.mu.Lock()
	s.commitFault = func(b *pebble.Batch) error {
		if e := b.Commit(pebble.Sync); e != nil {
			return e
		}
		return uncertain
	}
	s.mu.Unlock()
	if e = q.Complete(ctx, claim, historyBatch(1), nil, HistoryRawGroup{ID: group, Pins: []HistoryRawPin{{Raw: ref, Reference: "history-step-1"}}}); !errors.Is(e, uncertain) {
		t.Fatal(e)
	}
	if _, e = q.RawReceipt(ctx, group); !errors.Is(e, ErrOwnerReopenRequired) {
		t.Fatal("uncertain DB authorized pin release", e)
	}
	records, _ := a.RawRecords()
	if records[0].ActiveReferences != 1 {
		t.Fatal(records)
	}
	s.Close()
	s, e = Open(dir, "chain-parser-go", 1, 128)
	if e != nil {
		t.Fatal(e)
	}
	defer s.Close()
	q = historyQueue(t, s, 8)
	proof, e := q.RawReceipt(ctx, group)
	if e != nil || proof.Claim != claim || len(proof.Pins) != 1 || proof.Pins[0].Raw != ref {
		t.Fatal(proof, e)
	}
	if e = a.ReleaseRaw(ref, proof.Pins[0].Reference); e != nil {
		t.Fatal(e)
	}
	if e = q.AcknowledgeRaw(ctx, proof); e != nil {
		t.Fatal(e)
	}
}

func TestHistoryReviewQueueRequiresExplicitDatasetIdentity(t *testing.T) {
	s, e := Open(t.TempDir(), "chain-parser-go", 1, 128)
	if e != nil {
		t.Fatal(e)
	}
	defer s.Close()
	if _, e = s.OpenHistoryQueue(HistoryQueueOptions{Chain: "evm:31337"}); !errors.Is(e, ErrHistoryDataset) {
		t.Fatal("unbound queue may claim coverage without pool/factory/decoder identity", e)
	}
	if s.NeedsReopen() {
		t.Fatal("missing dataset validation poisoned owner")
	}
}
