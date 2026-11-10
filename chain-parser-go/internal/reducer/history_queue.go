package reducer

// History shares the owner's Pebble and transaction lock, but has no online
// publication capability. Coverage is qualified by dataset AND canonical revision.
import (
	"bytes"
	"context"
	"crypto/rand"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"github.com/cockroachdb/pebble"
	"math"
	"sort"
	"strings"
	"time"
	"xone/chain-parser-go/internal/domain"
	"xone/chain-parser-go/internal/ingest"
)

var (
	ErrHistoryCapacity = errors.New("HISTORY_CAPACITY")
	ErrHistoryClaim    = errors.New("HISTORY_CLAIM_REVOKED")
	ErrHistoryIdle     = errors.New("HISTORY_IDLE")
	ErrHistoryRevision = errors.New("HISTORY_CANONICAL_REVISION_CHANGED")
	ErrHistoryDataset  = errors.New("HISTORY_DATASET_SCOPE_CHANGED")
)
var historyQueueKey = []byte("history/jobs/v1")

const HistoryBaselineFactsGap = "BASELINE_BLOCK_FACTS_NOT_COLLECTED"

type HistoryQueueOptions struct {
	ExpectedOwner                    *domain.SnapshotRef
	Chain, DatasetID                 string
	MaxJobs, MaxBytes, MaxBlockBytes int
	ClaimTTL                         time.Duration
}
type HistoryCandidate struct {
	PoolID   string `json:"pool_id"`
	Protocol string `json:"protocol"`
	Token0   string `json:"token0,omitempty"`
	Token1   string `json:"token1,omitempty"`
	Factory  string `json:"factory,omitempty"`
}
type HistoryJob struct {
	Reason                     string
	Baseline                   domain.BlockRef
	CanonicalEpoch             uint64
	From, To, Next             uint64
	Done                       bool
	Attempts                   uint64
	RetryAt                    time.Time
	LastError, LastHash, Token string
	Until                      time.Time
}
type HistoryClaim struct {
	Height, From                                  uint64
	Epoch, Token, DatasetID, Revision, OwnerScope string
}
type HistoryQueueStatus struct {
	Chain, Dataset, DatasetID, Revision, CurrentRevision, DecoderVersion, Authority, Canonicality string
	CanonicalEpoch                                                                                uint64
	ScopeStale, Declared, Complete                                                                bool
	Jobs                                                                                          []HistoryJob
	Covered, Gaps                                                                                 []domain.Gap
	FactBlocks                                                                                    uint64
	Bytes                                                                                         int
}
type historyQueueDisk struct {
	Version                                              int
	Chain, Owner, Epoch, DatasetID, Revision, OwnerScope string
	Assignment, CanonicalEpoch                           uint64
	Jobs                                                 []HistoryJob
	FactBlocks                                           uint64
	Bytes                                                int // all retained datasets/revisions, receipts and manifests
}
type HistoryQueue struct {
	store   *Store
	options HistoryQueueOptions
	epoch   string
}
type HistoryRawPin struct {
	Raw       ingest.RawRef
	Reference string
}
type HistoryRawGroup struct {
	Parents []string
	ID      string
	Pins    []HistoryRawPin
}

// HistoryRawReceipt is an immutable same-batch owner receipt. Ack is separate:
// a missing ack is recoverable; a missing receipt NEVER authorizes pin release.
type HistoryRawReceipt struct {
	Parents                                                          []string
	Version                                                          int
	Group, DatasetID, Revision, CommitID, FactsDigest, CatalogDigest string
	Claim                                                            HistoryClaim
	Block                                                            domain.BlockRef
	Pins                                                             []HistoryRawPin
}

