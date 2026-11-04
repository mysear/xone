package analysis

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"os"
	"sort"
	"xone/chain-parser-go/internal/domain"
	"xone/chain-parser-go/internal/journal"
)

type JournalState struct {
	Schema          uint32                     `json:"schema_version"`
	Plugin          string                     `json:"plugin_version"`
	Interval        uint64                     `json:"interval_ms"`
	Chain           string                     `json:"chain_key"`
	Owner           string                     `json:"owner_service"`
	Assignment      uint64                     `json:"assignment_version"`
	Cursor          journal.Cursor             `json:"cursor"`
	TopicID         string                     `json:"topic_id"`
	Reference       *domain.SnapshotRef        `json:"snapshot_ref"`
	LastUnitHash    string                     `json:"last_unit_hash"`
	Pending         string                     `json:"pending"`
	Ready           bool                       `json:"ready"`
	SuspendedReason string                     `json:"suspended_reason"`
	CoverageStart   *domain.BlockRef           `json:"coverage_start"`
	Gaps            []domain.Gap               `json:"history_gaps"`
	Trades          map[string]Trade           `json:"trades"`
	Identities      map[string]string          `json:"event_content_hashes"`
	Revisions       map[string]uint64          `json:"revisions"`
	Windows         map[string]Candle          `json:"windows"`
	Blocks          map[string]domain.BlockRef `json:"canonical_blocks"`
}
type JournalRelease struct {
	EventTimeWatermarkMS  uint64              `json:"event_time_watermark_ms"`
	PluginVersion         string              `json:"plugin_version"`
	Publishable           bool                `json:"publishable"`
	Reason                string              `json:"reason,omitempty"`
	Finality              string              `json:"finality"`
	AmountUnits           string              `json:"amount_units"`
	Watermark             *domain.SnapshotRef `json:"watermark"`
	CoverageStart         *domain.BlockRef    `json:"coverage_start"`
	HistoryGaps           []domain.Gap        `json:"history_gaps"`
	Candles               []Candle            `json:"candles"`
	OnlineStateUnmodified bool                `json:"online_state_unmodified"`
}
type journalCheckpoint struct {
	State   *JournalState  `json:"state"`
	Release JournalRelease `json:"release"`
	SHA256  string         `json:"state_sha256"`
}

