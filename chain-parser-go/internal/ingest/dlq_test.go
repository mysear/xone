package ingest

import (
	"context"
	"encoding/json"
	"errors"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

func repairFixture(t *testing.T) (*Archive, DeadLetter, ReplaySpec) {
	t.Helper()
	a, err := OpenArchive(t.TempDir(), ArchiveLimits{MaxBytes: 4 << 20, MaxRecords: 1000, MaxRecordBytes: 4096})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { a.Close() })
	ref, err := a.Put(Owner{Chain: "evm:1", Height: 7, Hash: "block7", ProviderID: "rpc-a"}, []byte("raw"))
	if err != nil {
		t.Fatal(err)
	}
	d := DeadLetter{Raw: ref, Stage: "decode", Code: "UNKNOWN", Scope: "pool", Message: "original exception"}
	if err := a.Quarantine(d); err != nil {
		t.Fatal(err)
	}
	incident, err := a.CurrentIncident(DeadLetterID(d))
	if err != nil {
		t.Fatal(err)
	}
	s := ReplaySpec{IncidentID: incident.ID, DeadLetterID: DeadLetterID(d), Raw: ref, DecoderVersion: "decoder-v2", Namespace: NamespaceOnline, WriterID: "owner-a", Fence: 2, Generation: "gen-1"}
	return a, d, s
}
func replayLimits() ReplayLimits {
	return ReplayLimits{MaxRawBytes: 4096, MaxDecodedBytes: 4096, Timeout: time.Second}
}
func decoded(_ context.Context, _ ReplaySpec, _ []byte, _ int64) ([]byte, error) {
	return []byte("fixed"), nil
}
func confirmed(_ context.Context, intent ReplayIntent) (CommitReceipt, error) {
	return ReceiptFor(intent, CommitConfirmed, "journal:7"), nil
}

func TestDLQUnknownCommitSurvivesRestartWithoutRepublish(t *testing.T) {
	a, d, s := repairFixture(t)
	calls := 0
	cb := ReplayCallbacks{Decode: decoded, CommitOnline: func(ctx context.Context, i ReplayIntent, b []byte) (CommitReceipt, error) {
		calls++
		return CommitReceipt{}, errors.New("ack lost")
	}, ConfirmOnline: confirmed}
	r, err := a.Replay(context.Background(), s, replayLimits(), cb)
	if !errors.Is(err, ErrCommitUnknown) || r.Status != Retry {
		t.Fatalf("%+v %v", r, err)
	}
	if ok, err := a.IsQuarantined(d.Raw); err != nil || !ok {
		t.Fatal(ok, err)
	}
	path, lim := a.Directory(), a.Limits()
	a.Close()
	a, err = OpenArchive(path, lim)
	if err != nil {
		t.Fatal(err)
	}
	defer a.Close()
	r, err = a.Replay(context.Background(), s, replayLimits(), cb)
	if err != nil || r.Status != Resolved || calls != 1 {
		t.Fatalf("%+v calls=%d %v", r, calls, err)
	}
	r, err = a.Replay(context.Background(), s, replayLimits(), cb)
	if err != nil || calls != 1 || r.Status != Resolved {
		t.Fatal(r, calls, err)
	}
	entries, err := a.DeadLetters()
	if err != nil || len(entries) != 1 || entries[0] != d {
		t.Fatal(entries, err)
	}
	audits, err := a.ReplayAudit(s.DeadLetterID)
	if err != nil || len(audits) < 3 {
		t.Fatal(audits, err)
	}
}
func TestDLQCancellationAfterPublishRemainsUnknown(t *testing.T) {
	a, d, s := repairFixture(t)
	ctx, cancel := context.WithCancel(context.Background())
	cb := ReplayCallbacks{Decode: decoded, CommitOnline: func(_ context.Context, i ReplayIntent, _ []byte) (CommitReceipt, error) {
		cancel()
		return ReceiptFor(i, CommitConfirmed, "journal:7"), nil
	}}
	r, err := a.Replay(ctx, s, replayLimits(), cb)
	if !errors.Is(err, context.Canceled) || !errors.Is(err, ErrCommitUnknown) || r.Status != Retry {
		t.Fatal(r, err)
	}
	if ok, _ := a.IsQuarantined(d.Raw); !ok {
		t.Fatal("cancel released quarantine")
	}
}
func TestDLQScopeFencingBudgetAndReceipt(t *testing.T) {
	for _, mode := range []string{"raw", "history", "receipt", "budget", "decode", "fence"} {
		t.Run(mode, func(t *testing.T) {
			a, d, s := repairFixture(t)
			calls := 0
			cb := ReplayCallbacks{Decode: decoded, CommitOnline: func(_ context.Context, i ReplayIntent, _ []byte) (CommitReceipt, error) {
				calls++
				r := ReceiptFor(i, CommitConfirmed, "journal:7")
				if mode == "receipt" {
					r.Generation = "wrong"
				}
				return r, nil
			}}
			lim := replayLimits()
			switch mode {
			case "raw":
				s.Raw.Digest = "wrong"
			case "history":
				s.Namespace = NamespaceHistory
			case "budget":
				lim.MaxDecodedBytes = 1
			case "decode":
				cb.Decode = func(context.Context, ReplaySpec, []byte, int64) ([]byte, error) {
					return nil, errors.New("still broken")
				}
			case "fence":
				if err := a.AdvanceReplayFence(ReplayAuthority{Namespace: NamespaceOnline, Chain: d.Raw.Owner.Chain, WriterID: "new-owner", Fence: 3, Generation: "gen-2"}); err != nil {
					t.Fatal(err)
				}
			}
			r, err := a.Replay(context.Background(), s, lim, cb)
			if err == nil {
				t.Fatal("unsafe replay accepted", r)
			}
			if mode != "receipt" && calls != 0 {
				t.Fatal("publish ran", calls)
			}
			if ok, _ := a.IsQuarantined(d.Raw); !ok {
				t.Fatal("quarantine lost")
			}
		})
	}
}

