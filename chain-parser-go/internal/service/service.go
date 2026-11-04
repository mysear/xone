package service

import (
	"context"
	"crypto/subtle"
	"encoding/json"
	"errors"
	"fmt"
	"net/http"
	"os"
	"path/filepath"
	"sort"
	"strconv"
	"strings"
	"sync/atomic"
	"time"
	"xone/chain-parser-go/internal/bootstrap"
	"xone/chain-parser-go/internal/chain/evm"
	"xone/chain-parser-go/internal/checkpoint"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/control"
	"xone/chain-parser-go/internal/domain"
	"xone/chain-parser-go/internal/journal"
	"xone/chain-parser-go/internal/observability"
	"xone/chain-parser-go/internal/reducer"
	"xone/chain-parser-go/internal/rpc"
)

// SnapshotSource supplies a current baseline; normal services use their RPC adapters.
type SnapshotSource func(context.Context, config.Chain) (domain.Snapshot, error)

type Service struct {
	source    SnapshotSource
	control   *control.Postgres
	sessions  map[string]*writerSession
	cfg       *config.Snapshot
	stores    map[string]*reducer.Store
	adapters  map[string]*evm.Adapter
	log       *observability.Logger
	started   atomic.Bool
	lastError atomic.Uint64
}

func New(cfg *config.Snapshot) (*Service, error) { return NewWithSource(cfg, nil) }