func NewJournalState(chain, owner string, assignment, interval uint64) *JournalState {
	return &JournalState{Schema: 1, Plugin: "candles/1", Interval: interval, Chain: chain, Owner: owner, Assignment: assignment, Gaps: []domain.Gap{}, Trades: map[string]Trade{}, Identities: map[string]string{}, Revisions: map[string]uint64{}, Windows: map[string]Candle{}, Blocks: map[string]domain.BlockRef{}}
}
func (s *JournalState) analyzer() *Analyzer {
	return &Analyzer{interval: s.Interval, trades: s.Trades, revision: s.Revisions}
}
func (s *JournalState) Apply(u journal.Unit, cursor journal.Cursor) error {
	// Mutate a private copy: a rejected/unsupported fact never partly changes the
	// next cursor, canonical membership, revisions or published checkpoint.
	raw, e := json.Marshal(s)
	if e != nil {
		return e
	}
	var next JournalState
	if e = json.Unmarshal(raw, &next); e != nil {
		return e
	}
	if e = next.apply(u, cursor); e != nil {
		return e
	}
	*s = next
	return nil
}
func (s *JournalState) apply(u journal.Unit, cursor journal.Cursor) error {
	if s.Interval == 0 || s.Schema != 1 || s.Plugin != "candles/1" {
		return errors.New("ANALYTICS_STATE_SCHEMA")
	}
	if e := u.Snapshot.Validate(s.Owner, s.Assignment); e != nil {
		return e
	}
	r := u.Snapshot.Reference
	if r.Block.ChainKey != s.Chain || cursor.Topic == "" || cursor.Partition != 0 || cursor.Offset < 1 {
		return errors.New("ANALYTICS_CURSOR_IDENTITY")
	}
	if e := u.ValidateFacts(); e != nil {
		return e
	}
	content, e := domain.Hash(u)
	if e != nil {
		return e
	}
	if s.Reference != nil && r == *s.Reference {
		if content != s.LastUnitHash || cursor != s.Cursor {
			return errors.New("ANALYTICS_DUPLICATE_CONFLICT")
		}
		return nil
	}
	if s.Reference != nil && (cursor.Topic != s.Cursor.Topic || cursor.Offset <= s.Cursor.Offset) {
		return errors.New("ANALYTICS_CURSOR_REGRESSION")
	}
	if u.Kind == "ONLINE_BASELINE_COMMIT" {
		if s.Reference != nil {
			old := *s.Reference
			if r.OnlineGeneration <= old.OnlineGeneration || r.JournalSeq <= old.JournalSeq || r.WriterEpoch < old.WriterEpoch || r.CanonicalEpoch < old.CanonicalEpoch {
				return errors.New("ANALYTICS_STALE_BASELINE")
			}
			if r.CanonicalEpoch != old.CanonicalEpoch || s.Pending == "REORG_BEGIN" {
				blocks := map[string]bool{}
				for _, trade := range s.Trades {
					blocks[trade.Block] = true
				}
				for block := range blocks {
					s.analyzer().Revert(s.Chain, block)
				}
				s.Blocks = map[string]domain.BlockRef{}
				s.Gaps = append(s.Gaps, domain.Gap{From: 0, To: r.Block.Number, Reason: "REORG_HISTORY_REPAIR_REQUIRED"})
			} else if r.Block.Number < old.Block.Number {
				return errors.New("ANALYTICS_BASELINE_REWIND")
			} else if r.Block.Number > old.Block.Number {
				s.Gaps = append(s.Gaps, domain.Gap{From: old.Block.Number + 1, To: r.Block.Number, Reason: "HEAD_FIRST_FACT_GAP"})
			}
		} else {
			s.Gaps = append(s.Gaps, domain.Gap{From: 0, To: r.Block.Number, Reason: "BASELINE_HAS_NO_HISTORICAL_FACTS"})
		}
		s.Blocks[r.Block.Hash] = r.Block
		start := r.Block
		s.CoverageStart = &start
		s.Pending = ""
		s.Ready = true
	} else {
		if s.Reference == nil {
			return errors.New("ANALYTICS_BASELINE_REQUIRED")
		}
		old := *s.Reference
		if r.JournalSeq != old.JournalSeq+1 || r.OnlineGeneration != old.OnlineGeneration || r.WriterEpoch != old.WriterEpoch || r.ScopeHash != old.ScopeHash || r.AdapterSetHash != old.AdapterSetHash {
			return errors.New("ANALYTICS_JOURNAL_SEQUENCE")
		}
		switch u.Kind {
		case "ONLINE_SCOPE_REVOKED":
			if s.Pending != "" || r.CanonicalEpoch != old.CanonicalEpoch || r.Block != old.Block {
				return errors.New("ANALYTICS_REVOCATION_MISMATCH")
			}
			s.Pending = u.Kind
			s.Ready = false
		case "REORG_BEGIN":
			if (s.Pending != "" && s.Pending != "ONLINE_SCOPE_REVOKED") || old.CanonicalEpoch == ^uint64(0) || r.CanonicalEpoch != old.CanonicalEpoch+1 || r.Block != old.Block {
				return errors.New("ANALYTICS_REORG_BEGIN_MISMATCH")
			}
			s.Pending = u.Kind
			s.Ready = false
		case "BLOCK_COMMIT":
			if s.Pending != "" || r.CanonicalEpoch != old.CanonicalEpoch || r.Block.ParentHash != old.Block.Hash || r.Block.Number <= old.Block.Number || r.Block.Number-old.Block.Number != 1 {
				return errors.New("ANALYTICS_BLOCK_CONTINUITY")
			}
			if u.NormalizedFacts == nil {
				return errors.New("ANALYTICS_FACTS_REQUIRED")
			}
			if e = s.addBlocks(u.NormalizedFacts.Blocks); e != nil {
				return e
			}
			s.Ready = true
		case "REORG_COMMIT":
			if s.Pending != "REORG_BEGIN" || r.CanonicalEpoch != old.CanonicalEpoch || u.NormalizedFacts == nil {
				return errors.New("ANALYTICS_REORG_COMMIT_MISMATCH")
			}
			f := u.NormalizedFacts
			if len(f.RevertedBlocks) == 0 || f.RevertedBlocks[0] != old.Block {
				return errors.New("ANALYTICS_REVERT_TIP")
			}
			for _, b := range f.RevertedBlocks {
				known, ok := s.Blocks[b.Hash]
				if !ok || known != b {
					return errors.New("ANALYTICS_REVERT_NOT_TRACKED")
				}
				s.analyzer().Revert(s.Chain, b.Hash)
				delete(s.Blocks, b.Hash)
			}
			last := f.RevertedBlocks[len(f.RevertedBlocks)-1]
			ancestor, ok := s.Blocks[last.ParentHash]
			if !ok || ancestor.Number >= last.Number || last.Number-ancestor.Number != 1 {
				return errors.New("ANALYTICS_ANCESTOR_NOT_TRACKED")
			}
			if e = s.addBlocks(f.Blocks); e != nil {
				return e
			}
			s.Pending = ""
			s.Ready = true
		default:
			return errors.New("ANALYTICS_JOURNAL_KIND")
		}
	}
	s.Reference = &r
	s.LastUnitHash = content
	s.Cursor = cursor
	s.SuspendedReason = ""
	return nil
}
func (s *JournalState) addBlocks(blocks []domain.FactBlock) error {
	for _, b := range blocks {
		for _, f := range b.Swaps {
			identity, e := domain.Hash(f)
			if e != nil {
				return e
			}
			if h, ok := s.Identities[f.EventID]; ok && h != identity {
				return errors.New("ANALYTICS_EVENT_CONTENT_CONFLICT")
			}
			var base, quote domain.Amount
			if f.Amount0In.Big().Sign() > 0 && f.Amount1Out.Big().Sign() > 0 && f.Amount1In == "0" && f.Amount0Out == "0" {
				base, quote = f.Amount0In, f.Amount1Out
			} else if f.Amount1In.Big().Sign() > 0 && f.Amount0Out.Big().Sign() > 0 && f.Amount0In == "0" && f.Amount1Out == "0" {
				base, quote = f.Amount0Out, f.Amount1In
			} else {
				return errors.New("ANALYTICS_COMPLEX_SWAP_UNSUPPORTED")
			}
			trade := Trade{ID: f.EventID, Chain: s.Chain, Pool: f.PoolID, Block: b.Block.Hash, TimestampMS: b.Block.TimestampMS, TxIndex: f.TxIndex, EventIndex: f.LogIndex, Base: base, Quote: quote}
			a := s.analyzer()
			if e = a.Apply(trade); e != nil {
				return e
			}
			s.Identities[f.EventID] = identity
			key := a.key(trade)
			s.Windows[key] = Candle{Chain: trade.Chain, Pool: trade.Pool, WindowMS: trade.TimestampMS / s.Interval * s.Interval}
		}
		s.Blocks[b.Block.Hash] = b.Block
	}
	return nil
}
func (s *JournalState) Release() JournalRelease {
	ready := s.Ready && s.Pending == "" && s.SuspendedReason == ""
	reason := s.SuspendedReason
	if s.Pending != "" {
		reason = s.Pending
	}
	release := JournalRelease{PluginVersion: s.Plugin, Publishable: ready, Reason: reason, Finality: "HEAD_PROVISIONAL", AmountUnits: "TOKEN0_TOKEN1_ATOMIC_UNITS", Watermark: s.Reference, CoverageStart: s.CoverageStart, HistoryGaps: s.Gaps, Candles: []Candle{}, OnlineStateUnmodified: true}
	if s.Reference != nil {
		release.EventTimeWatermarkMS = s.Reference.Block.TimestampMS
	}
	if !ready {
		return release
	}
	candles := s.analyzer().Candles()
	active := map[string]bool{}
	for _, c := range candles {
		key := windowKey(c.Chain, c.Pool, c.WindowMS)
		active[key] = true
		release.Candles = append(release.Candles, c)
	}
	for key, c := range s.Windows {
		if !active[key] {
			c.Deleted = true
			c.Revision = s.Revisions[key]
			release.Candles = append(release.Candles, c)
		}
	}
	sort.Slice(release.Candles, func(i, j int) bool {
		l, r := release.Candles[i], release.Candles[j]
		if l.WindowMS != r.WindowMS {
			return l.WindowMS < r.WindowMS
		}
		return l.Pool < r.Pool
	})
	return release
}
func (s *JournalState) Save(path string, maxBytes int) error {
	b, e := json.Marshal(s)
	if e != nil {
		return e
	}
	h := sha256.Sum256(b)
	raw, e := json.Marshal(journalCheckpoint{State: s, Release: s.Release(), SHA256: hex.EncodeToString(h[:])})
	if e != nil {
		return e
	}
	limit := maxBytes
	if s.Ready && s.SuspendedReason == "" {
		// Keep bounded room in the last durable checkpoint for a subsequent
		// failure status, even when candidate retention reaches its budget.
		reserve := maxBytes / 8
		if reserve > 4096 {
			reserve = 4096
		}
		limit -= reserve
	}
	if maxBytes <= 0 || len(raw) > limit {
		return errors.New("ANALYTICS_CHECKPOINT_LIMIT")
	}
	return publishBytes(path, raw)
}
func LoadJournalState(path string, maxBytes int, chain, owner string, assignment, interval uint64) (*JournalState, error) {
	info, e := os.Stat(path)
	if errors.Is(e, os.ErrNotExist) {
		return NewJournalState(chain, owner, assignment, interval), nil
	}
	if e != nil {
		return nil, e
	}
	if !info.Mode().IsRegular() || info.Size() > int64(maxBytes) {
		return nil, errors.New("ANALYTICS_CHECKPOINT_LIMIT")
	}
	raw, e := os.ReadFile(path)
	if e != nil {
		return nil, e
	}
	var envelope journalCheckpoint
	if e = json.Unmarshal(raw, &envelope); e != nil || envelope.State == nil {
		return nil, errors.New("ANALYTICS_CHECKPOINT_SCHEMA")
	}
	s := envelope.State
	b, e := json.Marshal(s)
	if e != nil {
		return nil, e
	}
	h := sha256.Sum256(b)
	if hex.EncodeToString(h[:]) != envelope.SHA256 || s.Schema != 1 || s.Plugin != "candles/1" || s.Chain != chain || s.Owner != owner || s.Assignment != assignment || s.Interval != interval || s.Trades == nil || s.Identities == nil || s.Revisions == nil || s.Windows == nil || s.Blocks == nil {
		return nil, errors.New("ANALYTICS_CHECKPOINT_IDENTITY")
	}
	expected, _ := domain.Hash(s.Release())
	actual, _ := domain.Hash(envelope.Release)
	if expected != actual {
		return nil, errors.New("ANALYTICS_RELEASE_CONFLICT")
	}
	return s, nil
}

func (s *JournalState) ApplyAndSave(u journal.Unit, cursor journal.Cursor, path string, maxBytes int) error {
	raw, e := json.Marshal(s)
	if e != nil {
		return e
	}
	var next JournalState
	if e = json.Unmarshal(raw, &next); e != nil {
		return e
	}
	if e = next.apply(u, cursor); e != nil {
		return e
	}
	if e = next.Save(path, maxBytes); e != nil {
		return e
	}
	*s = next
	return nil
}

// ConfirmReadComplete follows a successful bounded read from the durable cursor.
// No missing/poison unit may be skipped to reach it, and it never clears a fork.
func (s *JournalState) ConfirmReadComplete() {
	if s.Reference != nil && s.Pending == "" {
		s.Ready = true
		s.SuspendedReason = ""
	}
}