func TestDLQDurabilityFailureBeforeAndAfterCommit(t *testing.T) {
	for _, window := range []string{"intent", "resolved"} {
		t.Run(window, func(t *testing.T) {
			a, d, s := repairFixture(t)
			calls := 0
			injected := errors.New("directory sync crash")
			cb := ReplayCallbacks{Decode: func(ctx context.Context, s ReplaySpec, b []byte, max int64) ([]byte, error) {
				if window == "intent" {
					count := 0
					a.syncDir = func(path string) error {
						count++
						if count == 2 {
							return injected
						}
						return directorySync(path)
					}
				}
				return decoded(ctx, s, b, max)
			}, CommitOnline: func(_ context.Context, i ReplayIntent, _ []byte) (CommitReceipt, error) {
				calls++
				a.syncDir = func(string) error { return injected }
				return ReceiptFor(i, CommitConfirmed, "journal:7"), nil
			}, ConfirmOnline: confirmed}
			r, err := a.Replay(context.Background(), s, replayLimits(), cb)
			if !errors.Is(err, injected) {
				t.Fatal(r, err)
			}
			if _, err := a.Read(d.Raw); err == nil {
				t.Fatal("uncertain durability did not poison")
			}
			path, lim := a.Directory(), a.Limits()
			a.Close()
			a, err = OpenArchive(path, lim)
			if err != nil {
				t.Fatal(err)
			}
			defer a.Close()
			r, err = a.Replay(context.Background(), s, replayLimits(), cb)
			if err != nil || r.Status != Resolved {
				t.Fatal(r, err)
			}
			if window == "intent" && calls != 0 {
				t.Fatal("unknown intent republished", calls)
			}
			if window == "resolved" && calls != 1 {
				t.Fatal("confirmed result republished", calls)
			}
		})
	}
}
func TestDLQRetryAndHistoryDoNotEraseOriginalFailure(t *testing.T) {
	a, d, s := repairFixture(t)
	cb := ReplayCallbacks{Decode: func(context.Context, ReplaySpec, []byte, int64) ([]byte, error) {
		return nil, errors.New("bad adapter")
	}, CommitOnline: func(c context.Context, i ReplayIntent, b []byte) (CommitReceipt, error) { return confirmed(c, i) }}
	first, err := a.Replay(context.Background(), s, replayLimits(), cb)
	if err == nil || first.Status != Failed {
		t.Fatal(first, err)
	}
	s.Attempt++
	s.Namespace = NamespaceHistory
	cb = ReplayCallbacks{Decode: decoded, CommitHistory: func(c context.Context, i ReplayIntent, b []byte) (CommitReceipt, error) { return confirmed(c, i) }}
	r, err := a.Replay(context.Background(), s, replayLimits(), cb)
	if err != nil || r.Status != Resolved {
		t.Fatal(r, err)
	}
	if q, _ := a.IsQuarantined(d.Raw); !q {
		t.Fatal("history cleared online quarantine")
	}
	s.Attempt++
	s.Namespace = NamespaceOnline
	cb = ReplayCallbacks{Decode: decoded, CommitOnline: func(c context.Context, i ReplayIntent, b []byte) (CommitReceipt, error) { return confirmed(c, i) }}
	r, err = a.Replay(context.Background(), s, replayLimits(), cb)
	if err != nil || r.Status != Resolved {
		t.Fatal(r, err)
	}
	if q, _ := a.IsQuarantined(d.Raw); q {
		t.Fatal("owner confirmed online repair still quarantined")
	}
	audits, err := a.ReplayAudit(s.DeadLetterID)
	if err != nil {
		t.Fatal(err)
	}
	found := false
	for _, record := range audits {
		if record.Status == Failed && record.Message == "bad adapter" {
			found = true
		}
	}
	if !found {
		t.Fatal("original failed repair overwritten")
	}
}
func TestDLQAuthoritativeNotCommittedAllowsExplicitNewAttempt(t *testing.T) {
	a, _, s := repairFixture(t)
	calls := 0
	cb := ReplayCallbacks{Decode: decoded, CommitOnline: func(_ context.Context, i ReplayIntent, _ []byte) (CommitReceipt, error) {
		calls++
		return CommitReceipt{}, errors.New("unknown")
	}, ConfirmOnline: func(_ context.Context, i ReplayIntent) (CommitReceipt, error) {
		return ReceiptFor(i, CommitNotCommitted, "fenced transaction and settled abort"), nil
	}}
	if _, err := a.Replay(context.Background(), s, replayLimits(), cb); !errors.Is(err, ErrCommitUnknown) {
		t.Fatal(err)
	}
	next := s
	next.Attempt++
	if _, err := a.Replay(context.Background(), next, replayLimits(), cb); !errors.Is(err, ErrReplayBusy) {
		t.Fatal("unknown commit bypass", err)
	}
	if r, err := a.Replay(context.Background(), s, replayLimits(), cb); err == nil || r.Status != Retry || r.Phase != "finished" {
		t.Fatal(r, err)
	}
	cb.CommitOnline = func(c context.Context, i ReplayIntent, b []byte) (CommitReceipt, error) {
		calls++
		return confirmed(c, i)
	}
	if r, err := a.Replay(context.Background(), next, replayLimits(), cb); err != nil || r.Status != Resolved || calls != 2 {
		t.Fatal(r, calls, err)
	}
}
func TestDLQActiveReplayAndTakeoverPreventStalePublication(t *testing.T) {
	a, d, s := repairFixture(t)
	entered := make(chan struct{})
	release := make(chan struct{})
	result := make(chan error, 1)
	go func() {
		_, err := a.Replay(context.Background(), s, replayLimits(), ReplayCallbacks{Decode: func(c context.Context, s ReplaySpec, b []byte, m int64) ([]byte, error) {
			close(entered)
			<-release
			return decoded(c, s, b, m)
		}, CommitOnline: func(context.Context, ReplayIntent, []byte) (CommitReceipt, error) {
			t.Error("stale publisher ran")
			return CommitReceipt{}, nil
		}})
		result <- err
	}()
	<-entered
	if _, err := a.Replay(context.Background(), s, replayLimits(), ReplayCallbacks{}); !errors.Is(err, ErrReplayBusy) {
		t.Fatal(err)
	}
	if err := a.AdvanceReplayFence(ReplayAuthority{Namespace: NamespaceOnline, Chain: d.Raw.Owner.Chain, WriterID: "new", Fence: 3, Generation: "g2"}); err != nil {
		t.Fatal(err)
	}
	close(release)
	if err := <-result; !errors.Is(err, ErrReplayFence) {
		t.Fatal(err)
	}
	if q, _ := a.IsQuarantined(d.Raw); !q {
		t.Fatal("fenced repair released quarantine")
	}
}

