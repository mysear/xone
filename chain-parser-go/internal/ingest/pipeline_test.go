package ingest

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"sync"
	"sync/atomic"
	"testing"
	"time"
)

func fixture(t *testing.T) (*Pipeline, *Archive) {
	t.Helper()
	a, err := OpenArchive(t.TempDir(), ArchiveLimits{MaxBytes: 1 << 20, MaxRecords: 100, MaxRecordBytes: 4096})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { a.Close() })
	p := &Pipeline{Chain: "evm:1", Limits: Limits{Workers: 2, MaxTasks: 2, MaxBytes: 1024, MaxBlockBytes: 512}, Archive: a,
		Fetch: func(ctx context.Context, h uint64, limit int64) (Fetched, error) {
			return Fetched{Height: h, Hash: fmt.Sprint(h), ParentHash: fmt.Sprint(h - 1), ProviderID: "provider", Raw: []byte(fmt.Sprint(h))}, nil
		},
		Decode:   func(ctx context.Context, b Fetched) ([]byte, error) { return []byte("decoded"), nil },
		Validate: func(ctx context.Context, b Fetched, decoded []byte) error { return nil },
	}
	return p, a
}

func TestOrderedActorStagesHPlusOneAndBoundsWindow(t *testing.T) {
	p, _ := fixture(t)
	second := make(chan struct{})
	var active, maxActive atomic.Int32
	fetch := p.Fetch
	p.Fetch = func(ctx context.Context, h uint64, limit int64) (Fetched, error) {
		n := active.Add(1)
		defer active.Add(-1)
		for old := maxActive.Load(); n > old; old = maxActive.Load() {
			if maxActive.CompareAndSwap(old, n) {
				break
			}
		}
		if h == 1 {
			select {
			case <-second:
			case <-ctx.Done():
				return Fetched{}, ctx.Err()
			}
		}
		if h == 2 {
			close(second)
		}
		return fetch(ctx, h, limit)
	}
	var mu sync.Mutex
	var heights []uint64
	p.Commit = func(ctx context.Context, expected Cursor, b Block) error {
		mu.Lock()
		defer mu.Unlock()
		if b.Height != expected.Height+1 {
			t.Error("non contiguous")
		}
		heights = append(heights, b.Height)
		return nil
	}
	cur, err := p.Run(context.Background(), Cursor{Hash: "0"}, 6)
	if err != nil || cur.Height != 6 || fmt.Sprint(heights) != "[1 2 3 4 5 6]" || maxActive.Load() > 2 {
		t.Fatal(cur, err, heights, maxActive.Load())
	}
}

func TestCriticalFailureNeverCrossesGapAndKeepsRawDLQ(t *testing.T) {
	for _, mode := range []string{"fetch", "decode", "validate", "parent", "bytes"} {
		t.Run(mode, func(t *testing.T) {
			p, a := fixture(t)
			fetch := p.Fetch
			var committed []uint64
			p.Fetch = func(ctx context.Context, h uint64, limit int64) (Fetched, error) {
				b, err := fetch(ctx, h, limit)
				if h == 2 {
					switch mode {
					case "fetch":
						return b, errors.New("shard missing")
					case "parent":
						b.ParentHash = "fork"
					case "bytes":
						b.Raw = make([]byte, 513)
					}
				}
				return b, err
			}
			p.Decode = func(ctx context.Context, b Fetched) ([]byte, error) {
				if mode == "decode" && b.Height == 2 {
					return nil, errors.New("unknown event")
				}
				return []byte("ok"), nil
			}
			p.Validate = func(ctx context.Context, b Fetched, d []byte) error {
				if mode == "validate" && b.Height == 2 {
					return errors.New("receipt root")
				}
				return nil
			}
			p.Commit = func(ctx context.Context, expected Cursor, b Block) error {
				committed = append(committed, b.Height)
				return nil
			}
			cur, err := p.Run(context.Background(), Cursor{Hash: "0"}, 4)
			if err == nil || cur.Height > 1 {
				t.Fatal(cur, err, committed)
			}
			for _, h := range committed {
				if h > 1 {
					t.Fatal("crossed gap", committed)
				}
			}
			if mode == "decode" || mode == "validate" {
				entries, err := a.DeadLetters()
				if err != nil || len(entries) != 1 {
					t.Fatal(entries, err)
				}
				raw, err := a.Read(entries[0].Raw)
				if err != nil || string(raw) != "2" {
					t.Fatal(string(raw), err)
				}
			}
		})
	}
}