func historyToken() (string, error) {
	var b [16]byte
	if _, e := rand.Read(b[:]); e != nil {
		return "", e
	}
	return hex.EncodeToString(b[:]), nil
}
func historyDigest(raw []byte) string { h, _ := domain.Hash(json.RawMessage(raw)); return h }
func validHistoryID(id string) bool {
	b, e := hex.DecodeString(id)
	return e == nil && len(b) == 32 && id == strings.ToLower(id)
}
func (q *HistoryQueue) revision() string {
	id, _ := domain.Hash(struct {
		Chain, Owner               string
		Assignment, CanonicalEpoch uint64
	}{q.options.Chain, q.store.owner, q.store.assignment, q.store.durable.CanonicalEpoch})
	return id
}
func (s *Store) OpenHistoryQueue(o HistoryQueueOptions) (*HistoryQueue, error) {
	if domain.ValidateChain(o.Chain) != nil {
		return nil, errors.New("HISTORY_CHAIN")
	}
	if !validHistoryID(o.DatasetID) {
		return nil, ErrHistoryDataset
	}
	if o.MaxJobs == 0 {
		o.MaxJobs = 128
	}
	if o.MaxBytes == 0 {
		o.MaxBytes = 64 << 20
	}
	if o.MaxBlockBytes == 0 {
		o.MaxBlockBytes = 4 << 20
	}
	if o.ClaimTTL == 0 {
		o.ClaimTTL = 30 * time.Second
	}
	if o.MaxJobs < 1 || o.MaxJobs > 4096 || o.MaxBytes < 1 || o.MaxBlockBytes < 1 || o.MaxBlockBytes > 64<<20 || o.ClaimTTL <= 0 {
		return nil, errors.New("HISTORY_LIMITS")
	}
	epoch, e := historyToken()
	if e != nil {
		return nil, e
	}
	q := &HistoryQueue{store: s, options: o, epoch: epoch}
	s.mu.Lock()
	defer s.mu.Unlock()
	if o.ExpectedOwner != nil && s.durable != *o.ExpectedOwner {
		return nil, ErrHistoryDataset
	}
	d, e := q.load(false)
	if e != nil {
		return nil, e
	}
	if len(d.Jobs) > o.MaxJobs || d.Bytes > o.MaxBytes {
		return nil, ErrHistoryCapacity
	}
	batch := s.db.NewBatch()
	defer batch.Close()
	if e = q.bind(&d, o.DatasetID, batch); e != nil {
		return nil, e
	}
	d.Epoch = epoch
	d.Assignment = s.assignment
	for i := range d.Jobs {
		d.Jobs[i].Token = ""
		d.Jobs[i].Until = time.Time{}
	}
	// Reconstruct declarations, including baseline H itself, after crash-before-hook.
	if raw, c, err := s.db.Get([]byte("online/current")); err == nil {
		var saved struct {
			Reference   domain.SnapshotRef `json:"reference"`
			HistoryGaps []domain.Gap       `json:"history_gaps"`
		}
		err = json.Unmarshal(raw, &saved)
		c.Close()
		if err != nil {
			return nil, err
		}
		if saved.Reference.Block.ChainKey != o.Chain || saved.Reference.OwnerService != s.owner || saved.Reference.OwnerAssignmentVersion > s.assignment {
			return nil, errors.New("HISTORY_BASELINE_OWNER")
		}
		if e = q.addBaseline(context.Background(), &d, saved.Reference, saved.HistoryGaps); e != nil {
			return nil, e
		}
	} else if !errors.Is(err, pebble.ErrNotFound) {
		return nil, err
	}
	if e = q.save(context.Background(), d, batch); e != nil {
		return nil, e
	}
	return q, nil
}
func (q *HistoryQueue) load(fence bool) (historyQueueDisk, error) {
	d := historyQueueDisk{Version: 2, Chain: q.options.Chain, Owner: q.store.owner, Assignment: q.store.assignment}
	if q.store.closed || q.store.failed {
		return d, ErrOwnerReopenRequired
	}
	b, c, e := q.store.db.Get(historyQueueKey)
	if errors.Is(e, pebble.ErrNotFound) {
		if fence {
			return d, ErrHistoryClaim
		}
		return d, nil
	}
	if e != nil {
		return d, e
	}
	defer c.Close()
	if len(b) > 4<<20 || json.Unmarshal(b, &d) != nil || (d.Version != 1 && d.Version != 2) || d.Chain != q.options.Chain || d.Owner != q.store.owner || d.Assignment > q.store.assignment || d.Bytes < 0 {
		return d, errors.New("HISTORY_QUEUE_IDENTITY")
	}
	if fence && (d.Epoch != q.epoch || d.Assignment != q.store.assignment) {
		return d, ErrHistoryClaim
	}
	return d, nil
}