func TestDLQFinalAuditCapacityFailureIsStillUnknown(t *testing.T) {
	a, d, s := repairFixture(t)
	a.limits.MaxRecords = a.records + 4
	calls := 0
	cb := ReplayCallbacks{Decode: decoded, CommitOnline: func(c context.Context, i ReplayIntent, b []byte) (CommitReceipt, error) {
		calls++
		return confirmed(c, i)
	}}
	r, err := a.Replay(context.Background(), s, replayLimits(), cb)
	if !errors.Is(err, ErrBudget) || !errors.Is(err, ErrCommitUnknown) || r.Status != Retry {
		t.Fatal("unrecorded success escaped", r, err)
	}
	if q, _ := a.IsQuarantined(d.Raw); !q {
		t.Fatal("capacity failure released quarantine")
	}
	path, lim := a.Directory(), a.Limits()
	lim.MaxRecords = 100
	a.Close()
	a, err = OpenArchive(path, lim)
	if err != nil {
		t.Fatal(err)
	}
	defer a.Close()
	cb.ConfirmOnline = confirmed
	r, err = a.Replay(context.Background(), s, replayLimits(), cb)
	if err != nil || r.Status != Resolved || calls != 1 {
		t.Fatal(r, calls, err)
	}
}
func TestDLQAuditMutationIsDetectedBeforeRecovery(t *testing.T) {
	a, _, s := repairFixture(t)
	if _, err := a.Replay(context.Background(), s, replayLimits(), ReplayCallbacks{Decode: decoded, CommitOnline: func(context.Context, ReplayIntent, []byte) (CommitReceipt, error) {
		return CommitReceipt{}, errors.New("lost ack")
	}}); !errors.Is(err, ErrCommitUnknown) {
		t.Fatal(err)
	}
	for _, e := range a.events {
		if e.Replay != nil && e.Replay.Phase == "prepared" {
			path := filepath.Join(a.directory, jsonID(e)+".event")
			if err := os.WriteFile(path, []byte("corrupt"), 0600); err != nil {
				t.Fatal(err)
			}
			break
		}
	}
	called := false
	_, err := a.Replay(context.Background(), s, replayLimits(), ReplayCallbacks{ConfirmOnline: func(c context.Context, i ReplayIntent) (CommitReceipt, error) { called = true; return confirmed(c, i) }})
	if !errors.Is(err, ErrCorrupt) || called {
		t.Fatal("corrupt audit reconciled", called, err)
	}
	if _, _, err := a.Usage(); err == nil {
		t.Fatal("corruption did not poison")
	}
}

func TestDLQPendingEnumeratesExactIntentAcrossRestart(t *testing.T) {
	a, _, s := repairFixture(t)
	if _, err := a.Replay(context.Background(), s, replayLimits(), ReplayCallbacks{Decode: decoded, CommitOnline: func(context.Context, ReplayIntent, []byte) (CommitReceipt, error) {
		return CommitReceipt{}, errors.New("unknown")
	}}); !errors.Is(err, ErrCommitUnknown) {
		t.Fatal(err)
	}
	path, lim := a.Directory(), a.Limits()
	a.Close()
	var err error
	a, err = OpenArchive(path, lim)
	if err != nil {
		t.Fatal(err)
	}
	defer a.Close()
	pending, err := a.PendingReplays()
	if err != nil || len(pending) != 1 || pending[0].Spec != s || pending[0].Intent.OutputDigest != digest([]byte("fixed")) {
		t.Fatal(pending, err)
	}
	pending[0].Status = Resolved
	again, err := a.PendingReplays()
	if err != nil || again[0].Status != Retry {
		t.Fatal(again, err)
	}
}

func TestDLQPreparedRestartHonorsSmallerOutputBudget(t *testing.T) {
	a, _, s := repairFixture(t)
	cb := ReplayCallbacks{Decode: func(c context.Context, s ReplaySpec, b []byte, m int64) ([]byte, error) {
		count := 0
		a.syncDir = func(path string) error {
			count++
			if count == 1 {
				return errors.New("prepared fsync crash")
			}
			return directorySync(path)
		}
		return decoded(c, s, b, m)
	}, CommitOnline: func(context.Context, ReplayIntent, []byte) (CommitReceipt, error) {
		t.Fatal("publisher should not run")
		return CommitReceipt{}, nil
	}}
	if _, err := a.Replay(context.Background(), s, replayLimits(), cb); err == nil {
		t.Fatal("injected error ignored")
	}
	path, lim := a.Directory(), a.Limits()
	a.Close()
	var err error
	a, err = OpenArchive(path, lim)
	if err != nil {
		t.Fatal(err)
	}
	defer a.Close()
	budget := replayLimits()
	budget.MaxDecodedBytes = 1
	if _, err := a.Replay(context.Background(), s, budget, cb); !errors.Is(err, ErrBudget) {
		t.Fatal(err)
	}
}
func TestDLQDecodeDeadlineIsAudited(t *testing.T) {
	a, d, s := repairFixture(t)
	l := replayLimits()
	l.Timeout = 10 * time.Millisecond
	r, err := a.Replay(context.Background(), s, l, ReplayCallbacks{Decode: func(ctx context.Context, _ ReplaySpec, _ []byte, _ int64) ([]byte, error) {
		<-ctx.Done()
		return nil, ctx.Err()
	}, CommitOnline: func(context.Context, ReplayIntent, []byte) (CommitReceipt, error) {
		t.Fatal("expired decode published")
		return CommitReceipt{}, nil
	}})
	if !errors.Is(err, context.DeadlineExceeded) || r.Status != Retry || r.Phase != "finished" {
		t.Fatal(r, err)
	}
	if q, _ := a.IsQuarantined(d.Raw); !q {
		t.Fatal("deadline released quarantine")
	}
}

