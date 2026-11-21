package service

import (
	"compress/gzip"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"reflect"
	"strconv"
	"strings"
	"sync"
	"sync/atomic"
	"testing"
	"time"

	"xone/chain-parser-go/internal/domain"
	"xone/chain-parser-go/internal/provenance"
)

var beaconTailProof struct {
	once  sync.Once
	proof *provenance.VerifiedExecutionFinality
	err   error
}

func retainedBeaconTail(t *testing.T, distance uint64) (*beaconOwnerView, domain.BlockRef, domain.BlockRef, map[uint64]domain.BlockRef) {
	t.Helper()
	beaconTailProof.once.Do(func() {
		file, err := os.Open(filepath.Join("../../test/fixtures/source/testdata/beacon-fork-handover", "electra-fulu.json.gz"))
		if err != nil {
			beaconTailProof.err = err
			return
		}
		defer file.Close()
		reader, err := gzip.NewReader(file)
		if err != nil {
			beaconTailProof.err = err
			return
		}
		defer reader.Close()
		var input provenance.OnlineBeaconProof
		if err = json.NewDecoder(reader).Decode(&input); err != nil {
			beaconTailProof.err = err
			return
		}
		beaconTailProof.proof, beaconTailProof.err = provenance.VerifyExecutionFinality("0xd203fe26634fb6d972b70fdc29d9684963183549cab12cae1ce60aea9aa2533c", input.Bootstrap, input.Finality, input.Updates)
	})
	if beaconTailProof.err != nil {
		t.Fatal(beaconTailProof.err)
	}
	height, signedHash := beaconTailProof.proof.ExecutionBlock()
	refs := make(map[uint64]domain.BlockRef, distance+1)
	for offset := uint64(0); offset <= distance; offset++ {
		number := height - offset
		hash := fmt.Sprintf("0x%064x", number)
		if offset == 0 {
			hash = signedHash
		}
		refs[number] = domain.BlockRef{ChainKey: "evm:1", Number: number, Hash: hash, ParentHash: fmt.Sprintf("0x%064x", number-1)}
	}
	view := &beaconOwnerView{proof: beaconTailProof.proof, checkpointAge: ^uint64(0), finalizedAge: ^uint64(0), hashes: map[uint64]string{height: signedHash}}
	return view, refs[height], refs[height-distance], refs
}

func beaconTailHeader(refs map[uint64]domain.BlockRef, tag string) (domain.BlockRef, error) {
	height, err := strconv.ParseUint(strings.TrimPrefix(tag, "0x"), 16, 64)
	if err != nil {
		return domain.BlockRef{}, err
	}
	block, ok := refs[height]
	if !ok {
		return domain.BlockRef{}, fmt.Errorf("unexpected ancestor %s", tag)
	}
	return block, nil
}

func receiveBeaconTail[T any](t *testing.T, ch <-chan T) T {
	t.Helper()
	select {
	case value := <-ch:
		return value
	case <-time.After(5 * time.Second):
		t.Fatal("beacon tail worker did not reach the expected boundary")
		var zero T
		return zero
	}
}

func TestBeaconTailPrefetchBoundAndOrder(t *testing.T) {
	view, head, base, refs := retainedBeaconTail(t, 32)
	started := make(chan uint64, 31)
	completed := make(chan uint64, 31)
	releases := make(map[uint64]chan struct{}, 31)
	for number := head.Number - 1; number > base.Number; number-- {
		releases[number] = make(chan struct{})
	}
	var active, maximum, calls atomic.Int32
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	done := make(chan error, 1)
	go func() {
		done <- view.admitTail(ctx, head, base, func(ctx context.Context, tag string) (domain.BlockRef, error) {
			block, err := beaconTailHeader(refs, tag)
			if err != nil {
				return block, err
			}
			calls.Add(1)
			now := active.Add(1)
			defer active.Add(-1)
			for old := maximum.Load(); now > old && !maximum.CompareAndSwap(old, now); old = maximum.Load() {
			}
			started <- block.Number
			select {
			case <-ctx.Done():
				return domain.BlockRef{}, ctx.Err()
			case <-releases[block.Number]:
				completed <- block.Number
				return block, nil
			}
		})
	}()
	initial := make(map[uint64]bool, 4)
	for range 4 {
		initial[receiveBeaconTail(t, started)] = true
	}
	for offset := uint64(1); offset <= 4; offset++ {
		if !initial[head.Number-offset] {
			t.Fatalf("initial four requests: %v", initial)
		}
	}
	if active.Load() != 4 {
		t.Fatalf("prefetch did not overlap four reads: %d", active.Load())
	}
	// Finish the lowest of the initial four first, before its parent header has
	// completed. Validation must use chain order rather than completion order.
	first := head.Number - 4
	close(releases[first])
	if number := receiveBeaconTail(t, completed); number != first {
		t.Fatalf("first completion: %d", number)
	}
	if number := receiveBeaconTail(t, started); number != head.Number-5 {
		t.Fatalf("next queued ancestor: %d", number)
	}
	for number, release := range releases {
		if number != first {
			close(release)
		}
	}
	if err := receiveBeaconTail(t, done); err != nil {
		t.Fatal(err)
	}
	if maximum.Load() != 4 || active.Load() != 0 || calls.Load() != 31 {
		t.Fatalf("worker accounting: max=%d active=%d calls=%d", maximum.Load(), active.Load(), calls.Load())
	}
	if len(view.hashes) != 32 {
		t.Fatalf("admitted hashes: %d", len(view.hashes))
	}
	for number := head.Number; number > base.Number; number-- {
		if err := view.check(refs[number]); err != nil {
			t.Fatalf("height %d: %v", number, err)
		}
	}
}