// bind runs under Store.mu, in the same batch as enqueue/claim or an explicit
// dataset update. Old bytes remain attributed to their immutable qualified keys.
func (q *HistoryQueue) bind(d *historyQueueDisk, dataset string, b *pebble.Batch) error {
	revision := q.revision()
	changed := d.Version != 2 || d.DatasetID != dataset || d.Revision != revision
	if changed {
		if len(d.Jobs) > 0 || d.FactBlocks > 0 {
			old, e := json.Marshal(d)
			if e != nil {
				return e
			}
			if len(old) > q.options.MaxBytes-d.Bytes {
				return ErrHistoryCapacity
			}
			oldDataset, oldRevision := d.DatasetID, d.Revision
			if oldDataset == "" {
				oldDataset = "legacy-unscoped-v1"
			}
			if oldRevision == "" {
				oldRevision = "legacy-height-only"
			}
			if e = b.Set([]byte("history/manifests/"+oldDataset+"/"+oldRevision), old, nil); e != nil {
				return e
			}
			d.Bytes += len(old)
		}
		for i := range d.Jobs {
			j := &d.Jobs[i]
			j.Next = j.From
			j.Done = false
			j.Attempts = 0
			j.RetryAt = time.Time{}
			j.LastError = ""
			j.LastHash = ""
			j.Token = ""
			j.Until = time.Time{}
			j.Baseline = q.store.durable.Block
			j.CanonicalEpoch = q.store.durable.CanonicalEpoch
		}
		d.FactBlocks = 0
	}
	if changed || d.OwnerScope != q.store.durable.ScopeHash {
		for i := range d.Jobs {
			d.Jobs[i].Token = ""
			d.Jobs[i].Until = time.Time{}
		}
	}
	d.Version = 2
	d.DatasetID = dataset
	d.Revision = revision
	d.OwnerScope = q.store.durable.ScopeHash
	d.CanonicalEpoch = q.store.durable.CanonicalEpoch
	return nil
}