func TestCommitFailureRestartAndCancelBudgets(t *testing.T) {
	p, a := fixture(t)
	durable := Cursor{Hash: "0"}
	fail := true
	p.Commit = func(ctx context.Context, expected Cursor, b Block) error {
		if expected != durable {
			return errors.New("stale cursor")
		}
		if b.Height == 2 && fail {
			return errors.New("transaction aborted")
		}
		durable = Cursor{Height: b.Height, Hash: b.Hash}
		return nil
	}
	cur, err := p.Run(context.Background(), durable, 3)
	if err == nil || cur.Height != 1 || durable.Height != 1 {
		t.Fatal(cur, durable, err)
	}
	path := a.Directory()
	a.Close()
	reopened, err := OpenArchive(path, a.Limits())
	if err != nil {
		t.Fatal(err)
	}
	defer reopened.Close()
	p.Archive = reopened
	fail = false
	cur, err = p.Run(context.Background(), durable, 3)
	if err != nil || cur.Height != 3 {
		t.Fatal(cur, err)
	}
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	if _, err := p.Run(ctx, cur, 4); !errors.Is(err, context.Canceled) {
		t.Fatal(err)
	}
	p.Limits.MaxBytes = 511
	if _, err := p.Run(context.Background(), cur, 4); !errors.Is(err, ErrBudget) {
		t.Fatal(err)
	}
}

func TestCancellationJoinsWorkersAndNoCommit(t *testing.T) {
	p, _ := fixture(t)
	var active atomic.Int32
	started := make(chan struct{}, 2)
	p.Fetch = func(ctx context.Context, h uint64, limit int64) (Fetched, error) {
		active.Add(1)
		defer active.Add(-1)
		started <- struct{}{}
		<-ctx.Done()
		return Fetched{}, ctx.Err()
	}
	p.Commit = func(context.Context, Cursor, Block) error { t.Error("canceled commit"); return nil }
	ctx, cancel := context.WithCancel(context.Background())
	done := make(chan error, 1)
	go func() { _, err := p.Run(ctx, Cursor{Hash: "0"}, 4); done <- err }()
	<-started
	cancel()
	select {
	case err := <-done:
		if !errors.Is(err, context.Canceled) || active.Load() != 0 {
			t.Fatal(err, active.Load())
		}
	case <-time.After(time.Second):
		t.Fatal("leaked workers")
	}
}

func TestFutureFailureCancelsBlockedEarlierFetch(t *testing.T) {
	p, _ := fixture(t)
	started := make(chan struct{})
	fetch := p.Fetch
	p.Fetch = func(ctx context.Context, h uint64, limit int64) (Fetched, error) {
		if h == 1 {
			close(started)
			<-ctx.Done()
			return Fetched{}, ctx.Err()
		}
		<-started
		b, _ := fetch(ctx, h, limit)
		return b, errors.New("critical shard failure")
	}
	p.Commit = func(context.Context, Cursor, Block) error { t.Error("unexpected commit"); return nil }
	ctx, cancel := context.WithTimeout(context.Background(), time.Second)
	defer cancel()
	_, err := p.Run(ctx, Cursor{Hash: "0"}, 3)
	if err == nil || errors.Is(err, context.DeadlineExceeded) {
		t.Fatal("stage failure did not stop actor", err)
	}
}

func TestDecodeFailurePreservesPoolScope(t *testing.T) {
	p, a := fixture(t)
	p.Decode = func(context.Context, Fetched) ([]byte, error) {
		return nil, &DecodeError{Code: "UNKNOWN_SWAP_VERSION", Scope: "pool-1/protocol-2", Err: errors.New("event 3")}
	}
	p.Commit = func(context.Context, Cursor, Block) error { t.Error("failed decoder committed"); return nil }
	if _, err := p.Run(context.Background(), Cursor{Hash: "0"}, 1); err == nil {
		t.Fatal("decode failure accepted")
	}
	entries, err := a.DeadLetters()
	if err != nil || len(entries) != 1 || entries[0].Code != "UNKNOWN_SWAP_VERSION" || entries[0].Scope != "pool-1/protocol-2" {
		t.Fatal(entries, err)
	}
}