func TestBeaconTailRejectsDisconnectedAncestryAtomically(t *testing.T) {
	for _, test := range []struct {
		name   string
		mutate func(*domain.BlockRef, *domain.BlockRef, map[uint64]domain.BlockRef)
		want   string
	}{
		{"middle hash", func(head, _ *domain.BlockRef, refs map[uint64]domain.BlockRef) {
			block := refs[head.Number-3]
			block.Hash = "changed"
			refs[block.Number] = block
		}, "BEACON_FINALITY_PARENT_CONFLICT"},
		{"middle parent", func(head, _ *domain.BlockRef, refs map[uint64]domain.BlockRef) {
			block := refs[head.Number-3]
			block.ParentHash = "changed"
			refs[block.Number] = block
		}, "BEACON_FINALITY_PARENT_CONFLICT"},
		{"middle height", func(head, _ *domain.BlockRef, refs map[uint64]domain.BlockRef) {
			block := refs[head.Number-3]
			block.Number--
			refs[head.Number-3] = block
		}, "BEACON_FINALITY_PARENT_CONFLICT"},
		{"middle chain", func(head, _ *domain.BlockRef, refs map[uint64]domain.BlockRef) {
			block := refs[head.Number-3]
			block.ChainKey = "evm:10"
			refs[block.Number] = block
		}, "BEACON_FINALITY_PARENT_CONFLICT"},
		{"base hash", func(_, base *domain.BlockRef, _ map[uint64]domain.BlockRef) { base.Hash = "changed" }, "BEACON_FINALITY_PARENT_CONFLICT"},
		{"base chain", func(_, base *domain.BlockRef, _ map[uint64]domain.BlockRef) { base.ChainKey = "evm:10" }, "BEACON_FINALITY_PARENT_CONFLICT"},
		{"base above head", func(head, base *domain.BlockRef, _ map[uint64]domain.BlockRef) { base.Number = head.Number + 1 }, "BEACON_FINALITY_PARENT_CONFLICT"},
		{"signed head hash", func(head, _ *domain.BlockRef, _ map[uint64]domain.BlockRef) { head.Hash = "changed" }, "BEACON_FINALITY_BLOCK_CONFLICT"},
		{"signed head chain", func(head, _ *domain.BlockRef, _ map[uint64]domain.BlockRef) { head.ChainKey = "evm:10" }, "BEACON_FINALITY_BLOCK_CONFLICT"},
	} {
		t.Run(test.name, func(t *testing.T) {
			view, head, base, refs := retainedBeaconTail(t, 8)
			view.hashes[head.Number-100] = "previously-admitted"
			before := map[uint64]string{head.Number: head.Hash, head.Number - 100: "previously-admitted"}
			test.mutate(&head, &base, refs)
			err := view.admitTail(context.Background(), head, base, func(_ context.Context, tag string) (domain.BlockRef, error) {
				return beaconTailHeader(refs, tag)
			})
			if err == nil || err.Error() != test.want {
				t.Fatalf("admission error: %v; want %s", err, test.want)
			}
			if !reflect.DeepEqual(view.hashes, before) {
				t.Fatalf("failed ancestry changed eligibility: %v", view.hashes)
			}
		})
	}
}