// BindDataset CASes against the exact owner reference supplied with a frozen
// pool/factory scope. It does no network work and revokes in-flight old claims.
func (q *HistoryQueue) BindDataset(ctx context.Context, id string, expected domain.SnapshotRef) error {
	if !validHistoryID(id) {
		return ErrHistoryDataset
	}
	q.store.mu.Lock()
	defer q.store.mu.Unlock()
	d, e := q.load(true)
	if e != nil {
		return e
	}
	if q.store.durable != expected {
		return ErrHistoryDataset
	}
	if d.DatasetID == id && d.Revision == q.revision() && d.OwnerScope == expected.ScopeHash {
		return ctx.Err()
	}
	b := q.store.db.NewBatch()
	defer b.Close()
	if e = q.bind(&d, id, b); e != nil {
		return e
	}
	if e = q.save(ctx, d, b); e != nil {
		return e
	}
	q.options.DatasetID = id
	return nil
}
func (q *HistoryQueue) save(ctx context.Context, d historyQueueDisk, b *pebble.Batch) error {
	if e := ctx.Err(); e != nil {
		return e
	}
	raw, e := json.Marshal(d)
	if e != nil {
		return e
	}
	if len(raw) > 4<<20 {
		return ErrHistoryCapacity
	}
	if b == nil {
		b = q.store.db.NewBatch()
		defer b.Close()
	}
	if e = b.Set(historyQueueKey, raw, nil); e != nil {
		return e
	}
	if e = ctx.Err(); e != nil {
		return e
	}
	// Only actual I/O reaches this primitive. Validation/budget failures above do
	// not poison owner state; an uncertain history write fences the entire owner.
	return q.store.commitOwnerBatch(b)
}
func (q *HistoryQueue) addBaseline(ctx context.Context, d *historyQueueDisk, r domain.SnapshotRef, gaps []domain.Gap) error {
	for _, g := range gaps {
		if g.To >= r.Block.Number {
			return errors.New("HISTORY_BASELINE_RANGE")
		}
	}
	if e := q.addGaps(ctx, d, gaps, &r); e != nil {
		return e
	}
	return q.addGaps(ctx, d, []domain.Gap{{From: r.Block.Number, To: r.Block.Number, Reason: HistoryBaselineFactsGap}}, &r)
}
func (q *HistoryQueue) EnqueueBaseline(ctx context.Context, s domain.Snapshot) error {
	if s.Reference.OwnerService != q.store.owner || s.Reference.OwnerAssignmentVersion != q.store.assignment || s.Reference.Block.ChainKey != q.options.Chain {
		return errors.New("HISTORY_BASELINE_OWNER")
	}
	q.store.mu.Lock()
	defer q.store.mu.Unlock()
	d, e := q.load(true)
	if e != nil {
		return e
	}
	if s.Reference != q.store.durable {
		return ErrHistoryRevision
	}
	b := q.store.db.NewBatch()
	defer b.Close()
	if e = q.bind(&d, d.DatasetID, b); e != nil {
		return e
	}
	if e = q.addBaseline(ctx, &d, s.Reference, s.HistoryGaps); e != nil {
		return e
	}
	return q.save(ctx, d, b)
}
func (q *HistoryQueue) Enqueue(ctx context.Context, gaps []domain.Gap) error {
	if e := ctx.Err(); e != nil {
		return e
	}
	q.store.mu.Lock()
	defer q.store.mu.Unlock()
	d, e := q.load(true)
	if e != nil {
		return e
	}
	b := q.store.db.NewBatch()
	defer b.Close()
	if e = q.bind(&d, d.DatasetID, b); e != nil {
		return e
	}
	if e = q.addGaps(ctx, &d, gaps, nil); e != nil {
		return e
	}
	return q.save(ctx, d, b)
}
func (q *HistoryQueue) addGaps(ctx context.Context, d *historyQueueDisk, gaps []domain.Gap, baseline *domain.SnapshotRef) error {
	if len(gaps) > q.options.MaxJobs {
		return ErrHistoryCapacity
	}
	for _, g := range gaps {
		if err := ctx.Err(); err != nil {
			return err
		}
		if g.From > g.To || g.To == math.MaxUint64 || len(g.Reason) > 256 {
			return errors.New("HISTORY_GAP")
		}
		// Scan sorted disjoint intervals once per declaration. Splitting a long gap
		// does not multiply nested pending-range scans or enumerate block heights.
		cursor := g.From
		covered := false
		added := make([]HistoryJob, 0, 1)
		appendRange := func(from, to uint64) error {
			if len(d.Jobs)+len(added) >= q.options.MaxJobs {
				return ErrHistoryCapacity
			}
			j := HistoryJob{From: from, To: to, Next: from, Reason: g.Reason}
			if baseline != nil {
				j.Baseline = baseline.Block
				j.CanonicalEpoch = baseline.CanonicalEpoch
			}
			added = append(added, j)
			return nil
		}
		for _, j := range d.Jobs {
			if err := ctx.Err(); err != nil {
				return err
			}
			if j.To < cursor {
				continue
			}
			if j.From > g.To {
				break
			}
			if j.From > cursor {
				if e := appendRange(cursor, j.From-1); e != nil {
					return e
				}
			}
			if j.To >= g.To {
				covered = true
				break
			}
			cursor = j.To + 1
		}
		if !covered {
			if e := appendRange(cursor, g.To); e != nil {
				return e
			}
		}
		d.Jobs = append(d.Jobs, added...)
		sort.Slice(d.Jobs, func(i, j int) bool { return d.Jobs[i].From < d.Jobs[j].From })
	}
	return nil
}