func TestDLQArchiveHasOneBoundedReplayActor(t *testing.T) {
	a, d, s := repairFixture(t)
	ref, err := a.Put(Owner{Chain: "evm:1", Height: 8, Hash: "h8", ProviderID: "rpc-a"}, []byte("other"))
	if err != nil {
		t.Fatal(err)
	}
	second := d
	second.Raw = ref
	if err := a.Quarantine(second); err != nil {
		t.Fatal(err)
	}
	s2 := s
	s2.Raw = ref
	s2.DeadLetterID = DeadLetterID(second)
	incident, err := a.CurrentIncident(s2.DeadLetterID)
	if err != nil {
		t.Fatal(err)
	}
	s2.IncidentID = incident.ID
	enter, release := make(chan struct{}), make(chan struct{})
	done := make(chan error, 1)
	go func() {
		_, err := a.Replay(context.Background(), s, replayLimits(), ReplayCallbacks{Decode: func(c context.Context, s ReplaySpec, b []byte, m int64) ([]byte, error) {
			close(enter)
			<-release
			return decoded(c, s, b, m)
		}, CommitOnline: func(c context.Context, i ReplayIntent, b []byte) (CommitReceipt, error) { return confirmed(c, i) }})
		done <- err
	}()
	<-enter
	_, err = a.Replay(context.Background(), s2, replayLimits(), ReplayCallbacks{Decode: decoded, CommitOnline: func(c context.Context, i ReplayIntent, b []byte) (CommitReceipt, error) { return confirmed(c, i) }})
	close(release)
	if firstErr := <-done; firstErr != nil {
		t.Fatal(firstErr)
	}
	if !errors.Is(err, ErrReplayBusy) {
		t.Fatal("replay callbacks exceeded the archive actor bound", err)
	}
}

// P1: diagnostic text and scope aliases must not create a second owner commit
// while a prior commit for the same raw/namespace is still unknown.
func TestDLQP1PendingRawAliasesCannotRepublish(t *testing.T) {
	for _, changed := range []string{"message", "code", "stage", "scope"} {
		t.Run(changed, func(t *testing.T) {
			a, d, s := repairFixture(t)
			publishes, decodes, confirms := 0, 0, 0
			cb := ReplayCallbacks{Decode: func(c context.Context, s ReplaySpec, b []byte, m int64) ([]byte, error) {
				decodes++
				return decoded(c, s, b, m)
			}, CommitOnline: func(context.Context, ReplayIntent, []byte) (CommitReceipt, error) {
				publishes++
				return CommitReceipt{}, errors.New("lost ack")
			}, ConfirmOnline: func(c context.Context, i ReplayIntent) (CommitReceipt, error) { confirms++; return confirmed(c, i) }}
			if _, err := a.Replay(context.Background(), s, replayLimits(), cb); !errors.Is(err, ErrCommitUnknown) {
				t.Fatal(err)
			}
			alias := d
			switch changed {
			case "message":
				alias.Message = "another diagnostic"
			case "code":
				alias.Code = "OTHER"
			case "stage":
				alias.Stage = "validate"
			case "scope":
				alias.Scope = "pool-other"
			}
			if err := a.Quarantine(alias); err != nil {
				t.Fatal(err)
			}
			path, lim := a.Directory(), a.Limits()
			a.Close()
			var err error
			a, err = OpenArchive(path, lim)
			if err != nil {
				t.Fatal(err)
			}
			defer a.Close()
			aliasSpec := s
			aliasSpec.DeadLetterID = DeadLetterID(alias)
			incident, err := a.CurrentIncident(aliasSpec.DeadLetterID)
			if err != nil {
				t.Fatal(err)
			}
			aliasSpec.IncidentID = incident.ID
			_, err = a.Replay(context.Background(), aliasSpec, replayLimits(), cb)
			if !errors.Is(err, ErrReplayBusy) || publishes != 1 || decodes != 1 || confirms != 0 {
				t.Fatalf("alias bypass: publishes=%d decodes=%d confirms=%d err=%v", publishes, decodes, confirms, err)
			}
			pending, err := a.PendingReplays()
			if err != nil || len(pending) != 1 {
				t.Fatal(pending, err)
			}
			if r, err := a.Replay(context.Background(), pending[0].Spec, replayLimits(), cb); err != nil || r.Status != Resolved || confirms != 1 || publishes != 1 {
				t.Fatal(r, publishes, confirms, err)
			}
		})
	}
}

// P1: an identical failure observed after successful repair is a new incident.
// The old immutable success must remain readable, without resolving the new one.
func TestDLQP1IdenticalFailureReopensAndNewDecoderRuns(t *testing.T) {
	a, d, s := repairFixture(t)
	decodes, publishes := 0, 0
	cb := ReplayCallbacks{Decode: func(c context.Context, s ReplaySpec, b []byte, m int64) ([]byte, error) {
		decodes++
		return decoded(c, s, b, m)
	}, CommitOnline: func(c context.Context, i ReplayIntent, b []byte) (CommitReceipt, error) {
		publishes++
		return confirmed(c, i)
	}}
	first, err := a.Replay(context.Background(), s, replayLimits(), cb)
	if err != nil || first.Status != Resolved {
		t.Fatal(first, err)
	}
	if q, err := a.IsQuarantined(d.Raw); err != nil || q {
		t.Fatal(q, err)
	}
	if err := a.Quarantine(d); err != nil {
		t.Fatal(err)
	}
	if q, err := a.IsQuarantined(d.Raw); err != nil || !q {
		t.Fatal("identical repeated failure stayed resolved", q, err)
	}
	path, lim := a.Directory(), a.Limits()
	a.Close()
	a, err = OpenArchive(path, lim)
	if err != nil {
		t.Fatal(err)
	}
	defer a.Close()
	if q, err := a.IsQuarantined(d.Raw); err != nil || !q {
		t.Fatal("restart lost repeated failure", q, err)
	}
	next := s
	incident, err := a.CurrentIncident(s.DeadLetterID)
	if err != nil {
		t.Fatal(err)
	}
	next.IncidentID = incident.ID
	next.DecoderVersion = "decoder-v3"
	next.WriterID = "new-owner"
	next.Fence++
	next.Generation = "gen-2"
	next.Attempt++
	result, err := a.Replay(context.Background(), next, replayLimits(), cb)
	if err != nil || result.Spec != next || result.Status != Resolved || decodes != 2 || publishes != 2 {
		t.Fatal("old resolution satisfied new repair", result, decodes, publishes, err)
	}
	audit, err := a.ReplayAudit(s.DeadLetterID)
	if err != nil {
		t.Fatal(err)
	}
	found := false
	for _, r := range audit {
		if r.AttemptID == first.AttemptID && r.Status == Resolved {
			found = true
		}
	}
	if !found {
		t.Fatal("original resolution overwritten")
	}
	entries, err := a.DeadLetters()
	if err != nil || len(entries) != 1 || entries[0] != d {
		t.Fatal(entries, err)
	}
}
func TestDLQP1NewReplaySpecNeverReturnsAnotherResolvedSpec(t *testing.T) {
	a, _, s := repairFixture(t)
	cb := ReplayCallbacks{Decode: decoded, CommitOnline: func(c context.Context, i ReplayIntent, b []byte) (CommitReceipt, error) { return confirmed(c, i) }}
	if _, err := a.Replay(context.Background(), s, replayLimits(), cb); err != nil {
		t.Fatal(err)
	}
	next := s
	next.DecoderVersion = "decoder-v3"
	next.Fence++
	next.Generation = "g2"
	next.Attempt++
	result, err := a.Replay(context.Background(), next, replayLimits(), cb)
	if err == nil && result.Spec != next {
		t.Fatal("nonidentical cached success returned", result)
	}
}