func TestBeaconTailFailureCancelsAndJoinsWorkers(t *testing.T) {
	for _, rpcFailure := range []bool{true, false} {
		name := "context cancellation"
		if rpcFailure {
			name = "RPC error"
		}
		t.Run(name, func(t *testing.T) {
			view, head, base, refs := retainedBeaconTail(t, 8)
			before := map[uint64]string{head.Number: head.Hash}
			ctx, cancel := context.WithCancel(context.Background())
			defer cancel()
			started := make(chan uint64, 7)
			completed := make(chan struct{}, 1)
			canceled := make(chan uint64, 7)
			success := make(chan struct{})
			fail := make(chan struct{})
			drain := make(chan struct{})
			defer func() {
				select {
				case <-drain:
				default:
					close(drain)
				}
			}()
			rpcErr := errors.New("ancestor RPC failed")
			var active atomic.Int32
			done := make(chan error, 1)
			go func() {
				done <- view.admitTail(ctx, head, base, func(ctx context.Context, tag string) (domain.BlockRef, error) {
					block, err := beaconTailHeader(refs, tag)
					if err != nil {
						return block, err
					}
					active.Add(1)
					defer active.Add(-1)
					started <- block.Number
					if block.Number == head.Number-1 {
						select {
						case <-success:
							completed <- struct{}{}
							return block, nil
						case <-ctx.Done():
							return domain.BlockRef{}, ctx.Err()
						}
					}
					if rpcFailure && block.Number == head.Number-2 {
						select {
						case <-fail:
							return domain.BlockRef{}, rpcErr
						case <-ctx.Done():
							return domain.BlockRef{}, ctx.Err()
						}
					}
					<-ctx.Done()
					canceled <- block.Number
					// Model a transport/raw recorder that needs time to unwind even
					// after cancellation. Admission must join it before returning.
					<-drain
					return domain.BlockRef{}, ctx.Err()
				})
			}()
			for range 4 {
				receiveBeaconTail(t, started)
			}
			close(success)
			receiveBeaconTail(t, completed)
			if number := receiveBeaconTail(t, started); number != head.Number-5 {
				t.Fatalf("next ancestor: %d", number)
			}
			waiting := 4
			if rpcFailure {
				close(fail)
				waiting--
			} else {
				cancel()
			}
			for range waiting {
				receiveBeaconTail(t, canceled)
			}
			select {
			case err := <-done:
				t.Fatalf("admission returned before worker cleanup: %v", err)
			default:
			}
			if !reflect.DeepEqual(view.hashes, before) {
				t.Fatalf("incomplete reads changed eligibility: %v", view.hashes)
			}
			close(drain)
			err := receiveBeaconTail(t, done)
			want := error(context.Canceled)
			if rpcFailure {
				want = rpcErr
			}
			if !errors.Is(err, want) || active.Load() != 0 {
				t.Fatalf("failure cleanup: err=%v active=%d", err, active.Load())
			}
			if !reflect.DeepEqual(view.hashes, before) {
				t.Fatalf("failed reads changed eligibility: %v", view.hashes)
			}
		})
	}
}

func TestBeaconTailRechecksAdmissionConditions(t *testing.T) {
	for _, cancelAfterRead := range []bool{true, false} {
		name := "proof freshness"
		if cancelAfterRead {
			name = "context after successful read"
		}
		t.Run(name, func(t *testing.T) {
			view, head, base, refs := retainedBeaconTail(t, 2)
			before := map[uint64]string{head.Number: head.Hash}
			ctx, cancel := context.WithCancel(context.Background())
			defer cancel()
			err := view.admitTail(ctx, head, base, func(_ context.Context, tag string) (domain.BlockRef, error) {
				if cancelAfterRead {
					cancel()
				} else {
					view.finalizedAge = 1
				}
				return beaconTailHeader(refs, tag)
			})
			if cancelAfterRead {
				if !errors.Is(err, context.Canceled) {
					t.Fatalf("canceled admission: %v", err)
				}
			} else if err == nil || err.Error() != "PROVENANCE_BEACON_FINALITY_AGE" {
				t.Fatalf("stale admission: %v", err)
			}
			if !reflect.DeepEqual(view.hashes, before) {
				t.Fatalf("failed final check changed eligibility: %v", view.hashes)
			}
		})
	}
}

func TestBeaconTailShortBoundaries(t *testing.T) {
	for distance := uint64(0); distance <= 2; distance++ {
		t.Run(fmt.Sprint(distance), func(t *testing.T) {
			view, head, base, refs := retainedBeaconTail(t, distance)
			var calls atomic.Int32
			if err := view.admitTail(context.Background(), head, base, func(_ context.Context, tag string) (domain.BlockRef, error) {
				calls.Add(1)
				return beaconTailHeader(refs, tag)
			}); err != nil {
				t.Fatal(err)
			}
			want := int32(0)
			if distance > 1 {
				want = int32(distance - 1)
			}
			if calls.Load() != want || len(view.hashes) != int(want)+1 {
				t.Fatalf("short tail: calls=%d hashes=%d", calls.Load(), len(view.hashes))
			}
			base.Hash = "changed"
			if err := view.admitTail(context.Background(), head, base, func(_ context.Context, tag string) (domain.BlockRef, error) {
				return beaconTailHeader(refs, tag)
			}); err == nil || err.Error() != "BEACON_FINALITY_PARENT_CONFLICT" {
				t.Fatalf("short tail base substitution: %v", err)
			}
		})
	}
	view, head, base, _ := retainedBeaconTail(t, 2)
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	if err := view.admitTail(ctx, head, base, func(context.Context, string) (domain.BlockRef, error) {
		t.Error("read after context cancellation")
		return domain.BlockRef{}, nil
	}); !errors.Is(err, context.Canceled) {
		t.Fatalf("pre-canceled tail: %v", err)
	}
}