func (q *HistoryQueue) Claim(ctx context.Context, now time.Time) (HistoryClaim, error) {
	if e := ctx.Err(); e != nil {
		return HistoryClaim{}, e
	}
	q.store.mu.Lock()
	defer q.store.mu.Unlock()
	d, e := q.load(true)
	if e != nil {
		return HistoryClaim{}, e
	}
	b := q.store.db.NewBatch()
	defer b.Close()
	switched := d.Revision != q.revision()
	if switched {
		if e = q.bind(&d, d.DatasetID, b); e != nil {
			return HistoryClaim{}, e
		}
	}
	if d.OwnerScope != q.store.durable.ScopeHash {
		return HistoryClaim{}, ErrHistoryDataset
	}
	for _, j := range d.Jobs {
		if j.Token != "" && now.Before(j.Until) {
			return HistoryClaim{}, ErrHistoryIdle
		}
	}
	for i := range d.Jobs {
		j := &d.Jobs[i]
		if j.Done || now.Before(j.RetryAt) {
			continue
		}
		if q.store.durable.Block.ChainKey != "" && j.Next > q.store.durable.Block.Number {
			continue
		}
		token, e := historyToken()
		if e != nil {
			return HistoryClaim{}, e
		}
		j.Token = token
		j.Until = now.Add(q.options.ClaimTTL)
		c := HistoryClaim{Height: j.Next, From: j.From, Epoch: q.epoch, Token: token, DatasetID: d.DatasetID, Revision: d.Revision, OwnerScope: d.OwnerScope}
		return c, q.save(ctx, d, b)
	}
	if switched {
		if e = q.save(ctx, d, b); e != nil {
			return HistoryClaim{}, e
		}
	}
	return HistoryClaim{}, ErrHistoryIdle
}
func (q *HistoryQueue) claimed(d *historyQueueDisk, c HistoryClaim, live bool) (*HistoryJob, error) {
	if c.Epoch != q.epoch || c.Token == "" || c.DatasetID != d.DatasetID || c.Revision != d.Revision || c.OwnerScope != d.OwnerScope {
		return nil, ErrHistoryClaim
	}
	if d.Revision != q.revision() {
		return nil, ErrHistoryRevision
	}
	if d.OwnerScope != q.store.durable.ScopeHash {
		return nil, ErrHistoryDataset
	}
	for i := range d.Jobs {
		j := &d.Jobs[i]
		if j.From == c.From && j.Next == c.Height && j.Token == c.Token && !j.Done && (!live || time.Now().Before(j.Until)) {
			return j, nil
		}
	}
	return nil, ErrHistoryClaim
}
func historyFactKey(dataset, revision string, n uint64) []byte {
	return []byte(fmt.Sprintf("history/facts/%s/%s/%020d", dataset, revision, n))
}
func historyCandidateKey(dataset, revision string, n uint64) []byte {
	return []byte(fmt.Sprintf("history/catalog/%s/%s/%020d", dataset, revision, n))
}
func historyReceiptKey(group string) []byte { return []byte("history/raw-receipts/" + group) }
func (q *HistoryQueue) Complete(ctx context.Context, c HistoryClaim, f domain.FactBatch, candidates []HistoryCandidate, groups ...HistoryRawGroup) error {
	if e := ctx.Err(); e != nil {
		return e
	}
	if len(f.Blocks) != 1 || f.Blocks[0].Block.ChainKey != q.options.Chain || f.Blocks[0].Block.Number != c.Height {
		return errors.New("HISTORY_FACT_POSITION")
	}
	if e := f.Validate("BLOCK_COMMIT", f.Blocks[0].Block); e != nil {
		return e
	}
	if len(candidates) > 1024 || len(groups) > 1 {
		return ErrHistoryCapacity
	}
	for _, v := range candidates {
		if v.PoolID == "" || v.Protocol == "" || len(v.PoolID)+len(v.Protocol)+len(v.Token0)+len(v.Token1)+len(v.Factory) > 1024 {
			return errors.New("HISTORY_CANDIDATE")
		}
	}
	facts, e := json.Marshal(f)
	if e != nil {
		return e
	}
	catalog, e := json.Marshal(candidates)
	if e != nil {
		return e
	}
	size := len(facts) + len(catalog)
	var proof HistoryRawReceipt
	var rawProof []byte
	if len(groups) > 0 {
		g := groups[0]
		if !validHistoryID(g.ID) || len(g.Pins) == 0 || len(g.Pins) > 512 || len(g.Parents) > 16 {
			return errors.New("HISTORY_RAW_GROUP")
		}
		seen := map[string]bool{}
		for _, p := range g.Pins {
			if p.Raw.Owner.Chain != q.options.Chain || !validHistoryID(p.Raw.ID) || !validHistoryID(p.Raw.Digest) || p.Raw.Bytes < 0 || p.Reference == "" || len(p.Reference) > 512 || seen[p.Raw.ID+"/"+p.Reference] {
				return errors.New("HISTORY_RAW_PIN")
			}
			seen[p.Raw.ID+"/"+p.Reference] = true
		}
		for _, parent := range g.Parents {
			if !validHistoryID(parent) || parent == g.ID {
				return errors.New("HISTORY_RAW_PARENT")
			}
		}
		proof = HistoryRawReceipt{Parents: g.Parents, Version: 1, Group: g.ID, DatasetID: c.DatasetID, Revision: c.Revision, Claim: c, Block: f.Blocks[0].Block, FactsDigest: historyDigest(facts), CatalogDigest: historyDigest(catalog), Pins: g.Pins}
		proof.CommitID, _ = domain.Hash(proof)
		rawProof, e = json.Marshal(proof)
		if e != nil {
			return e
		}
		size += len(rawProof) + 256
	}
	if size > q.options.MaxBlockBytes {
		return ErrHistoryCapacity
	}
	q.store.mu.Lock()
	defer q.store.mu.Unlock()
	d, e := q.load(true)
	if e != nil {
		return e
	}
	j, e := q.claimed(&d, c, true)
	if e != nil {
		return e
	}
	for _, parent := range proof.Parents {
		p, e := q.rawReceiptLocked(parent)
		if e != nil {
			return e
		}
		if p.Block.ChainKey != q.options.Chain {
			return errors.New("HISTORY_RAW_PARENT_CHAIN")
		}
	}
	block := f.Blocks[0].Block
	if j.LastHash != "" && block.ParentHash != j.LastHash {
		return errors.New("HISTORY_PARENT_DISCONTINUITY")
	}
	if j.Baseline.Number > 0 && c.Height == j.Baseline.Number-1 && block.Hash != j.Baseline.ParentHash || j.Baseline.ChainKey != "" && c.Height == j.Baseline.Number && block != j.Baseline {
		return errors.New("HISTORY_BASELINE_ANCHOR_MISMATCH")
	}
	if e = q.checkNeighbors(d, block); e != nil {
		return e
	}
	if size > q.options.MaxBytes-d.Bytes {
		return ErrHistoryCapacity
	}
	b := q.store.db.NewBatch()
	defer b.Close()
	if e = b.Set(historyFactKey(d.DatasetID, d.Revision, c.Height), facts, nil); e != nil {
		return e
	}
	if e = b.Set(historyCandidateKey(d.DatasetID, d.Revision, c.Height), catalog, nil); e != nil {
		return e
	}
	if len(rawProof) > 0 {
		if old, closer, err := q.store.db.Get(historyReceiptKey(proof.Group)); err == nil {
			same := bytes.Equal(old, rawProof)
			closer.Close()
			if !same {
				return errors.New("HISTORY_RAW_RECEIPT_CONFLICT")
			}
		} else if !errors.Is(err, pebble.ErrNotFound) {
			return err
		}
		if e = b.Set(historyReceiptKey(proof.Group), rawProof, nil); e != nil {
			return e
		}
	}
	j.LastHash = block.Hash
	j.Token = ""
	j.Until = time.Time{}
	j.RetryAt = time.Time{}
	j.LastError = ""
	j.Done = j.Next == j.To
	j.Next++
	d.FactBlocks++
	d.Bytes += size
	return q.save(ctx, d, b)
}
func (q *HistoryQueue) Retry(ctx context.Context, c HistoryClaim, at time.Time, code string) error {
	if e := ctx.Err(); e != nil {
		return e
	}
	if len(code) > 256 {
		return errors.New("HISTORY_RETRY_CODE")
	}
	q.store.mu.Lock()
	defer q.store.mu.Unlock()
	d, e := q.load(true)
	if e != nil {
		return e
	}
	j, e := q.claimed(&d, c, false)
	if e != nil {
		return e
	}
	if j.Attempts < math.MaxUint64 {
		j.Attempts++
	}
	j.RetryAt = at
	j.LastError = code
	j.Token = ""
	j.Until = time.Time{}
	return q.save(ctx, d, nil)
}
func (q *HistoryQueue) Status(ctx context.Context) (HistoryQueueStatus, error) {
	if e := ctx.Err(); e != nil {
		return HistoryQueueStatus{}, e
	}
	q.store.mu.RLock()
	defer q.store.mu.RUnlock()
	d, e := q.load(true)
	if e != nil {
		return HistoryQueueStatus{}, e
	}
	st := HistoryQueueStatus{Chain: d.Chain, Dataset: "explicit-v2-pool-and-factory-scope; not entire market", DatasetID: d.DatasetID, Revision: d.Revision, CurrentRevision: q.revision(), CanonicalEpoch: d.CanonicalEpoch, DecoderVersion: "uniswap-v2/1", Authority: "public-block-complete-receipts-root-and-configured-witnesses", Canonicality: "RPC-observed; finality and independent consensus not established", Jobs: d.Jobs, FactBlocks: d.FactBlocks, Bytes: d.Bytes}
	st.ScopeStale = d.Revision != st.CurrentRevision || d.OwnerScope != q.store.durable.ScopeHash
	for i, j := range d.Jobs {
		if st.ScopeStale {
			j.Next = j.From
			j.Done = false
			j.Token = ""
			st.Jobs[i] = j
		}
		if j.Next > j.From {
			st.Covered = append(st.Covered, domain.Gap{From: j.From, To: j.Next - 1})
		}
		if !j.Done {
			reason := j.LastError
			if reason == "" {
				reason = j.Reason
			}
			st.Gaps = append(st.Gaps, domain.Gap{From: j.Next, To: j.To, Reason: reason})
		}
	}
	if st.ScopeStale {
		st.FactBlocks = 0
	}
	st.Declared = len(st.Jobs) > 0
	st.Complete = st.Declared && !st.ScopeStale && len(st.Gaps) == 0
	return st, nil
}
func (q *HistoryQueue) readLocked(key []byte, out any) error {
	b, c, e := q.store.db.Get(key)
	if e != nil {
		return e
	}
	defer c.Close()
	return json.Unmarshal(b, out)
}
func (q *HistoryQueue) Facts(ctx context.Context, n uint64) (domain.FactBatch, error) {
	var f domain.FactBatch
	if e := ctx.Err(); e != nil {
		return f, e
	}
	q.store.mu.RLock()
	defer q.store.mu.RUnlock()
	d, e := q.load(true)
	if e != nil {
		return f, e
	}
	if d.Revision != q.revision() || d.OwnerScope != q.store.durable.ScopeHash {
		return f, ErrHistoryRevision
	}
	e = q.readLocked(historyFactKey(d.DatasetID, d.Revision, n), &f)
	return f, e
}
func (q *HistoryQueue) FactsAt(ctx context.Context, dataset, revision string, n uint64) (domain.FactBatch, error) {
	var f domain.FactBatch
	if !validHistoryID(dataset) || !validHistoryID(revision) {
		return f, ErrHistoryDataset
	}
	if e := ctx.Err(); e != nil {
		return f, e
	}
	q.store.mu.RLock()
	defer q.store.mu.RUnlock()
	if _, e := q.load(true); e != nil {
		return f, e
	}
	e := q.readLocked(historyFactKey(dataset, revision, n), &f)
	return f, e
}
func (q *HistoryQueue) Candidates(ctx context.Context, n uint64) ([]HistoryCandidate, error) {
	var out []HistoryCandidate
	if e := ctx.Err(); e != nil {
		return nil, e
	}
	q.store.mu.RLock()
	defer q.store.mu.RUnlock()
	d, e := q.load(true)
	if e != nil {
		return nil, e
	}
	if d.Revision != q.revision() || d.OwnerScope != q.store.durable.ScopeHash {
		return nil, ErrHistoryRevision
	}
	e = q.readLocked(historyCandidateKey(d.DatasetID, d.Revision, n), &out)
	return out, e
}
func (q *HistoryQueue) checkNeighbors(d historyQueueDisk, block domain.BlockRef) error {
	heights := []uint64{block.Number + 1}
	if block.Number > 0 {
		heights = append(heights, block.Number-1)
	}
	for _, n := range heights {
		var saved struct {
			Blocks []struct {
				Block domain.BlockRef `json:"block"`
			} `json:"blocks"`
		}
		e := q.readLocked(historyFactKey(d.DatasetID, d.Revision, n), &saved)
		if errors.Is(e, pebble.ErrNotFound) {
			continue
		}
		if e != nil {
			return e
		}
		if len(saved.Blocks) != 1 {
			return errors.New("HISTORY_FACT_CORRUPT")
		}
		neighbor := saved.Blocks[0].Block
		if n < block.Number && block.ParentHash != neighbor.Hash || n > block.Number && neighbor.ParentHash != block.Hash {
			return errors.New("HISTORY_PARENT_DISCONTINUITY")
		}
	}
	return nil
}
func (q *HistoryQueue) rawReceiptLocked(group string) (HistoryRawReceipt, error) {
	var p HistoryRawReceipt
	if !validHistoryID(group) {
		return p, errors.New("HISTORY_RAW_GROUP")
	}
	if e := q.readLocked(historyReceiptKey(group), &p); e != nil {
		return p, e
	}
	id := p.CommitID
	p.CommitID = ""
	expected, e := domain.Hash(p)
	p.CommitID = id
	if e != nil || p.Version != 1 || p.Group != group || id != expected || p.DatasetID != p.Claim.DatasetID || p.Revision != p.Claim.Revision || p.Block.Number != p.Claim.Height {
		return p, errors.New("HISTORY_RAW_PROOF_CORRUPT")
	}
	for _, v := range []struct {
		key    []byte
		digest string
	}{{historyFactKey(p.DatasetID, p.Revision, p.Block.Number), p.FactsDigest}, {historyCandidateKey(p.DatasetID, p.Revision, p.Block.Number), p.CatalogDigest}} {
		raw, c, e := q.store.db.Get(v.key)
		if e != nil {
			return p, e
		}
		same := historyDigest(raw) == v.digest
		c.Close()
		if !same {
			return p, errors.New("HISTORY_RAW_PROOF_CONTENT")
		}
	}
	return p, nil
}
func (q *HistoryQueue) RawReceipt(ctx context.Context, group string) (HistoryRawReceipt, error) {
	if e := ctx.Err(); e != nil {
		return HistoryRawReceipt{}, e
	}
	q.store.mu.RLock()
	defer q.store.mu.RUnlock()
	if _, e := q.load(true); e != nil {
		return HistoryRawReceipt{}, e
	}
	return q.rawReceiptLocked(group)
}