func TestDLQP1IncidentCyclesBindResolutionAndPreserveAliases(t *testing.T) {
	a, d, s := repairFixture(t)
	ctx := context.Background()
	cb := ReplayCallbacks{Decode: decoded, CommitOnline: func(c context.Context, i ReplayIntent, b []byte) (CommitReceipt, error) { return confirmed(c, i) }}
	first, err := a.CurrentIncident(s.DeadLetterID)
	if err != nil {
		t.Fatal(err)
	}
	alias := d
	alias.Message = "additional diagnostic"
	joined, err := a.QuarantineIncident(alias)
	if err != nil || joined.ID != first.ID {
		t.Fatal("diagnostic alias created another incident", joined, first, err)
	}
	if _, err := a.Replay(ctx, s, replayLimits(), cb); err != nil {
		t.Fatal(err)
	}
	if q, _ := a.IsQuarantined(d.Raw); q {
		t.Fatal("same-scope aliases were not resolved together")
	}
	next, err := a.QuarantineIncident(d)
	if err != nil || next.ID == first.ID || next.Cycle != first.Cycle+1 {
		t.Fatal(next, first, err)
	}
	// The same diagnostic must join the current cycle on repeated observation.
	repeated, err := a.QuarantineIncident(d)
	if err != nil || repeated.ID != next.ID {
		t.Fatal(repeated, err)
	}
	if _, err := a.Replay(ctx, s, replayLimits(), cb); !errors.Is(err, ErrFailureIncident) {
		t.Fatal("stale cycle accepted", err)
	}
	// Even a valid receipt for a different scope cannot discharge this scope.
	other := d
	other.Scope = "pool-other"
	scopeIncident, err := a.QuarantineIncident(other)
	if err != nil {
		t.Fatal(err)
	}
	scoped := s
	scoped.DeadLetterID = DeadLetterID(other)
	scoped.IncidentID = scopeIncident.ID
	if _, err := a.Replay(ctx, scoped, replayLimits(), cb); err != nil {
		t.Fatal(err)
	}
	if q, _ := a.IsQuarantined(d.Raw); !q {
		t.Fatal("another scope's resolution cleared new failure")
	}
	nextSpec := s
	nextSpec.IncidentID = next.ID
	nextSpec.DecoderVersion = "decoder-v3"
	nextSpec.Fence++
	nextSpec.Generation = "g2"
	nextSpec.Attempt++
	if _, err := a.Replay(ctx, nextSpec, replayLimits(), cb); err != nil {
		t.Fatal(err)
	}
	if q, _ := a.IsQuarantined(d.Raw); q {
		t.Fatal("current scope not resolved")
	}
	path, lim := a.Directory(), a.Limits()
	a.Close()
	a, err = OpenArchive(path, lim)
	if err != nil {
		t.Fatal(err)
	}
	defer a.Close()
	cycles, err := a.FailureIncidents(s.DeadLetterID)
	if err != nil || len(cycles) != 2 || cycles[0].ID != first.ID || cycles[1].ID != next.ID {
		t.Fatal(cycles, err)
	}
	if q, _ := a.IsQuarantined(d.Raw); q {
		t.Fatal("restart lost current-cycle resolution")
	}
}
func TestDLQP1AuthorityCheckedBeforeCachedResolution(t *testing.T) {
	a, _, s := repairFixture(t)
	cb := ReplayCallbacks{Decode: decoded, CommitOnline: func(c context.Context, i ReplayIntent, b []byte) (CommitReceipt, error) { return confirmed(c, i) }}
	if _, err := a.Replay(context.Background(), s, replayLimits(), cb); err != nil {
		t.Fatal(err)
	}
	authority := s.authority()
	authority.Fence++
	authority.WriterID = "takeover"
	authority.Generation = "g2"
	if err := a.AdvanceReplayFence(authority); err != nil {
		t.Fatal(err)
	}
	if _, err := a.Replay(context.Background(), s, replayLimits(), cb); !errors.Is(err, ErrReplayFence) {
		t.Fatal("fenced cached result accepted", err)
	}
}