func TestByteBudgetBoundsOutstandingTasksIncludingStagedResults(t *testing.T) {
	p, _ := fixture(t)
	p.Limits = Limits{Workers: 4, MaxTasks: 4, MaxBytes: 1024, MaxBlockBytes: 512}
	firstStarted := make(chan struct{})
	secondFinished := make(chan struct{})
	release := make(chan struct{})
	var fetched atomic.Int32
	fetch := p.Fetch
	p.Fetch = func(ctx context.Context, h uint64, limit int64) (Fetched, error) {
		fetched.Add(1)
		if h == 1 {
			close(firstStarted)
			select {
			case <-release:
			case <-ctx.Done():
				return Fetched{}, ctx.Err()
			}
		}
		if h == 2 {
			close(secondFinished)
		}
		if h > 2 {
			select {
			case <-release:
			default:
				t.Errorf("height %d exceeded staging window", h)
			}
		}
		return fetch(ctx, h, limit)
	}
	p.Commit = func(context.Context, Cursor, Block) error { return nil }
	done := make(chan error, 1)
	go func() { _, err := p.Run(context.Background(), Cursor{Hash: "0"}, 5); done <- err }()
	<-firstStarted
	<-secondFinished
	// Let worker 2 finish staging while H stays blocked. No replacement task may
	// start just because H+1 finished; its bytes still occupy a window slot.
	time.Sleep(20 * time.Millisecond)
	if fetched.Load() != 2 {
		t.Error("unbounded staging", fetched.Load())
	}
	close(release)
	if err := <-done; err != nil {
		t.Fatal(err)
	}
}

func TestAmbiguousCommitRestartLoadsPersistedCursor(t *testing.T) {
	p, a := fixture(t)
	cursorPath := filepath.Join(t.TempDir(), "cursor.json")
	writeCursor := func(cur Cursor) error {
		data, _ := json.Marshal(cur)
		temp, err := os.CreateTemp(filepath.Dir(cursorPath), "cursor-")
		if err != nil {
			return err
		}
		defer os.Remove(temp.Name())
		if _, err = temp.Write(data); err != nil {
			temp.Close()
			return err
		}
		if err = temp.Sync(); err != nil {
			temp.Close()
			return err
		}
		if err = temp.Close(); err != nil {
			return err
		}
		if err = os.Rename(temp.Name(), cursorPath); err != nil {
			return err
		}
		return directorySync(filepath.Dir(cursorPath))
	}
	readCursor := func() Cursor {
		data, err := os.ReadFile(cursorPath)
		if err != nil {
			t.Fatal(err)
		}
		var cur Cursor
		if err = json.Unmarshal(data, &cur); err != nil {
			t.Fatal(err)
		}
		return cur
	}
	if err := writeCursor(Cursor{Hash: "0"}); err != nil {
		t.Fatal(err)
	}
	acknowledged := false
	p.Commit = func(ctx context.Context, expected Cursor, b Block) error {
		if readCursor() != expected {
			return errors.New("stale transaction")
		}
		if err := writeCursor(Cursor{Height: b.Height, Hash: b.Hash}); err != nil {
			return err
		}
		if b.Height == 2 && !acknowledged {
			return errors.New("acknowledgement lost after durable commit")
		}
		return nil
	}
	got, err := p.Run(context.Background(), readCursor(), 3)
	if err == nil || got.Height != 1 || readCursor().Height != 2 {
		t.Fatal(got, readCursor(), err)
	}
	path := a.Directory()
	limits := a.Limits()
	a.Close()
	reopened, err := OpenArchive(path, limits)
	if err != nil {
		t.Fatal(err)
	}
	defer reopened.Close()
	acknowledged = true
	restarted := &Pipeline{Chain: p.Chain, Limits: p.Limits, Archive: reopened, Fetch: p.Fetch, Decode: p.Decode, Validate: p.Validate, Commit: p.Commit}
	got, err = restarted.Run(context.Background(), readCursor(), 3)
	if err != nil || got.Height != 3 || readCursor() != got {
		t.Fatal(got, err)
	}
}

func TestDLQCapacityFailureStillReturnsDurableRawLocation(t *testing.T) {
	p, a := fixture(t)
	path := a.Directory()
	limits := a.Limits()
	a.Close()
	limits.MaxRecords = 1
	a, err := OpenArchive(path, limits)
	if err != nil {
		t.Fatal(err)
	}
	defer a.Close()
	p.Archive = a
	p.Decode = func(context.Context, Fetched) ([]byte, error) { return nil, errors.New("decode rejected") }
	p.Commit = func(context.Context, Cursor, Block) error { t.Error("capacity failed block committed"); return nil }
	_, err = p.Run(context.Background(), Cursor{Hash: "0"}, 1)
	var failure *StageError
	if !errors.Is(err, ErrBudget) || !errors.As(err, &failure) || failure.Raw.ID == "" {
		t.Fatal(err)
	}
	raw, err := a.Read(failure.Raw)
	if err != nil || string(raw) != "1" {
		t.Fatal(string(raw), err)
	}
}