// RawReceiptPage scans at most limit immutable receipts, including acknowledged
// ones. A caller keeps next only for a bounded maintenance pass, never as proof.
func (q *HistoryQueue) RawReceiptPage(ctx context.Context, after string, limit int) (pending []HistoryRawReceipt, next string, err error) {
	if e := ctx.Err(); e != nil {
		return nil, "", e
	}
	if limit < 1 || limit > 128 || after != "" && !validHistoryID(after) {
		return nil, "", errors.New("HISTORY_RAW_PAGE")
	}
	q.store.mu.RLock()
	defer q.store.mu.RUnlock()
	if _, e := q.load(true); e != nil {
		return nil, "", e
	}
	it, e := q.store.db.NewIter(&pebble.IterOptions{LowerBound: []byte("history/raw-receipts/"), UpperBound: []byte("history/raw-receipts0")})
	if e != nil {
		return nil, "", e
	}
	defer it.Close()
	ok := it.First()
	if after != "" {
		ok = it.SeekGE(append(historyReceiptKey(after), 0))
	}
	scanned := 0
	for ok && scanned < limit {
		if e := ctx.Err(); e != nil {
			return nil, "", e
		}
		group := strings.TrimPrefix(string(it.Key()), "history/raw-receipts/")
		next = group
		scanned++
		_, c, e := q.store.db.Get([]byte("history/raw-acks/" + group))
		if e == nil {
			c.Close()
		} else if errors.Is(e, pebble.ErrNotFound) {
			p, e := q.rawReceiptLocked(group)
			if e != nil {
				return nil, "", e
			}
			pending = append(pending, p)
		} else {
			return nil, "", e
		}
		ok = it.Next()
	}
	if !ok {
		next = ""
	}
	return pending, next, it.Error()
}
func (q *HistoryQueue) AcknowledgeRaw(ctx context.Context, p HistoryRawReceipt) error {
	q.store.mu.Lock()
	defer q.store.mu.Unlock()
	if _, e := q.load(true); e != nil {
		return e
	}
	stored, e := q.rawReceiptLocked(p.Group)
	if e != nil {
		return e
	}
	a, _ := json.Marshal(stored)
	b, _ := json.Marshal(p)
	if !bytes.Equal(a, b) {
		return errors.New("HISTORY_RAW_PROOF_UNCONFIRMED")
	}
	batch := q.store.db.NewBatch()
	defer batch.Close()
	if e = batch.Set([]byte("history/raw-acks/"+p.Group), []byte(p.CommitID), nil); e != nil {
		return e
	}
	if e = ctx.Err(); e != nil {
		return e
	}
	return q.store.commitOwnerBatch(batch)
}

func (q *HistoryQueue) Chain() string { return q.options.Chain }