func TestDLQP1NewIncidentSyncFailureRecoversQuarantine(t *testing.T) {
	a, d, s := repairFixture(t)
	cb := ReplayCallbacks{Decode: decoded, CommitOnline: func(c context.Context, i ReplayIntent, b []byte) (CommitReceipt, error) { return confirmed(c, i) }}
	if _, err := a.Replay(context.Background(), s, replayLimits(), cb); err != nil {
		t.Fatal(err)
	}
	injected := errors.New("incident directory sync failure")
	calls := 0
	a.syncDir = func(path string) error {
		calls++
		if calls == 2 {
			return injected
		}
		return directorySync(path)
	}
	if err := a.Quarantine(d); !errors.Is(err, injected) {
		t.Fatal(err)
	}
	if _, err := a.IsQuarantined(d.Raw); err == nil {
		t.Fatal("uncertain incident reused a resolved handle")
	}
	path, lim := a.Directory(), a.Limits()
	a.Close()
	var err error
	a, err = OpenArchive(path, lim)
	if err != nil {
		t.Fatal(err)
	}
	defer a.Close()
	now, err := a.CurrentIncident(s.DeadLetterID)
	if err != nil || now.ID == s.IncidentID {
		t.Fatal(now, err)
	}
	if q, err := a.IsQuarantined(d.Raw); err != nil || !q {
		t.Fatal(q, err)
	}
	if _, err := a.Replay(context.Background(), s, replayLimits(), cb); !errors.Is(err, ErrFailureIncident) {
		t.Fatal(err)
	}
}
func TestDLQP1WrongIncidentReceiptDoesNotResolveCurrentFailure(t *testing.T) {
	a, d, s := repairFixture(t)
	var oldIntent ReplayIntent
	cb := ReplayCallbacks{Decode: decoded, CommitOnline: func(c context.Context, i ReplayIntent, b []byte) (CommitReceipt, error) {
		oldIntent = i
		return confirmed(c, i)
	}}
	if _, err := a.Replay(context.Background(), s, replayLimits(), cb); err != nil {
		t.Fatal(err)
	}
	i, err := a.QuarantineIncident(d)
	if err != nil {
		t.Fatal(err)
	}
	next := s
	next.IncidentID = i.ID
	next.DecoderVersion = "decoder-v3"
	next.Fence++
	next.Generation = "g2"
	cb.CommitOnline = func(c context.Context, newIntent ReplayIntent, b []byte) (CommitReceipt, error) {
		receipt, _ := confirmed(c, newIntent)
		receipt.IncidentID = oldIntent.Spec.IncidentID
		return receipt, nil
	}
	if result, err := a.Replay(context.Background(), next, replayLimits(), cb); !errors.Is(err, ErrCommitUnknown) || result.Status != Retry {
		t.Fatal(result, err)
	}
	if q, _ := a.IsQuarantined(d.Raw); !q {
		t.Fatal("old incident receipt discharged new failure")
	}
}
func TestDLQP1HistoryPendingIsIsolatedFromOnlineCommitDomain(t *testing.T) {
	a, d, s := repairFixture(t)
	if _, err := a.Replay(context.Background(), s, replayLimits(), ReplayCallbacks{Decode: decoded, CommitOnline: func(context.Context, ReplayIntent, []byte) (CommitReceipt, error) {
		return CommitReceipt{}, errors.New("unknown online commit")
	}}); !errors.Is(err, ErrCommitUnknown) {
		t.Fatal(err)
	}
	h := s
	h.Namespace = NamespaceHistory
	if result, err := a.Replay(context.Background(), h, replayLimits(), ReplayCallbacks{Decode: decoded, CommitHistory: func(c context.Context, i ReplayIntent, b []byte) (CommitReceipt, error) { return confirmed(c, i) }}); err != nil || result.Status != Resolved {
		t.Fatal(result, err)
	}
	if q, _ := a.IsQuarantined(d.Raw); !q {
		t.Fatal("history confirmation resolved online pending")
	}
	pending, err := a.PendingReplays()
	if err != nil || len(pending) != 1 || pending[0].Spec != s {
		t.Fatal(pending, err)
	}
}

func legacyFailureFixture(t *testing.T) (*Archive, DeadLetter, ReplaySpec) {
	t.Helper()
	a, err := OpenArchive(t.TempDir(), ArchiveLimits{MaxBytes: 4 << 20, MaxRecords: 1000, MaxRecordBytes: 4096})
	if err != nil {
		t.Fatal(err)
	}
	ref, err := a.Put(Owner{Chain: "evm:1", Height: 7, Hash: "h7", ProviderID: "rpc"}, []byte("raw"))
	if err != nil {
		t.Fatal(err)
	}
	d := DeadLetter{Raw: ref, Stage: "decode", Code: "UNKNOWN", Scope: "pool", Message: "legacy diagnostic"}
	// Model pre-incident data: only the original durable diagnostic, no incident event.
	a.mu.Lock()
	err = a.quarantine(d)
	a.mu.Unlock()
	if err != nil {
		t.Fatal(err)
	}
	path, lim := a.Directory(), a.Limits()
	a.Close()
	a, err = OpenArchive(path, lim)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { a.Close() })
	s := ReplaySpec{DeadLetterID: DeadLetterID(d), Raw: ref, DecoderVersion: "legacy-v2", Namespace: NamespaceOnline, WriterID: "old-owner", Fence: 2, Generation: "g1"}
	return a, d, s
}