func NewWithSource(cfg *config.Snapshot, source SnapshotSource) (*Service, error) {
	if e := cfg.Config.Validate("chain-parser-go"); e != nil {
		return nil, e
	}
	if cfg.Config.Service.Role != "online" {
		return nil, errors.New("ANALYTICS_REQUIRES_JOB_ENTRY")
	}
	s := &Service{cfg: cfg, source: source, stores: map[string]*reducer.Store{}, adapters: map[string]*evm.Adapter{}, log: observability.New(os.Stdout, "chain-parser-go", cfg.Config.Service.Role, cfg.Release, cfg.PublicHash, cfg.Config.Logging)}
	for _, chain := range cfg.Config.Chains {
		store, e := reducer.OpenWithLogger(filepath.Join(cfg.Resolve(cfg.Config.State.Path), strings.ReplaceAll(chain.Key, ":", "-")), chain.Owner, chain.Assignment, chain.ReorgWindow, observability.PebbleLogger{Log: s.log})
		if e != nil {
			s.Close()
			return nil, e
		}
		s.stores[chain.Key] = store
		if source == nil {
			s.adapters[chain.Key] = &evm.Adapter{Client: rpc.New(chain.RPCURLs, time.Duration(cfg.Config.Journal.TimeoutMS)*time.Millisecond, cfg.Config.Journal.MaxBytes), Chain: chain}
		}
	}
	return s, nil
}
func (s *Service) Start(ctx context.Context) error {
	if s.cfg.Config.Journal.Transport == "kafka" {
		pg, e := control.Open(ctx, s.cfg.ControlDSN)
		if e != nil {
			return e
		}
		s.control = pg
		s.sessions = map[string]*writerSession{}
	}
	for _, chain := range s.cfg.Config.Chains {
		if s.control != nil {
			session, e := s.openSession(ctx, chain)
			if e != nil {
				if e.Error() == "LEASE_NOT_OWNED" {
					continue
				}
				return e
			}
			s.sessions[chain.Key] = session
		}

		if a := s.adapters[chain.Key]; a != nil {
			if e := a.Client.VerifyEVM(ctx, chain.Key, chain.Genesis); e != nil {
				return e
			}
		}
		if e := s.reanchor(ctx, chain, false); e != nil {
			return e
		}
	}
	s.started.Store(true)
	return nil
}
func (s *Service) reanchor(ctx context.Context, chain config.Chain, fork bool) error {
	store := s.stores[chain.Key]
	old := store.DurableRef()
	revoked := store.ReorgRevoked()
	if w := s.sessions[chain.Key]; w != nil {
		revoked = revoked || w.reorgUnresolved == "ONLINE_SCOPE_REVOKED"
	}
	var snapshot domain.Snapshot
	if s.source != nil {
		var e error
		snapshot, e = s.source(ctx, chain)
		if e != nil {
			return e
		}
		if e = snapshot.Validate(chain.Owner, chain.Assignment); e != nil {
			return e
		}
		if snapshot.Reference.Block.ChainKey != chain.Key {
			return errors.New("BASELINE_WRONG_CHAIN")
		}
	} else {
		a := s.adapters[chain.Key]
		if e := a.Client.VerifyEVM(ctx, chain.Key, chain.Genesis); e != nil {
			return e
		}
		block, pools, e := a.ReadState(ctx, chain.Confirmation, false)
		if e != nil {
			return e
		}
		unresolved := store.ReorgPending()
		if w := s.sessions[chain.Key]; w != nil {
			unresolved = unresolved || w.reorgUnresolved != ""
		}
		if unresolved {
			if e := a.ConfirmBlock(ctx, block); e != nil {
				return e
			}
			if revoked {
				previous := old
				if w := s.sessions[chain.Key]; w != nil && w.floor.JournalSeq > previous.JournalSeq {
					previous = w.floor
				}
				if previous.Block.Number > block.Number {
					fork = true
				} else {
					anchor, e := a.Header(ctx, fmt.Sprintf("0x%x", previous.Block.Number))
					if e != nil {
						return e
					}
					if e = a.ConfirmBlock(ctx, anchor); e != nil {
						return e
					}
					fork = anchor.Hash != previous.Block.Hash
				}
			}
		}
		versions := []string{}
		seen := map[string]bool{}
		for _, p := range pools {
			if !seen[p.AdapterVersion] {
				versions = append(versions, p.AdapterVersion)
				seen[p.AdapterVersion] = true
			}
		}
		sort.Strings(versions)
		adapterHash, _ := domain.Hash(versions)
		scope, e := domain.ScopeHash(pools)
		if e != nil {
			return e
		}
		snapshot = domain.Snapshot{Reference: domain.SnapshotRef{Block: block, SchemaVersion: 1, CanonicalEpoch: 1, AdapterSetHash: adapterHash, ScopeHash: scope, OwnerService: chain.Owner, OwnerAssignmentVersion: chain.Assignment}, Pools: pools, Source: "CURRENT_CHAIN_STATE", HistoryGaps: []domain.Gap{}}
		if block.Number > old.Block.Number+1 {
			snapshot.HistoryGaps = append(snapshot.HistoryGaps, domain.Gap{From: old.Block.Number + 1, To: block.Number - 1, Reason: "HEAD_FIRST_HISTORY_DECOUPLED"})
		}
	}
	snapshot.Reference.OnlineGeneration = old.OnlineGeneration + 1
	snapshot.Reference.WriterEpoch = old.WriterEpoch + 1
	snapshot.Reference.JournalSeq = old.JournalSeq + 1
	if old.CanonicalEpoch > snapshot.Reference.CanonicalEpoch {
		snapshot.Reference.CanonicalEpoch = old.CanonicalEpoch
	}
	if fork && s.control == nil {
		snapshot.Reference.CanonicalEpoch++
	}
	if e := bootstrap.CheckFresh(snapshot, uint64(time.Now().UnixMilli()), s.cfg.Config.Startup.MaxAge); e != nil {
		return e
	}
	var applyError error
	if w := s.sessions[chain.Key]; w != nil {
		if e := w.baseline(ctx, &snapshot, fork); e != nil {
			return e
		}
		applyError = w.publish(ctx, journal.Unit{Kind: "ONLINE_BASELINE_COMMIT", Snapshot: snapshot})
	} else {
		applyError = store.InstallBaseline(snapshot)
	}
	if e := applyError; e != nil {
		return e
	}
	s.log.Event("info", "baseline_committed", map[string]any{"chain_key": chain.Key, "online_generation": snapshot.Reference.OnlineGeneration, "journal_seq": snapshot.Reference.JournalSeq, "history_gap_count": len(snapshot.HistoryGaps)})
	return nil
}
func (s *Service) Tick(ctx context.Context) {
	if s.control != nil {
		for _, ch := range s.cfg.Config.Chains {
			w := s.sessions[ch.Key]
			if w == nil {
				session, e := s.openSession(ctx, ch)
				if e != nil {
					continue
				}
				s.sessions[ch.Key] = session
				if e = s.reanchor(ctx, ch, false); e != nil {
					session.close()
					delete(s.sessions, ch.Key)
				}
				continue
			}
			if e := w.heartbeat(ctx); e != nil {
				w.close()
				delete(s.sessions, ch.Key)
				s.log.Event("warn", "writer_lease_lost", map[string]any{"chain_key": ch.Key, "error_code": e.Error()})
			}
		}
	}
	if s.source != nil {
		if s.control != nil {
			for _, chain := range s.cfg.Config.Chains {
				if s.sessions[chain.Key] == nil {
					w, e := s.openSession(ctx, chain)
					if e == nil {
						s.sessions[chain.Key] = w
						if e = s.reanchor(ctx, chain, false); e != nil {
							w.close()
							delete(s.sessions, chain.Key)
						}
					}
				}
			}
		}
		return
	}
	for _, chain := range s.cfg.Config.Chains {
		if s.control != nil && s.sessions[chain.Key] == nil {
			continue
		}
		if e := s.syncChain(ctx, chain); e != nil {
			s.lastError.Add(1)
			// A failed source read must revoke other replicas as well. Only
			// later independent fork confirmation may advance canonical epoch.
			if current, ready := s.stores[chain.Key].Current(); ready {
				if revokeError := s.revokeScope(ctx, chain, current); revokeError != nil {
					s.log.Event("error", "scope_revocation_failed", map[string]any{"chain_key": chain.Key, "error_code": revokeError.Error()})
				}
			}
			s.stores[chain.Key].Suspend()
			if w := s.sessions[chain.Key]; w != nil {
				w.close()
				delete(s.sessions, chain.Key)
			}
			s.log.Event("error", "chain_sync_failed", map[string]any{"chain_key": chain.Key, "error_code": e.Error()})
		}
	}
}
func (s *Service) syncChain(ctx context.Context, ch config.Chain) error {
	store, a := s.stores[ch.Key], s.adapters[ch.Key]
	current, ready := store.Current()
	if !ready {
		return s.reanchor(ctx, ch, false)
	}
	head, e := a.Header(ctx, ch.Confirmation)
	if e != nil {
		return e
	}
	// Live updates obey the same clock admission as HEAD_FIRST recovery.
	candidate := current
	candidate.Reference.Block = head
	if e := bootstrap.CheckFresh(candidate, uint64(time.Now().UnixMilli()), s.cfg.Config.Startup.MaxAge); e != nil {
		return e
	}
	if head.Number < current.Reference.Block.Number || head.Number == current.Reference.Block.Number && head.Hash != current.Reference.Block.Hash {
		return s.recoverFork(ctx, ch, current, a)
	}
	if head.Number == current.Reference.Block.Number {
		return nil
	}
	if head.Number-current.Reference.Block.Number > s.cfg.Config.Startup.MaxTail {
		// Reanchor suspends local state before reading the new bank. Publish
		// the old scope's barrier while its admitted snapshot is still available,
		// so an admission failure cannot leave remote replicas quoting it.
		if e := s.revokeScope(ctx, ch, current); e != nil {
			return e
		}
		store.Suspend()
		return s.reanchor(ctx, ch, false)
	}
	for h := current.Reference.Block.Number + 1; h <= head.Number; h++ {
		block, pools, swaps, e := a.ReadComplete(ctx, fmt.Sprintf("0x%x", h))
		if e != nil {
			// A fork can occur while an EIP-1898 read is in flight. Confirm
			// against the currently applied block before treating it as a
			// transient source failure and reanchoring.
			if anchor, check := a.Header(ctx, fmt.Sprintf("0x%x", current.Reference.Block.Number)); check == nil && anchor.Hash != current.Reference.Block.Hash {
				return s.recoverFork(ctx, ch, current, a)
			}
			return e
		}
		if block.ParentHash != current.Reference.Block.Hash {
			return s.recoverFork(ctx, ch, current, a)
		}
		next := current
		next.Reference.Block = block
		next.Reference.JournalSeq++
		next.Pools = pools
		if e := bootstrap.CheckFresh(next, uint64(time.Now().UnixMilli()), s.cfg.Config.Startup.MaxAge); e != nil {
			return e
		}
		unit := journal.Unit{Kind: "BLOCK_COMMIT", Snapshot: next}
		if hasV2Scope(next.Pools) {
			unit.NormalizedFacts = &domain.FactBatch{SchemaVersion: 1, DecoderVersion: "uniswap-v2/1", Blocks: []domain.FactBlock{{Block: block, Swaps: swaps}}}
		}
		var applyError error
		if w := s.sessions[ch.Key]; w != nil {
			applyError = w.publish(ctx, unit)
		} else {
			applyError = store.ApplyLocal(unit)
		}
		if e = applyError; e != nil {
			return e
		}
		current = next
	}
	return nil
}
func (s *Service) Handler() http.Handler {
	mux := http.NewServeMux()
	write := func(w http.ResponseWriter, status int, v any) {
		w.Header().Set("Content-Type", "application/json")
		w.WriteHeader(status)
		json.NewEncoder(w).Encode(v)
	}
	mux.HandleFunc("GET /health/live", func(w http.ResponseWriter, r *http.Request) { write(w, 200, map[string]bool{"live": true}) })
	mux.HandleFunc("GET /health/startup", func(w http.ResponseWriter, r *http.Request) {
		code := 200
		if !s.started.Load() {
			code = 503
		}
		write(w, code, map[string]bool{"started": s.started.Load()})
	})
	mux.HandleFunc("GET /health/ready", func(w http.ResponseWriter, r *http.Request) {
		ready := false
		for _, store := range s.stores {
			if cur, ok := store.Current(); ok && bootstrap.CheckFresh(cur, uint64(time.Now().UnixMilli()), s.cfg.Config.Startup.MaxAge) == nil {
				ready = true
			}
		}
		code := 200
		if !ready {
			code = 503
		}
		write(w, code, map[string]any{"ready": ready, "scope": "PARSER_STATE_SCOPE"})
	})
	mux.HandleFunc("GET /metrics", func(w http.ResponseWriter, r *http.Request) {
		fmt.Fprintf(w, "xone_log_dropped_total %d\nxone_log_write_errors_total %d\nxone_chain_sync_errors_total %d\n", s.log.Dropped.Load(), s.log.WriteErrors.Load(), s.lastError.Load())
	})
	mux.HandleFunc("GET /internal/config", func(w http.ResponseWriter, r *http.Request) {
		write(w, 200, map[string]any{"release": s.cfg.Release, "public_hash": s.cfg.PublicHash, "economic_hash": s.cfg.EconomicHash})
	})
	mux.HandleFunc("GET /internal/chains/{chain}/bootstrap", func(w http.ResponseWriter, r *http.Request) {
		store := s.stores[r.PathValue("chain")]
		if store == nil {
			write(w, 404, map[string]string{"error": "UNKNOWN_CHAIN"})
			return
		}
		boot, e := store.Bootstrap()
		if e != nil {
			write(w, 503, map[string]string{"error": "BOOTSTRAP_NOT_READY"})
			return
		}
		raw, e := journal.EncodeBootstrap(boot)
		if e != nil {
			write(w, 500, map[string]string{"error": "BOOTSTRAP_ENCODE"})
			return
		}
		w.Header().Set("Content-Type", "application/x-protobuf")
		w.WriteHeader(200)
		_, _ = w.Write(raw)
	})
	mux.HandleFunc("GET /internal/chains/{chain}/baseline", func(w http.ResponseWriter, r *http.Request) {
		store := s.stores[r.PathValue("chain")]
		if store == nil {
			write(w, 404, map[string]string{"error": "UNKNOWN_CHAIN"})
			return
		}
		snapshot, ok := store.Current()
		if !ok {
			write(w, 503, map[string]string{"error": "CHAIN_NOT_READY"})
			return
		}
		write(w, 200, snapshot)
	})
	mux.HandleFunc("GET /internal/chains/{chain}/status", func(w http.ResponseWriter, r *http.Request) {
		store := s.stores[r.PathValue("chain")]
		if store == nil {
			write(w, 404, map[string]string{"error": "UNKNOWN_CHAIN"})
			return
		}
		snapshot, ready := store.Current()
		write(w, 200, map[string]any{"ready": ready, "snapshot_ref": snapshot.Reference, "history_gaps": snapshot.HistoryGaps, "readiness_scope": "PARSER_STATE_SCOPE"})
	})
	mux.HandleFunc("GET /internal/chains/{chain}/journal", func(w http.ResponseWriter, r *http.Request) {
		store := s.stores[r.PathValue("chain")]
		if store == nil {
			write(w, 404, map[string]string{"error": "UNKNOWN_CHAIN"})
			return
		}
		after, e := strconv.ParseUint(r.URL.Query().Get("after"), 10, 64)
		if e != nil {
			write(w, 400, map[string]string{"error": "INVALID_CURSOR"})
			return
		}
		records, e := store.Records(after, 100)
		if e != nil {
			write(w, 500, map[string]string{"error": "JOURNAL_READ_FAILED"})
			return
		}
		write(w, 200, records)
	})
	mux.HandleFunc("POST /internal/checkpoints", func(w http.ResponseWriter, r *http.Request) {
		paths := []string{}
		for key, store := range s.stores {
			snapshot, ok := store.Current()
			if !ok {
				continue
			}
			path, e := checkpoint.Write(filepath.Join(s.cfg.Resolve(s.cfg.Config.State.Path), "checkpoints", strings.ReplaceAll(key, ":", "-")), snapshot)
			if e != nil {
				write(w, 500, map[string]string{"error": "CHECKPOINT_FAILED"})
				return
			}
			paths = append(paths, path)
		}
		write(w, 200, map[string]any{"manifests": paths})
	})
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if strings.HasPrefix(r.URL.Path, "/internal/") && s.cfg.Token != "" {
			expected := "Bearer " + s.cfg.Token
			got := r.Header.Get("Authorization")
			if subtle.ConstantTimeCompare([]byte(got), []byte(expected)) != 1 {
				write(w, 401, map[string]string{"error": "UNAUTHORIZED"})
				return
			}
		}
		mux.ServeHTTP(w, r)
	})
}
func (s *Service) Close() {
	for _, w := range s.sessions {
		w.close()
	}
	if s.control != nil {
		s.control.Close()
	}
	for _, store := range s.stores {
		store.Close()
	}
	if s.log != nil {
		s.log.Close()
	}
}

// Fact completeness is decoder-specific. A V3-only scope must not publish an
// empty V2 fact batch as evidence that V3 trades were decoded.
func hasV2Scope(pools []domain.Pool) bool {
	for _, p := range pools {
		if p.Protocol == "uniswap-v2" {
			return true
		}
	}
	return false
}