// Write the original format without the new admission checks, modelling the
// competing unknown records already persisted by the old alias bug.
func appendLegacyUnknown(t *testing.T, a *Archive, s ReplaySpec) {
	t.Helper()
	r := ReplayRecord{AttemptID: s.ID(), Spec: s, Intent: ReplayIntent{AttemptID: s.ID(), Spec: s}, Status: Retry}
	for _, phase := range []string{"started", "prepared", "publishing", "unknown"} {
		r.Phase = phase
		r.At = time.Now().UTC()
		r.Decoded = nil
		if phase == "prepared" {
			r.Decoded = []byte("fixed")
			r.Intent.OutputDigest = digest(r.Decoded)
			r.Intent.OutputBytes = int64(len(r.Decoded))
		}
		copy := cloneReplay(r)
		e := archiveEvent{Sequence: uint64(len(a.events)) + 1, Kind: "replay", Replay: &copy}
		if len(a.events) > 0 {
			e.Previous = jsonID(a.events[len(a.events)-1])
		}
		if err := a.write(jsonID(e)+".event", e); err != nil {
			t.Fatal(err)
		}
		a.applyEvent(e)
		a.events = append(a.events, e)
	}
}
func TestDLQP1LegacyUnknownConfirmsOriginalIdentityAndOpensNewCycle(t *testing.T) {
	a, d, s := legacyFailureFixture(t)
	appendLegacyUnknown(t, a, s)
	path, lim := a.Directory(), a.Limits()
	a.Close()
	var err error
	a, err = OpenArchive(path, lim)
	if err != nil {
		t.Fatal(err)
	}
	defer a.Close()
	pending, err := a.PendingReplays()
	if err != nil || len(pending) != 1 || pending[0].Spec != s || pending[0].AttemptID != s.ID() {
		t.Fatal(pending, err)
	}
	if r, err := a.Replay(context.Background(), s, replayLimits(), ReplayCallbacks{ConfirmOnline: confirmed}); err != nil || r.Status != Resolved {
		t.Fatal(r, err)
	}
	if err := a.Quarantine(d); err != nil {
		t.Fatal(err)
	}
	current, err := a.CurrentIncident(s.DeadLetterID)
	if err != nil || current.Cycle != 1 {
		t.Fatal(current, err)
	}
	a.Close()
	a, err = OpenArchive(path, lim)
	if err != nil {
		t.Fatal(err)
	}
	defer a.Close()
	cycles, err := a.FailureIncidents(s.DeadLetterID)
	if err != nil || len(cycles) != 2 || cycles[0].Cycle != 0 || cycles[1].ID != current.ID {
		t.Fatal(cycles, err)
	}
	if q, _ := a.IsQuarantined(d.Raw); !q {
		t.Fatal("legacy resolution cleared a new failure")
	}
}
func TestDLQP1LegacyCompetingUnknownsCanOnlyBeConfirmed(t *testing.T) {
	a, d, s := legacyFailureFixture(t)
	alias := d
	alias.Message = "second old diagnostic"
	a.mu.Lock()
	err := a.quarantine(alias)
	a.mu.Unlock()
	if err != nil {
		t.Fatal(err)
	}
	appendLegacyUnknown(t, a, s)
	s2 := s
	s2.DeadLetterID = DeadLetterID(alias)
	appendLegacyUnknown(t, a, s2)
	path, lim := a.Directory(), a.Limits()
	a.Close()
	a, err = OpenArchive(path, lim)
	if err != nil {
		t.Fatal("old alias bug evidence cannot be reconciled", err)
	}
	defer a.Close()
	pending, err := a.PendingReplays()
	if err != nil || len(pending) != 2 {
		t.Fatal(pending, err)
	}
	i, err := a.CurrentIncident(s.DeadLetterID)
	if err != nil {
		t.Fatal(err)
	}
	next := s
	next.IncidentID = i.ID
	next.Attempt++
	if _, err := a.Replay(context.Background(), next, replayLimits(), ReplayCallbacks{Decode: decoded, CommitOnline: func(context.Context, ReplayIntent, []byte) (CommitReceipt, error) {
		t.Fatal("published over legacy unknowns")
		return CommitReceipt{}, nil
	}}); !errors.Is(err, ErrReplayBusy) {
		t.Fatal(err)
	}
	for index, r := range pending {
		if _, err := a.Replay(context.Background(), r.Spec, replayLimits(), ReplayCallbacks{ConfirmOnline: confirmed}); err != nil {
			t.Fatal("could not settle original unknown", err)
		}
		if index == 0 {
			if q, _ := a.IsQuarantined(d.Raw); !q {
				t.Fatal("one confirmation hid the other unknown")
			}
		}
	}
	if q, _ := a.IsQuarantined(d.Raw); q {
		t.Fatal("all owner confirmations did not settle legacy cycle")
	}
}

func resolvedFailureWithArchive(t *testing.T) (*Archive, DeadLetter, ReplaySpec, *LocalArchiveSink) {
	t.Helper()
	a, d, s := repairFixture(t)
	sink, err := OpenLocalArchiveSink(t.TempDir())
	if err != nil {
		t.Fatal(err)
	}
	if _, err := a.ArchiveRaw(context.Background(), d.Raw, ArchiveMetadata{Namespace: NamespaceOnline, DecoderVersion: "v2", CanonicalProof: "journal"}, sink); err != nil {
		t.Fatal(err)
	}
	if _, err := a.Replay(context.Background(), s, replayLimits(), ReplayCallbacks{Decode: decoded, CommitOnline: func(c context.Context, i ReplayIntent, b []byte) (CommitReceipt, error) { return confirmed(c, i) }}); err != nil {
		t.Fatal(err)
	}
	return a, d, s, sink
}
func TestDLQP1FailureBeforeIncidentAppendCannotRestoreResolved(t *testing.T) {
	for _, window := range []string{"barrier-fsync", "diagnostic-fsync", "after-diagnostic-before-incident"} {
		t.Run(window, func(t *testing.T) {
			a, d, s, sink := resolvedFailureWithArchive(t)
			alias := d
			alias.Message = "new failure diagnostic after prior repair"
			if window == "barrier-fsync" {
				a.syncDir = func(string) error { return errors.New("barrier sync failure") }
			} else if window == "diagnostic-fsync" {
				calls := 0
				a.syncDir = func(path string) error {
					calls++
					if calls == 2 {
						return errors.New("diagnostic sync failure")
					}
					return directorySync(path)
				}
			} else {
				a.limits.MaxRecords = a.records + 1
			}
			if err := a.Quarantine(alias); err == nil {
				t.Fatal("injected failure accepted")
			}
			if _, _, err := a.Usage(); err == nil {
				t.Error("failed failure observation left usable handle")
			}
			path, lim := a.Directory(), a.Limits()
			a.Close()
			var err error
			a, err = OpenArchive(path, lim)
			if err != nil {
				t.Fatal(err)
			}
			defer a.Close()
			if q, err := a.IsQuarantined(d.Raw); err != nil || !q {
				t.Error("reopen restored old resolved state", q, err)
			}
			if _, err := a.Replay(context.Background(), s, replayLimits(), ReplayCallbacks{}); err == nil {
				t.Error("old cached success escaped")
			}
			if err := a.ReclaimRaw(context.Background(), d.Raw, sink); err == nil {
				t.Error("raw with unrecorded failure was reclaimed")
			}
			if _, err := os.Stat(filepath.Join(path, d.Raw.ID+".raw")); err != nil {
				t.Fatal("raw lost", err)
			}
		})
	}
}
func archiveEventEncodedBytes(t *testing.T, a *Archive, e archiveEvent) int64 {
	t.Helper()
	e.Sequence = uint64(len(a.events)) + 1
	if len(a.events) > 0 {
		e.Previous = jsonID(a.events[len(a.events)-1])
	}
	payload, err := json.Marshal(e)
	if err != nil {
		t.Fatal(err)
	}
	encoded, err := json.Marshal(envelope{Digest: digest(payload), Payload: payload})
	if err != nil {
		t.Fatal(err)
	}
	return int64(len(encoded))
}
func TestDLQP1PositiveReserveIncidentBudgetCannotPermitSmallerGC(t *testing.T) {
	a, d, s, sink := resolvedFailureWithArchive(t)
	current, err := a.CurrentIncident(s.DeadLetterID)
	if err != nil {
		t.Fatal(err)
	}
	next := FailureIncident{ID: failureID(d.Raw, d.Scope, current.Cycle+1), Raw: d.Raw, Scope: d.Scope, Cycle: current.Cycle + 1, OpenedBy: s.DeadLetterID, At: time.Now().UTC()}
	incidentBytes := archiveEventEncodedBytes(t, a, archiveEvent{Kind: "incident", Incident: &next})
	reclaimBytes := archiveEventEncodedBytes(t, a, archiveEvent{Kind: "reclaim", Reclaimed: &d.Raw})
	if incidentBytes <= reclaimBytes {
		t.Fatal("fixture does not reproduce incident/reclaim size gap", incidentBytes, reclaimBytes)
	}
	spare := (incidentBytes + reclaimBytes) / 2
	a.limits.MaxBytes = a.used + spare
	a.limits.ReservedBytes = 1024
	if err := a.Quarantine(d); !errors.Is(err, ErrBudget) {
		t.Fatal("incident should exceed available bytes", spare, incidentBytes, reclaimBytes, err)
	}
	if _, _, err := a.Usage(); err == nil {
		t.Error("capacity failure left usable handle")
	}
	if q, err := a.IsQuarantined(d.Raw); err == nil && !q {
		t.Error("capacity failure exposed old success")
	}
	if err := a.ReclaimRaw(context.Background(), d.Raw, sink); err == nil {
		t.Error("smaller tombstone escaped capacity failure")
	}
	path, lim := a.Directory(), a.Limits()
	a.Close()
	a, err = OpenArchive(path, lim)
	if err != nil {
		t.Fatal(err)
	}
	defer a.Close()
	if q, err := a.IsQuarantined(d.Raw); err != nil || !q {
		t.Fatal("reserve reopen lost quarantine", q, err)
	}
	if _, err := a.Replay(context.Background(), s, replayLimits(), ReplayCallbacks{}); err == nil {
		t.Error("reserve reopen reused old success")
	}
	if err := a.ReclaimRaw(context.Background(), d.Raw, sink); err == nil {
		t.Error("reserve reopen permitted GC")
	}
}

func TestDLQP1UncertainFirstMarkerCannotDisappearOnReopen(t *testing.T) {
	a, d, s, sink := resolvedFailureWithArchive(t)
	a.syncDir = func(string) error { return errors.New("first isolation sync failed") }
	if err := a.Quarantine(d); err == nil {
		t.Fatal("failure accepted")
	}
	path, lim := a.Directory(), a.Limits()
	a.Close()
	// Model loss of a directory entry whose sync failed. The already-durable
	// session guard must still prevent old Resolved from becoming authority.
	entries, err := os.ReadDir(path)
	if err != nil {
		t.Fatal(err)
	}
	for _, e := range entries {
		if strings.HasPrefix(e.Name(), failureBarrierPrefix) {
			if err := os.Remove(filepath.Join(path, e.Name())); err != nil {
				t.Fatal(err)
			}
		}
	}
	a, err = OpenArchive(path, lim)
	if err != nil {
		t.Fatal(err)
	}
	defer a.Close()
	if !a.FailureRecoveryRequired() {
		t.Fatal("lost marker silently treated as clean restart")
	}
	if q, err := a.IsQuarantined(d.Raw); err != nil || !q {
		t.Fatal(q, err)
	}
	if _, _, err := a.Usage(); !errors.Is(err, ErrFailureRecoveryRequired) {
		t.Fatal(err)
	}
	if _, err := a.Replay(context.Background(), s, replayLimits(), ReplayCallbacks{}); !errors.Is(err, ErrFailureRecoveryRequired) {
		t.Fatal(err)
	}
	if err := a.ReclaimRaw(context.Background(), d.Raw, sink); err == nil {
		t.Fatal("uncertain failure allowed GC")
	}
	a.Close()
	a, err = OpenArchive(path, lim)
	if err != nil {
		t.Fatal(err)
	}
	defer a.Close()
	if !a.FailureRecoveryRequired() {
		t.Fatal("another reopen auto-cleared recovery hold")
	}
}
func TestDLQP1DurableBarrierCanFinishAfterCapacityIsRestored(t *testing.T) {
	a, d, s, _ := resolvedFailureWithArchive(t)
	a.limits.MaxRecords = a.records
	if err := a.Quarantine(d); !errors.Is(err, ErrBudget) {
		t.Fatal(err)
	}
	path, lim := a.Directory(), a.Limits()
	lim.MaxRecords += 10
	a.Close()
	var err error
	a, err = OpenArchive(path, lim)
	if err != nil {
		t.Fatal(err)
	}
	defer a.Close()
	if a.FailureRecoveryRequired() {
		t.Fatal("durable scoped barrier was mistaken for unknown scope")
	}
	if _, err := a.CurrentIncident(s.DeadLetterID); !errors.Is(err, ErrFailureRecoveryRequired) {
		t.Fatal("unfinished barrier exposed old incident", err)
	}
	current, err := a.QuarantineIncident(d)
	if err != nil || current.ID == s.IncidentID {
		t.Fatal(current, err)
	}
	next := s
	next.IncidentID = current.ID
	next.DecoderVersion = "v3"
	next.Fence++
	next.Generation = "g2"
	if result, err := a.Replay(context.Background(), next, replayLimits(), ReplayCallbacks{Decode: decoded, CommitOnline: func(c context.Context, i ReplayIntent, b []byte) (CommitReceipt, error) { return confirmed(c, i) }}); err != nil || result.Status != Resolved {
		t.Fatal(result, err)
	}
	if q, err := a.IsQuarantined(d.Raw); err != nil || q {
		t.Fatal(q, err)
	}
}
